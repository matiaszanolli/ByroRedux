# REN-D11-2026-09-21-02: auto-exposure adapts each per-FIF slot from its own two-frame-old value with a one-frame alpha — the effective time constant is 2τ, not the documented τ

**Labels**: low, renderer, bug

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: LOW (opt-in `--auto-exposure`; adaptation speed off by 2×) · **Dimension**: FSR/Presentation
**Location**:
- `crates/renderer/shaders/exposure_meter.comp`: the header doc (~:14-21) and `imageLoad(dstExposure)` + `mix` (~:92-94).
- Host alpha: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (~:1070-1073), `adaptation_alpha(frame_delta_seconds, self.exposure_adaptation_seconds)`.
- `crates/renderer/src/vulkan/exposure.rs` `adaptation_alpha`.

**Status**: NEW (introduced by `c5663fe39` / `d54382415`)
**Verified against**: HEAD `f97775ca8`

## Description

In auto mode the meter blends `adapted = mix(previous, target, alpha)`. Here `previous = imageLoad(dstExposure)` is this frame-in-flight slot's own texel. With `MAX_FRAMES_IN_FLIGHT = 2`, that texel was written two frames earlier. The host computes `alpha = 1 − exp(−dt/τ)` from a single frame's `dt`, where τ = `exposure_adaptation_seconds` (default 0.2 s).

So the two slots run as two independent adaptation chains. Each advances by a one-frame step once every two frames. The effective time constant is 2τ (0.2 s documented, ~0.4 s actual). Each chain updates at half the frame rate, and presented frames alternate between the two chains.

The shader header says "adaptation simply lags MAX_FRAMES_IN_FLIGHT frames, which is negligible". That is wrong: this is a rate change, not a lag.

## Evidence

- `exposure_meter.comp`: `float previous = imageLoad(dstExposure, ivec2(0)).r; float adapted = mix(previous, target, clamp(params.mode.w, 0.0, 1.0));`
- Host: `mode.w = adaptation_alpha(frame_delta_seconds, self.exposure_adaptation_seconds)`, with `exposure_adaptation_seconds: 0.2` (`context/init.rs`) and `adaptation_seconds: 0.2` (`byroredux/src/components.rs`).
- `crates/renderer/src/vulkan/sync.rs`: `MAX_FRAMES_IN_FLIGHT: usize = 2`.

## Impact

With `--auto-exposure`, eye adaptation runs at half the tuned and documented speed. Because presented frames alternate between two chains, transitions show a subtle frame-to-frame exposure alternation. Fixed mode, the default, is unaffected.

## Related

- SAFE-D7-2026-09-21-01 (`docs/audits/AUDIT_SAFETY_2026-09-21.md`): the meter's averaging-divisor bug, a separate defect. That report defers this adaptation-history issue to this finding.
- `docs/audits/AUDIT_CONCURRENCY_2026-09-21.md` cites this as a cross-frame indexing issue whose barriers trace clean.
- REN-D11-2026-09-21-03 (#4591): the same pass lacks the raw-debug gate.

## Suggested Fix

Pick one of two fixes:
- **Host-side alpha.** Compute alpha over the slot's real interval: `adaptation_alpha(dt_since_this_slot_was_written, τ)`, about `MAX_FRAMES_IN_FLIGHT × dt`.
- **Shader-side read.** Read the other slot's most recent exposure (the previous frame's) as `previous`. This adds a cross-FIF read and the barrier it requires.

Either way, correct the shader header. Add a test that simulates N frames and checks that the effective time constant equals τ.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D11-2026-09-21-02)

## Completeness Checks
- [ ] **SIBLING**: FSR's per-FIF exposure-texel consumption re-checked under the chosen fix (FSR and presentation read the same slot)
- [ ] **TESTS**: a host-side simulation pinning the effective time constant to `exposure_adaptation_seconds`
