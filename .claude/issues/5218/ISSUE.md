# #5218 — REN-D11-2026-10-03-02: `exposure_meter.comp` falls outside both the single-source constant net and the mirror pins, so the #5158 clamp-then-compensate order is guarded only on the Rust side

**Labels**: low,renderer,shaders,test-gap,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: FSR/Presentation
- **Location**:
  - `exposure_meter.comp` `main`, in the single-thread epilogue.
  - The `EXPOSURE_CONSTANT` doc in `crates/renderer/src/vulkan/exposure.rs`.
  - The `auto_exposure` mirror and its test `envelope_caps_dark_scene_lift_and_compensation_biases_around_it`.
  - The source pins in `crates/renderer/src/vulkan/exposure_meter.rs` `mod tests`.
- **Status**: NEW.
- **Description**:
  - **Claim (a)**: "metering shader, chroma compress and this module cannot disagree".
    - The `EXPOSURE_CONSTANT` doc says it "Resolves to the shared `EXPOSURE_METER_NEUTRAL` (#5154) so the metering shader, the presentation chroma compress and this module cannot disagree". The #5154 commit message makes the same single-source claim.
    - `exposure_meter.comp` has no `#include "include/shader_constants.glsl"`. It hand-types `1.2 * exp2(-ev100)`, `* 8.0` (S/K) and the Rec.709 luma weights `vec3(0.2126, 0.7152, 0.0722)`.
    - A change to `EXPOSURE_METER_NEUTRAL` would move presentation's neutral point and the host mirror, but not the meter.
  - **Claim (b)**: the shader's clamp-then-compensate order is pinned.
    - The core of 7d99ba7f0 is this ordering: clamp the *metered* target, then apply `exp2(-params.mode.z)`.
    - Only the Rust `auto_exposure` mirror is tested for it.
    - No pin in `exposure_meter.rs` covers the shader's order. Its source tests cover the workgroup size, the #4597 divisor and the sample policy only.
    - Reverting the shader alone would leave every guard green while `exposure ev` goes dead again in clamped scenes.
- **Evidence**:
  - `exposure_meter.comp` contains the line `float target = 1.2 * exp2(-ev100);`.
  - Within it, `target = clamp(target, params.limits.x, params.limits.y);` precedes `target *= exp2(-params.mode.z);`.
  - `git grep` finds no Rust test that references `params.limits` or `exp2(-params.mode.z)`.
- **Impact**: Today there is no behaviour change, because the values agree. This is a latent drift path on the default render path's metering, and a code comment states a guarantee the code does not provide.
- **Related**: #5154, #5158, #4597.
- **Suggested Fix**:
  - Include `shader_constants.glsl` in the meter and use `EXPOSURE_METER_NEUTRAL` and `LUMA_REC709`.
  - Add a source-order pin in `exposure_meter.rs`: clamp before the compensation multiply.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix
