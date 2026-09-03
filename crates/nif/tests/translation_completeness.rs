//! Cross-game translation-completeness harness — #1277 Task 8.
//!
//! For each supported game, walk a bounded sample of meshes from the
//! primary archive, run them through the NIF importer, and collect
//! per-game `MaterialStats` covering the canonical `Material` slots
//! the renderer downstream depends on. Compare across games and assert
//! structural invariants.
//!
//! ## What this catches
//!
//! - **Translation-layer regressions**: a parser/importer change that
//!   silently drops `texture_path` or stops setting `material_kind` on
//!   one game while still working on others. Today the per-game audits
//!   inspect code; this harness inspects *output*.
//! - **Convention drift**: when one game starts producing wildly
//!   different fill-rates for canonical fields (e.g., FNV suddenly
//!   reports 0% `metalness_override` set when it had been 12%) the
//!   harness surfaces the regression in the printed comparison table.
//! - **Structural invariants**: every imported mesh must have
//!   `positions.len() == normals.len() == colors.len()`. A regression
//!   that produces mismatched buffer lengths would crash the GPU
//!   upload silently — this gate fires before it reaches Vulkan.
//!
//! ## What this does NOT catch (yet)
//!
//! - **Value-level equivalence**: this harness doesn't load "the same
//!   wood door across 4 games" and compare canonical Material values.
//!   That needs a curated equivalent-surface registry (FNV path X
//!   corresponds to FO4 path Y) which is a separate effort. The
//!   per-game *aggregate* stats here are the cheapest first-pass
//!   regression guard.
//!
//! ## Invocation
//!
//! ```sh
//! cargo test -p byroredux-nif --test translation_completeness -- --ignored
//! ```
//!
//! All tests are `#[ignore]`d because they require real game data
//! (BYROREDUX_*_DATA env vars or default Steam install paths). CI runs
//! them only on the dev machine where the data is present.

mod common;

use byroredux_core::string::StringPool;
use byroredux_nif::import::{import_nif_with_resolver, ImportedMesh, MeshResolver};
use byroredux_nif::parse_nif;
use common::{open_ba2_by_name, open_mesh_archive, Game, MeshArchive};
use std::collections::{BTreeMap, VecDeque};

/// Every game the cross-game completeness signal walks. Kept in lock-step
/// with [`Game::ALL`] by `harness_enumerates_every_supported_game` so the
/// two newest/hardest games (FO76, Starfield) can't be silently dropped
/// from the regression signal again (#1362).
const HARNESS_GAMES: &[(&str, Game)] = &[
    ("Oblivion", Game::Oblivion),
    ("FO3", Game::Fallout3),
    ("FNV", Game::FalloutNV),
    ("SkyrimSE", Game::SkyrimSE),
    ("FO4", Game::Fallout4),
    ("FO76", Game::Fallout76),
    ("Starfield", Game::Starfield),
];

/// A [`MeshResolver`] backed by a chain of BA2 archives. Starfield's
/// `BSGeometry` blocks reference external `geometries\<hash>.mesh` companion
/// files (#1292); without a resolver every Starfield mesh imports as zero
/// geometry and the completeness signal is blind to the entire Starfield
/// translation path (#1362). The importer composes the canonical
/// `geometries\<X>.mesh` key itself, so `resolve` only has to extract that key
/// (trying separator variants) from the first archive in the chain that has it.
struct ChainResolver {
    archives: Vec<MeshArchive>,
}

impl MeshResolver for ChainResolver {
    fn resolve(&self, mesh_name: &str) -> Option<Vec<u8>> {
        let candidates = [
            mesh_name.to_string(),
            mesh_name.replace('/', "\\"),
            mesh_name.replace('\\', "/"),
        ];
        self.archives
            .iter()
            .find_map(|a| candidates.iter().find_map(|c| a.extract(c).ok()))
    }
}

