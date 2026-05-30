# #1362 — D7-A: translation-completeness harness blind to Starfield, omits FO76

_Snapshot from AUDIT_NIFAL_2026-05-30. GitHub is authoritative for live state._

**Severity**: MEDIUM · **Source**: AUDIT_NIFAL_2026-05-30 (D7-A) · cross-ref `/audit-nifal`

**Dimension**: Completeness · **Tier Violated**: completeness-signal (cross-cutting) · **Game Affected**: Starfield (critical), FO76 (omitted)

**Location**: `crates/nif/tests/translation_completeness.rs:199-205` (hard-coded 5-game `games` array: Oblivion/FO3/FNV/SkyrimSE/FO4)

**Description**: `cross_game_translation_completeness` — whose stated job is to be the cross-game coverage signal that catches unverified-game translation leaks — omits FO76 and Starfield, the two games most likely to harbor one. `common::Game` + `open_mesh_archive` fully support both and the data dirs are installed. A throwaway probe through the identical `collect_stats` logic measured FO76 = 293 meshes / healthy fill (pure omission, would pass) and **Starfield = 0 imported meshes from 200 parsed NIFs**. Starfield's 0 is real: its geometry is external `.mesh` companion files (#1292) and the harness calls the no-resolver `import_nif`, so `extract_bs_geometry` (`crates/nif/src/import/mesh/bs_geometry.rs:54`, `let resolver = resolver?;`) returns `None` for every external LOD.

**Impact**: A future break in the Starfield BSGeometry / `.mat` / SkinAttach translation path would be invisible to the one harness designed to catch it.

**Suggested Fix**: (1) add FO76 + Starfield to the `games` array (FO76 is free); (2) for Starfield, run `import_nif_with_resolver` with a `MeshResolver` backed by the `Starfield - Meshes01.ba2` + `geometries` chain (the resolver impl already exists for the cell loader, SF-D4-02 Stage B) — OR add an explicit `inline-geometry-only` Starfield row + comment so the 0% isn't mistaken for a regression. Without one of these the Starfield row reads 0% forever.

**Related**: adjacent to #1320 (OPEN, `translation_completeness` *empty thresholds*) — distinct defect (that's "asserts nothing"; this is "omits two games"). Fix both in the same harness pass.

## Completeness Checks
- [ ] **SIBLING**: pairs with #1320 — wire real per-game fill-rate thresholds while adding the two games.
- [ ] **TESTS**: this finding IS about the test; the deliverable is the harness covering all 7 games with a working Starfield resolver path.
