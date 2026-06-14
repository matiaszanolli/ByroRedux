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
        if mesh.texture_path.is_some() {
            self.with_texture_path += 1;
        }
        if mesh.material_path.is_some() {
            self.with_material_path += 1;
        }
        if mesh.material_kind != 0 {
            self.with_material_kind += 1;
        }
        if mesh.metalness_override.is_some() {
            self.with_metalness_override += 1;
        }
        if mesh.roughness_override.is_some() {
            self.with_roughness_override += 1;
        }
        if mesh.normal_map.is_some() {
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

/// Walk the first `SAMPLE_LIMIT` NIFs in `archive`, parse + import each,
/// aggregate `MaterialStats`. Skips files that fail to extract or parse
/// (those are surfaced separately by parse_real_nifs.rs).
///
/// `resolver` supplies external `.mesh` companion geometry (Starfield
/// `BSGeometry`); pass `None` for games whose geometry is inline.
fn collect_stats(archive: &MeshArchive, resolver: Option<&dyn MeshResolver>) -> MaterialStats {
    let mut stats = MaterialStats::default();
    // Sort before sampling so the 200-NIF window is identical across
    // runs. `list_files()` returns items in archive-internal order,
    // which on BA2 happens to be HashMap-iteration order — without the
    // sort, two consecutive runs sample different 200 NIFs and the
    // fill-rate comparison becomes useless. Pinned by #1279 verification.
    let mut files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
        .collect();
    files.sort();
    files.truncate(SAMPLE_LIMIT);

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
#[ignore]
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
    eprintln!("\n=== PER-GAME FILL-RATE FLOORS (#1320) ===");
    let mut fill_assertions: Vec<(&str, Box<dyn Fn(&MaterialStats, &str)>)> = vec![
        (
            "Oblivion",
            Box::new(|s, label| {
                // Oblivion has no native BGSM; minimal metadata in NIF properties.
                assert!(
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes) >= 60.0,
                    "[{label}] texture_path fill < 60% (got {:.1}%)",
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_tangents, s.imported_meshes) >= 40.0,
                    "[{label}] tangents fill < 40% (got {:.1}%)",
                    MaterialStats::pct(s.with_tangents, s.imported_meshes)
                );
            }),
        ),
        (
            "FO3",
            Box::new(|s, label| {
                // FO3 similar to Oblivion; improved with some material metadata.
                assert!(
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes) >= 65.0,
                    "[{label}] texture_path fill < 65% (got {:.1}%)",
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_tangents, s.imported_meshes) >= 45.0,
                    "[{label}] tangents fill < 45% (got {:.1}%)",
                    MaterialStats::pct(s.with_tangents, s.imported_meshes)
                );
            }),
        ),
        (
            "FNV",
            Box::new(|s, label| {
                // FNV uses BSShaderPPLightingProperty, which never sets
                // `material_kind` (only the Skyrim+ BSLightingShaderProperty arm
                // does — see import/material/walker.rs). So FNV only classifies
                // its engine-synthesized effect/nolighting meshes. Measured
                // 2026-06-13: texture_path 95.1%, material_kind 8.1%, tangents
                // 97.3% (#1512 recalibration — the old 35% material_kind floor
                // was an era-assumption never achievable on this corpus).
                assert!(
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes) >= 70.0,
                    "[{label}] texture_path fill < 70% (got {:.1}%)",
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_material_kind, s.imported_meshes) >= 5.0,
                    "[{label}] material_kind fill < 5% (got {:.1}%)",
                    MaterialStats::pct(s.with_material_kind, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_tangents, s.imported_meshes) >= 50.0,
                    "[{label}] tangents fill < 50% (got {:.1}%)",
                    MaterialStats::pct(s.with_tangents, s.imported_meshes)
                );
            }),
        ),
        (
            "SkyrimSE",
            Box::new(|s, label| {
                // SkyrimSE materials are INLINE BSLightingShaderProperty, not
                // external BGSM (BGSM is FO4+) — so `material_path` is ~0% and the
                // identity slot is `material_kind` (set from shader_type on the
                // BSLightingShaderProperty arm). Measured 2026-06-13: texture_path
                // 100%, material_kind 60.8%, tangents 100% (#1512 — the old
                // material_path>=35% floor mis-described Skyrim as "native BGSM"
                // and could never pass on vanilla inline-material content).
                assert!(
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes) >= 75.0,
                    "[{label}] texture_path fill < 75% (got {:.1}%)",
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_material_kind, s.imported_meshes) >= 35.0,
                    "[{label}] material_kind fill < 35% (got {:.1}%)",
                    MaterialStats::pct(s.with_material_kind, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_tangents, s.imported_meshes) >= 60.0,
                    "[{label}] tangents fill < 60% (got {:.1}%)",
                    MaterialStats::pct(s.with_tangents, s.imported_meshes)
                );
            }),
        ),
        (
            "FO4",
            Box::new(|s, label| {
                // FO4 has full BGSM + modern material system.
                assert!(
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes) >= 75.0,
                    "[{label}] texture_path fill < 75% (got {:.1}%)",
                    MaterialStats::pct(s.with_texture_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_material_path, s.imported_meshes) >= 40.0,
                    "[{label}] material_path fill < 40% (got {:.1}%)",
                    MaterialStats::pct(s.with_material_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_tangents, s.imported_meshes) >= 65.0,
                    "[{label}] tangents fill < 65% (got {:.1}%)",
                    MaterialStats::pct(s.with_tangents, s.imported_meshes)
                );
            }),
        ),
        (
            "FO76",
            Box::new(|s, label| {
                // FO76 fully migrated texture references into BGSM — inline
                // texture_path is nearly empty (~10%); the material identity lives
                // in material_path. Measured 2026-06-13: texture_path 9.6%,
                // material_path 90.4%, tangents 100% (#1512 — the old
                // texture_path>=75% floor assumed FO4-style inline paths, which
                // FO76 dropped; assert the slot that actually carries the data).
                assert!(
                    MaterialStats::pct(s.with_material_path, s.imported_meshes) >= 75.0,
                    "[{label}] material_path fill < 75% (got {:.1}%)",
                    MaterialStats::pct(s.with_material_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_tangents, s.imported_meshes) >= 65.0,
                    "[{label}] tangents fill < 65% (got {:.1}%)",
                    MaterialStats::pct(s.with_tangents, s.imported_meshes)
                );
            }),
        ),
        (
            "Starfield",
            Box::new(|s, label| {
                // Starfield BSGeometry carries NO inline texture path at all —
                // material lives entirely in material_path (CDB-resolved). Measured
                // 2026-06-13: texture_path 0.0%, material_path 100%, tangents 100%
                // (#1512 — the old texture_path>=75% floor was canonically
                // impossible for BSGeometry; assert material_path + tangents, the
                // slots that prove the external-.mesh resolver path is intact).
                assert!(
                    MaterialStats::pct(s.with_material_path, s.imported_meshes) >= 75.0,
                    "[{label}] material_path fill < 75% (got {:.1}%)",
                    MaterialStats::pct(s.with_material_path, s.imported_meshes)
                );
                assert!(
                    MaterialStats::pct(s.with_tangents, s.imported_meshes) >= 65.0,
                    "[{label}] tangents fill < 65% (got {:.1}%)",
                    MaterialStats::pct(s.with_tangents, s.imported_meshes)
                );
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