/// Build the external-`.mesh` resolver for Starfield from whatever standard
/// mesh archives are installed. Returns `None` when none are present (the
/// harness then reports Starfield as 0 meshes with an explicit note rather
/// than mistaking it for a translation regression).
fn starfield_resolver() -> Option<ChainResolver> {
    // The standard (non-LOD, non-Face) geometry archives. Vanilla Starfield
    // ships Meshes01 + Meshes02; MeshesPatch lands with updates.
    let archives: Vec<MeshArchive> = [
        "Starfield - Meshes01.ba2",
        "Starfield - Meshes02.ba2",
        "Starfield - MeshesPatch.ba2",
    ]
    .iter()
    .filter_map(|name| open_ba2_by_name(Game::Starfield, name))
    .collect();
    (!archives.is_empty()).then_some(ChainResolver { archives })
}

/// Bounded sample size per game. Each NIF parses + imports in a few
/// ms on the dev machine; 200 is enough to detect regressions in the
/// importer's fill-rates without making the test runtime per-game
/// exceed ~5 s.
const SAMPLE_LIMIT: usize = 200;

/// Per-game aggregate statistics over imported meshes. Every field is
/// either a count or a sum — the printed comparison divides counts by
/// `imported_meshes` to produce fill-rates.
#[derive(Debug, Default)]
struct MaterialStats {
    /// Total meshes successfully extracted across all parsed NIFs.
    imported_meshes: usize,
    /// Meshes whose `texture_path` slot is populated.
    with_texture_path: usize,
    /// Meshes whose `material_path` slot is populated (FO4+ BGSM, FNV
    /// `.mat`, …).
    with_material_path: usize,
    /// Meshes whose `material_kind` is non-zero (i.e. classified into
    /// some category). 0 = default-unspecialized.
    with_material_kind: usize,
    /// Meshes whose `metalness_override` is `Some`.
    with_metalness_override: usize,
    /// Meshes whose `roughness_override` is `Some`.
    with_roughness_override: usize,
    /// Meshes whose `normal_map` slot is populated.
    with_normal_map: usize,
    /// Meshes whose `tangents` vector is non-empty (tangent extraction
    /// produced data — either authored or synthesized).
    with_tangents: usize,
    /// Meshes whose vertex buffers pass the structural-consistency
    /// check (positions / normals / colors / uvs same length).
    structurally_consistent: usize,
    /// Meshes that FAILED the structural-consistency check. A non-zero
    /// value here is a HARD REGRESSION.
    structurally_inconsistent: Vec<String>,
}

impl MaterialStats {
    fn record(&mut self, source_nif: &str, mesh: &ImportedMesh) {
        self.imported_meshes += 1;
        if mesh.material.textures.base_color.is_some() {
            self.with_texture_path += 1;
        }
        if mesh.material.material_path.is_some() {
            self.with_material_path += 1;
        }
        if mesh.material.material_kind != 0 {
            self.with_material_kind += 1;
        }
        if mesh.material.metalness_override.is_some() {
            self.with_metalness_override += 1;
        }
        if mesh.material.roughness_override.is_some() {
            self.with_roughness_override += 1;
        }
        if mesh.material.textures.normal.is_some() {
            self.with_normal_map += 1;
        }
        if !mesh.tangents.is_empty() {
            self.with_tangents += 1;
        }
        // Structural consistency: per-vertex buffers must be the same
        // length (or empty if the mesh authored no data for them).
        let n = mesh.positions.len();
        let ok = (mesh.normals.is_empty() || mesh.normals.len() == n)
            && (mesh.colors.is_empty() || mesh.colors.len() == n)
            && (mesh.uvs.is_empty() || mesh.uvs.len() == n)
            && (mesh.tangents.is_empty() || mesh.tangents.len() == n);
        if ok {
            self.structurally_consistent += 1;
        } else {
            // Surface up to 10 examples — a flood would dilute signal.
            if self.structurally_inconsistent.len() < 10 {
                self.structurally_inconsistent.push(format!(
                    "{}: positions={} normals={} colors={} uvs={} tangents={}",
                    source_nif,
                    n,
                    mesh.normals.len(),
                    mesh.colors.len(),
                    mesh.uvs.len(),
                    mesh.tangents.len(),
                ));
            }
        }
    }

