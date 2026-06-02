# Issues 1397 + 1400

## #1397 NCPS-03: TAA/bloom redundant HOST→COMPUTE UBO barriers
**Files:** `crates/renderer/src/vulkan/taa.rs:671`, `bloom.rs:481`, `context/draw.rs`
**Domain:** renderer
**Fix:**
- Add `upload_params` to TaaPipeline and BloomPipeline that write the UBO(s) host-side
- Call them in `draw.rs` BEFORE the pre-render-pass bulk barrier (mirrors SVGF fold from #961)
- Remove the per-dispatch HOST→COMPUTE barriers from `dispatch()` in both pipelines
- The bulk barrier at draw.rs:2010 already covers `COMPUTE_SHADER` as dst_stage, so no new
  coverage is needed — this is a pure fold of an existing execution dependency

## #1400 NCPS-05: TAA first-frame CPU-side test missing
**File:** `crates/renderer/src/vulkan/taa.rs`
**Domain:** renderer
**Fix:** Add `#[cfg(test)] mod tests` that asserts `TaaParams::params.y == 1.0` when
`frames_since_creation == 0` (i.e. `should_force_history_reset(0)` is true).
