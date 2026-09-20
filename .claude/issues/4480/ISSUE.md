# EXT-D1-2026-09-19-01: bin-crate test suite does not compile at HEAD — six CellData fixtures miss encounter_zone_form

- **ID**: EXT-D1-2026-09-19-01
- **Labels**: high,terrain-exterior,bug,test-gap
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4480

**Severity**: HIGH · **Dimension**: EXAL boundary (all-dimension verification gate) · **Game Affected**: all
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D1-2026-09-19-01; independently flagged by audit Dims 1–6)

**Location**: `byroredux/src/cell_loader/exterior.rs:562,737,990`, `cell_loader/lod_support.rs:346`, `cell_loader/lgtm_fallback_tests.rs:48`, `scene/world_setup.rs:1436` (all `#[cfg(test)]`); root cause `crates/plugin/src/esm/cell/mod.rs:265`

**Description**
`d574d9bd1` (2026-09-19, "feat(hud): add FO3/FNV per-game profiles…", which bundled the #4173 XEZN parse) added `CellData::encounter_zone_form` and updated production consumers but missed six test-fixture struct literals in the bin crate. `cargo test -p byroredux` (any filter, any toolchain) fails with 6 × E0063 — the exact command AGENTS.md and `docs/contributing.md` §Tests mandate before pushing any `byroredux/src/` change. `cargo check -p byroredux` (production) is clean, so only a test run reveals it. Reproducer caveat: piping cargo through `tail` masks the exit code.

**Evidence**
Verified at HEAD `7f8d71050`: 6 × `error[E0063]: missing field 'encounter_zone_form' in initializer of 'CellData'`. Patched-worktree probes ran green (terrain 72/0/2, water 128/0/1), so the underlying suites are healthy — the gate itself is dead.

**Impact**
Zero test feedback for the entire bin crate (~2,166 tests — every EXAL boundary, terrain, water, weather, LOD guard) until fixed; a silent-regression window has been open since 2026-09-19.

**Related**: #4173 (the XEZN fix this rides on)

**Suggested Fix**
Add `encounter_zone_form: None` to the six fixtures (mechanical, probe-verified), or convert them to a `..Default::default()`-style fixture helper so the next field addition cannot break six sites at once. Run the 1.96-toolchain test target in CI so this class fails the gate rather than an audit.

## Completeness Checks
- [ ] **SIBLING**: Grep for other exhaustive struct literals of recently-grown types in test modules
- [ ] **TESTS**: The full bin suite compiles and runs green on the 1.96 toolchain; CI gate added