    fn pct(num: usize, denom: usize) -> f64 {
        if denom == 0 {
            0.0
        } else {
            100.0 * num as f64 / denom as f64
        }
    }

    fn print_row(&self, label: &str) {
        eprintln!(
            "  {:<12} meshes={:>4}  tex={:>5.1}%  mat_path={:>5.1}%  m_kind={:>5.1}%  metO={:>5.1}%  rghO={:>5.1}%  nrm={:>5.1}%  tan={:>5.1}%  consistent={:>5.1}%",
            label,
            self.imported_meshes,
            Self::pct(self.with_texture_path, self.imported_meshes),
            Self::pct(self.with_material_path, self.imported_meshes),
            Self::pct(self.with_material_kind, self.imported_meshes),
            Self::pct(self.with_metalness_override, self.imported_meshes),
            Self::pct(self.with_roughness_override, self.imported_meshes),
            Self::pct(self.with_normal_map, self.imported_meshes),
            Self::pct(self.with_tangents, self.imported_meshes),
            Self::pct(self.structurally_consistent, self.imported_meshes),
        );
    }
}

fn assert_pbr_override_fill(s: &MaterialStats, label: &str, floor: f64) {
    let metalness = MaterialStats::pct(s.with_metalness_override, s.imported_meshes);
    let roughness = MaterialStats::pct(s.with_roughness_override, s.imported_meshes);
    assert!(
        metalness >= floor,
        "[{label}] metalness_override fill < {floor:.1}% (got {metalness:.1}%)"
    );
    assert!(
        roughness >= floor,
        "[{label}] roughness_override fill < {floor:.1}% (got {roughness:.1}%)"
    );
}

/// The stratification bucket for one NIF path: the top-level *content*
/// directory under the shared `meshes\` archive root (e.g. `actors`,
/// `architecture`, `clutter`), or the path's own first component when it
/// isn't rooted under `meshes\` at all. Case-folded and separator-agnostic
/// (`\` and `/` both appear across BSA/BA2 archives depending on
/// game/tooling). `meshes\` itself is never a useful bucket key — nearly
/// every sampled path shares it — so it's stripped before bucketing.
///
/// #2213 (NIFAL-D9-01) — this is the key stratified sampling buckets on.
fn stratification_bucket(path: &str) -> String {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let mut parts = normalized.split('/').filter(|s| !s.is_empty());
    let first = parts.next().unwrap_or_default();
    if first == "meshes" {
        parts.next().unwrap_or(first).to_string()
    } else {
        first.to_string()
    }
}

/// Round-robin `limit` files out of `files` across [`stratification_bucket`]
/// groups instead of taking a flat prefix.
///
/// #2213 (NIFAL-D9-01) — `files.sort(); files.truncate(limit)` alone
/// collapses the entire sample onto whatever directory sorts first
/// alphabetically (Skyrim: 100% from `meshes\actors\`; Oblivion: 100% from
/// `meshes\architecture\`), confounding the cross-game comparison this
/// harness exists to make with content-class differences instead. The sort
/// itself is deliberate and stays (#1279 — without it, BA2 HashMap-iteration
/// order makes consecutive runs sample different files); the defect was
/// truncating a flat sorted list, not sorting it.
///
/// Fully deterministic: `BTreeMap` iterates buckets in sorted key order, and
/// each bucket's `VecDeque` preserves the caller's (already-sorted) file
/// order, so the same input always yields the same sample.
fn stratified_sample(files: Vec<String>, limit: usize) -> Vec<String> {
    if files.len() <= limit {
        return files;
    }
    let mut buckets: BTreeMap<String, VecDeque<String>> = BTreeMap::new();
    for f in files {
        buckets
            .entry(stratification_bucket(&f))
            .or_default()
            .push_back(f);
    }
    let mut sampled = Vec::with_capacity(limit);
    loop {
        let mut progressed = false;
        for queue in buckets.values_mut() {
            let Some(f) = queue.pop_front() else {
                continue;
            };
            sampled.push(f);
            progressed = true;
            if sampled.len() == limit {
                return sampled;
            }
        }
        if !progressed {
            return sampled;
        }
    }
}

