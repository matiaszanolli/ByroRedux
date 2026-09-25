# REN-D7-2026-09-24-03: the raw-output-gate assertions for TAA and the exposure meter in `taa_resolves_the_post_bloom_scene_tap` cannot fail

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D7-2026-09-24-03**._

_MERGED: D7-03 = D11-03 = D12-01_

- **Severity**: LOW (test gap; the code is correct today).
- **Dimension**: TAA / Debug-Telemetry
- **Location**: `crates/renderer/src/vulkan/context/post_passes.rs` — `mod tests::taa_resolves_the_post_bloom_scene_tap` (`taa_body = &src[taa_fn_start..]`, `meter_body = &src[meter_fn_start..]` slice to end of production code).
- **Status**: NEW (follow-up to closed #4591 / #3572). The Dim 4 auditor's guard table lists this test as asserting the gate on both passes; it does not.
- **Description / Evidence**: Functions after each slice start (`record_bloom_pass`, `record_upscale_pass`) also call `render_debug_requires_raw_output(`, so the assertion is satisfied by other functions' gates. Three auditors simulated the deletion of each function's own gate on a copy of the text; `contains` stays true. Only `record_bloom_pass_skips_raw_correctness_views_before_dispatch` is properly bounded (`fn_end` at `record_composite_pass`, `gate < dispatch && return;`).
- **Impact**: Dropping the gate from `record_taa_pass` re-smooths raw correctness views with TAA history; dropping it from the meter lets auto-exposure adapt to debug imagery and pop on dismissal. Both are `--upscaler taa`, auto-exposure and developer paths.
- **Suggested Fix**: Bound each slice at the next `fn` (as the bloom test does) and assert the gate precedes the dispatch call and is followed by `return;`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

