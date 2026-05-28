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
use byroredux_nif::{import::ImportedMesh, parse_nif};
use common::{open_mesh_archive, Game, MeshArchive};

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
fn collect_stats(archive: &MeshArchive) -> MaterialStats {
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
        let Ok(bytes) = archive.extract(path) else { continue };
        let Ok(scene) = parse_nif(&bytes) else { continue };
        let mut pool = StringPool::new();
        let meshes = byroredux_nif::import::import_nif(&scene, &mut pool);
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
    let games = [
        ("Oblivion", Game::Oblivion),
        ("FO3", Game::Fallout3),
        ("FNV", Game::FalloutNV),
        ("SkyrimSE", Game::SkyrimSE),
        ("FO4", Game::Fallout4),
    ];

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
    for (label, game) in games {
        let Some(archive) = open_mesh_archive(game) else {
            eprintln!("  {label:<12} SKIP (no data)");
            continue;
        };
        probed += 1;
        let stats = collect_stats(&archive);
        stats.print_row(label);
        if !stats.structurally_inconsistent.is_empty() {
            hard_failures.push((label.to_string(), stats.structurally_inconsistent.clone()));
        }
    }
    eprintln!();

    if probed == 0 {
        eprintln!("  (no game data resolved; harness ran no games — install at least one)");
        return; // Treat as skip rather than failure.
    }

    // Structural consistency is the only HARD assertion in v1. The
    // per-game fill-rate percentages are diagnostic — drift surfaces
    // in the printed table for human triage, but the test passes as
    // long as no buffer-length mismatches landed. A future task can
    // tighten this into per-game min-fill-rate bands once we have a
    // baseline to compare against.
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
}