/// Walk a stratified sample of up to `SAMPLE_LIMIT` NIFs in `archive`,
/// parse + import each, aggregate `MaterialStats`. Skips files that fail to
/// extract or parse (those are surfaced separately by parse_real_nifs.rs).
///
/// `resolver` supplies external `.mesh` companion geometry (Starfield
/// `BSGeometry`); pass `None` for games whose geometry is inline.
fn collect_stats(archive: &MeshArchive, resolver: Option<&dyn MeshResolver>) -> MaterialStats {
    let mut stats = MaterialStats::default();
    // Sort before sampling so the candidate list is identical across runs.
    // `list_files()` returns items in archive-internal order, which on BA2
    // happens to be HashMap-iteration order — without the sort, two
    // consecutive runs would sample different files and the fill-rate
    // comparison becomes useless. Pinned by #1279 verification.
    let mut files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
        .collect();
    files.sort();
    let files = stratified_sample(files, SAMPLE_LIMIT);

    for path in &files {
        let Ok(bytes) = archive.extract(path) else {
            continue;
        };
        let Ok(scene) = parse_nif(&bytes) else {
            continue;
        };
        let mut pool = StringPool::new();
        let meshes = import_nif_with_resolver(&scene, &mut pool, resolver);
        for mesh in &meshes {
            stats.record(path, mesh);
        }
    }

    stats
}

