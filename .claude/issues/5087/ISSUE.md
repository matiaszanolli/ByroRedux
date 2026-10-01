# TD1-2026-09-29-01: `context/draw.rs` regrew to 2211 production LOC; the #4767 budget pins only `draw_frame` (regression of #4767)

**Labels**: low,renderer,tech-debt,bug

**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-09-29.md`

**Regression of #4767** (#4767 is CLOSED; filed as a new issue rather than reopening it).

- **Severity**: LOW · **Dimension**: 1 · **Status**: Regression of #4767 · **Effort**: small
- **Location**: `crates/renderer/src/vulkan/context/draw.rs:29-2033` (helpers), `:2034` (`draw_frame`,
  714 lines), `draw_frame_stays_within_its_line_budget` (`:4098`, cap 720)
- **Age**:
  - `e17b4b653` (Fix #4767, 09-23) cut the file to 1982.
  - `0572bfd5a` took it back to 2051 the same day.
  - `88c23887b` (+106) and the 09-28 fix batches brought it to 2211.
- **Description**: the growth landed in the pure helper functions above `draw_frame`, which the
  function-scoped budget cannot see: `build_composite_params` (~260 lines), `build_sky_cube_params`,
  DoF/FSR parameter builders, camera deltas and jitter. `draw_frame` itself sits at 714 of 720.
- **Suggested Fix**:
  - Move lines 29–2033 (pure fns, with their test mods) to `context/frame_params.rs`.
  - Repoint the `include_str!("draw.rs")` scans in `bloom.rs`, `sync.rs`, `sky_dome.rs`,
    `post_passes.rs`, `context/{mod,resources,build_and_upload_instances}.rs`, `skin_compute.rs` and the
    draw.rs self-scans.
  - Add a file-level prod_loc pin beside the function budget.

**Validated at HEAD 9fcfdc3fc**: `prod_loc crates/renderer/src/vulkan/context/draw.rs` = 2211 (prod_loc self-test ok); `draw_frame` starts at :2034 and is 714 lines; `draw_frame_stays_within_its_line_budget` (:4098) is the only budget pin.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---

## Solution

**Fixed**: lines 29–2065 (pure helpers, interleaved test mods, FrameInputs)
plus the post-impl free fns (rebase_model_matrix,
origin_corrected_prev_view_proj) and their test mods moved to
context/frame_params.rs (1342 prod); draw.rs keeps the impl + orchestration
(815 prod). New file-level budget test beside the function budget:
draw.rs ≤ 900, frame_params.rs ≤ 1500 production lines (strip-every-test-mod
counter, reviewed into UNWRAPPED_SELF_INCLUDES at +1). Every scanner whose
needle moved repointed: context/mod.rs FrameInputs.pose_dirty pin,
material-kind-11 + layer-discriminant contract scans, sky_dome
weather_wind packing, shader_discriminants fog shapes; helpers.rs and
bloom.rs negative scans now cover both halves. Commit: `Fix #5087` (c57e5cc4a).

## Verification

- cargo test -p byroredux-renderer: 1323/1323 green
- bin crate: 2576/2576 green (FrameInputs re-export preserved —
  context::FrameInputs path unchanged for app_frame.rs)
