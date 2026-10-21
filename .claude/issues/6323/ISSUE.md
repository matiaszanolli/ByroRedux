# #6323: LC-2026-10-21-D1-03: Prose and test docs still name ZYX / mode 1 as the shipping REFR convention

**Labels**: low,documentation,legacy-compat,doc-rot

**Source**: docs/audits/AUDIT_LEGACY_COMPAT_2026-10-21.md · **Severity**: LOW · **Dimension**: 1 — REFR Euler (doc rot after `2a8b8e505`) · **Location**: `byroredux/src/cell_loader/transition.rs:410` and `:706`: "the shipping default (`--rotation-mode 1` ≡ the plain helper's formula)".; `byroredux/src/cell_loader/references/trigger_volume_tests.rs:80`: "same shipping mode (1 = CW+ZYX)".; `byroredux/src/ragdoll_pose_tests.rs:46-53`: "this convention is not the REFR `ZYX` product". The test compares XRGD against mode 1, which is no longer the REFR convention; XRGD now *equals* the REFR ship.; `docs/engine/geometry-defect-triage-workflow.md:146-150`: "The 2026-05-26 ZYX-OpenMW fix … was the canonical mode flip for FNV — pre-fix content rendered with mode 0 would show this class". This is inverted.; `crates/plugin/tests/parse_real_esm.rs:3698`: "post the 2026-05-26 fix".; `crates/plugin/examples/cell_rot_sweep.rs:1-6`: "post-mode-1 … shipped ZYX-vs-XYZ flip".

- **Severity**: LOW
- **Dimension**: 1 — REFR Euler (doc rot after `2a8b8e505`)
- **Location**:
  - `byroredux/src/cell_loader/transition.rs:410` and `:706`: "the shipping default (`--rotation-mode 1` ≡ the plain helper's formula)".
  - `byroredux/src/cell_loader/references/trigger_volume_tests.rs:80`: "same shipping mode (1 = CW+ZYX)".
  - `byroredux/src/ragdoll_pose_tests.rs:46-53`: "this convention is not the REFR `ZYX` product". The test compares XRGD against mode 1, which is no longer the REFR convention; XRGD now *equals* the REFR ship.
  - `docs/engine/geometry-defect-triage-workflow.md:146-150`: "The 2026-05-26 ZYX-OpenMW fix … was the canonical mode flip for FNV — pre-fix content rendered with mode 0 would show this class". This is inverted.
  - `crates/plugin/tests/parse_real_esm.rs:3698`: "post the 2026-05-26 fix".
  - `crates/plugin/examples/cell_rot_sweep.rs:1-6`: "post-mode-1 … shipped ZYX-vs-XYZ flip".
- **Status**: NEW
- **Description**: `2a8b8e505` updated `coord.rs`, `euler.rs`, `boot/`, `coordinate-system.md` and the CLI docs, but not these sites. The functional assertions are still true: the dispatcher test is a source scan, and the trigger-volume test uses a Z-only rotation, on which modes 0 and 1 agree. Only the prose is wrong.
- **Impact**: The triage workflow tells an operator that mode-0 output is the known-bad class. That is exactly backwards for the next "walls rotated" report, and it is the doc most likely to be read in that situation.
- **Related**: D1-02; memory note *gamebryo_cw_rotation*.
- **Suggested Fix**: Re-word the six sites to name mode 0 / XYZ as the ship. Rename the ragdoll-pose test (for example, `rotation_is_the_gamebryo_xyz_product_not_zyx`) and say in its doc that XRGD and REFR now share the convention.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)

_Source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
