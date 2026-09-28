# #4966: REN-D7-2026-09-27-04: Leaving a raw-output debug view resumes TAA and FSR against history stale by the length of the debug session

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4966
- **Labels**: low,renderer,bug

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-27.md` (full renderer audit, all 12 dimensions, audited `main` @ `7e9da5dcc`; code at the finding's location unchanged at `fe80f4d76`). Finding ID: **REN-D7-2026-09-27-04**._

- **Severity**: LOW (debug tooling only)
- **Dimension**: TAA
- **Location**: `crates/renderer/src/vulkan/context/render_debug.rs` `set_render_debug_mode` (only logs and assigns); `post_passes.rs` `record_taa_pass` (the raw-view early return leaves `history[f]` unwritten); FSR is skipped via `is_fsr_dispatch_active()`.
- **Status**: NEW. It is the unfixed half of closed #3632, whose fix (`1ce99197`) folded the raw-output predicate into `is_fsr_dispatch_active()` for **jitter**. The issue's second bullet ("the first frame back at `RENDER_DEBUG_FINAL` therefore dispatches with `reset: false` against reconstruction history that is stale", "`set_render_debug_mode` … does not call `signal_temporal_discontinuity`") still holds at HEAD.
- **Description**:
  - During a raw view, TAA does not dispatch, so its history slots freeze. The G-buffer mesh ID, normal and motion keep updating.
  - On return, `taa.comp` validates the stale `uPrevHistory` against **fresh** previous-frame mesh IDs and normals. Those describe a different frame, so the disocclusion test is meaningless for the first frame back.
  - FSR resumes with its last `reset_pending` value.
- **Evidence**: `render_debug.rs`: `if self.render_debug_mode != mode { log::info!(..); self.render_debug_mode = mode; }`. No raw-output transition tracking exists anywhere (grep for `was_raw` / `prev_raw` finds none).
- **Impact**: Roughly 10–20 frames of ghost from the pre-debug view when the camera moved during the debug session. It also contaminates any capture taken right after leaving a view.
- **Related**: #3632, #4513.
- **Suggested Fix**: In `set_render_debug_mode`, when `render_debug_requires_raw_output(flags, old)` differs from `render_debug_requires_raw_output(flags, new)`, call `self.signal_temporal_discontinuity(..)` (which already resets TAA, FSR and volumetrics).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
