# REN-D11-2026-09-21-03: `record_exposure_meter_pass` lacks the `render_debug_requires_raw_output` gate that bloom, TAA and the upscale carry — auto-exposure adapts to debug imagery

**Labels**: low, renderer, bug

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: LOW (opt-in auto mode; debug-view side effect) · **Dimension**: FSR/Presentation
**Location**: `crates/renderer/src/vulkan/context/post_passes.rs` `record_exposure_meter_pass` (~:1030). The gated siblings are `record_taa_pass` (~:864), `record_bloom_pass` (~:1110) and `record_upscale_pass` (~:1199, ~:1245).
**Status**: NEW (introduced by `d54382415`, the Stage-1 wiring)
**Verified against**: HEAD `f97775ca8`

## Description

Every other post pass after composite checks `render_debug_requires_raw_output(flags, mode)` and skips its look transform when a raw correctness view is active. That covers TAA, bloom, the upscaler (which is forced to the native path) and presentation (`presentation.frag` returns early, before exposure). `record_exposure_meter_pass` has no such gate. It returns early only when the meter failed or is missing.

In auto mode the meter therefore meters the debug image (false-colour views, raw AO, facing ratio, …) and adapts the persistent per-FIF exposure toward it. Nothing is visible while the view is up, because presentation bypasses exposure for raw views. When the user leaves the view, though, the frame starts from a debug-driven exposure and visibly re-adapts, at the 2τ rate of REN-D11-2026-09-21-02 (#4590).

## Evidence

- The `record_exposure_meter_pass` body is `if self.exposure_meter_failed { return; }` followed by `meter.dispatch(...)`. It never checks `render_debug_requires_raw_output`.
- `record_taa_pass`, `record_bloom_pass` and `record_upscale_pass` each call `crate::shader_constants::render_debug_requires_raw_output(...)`.

## Impact

With `--auto-exposure`, turning off any raw debug view produces an exposure "pop", then re-adaptation from the wrong starting value. Debugging sessions that alternate views see exposure drift unrelated to the scene. Fixed mode, the default, is unaffected: the meter just writes the fixed value.

## Related

- #4513 (closed): the same missing-debug-gate class (Halton jitter applied under a raw-output view).
- REN-D11-2026-09-21-02 (#4590): the adaptation rate of the same pass.

## Suggested Fix

In `record_exposure_meter_pass`, skip the dispatch when auto mode is on and `render_debug_requires_raw_output(flags, mode)` is true. The slot then holds its last scene-driven value. Fixed mode can keep writing the constant. Extend the post-pass debug-gate source-shape pins to cover the meter.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D11-2026-09-21-03)

## Completeness Checks
- [ ] **SIBLING**: every `record_*_pass` in `record_post_passes` audited for the raw-output gate (TAA, bloom, upscale, presentation, meter)
- [ ] **TESTS**: a source-shape pin that `record_exposure_meter_pass` consults `render_debug_requires_raw_output`
