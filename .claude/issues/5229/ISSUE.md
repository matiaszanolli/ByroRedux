# #5229 — FO4-D4-01: FO4 TRNS rotation is radians but the LSCR stage converts it as degrees — 126/127 loading-screen models are mis-posed

https://github.com/matiaszanolli/ByroRedux/issues/5229

Source: `docs/audits/AUDIT_FO4_2026-10-03.md` (HEAD `32f4450d9`)

**Source**: `docs/audits/AUDIT_FO4_2026-10-03.md` (FO4-2026-10-03-D4-01)

- **Severity**: MEDIUM (visual artifact on a shipped feature; no crash)
- **Dimension**: ESM architecture records. The decode is e60911864; the consumer is the loading cover.
- **Location**:
  - `crates/plugin/src/esm/records/load_screen.rs:160-209` (`LoadScreenTransform::rotation_deg` and its doc, filled from `DATA` floats 3..6).
  - `byroredux/src/loading_screen.rs:480-497` (`stage_pose`: `rotation_deg[i].to_radians()`).
- **Status**: NEW. Searches for "TRNS rotation", "TRNS radians" and "loading screen rotation" find nothing. #5193 and #5206 are different, renderer-side defects in the same feature.
- **Description**:
  - The struct doc says TRNS `DATA` rotation is "degrees — same float-degree convention as FO4 REFR DATA".
  - FO4 REFR `DATA` is radians: `PlacedRef.rotation` is documented as radians (`cell/mod.rs:434`) and feeds `euler_zup_to_quat_yup_refr` raw.
  - `stage_pose` applies `to_radians()` to the TRNS value anyway, so a radian angle is read as degrees.
  - Skyrim's inline `RNAM` (i16 degrees) shares the call correctly. Only the FO4/SF TRNS branch is wrong.
- **Evidence**:
  - **Census over Fallout4.esm**: 949 TRNS, min 0.0, max 6.2831855 (f32 2π), 0 values above 2π.
  - **Census across the masters and 4 DLCs**: 1,259 TRNS, all components in [0, 2π]. Values sit on radian landmarks (3.142, 1.571, 0.524).
  - Restricted to the 127 TRNS that LSCR `TNAM` references: 126 have a non-zero rotation. Per-axis error: median 6.3°, p90 137°, max 176.9°. 202 of 381 axes are off by more than 5°.
  - The commit's own sample: `LoadingBarberTransform` rot [6.1, 0, 6.1] should be −10.5° and renders as +6.1°.
  - Why the tests miss it: `trns_transform_wins_over_inline_skyrim_defaults` feeds `[90.0, 0, 0]`, a degree value FO4 never ships, and asserts only translation and scale.
- **Impact**:
  - Every FO4 loading-screen model cover, and Starfield's through the same parse, is posed near identity instead of its authored orientation. Models that need a quarter- or half-turn appear side-on or backwards.
  - `docs/smoke-tests/p6-loading-model.sh` checks the census, presentation and a pixel floor, not orientation, so it stays green.
- **Related**: e60911864; #5193, #5206 (same feature, renderer side).
- **Suggested Fix**:
  - Rename the field to `rotation_rad` and fix its doc ("radians, wbPosRot, like REFR DATA").
  - In `stage_pose`, apply `to_radians()` only to the Skyrim `RNAM` branch.
  - Pin it with `[0, 0, π]` → a 180° yaw test.
  - Accept on the p6 smoke script plus a visual check of one FO4 cover with a large authored rotation.

## Completeness Checks
- [x] **TESTS**: a `[0, 0, π]` → 180° yaw pin on the FO4 TRNS branch; the Skyrim `RNAM` (i16 degrees) branch keeps `to_radians()`
- [x] **SIBLING**: Starfield shares the TRNS parse through `load_screen.rs` — its covers are posed from radians too