/// Run the harness across every game with available data and assert
/// the structural-consistency invariant. Prints a per-game comparison
/// table that's the regression-detection signal for the canonical
/// translation layer.
#[test]
#[ignore = "needs game data on disk for one or more of Oblivion/FO3/FNV/SkyrimSE/FO4/FO76/Starfield"]
fn cross_game_translation_completeness() {
    eprintln!("\n=== #1277 Task 8: cross-game translation completeness ===");
    eprintln!(
        "  {:<12} {:>14}  {:>10}  {:>13}  {:>10}  {:>9}  {:>9}  {:>9}  {:>9}  {:>16}",
        "game",
        "imported",
        "tex%",
        "mat_path%",
        "m_kind%",
        "metO%",
        "rghO%",
        "nrm%",
        "tan%",
        "consistent%",
    );

    let mut hard_failures: Vec<(String, Vec<String>)> = Vec::new();
    let mut probed = 0usize;
    for &(label, game) in HARNESS_GAMES {
        let Some(archive) = open_mesh_archive(game) else {
            eprintln!("  {label:<12} SKIP (no data)");
            continue;
        };
        probed += 1;
        // Starfield geometry lives in external `geometries\X.mesh` files
        // (#1292); supply a chain resolver so the completeness signal can
        // actually see its translation path (#1362). Inline-geometry games
        // pass `None`.
        let sf_resolver = (game == Game::Starfield).then(starfield_resolver).flatten();
        let stats = collect_stats(
            &archive,
            sf_resolver.as_ref().map(|r| r as &dyn MeshResolver),
        );
        stats.print_row(label);
        // A 0-mesh Starfield row means the external geometry archives are
        // absent (or the resolver couldn't be built), NOT a translation
        // regression — call that out so the row isn't misread.
        if game == Game::Starfield && stats.imported_meshes == 0 {
            eprintln!(
                "  (Starfield: 0 meshes — external geometry archive(s) unavailable or \
                 unresolved; environment gap, not a regression. See #1362.)"
            );
        }
        if !stats.structurally_inconsistent.is_empty() {
            hard_failures.push((label.to_string(), stats.structurally_inconsistent.clone()));
        }
    }
    eprintln!();

    if probed == 0 {
        eprintln!("  (no game data resolved; harness ran no games — install at least one)");
        return; // Treat as skip rather than failure.
    }

    // Structural consistency is the HARD assertion in v1. The per-game
    // fill-rate percentages are diagnostic — drift surfaces in the
    // printed table for human triage. Per-game fill-rate floor assertions
    // (#1320 TH6-NEW-02) catch silent regressions in per-game translation
    // completeness (e.g., FNV losing metalness override or tangent synthesis).
    if !hard_failures.is_empty() {
        eprintln!("\n=== STRUCTURAL INCONSISTENCY (HARD FAILURE) ===");
        for (game, examples) in &hard_failures {
            eprintln!("[{game}] {} mismatched buffer lengths:", examples.len());
            for e in examples {
                eprintln!("  {e}");
            }
        }
        panic!(
            "structural-consistency invariant violated in {} game(s); see output above",
            hard_failures.len(),
        );
    }

    // Per-game fill-rate floor assertions. The thresholds are conservative
    // baselines: old games (pre-FO4) have lower fill rates due to limited
    // native metadata; newer games (FO4+) have higher due to BGSM support.
    // Thresholds are tracked in each closure; drift beyond them signals a
    // translation regression.
    //
    // Re-measured 2026-08-06 (#2307 / NIFAL-D9-03) against the #2213
    // stratified sampler (round-robin across content-class buckets instead
    // of a flat alphabetical prefix) — every floor below that predates
    // 7dacef90 was measured on the OLD confounded sample (Skyrim/Oblivion
    // 100% from a single content bucket) and has been re-derived here.
    // Floors keep a margin below the re-measured value (roughly 10-15pp,
    // more where a metric is inherently low/noisy) rather than pinning to
    // it exactly, so ordinary content-mix drift doesn't make the gate
    // flaky; that's intentionally less than a byte-exact pin.
    //
    // #2707 deliberately stopped fabricating metalness/roughness overrides
    // when a source material has no PBR classifier signal. `metO`/`rghO` are
    // therefore content-dependent completeness signals again. Their floors
    // below are per-game post-#2707 measurements with regression margin, not
    // the stale 100%-by-construction invariant this harness used to assert.
    eprintln!("\n=== PER-GAME FILL-RATE FLOORS (#1320 / #2307) ===");
    type FillRateAssertion = (&'static str, Box<dyn Fn(&MaterialStats, &str)>);
    let mut fill_assertions: Vec<FillRateAssertion> = vec![
        (
            "Oblivion",
            Box::new(|s, label| {
                // Oblivion has no native BGSM; minimal metadata in NIF properties.
                // Measured 2026-08-06 (stratified): tex 91.7%, tan 84.2%.
                assert!(
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes) >= 80.0,
                    "[{label}] texture_path fill < 80% (got {:.1}%)",
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_tangents, s.imported_meshes) >= 70.0,
                    "[{label}] tangents fill < 70% (got {:.1}%)",
                    MaterialStats::pct(s.with_tangents, s.imported_meshes)
                );
                assert_pbr_override_fill(s, label, 90.0);
                // No normal_map floor: measured 0.0% across the stratified
                // (multi-content-class) sample, not just the pre-#2213
                // architecture-only one. `apply_texturing_property`
                // (legacy_properties.rs) does extract Oblivion's
                // `bump_texture` fallback when authored — this corpus (and,
                // per #2213's own investigation, Oblivion content
                // generally) essentially never authors it. Asserting `>=
                // 0.0` here would guard nothing; a positive floor would be
                // wrong.
            }),
        ),
        (
            "FO3",
            Box::new(|s, label| {
                // FO3 similar to Oblivion; improved with some material metadata.
                // Measured 2026-08-06 (stratified): tex 92.6%, tan 99.1%, nrm 79.2%.
                assert!(
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes) >= 80.0,
                    "[{label}] texture_path fill < 80% (got {:.1}%)",
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_tangents, s.imported_meshes) >= 85.0,
                    "[{label}] tangents fill < 85% (got {:.1}%)",
                    MaterialStats::pct(s.with_tangents, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_normal_map, s.imported_meshes) >= 55.0,
                    "[{label}] normal_map fill < 55% (got {:.1}%)",
                    MaterialStats::pct(s.with_normal_map, s.imported_meshes)
                );
                assert_pbr_override_fill(s, label, 80.0);
            }),
        ),
        (
            "FNV",
            Box::new(|s, label| {
                // FNV uses BSShaderPPLightingProperty, which never sets
                // `material_kind` (only the Skyrim+ BSLightingShaderProperty arm
                // does — see import/material/walker.rs). So FNV only classifies
                // its engine-synthesized effect/nolighting meshes. Measured
                // 2026-08-06 (stratified): texture_path 95.3%, material_kind
                // 17.6%, tangents 99.3%, normal_map 78.9% (#1512 recalibration —
                // the old 35% material_kind floor was an era-assumption never
                // achievable on this corpus).
                assert!(
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes) >= 82.0,
                    "[{label}] texture_path fill < 82% (got {:.1}%)",
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_material_kind, s.imported_meshes) >= 10.0,
                    "[{label}] material_kind fill < 10% (got {:.1}%)",
                    MaterialStats::pct(s.with_material_kind, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_tangents, s.imported_meshes) >= 85.0,
                    "[{label}] tangents fill < 85% (got {:.1}%)",
                    MaterialStats::pct(s.with_tangents, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_normal_map, s.imported_meshes) >= 55.0,
                    "[{label}] normal_map fill < 55% (got {:.1}%)",
                    MaterialStats::pct(s.with_normal_map, s.imported_meshes)
                );
                assert_pbr_override_fill(s, label, 82.0);
            }),
        ),
        (
            "SkyrimSE",
            Box::new(|s, label| {
                // SkyrimSE materials are INLINE BSLightingShaderProperty, not
                // external BGSM (BGSM is FO4+) — so `material_path` is ~0% and the
                // identity slot is `material_kind` (set from shader_type on the
                // BSLightingShaderProperty arm). Measured 2026-08-06 (stratified):
                // texture_path 93.8%, material_kind 35.5%, tangents 94.8%,
                // normal_map 76.1%. material_kind's old 60.8% figure (#1512) was
                // measured on the pre-#2213 confounded sample (100% from
                // `meshes\actors\`); the stratified, cross-content-class number
                // is lower — the floor already sits within 1pp of it, so it's
                // left as-is rather than tightened further.
                assert!(
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes) >= 82.0,
                    "[{label}] texture_path fill < 82% (got {:.1}%)",
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_material_kind, s.imported_meshes) >= 35.0,
                    "[{label}] material_kind fill < 35% (got {:.1}%)",
                    MaterialStats::pct(s.with_material_kind, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_tangents, s.imported_meshes) >= 80.0,
                    "[{label}] tangents fill < 80% (got {:.1}%)",
                    MaterialStats::pct(s.with_tangents, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_normal_map, s.imported_meshes) >= 50.0,
                    "[{label}] normal_map fill < 50% (got {:.1}%)",
                    MaterialStats::pct(s.with_normal_map, s.imported_meshes)
                );
                assert_pbr_override_fill(s, label, 80.0);
            }),
        ),
        (
            "FO4",
            Box::new(|s, label| {
                // FO4 has full BGSM + modern material system. Measured
                // 2026-08-06 (stratified): tex 92.7%, mat_path 57.9%, tan 96.3%,
                // nrm 82.9%.
                assert!(
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes) >= 82.0,
                    "[{label}] texture_path fill < 82% (got {:.1}%)",
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_material_path, s.imported_meshes) >= 48.0,
                    "[{label}] material_path fill < 48% (got {:.1}%)",
                    MaterialStats::pct(s.with_material_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_tangents, s.imported_meshes) >= 84.0,
                    "[{label}] tangents fill < 84% (got {:.1}%)",
                    MaterialStats::pct(s.with_tangents, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_normal_map, s.imported_meshes) >= 55.0,
                    "[{label}] normal_map fill < 55% (got {:.1}%)",
                    MaterialStats::pct(s.with_normal_map, s.imported_meshes)
                );
                assert_pbr_override_fill(s, label, 85.0);
            }),
        ),
        (
            "FO76",
            Box::new(|s, label| {
                // FO76 fully migrated texture references into BGSM — inline
                // texture_path is nearly empty; the material identity lives in
                // material_path. Measured 2026-08-06 (stratified): texture_path
                // 12.6%, material_path 81.3%, tangents 96.6%, normal_map 9.9%
                // (#1512 — the old texture_path>=75% floor assumed FO4-style
                // inline paths, which FO76 dropped; assert the slot that
                // actually carries the data). normal_map's floor stays low and
                // proportionate to its measured value rather than matching the
                // other games' ~55% — FO76's own texture identity has mostly
                // moved to material_path/BGSM by the same shift, so the raw
                // `Imported*` tier's inline normal_map slot is a minority case
                // here, not a translation gap (see #2214 — this harness measures
                // the pre-BGSM-merge tier).
                assert!(
                    MaterialStats::pct(s.with_material_path, s.imported_meshes) >= 78.0,
                    "[{label}] material_path fill < 78% (got {:.1}%)",
                    MaterialStats::pct(s.with_material_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_tangents, s.imported_meshes) >= 84.0,
                    "[{label}] tangents fill < 84% (got {:.1}%)",
                    MaterialStats::pct(s.with_tangents, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_normal_map, s.imported_meshes) >= 3.0,
                    "[{label}] normal_map fill < 3% (got {:.1}%)",
                    MaterialStats::pct(s.with_normal_map, s.imported_meshes)
                );
                assert_pbr_override_fill(s, label, 8.0);
            }),
        ),
        (
            "Starfield",
            Box::new(|s, label| {
                // Starfield BSGeometry carries NO inline texture path at all —
                // material lives entirely in material_path (CDB-resolved, and
                // even then only the reference string: this harness measures the
                // raw `Imported*` tier, before the CDB/BGSM merge — #2214).
                // Measured 2026-08-06 (stratified): texture_path 0.0%,
                // material_path 94.9%, tangents 100.0%, normal_map 0.0% (#1512 —
                // the old texture_path>=75% floor was canonically impossible for
                // BSGeometry; assert material_path + tangents, the slots that
                // prove the external-.mesh resolver path is intact).
                assert!(
                    MaterialStats::pct(s.with_material_path, s.imported_meshes) >= 85.0,
                    "[{label}] material_path fill < 85% (got {:.1}%)",
                    MaterialStats::pct(s.with_material_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_tangents, s.imported_meshes) >= 88.0,
                    "[{label}] tangents fill < 88% (got {:.1}%)",
                    MaterialStats::pct(s.with_tangents, s.imported_meshes)
                );
                assert_pbr_override_fill(s, label, 1.0);
                // No normal_map floor: BSGeometry resolves textures entirely
                // through the CDB material file, which this raw-tier harness
                // never merges in (#2214) — 0.0% here is structural, not a gap.
            }),
        ),
    ];

    // Re-run the games one more time, this time asserting fill-rate floors.
    eprintln!("Checking per-game fill-rate floors...");
    for &(label, game) in HARNESS_GAMES {
        let Some(archive) = open_mesh_archive(game) else {
            continue;
        };
        let sf_resolver = (game == Game::Starfield).then(starfield_resolver).flatten();
        let stats = collect_stats(
            &archive,
            sf_resolver.as_ref().map(|r| r as &dyn MeshResolver),
        );
        if let Some(assertion) = fill_assertions.iter_mut().find(|(l, _)| l == &label) {
            (assertion.1)(&stats, label);
            eprintln!("  [{label}] all fill-rate floors passed");
        }
    }
}

