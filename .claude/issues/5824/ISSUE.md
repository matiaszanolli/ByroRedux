# TD2-2026-10-15-23: The weak `#[cfg(test)]` source cut survives at 18 sites in 12 files; guard unchanged; second report without an issue (TD2-2026-10-12-06, carried)

Labels: low,test-gap,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD2-2026-10-15-23`).

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/debug-server/src/registration.rs:470,518,563,648`, `crates/physics/src/water.rs:1404`, `crates/renderer/src/vulkan/presentation.rs:917`, `svgf.rs:1716,2054`, `crates/nif/src/import/collision/mod.rs:789,833`, `crates/nif/src/import/walk/lights.rs:265,318`, `byroredux/src/material_translate.rs:1522`, `boot/world.rs:553`, `systems/cinematic.rs:2091`, `systems/combat_ai.rs:689`, `cell_loader/terrain_lod_btr.rs:616`, `cell_loader/load.rs:1065`; guard `no_source_scan_cuts_with_the_weak_cfg_test_needle` at `byroredux/src/workspace_hygiene_tests.rs:709`
- **Status**: Regression of #5469 (CLOSED); `git grep` unchanged since 10-12 (line numbers moved; four of the files were edited since)
- **Age**: `334458b3f` (#5469) landed the helper and guard; survivors predate it
- **Effort**: small
- **Description**: the guard's needles are `find(`, `rfind(`, `split(` and `split_once("\n<attr>")`; the survivors' spelling `.split_once("#[cfg(test)]")` (no leading newline) matches none, so the guard is green. The four `nif/import/{collision/mod,walk/lights}.rs` sites also fall back to the whole file via `.unwrap_or(src)`, the vacuity #4842 removed elsewhere. `crate::source_scan` is reachable from all of these (only `pex`/`facegen` lack the dependency and are allowlisted).
- **Evidence**: `grep -rnF '"#[cfg(test)]"' crates byroredux tools --include='*.rs'` minus the allowlist = 18 `.split_once(` lines.
- **Impact**: each is a guard that a test module in its own file can satisfy.
- **Related**: #5469, #5100, #4842, #4604.
- **Suggested Fix**: migrate the 18 sites to `production_text` / `production_text_or_whole` / `strip_test_modules` and add `split_once("{attr}")` (bare) to the guard needle set. If this was declined on purpose, record that in the issue so the 10-11 and 10-12 carry stops.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