/// Guards #1362: the cross-game completeness signal must enumerate *every*
/// supported game. A `Game` variant absent from [`HARNESS_GAMES`] would make
/// the regression signal blind to that game (exactly how FO76 + Starfield
/// were silently omitted). Runs in CI without game data — pure metadata.
#[test]
fn harness_enumerates_every_supported_game() {
    for game in Game::ALL {
        assert!(
            HARNESS_GAMES.iter().any(|&(_, g)| g == game),
            "Game::{game:?} is supported but missing from HARNESS_GAMES — the \
             translation-completeness regression signal would be blind to it (#1362)"
        );
    }
}

/// Regression for #2213 (NIFAL-D9-01): a flat `sort(); truncate(limit)`
/// collapses the whole sample onto whichever top-level content directory
/// sorts first. Three directories, none of which sort first alphabetically
/// relative to each other in a way that would let a flat truncate cover all
/// three within a small limit, must each contribute at least one file.
#[test]
fn stratified_sample_spans_multiple_top_level_directories() {
    let files: Vec<String> = [
        "meshes\\actors\\deathclaw\\deathclaw.nif",
        "meshes\\actors\\dog\\dog.nif",
        "meshes\\architecture\\prospectorsaloon\\door01.nif",
        "meshes\\architecture\\prospectorsaloon\\wall01.nif",
        "meshes\\clutter\\bottle01.nif",
        "meshes\\clutter\\can01.nif",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let sampled = stratified_sample(files, 3);
    assert_eq!(sampled.len(), 3);

    let buckets: std::collections::BTreeSet<String> =
        sampled.iter().map(|p| stratification_bucket(p)).collect();
    assert_eq!(
        buckets,
        std::collections::BTreeSet::from([
            "actors".to_string(),
            "architecture".to_string(),
            "clutter".to_string(),
        ]),
        "a 3-file sample over 3 directories must draw one from each, not \
         collapse onto the alphabetically-first directory"
    );
}

/// The pre-fix behaviour this issue reports: a flat sorted truncate over
/// the same corpus would have picked all 3 files from `actors` (the
/// alphabetically-first directory) and none from `architecture`/`clutter`.
/// Documents the exact confound the stratified sampler fixes.
#[test]
fn flat_truncate_would_have_collapsed_to_one_directory() {
    let mut files: Vec<String> = [
        "meshes\\actors\\deathclaw\\deathclaw.nif",
        "meshes\\actors\\dog\\dog.nif",
        "meshes\\actors\\radscorpion\\radscorpion.nif",
        "meshes\\architecture\\prospectorsaloon\\door01.nif",
        "meshes\\clutter\\bottle01.nif",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    files.sort();
    files.truncate(3);

    let buckets: std::collections::BTreeSet<String> =
        files.iter().map(|p| stratification_bucket(p)).collect();
    assert_eq!(
        buckets,
        std::collections::BTreeSet::from(["actors".to_string()]),
        "a flat sort+truncate is expected to collapse onto one directory — \
         this is the confound #2213's stratified_sample fixes"
    );
}

/// Determinism: the same input must always yield the same sample (matches
/// the #1279 guarantee the pre-existing flat sort already provided).
#[test]
fn stratified_sample_is_deterministic() {
    let mut files: Vec<String> = [
        "meshes\\actors\\deathclaw\\deathclaw.nif",
        "meshes\\actors\\dog\\dog.nif",
        "meshes\\architecture\\prospectorsaloon\\door01.nif",
        "meshes\\clutter\\bottle01.nif",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    files.sort();

    let a = stratified_sample(files.clone(), 3);
    let b = stratified_sample(files, 3);
    assert_eq!(a, b);
}

/// A sample under the limit passes through unchanged (no spurious
/// reordering when stratification isn't even needed).
#[test]
fn stratified_sample_under_limit_returns_all_files_unchanged() {
    let files: Vec<String> = [
        "meshes\\actors\\deathclaw\\deathclaw.nif",
        "meshes\\clutter\\bottle01.nif",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let sampled = stratified_sample(files.clone(), 200);
    assert_eq!(sampled, files);
}

/// `meshes\` itself must never be the bucket key — nearly every path shares
/// it, so bucketing on it would defeat stratification entirely.
#[test]
fn stratification_bucket_strips_shared_meshes_root() {
    assert_eq!(
        stratification_bucket("meshes\\actors\\dog\\dog.nif"),
        "actors"
    );
    assert_eq!(
        stratification_bucket("meshes/architecture/door01.nif"),
        "architecture"
    );
    // A path not rooted under `meshes\` falls back to its own first
    // component instead of being merged into an unrelated bucket.
    assert_eq!(
        stratification_bucket("textures\\actors\\dog.dds"),
        "textures"
    );
}
