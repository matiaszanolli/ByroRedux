# REN-DIM17-02: water caustic sun-direction sign inverted

**Issue:** #1459
**Filed:** 2026-06-04
**Source report:** docs/audits/AUDIT_RENDERER_2026-06-04_DIM17.md

---

**Severity:** HIGH (rendering correctness; broad blast radius — all daytime exterior water)
**Dimension:** Water (M38)
**Source report:** `docs/audits/AUDIT_RENDERER_2026-06-04_DIM17.md`
**Location:** `crates/renderer/shaders/water.frag:533-602` (caustic block)

## Description
The host's sun vector points **toward the sun**, not along light travel. `compute_sun_arc` (`byroredux/src/systems/weather.rs:90-107`) builds `sun_dir` as `[cos θ, sin θ, tilt]`: at solar noon `θ≈π/2` → `[0, +1, ~0.15]` (straight up); night fallback `[0, -1, 0]`. This same vector feeds both `GpuLight.direction_angle.xyz` (consumed by `triangle.frag`) and `CameraUBO.sunDirection.xyz` (consumed by `water.frag`).

`triangle.frag:3083` fires its directional shadow ray along `+L` (toward the sun) — **correct**. `water.frag` mislabels the vector as light-*travel* and inverts it three times:

1. **Shadow ray** (`water.frag:540`): direction `-sunRay` points **down into the floor**, almost always hits terrain → `sunVisible` false → caustic branch skipped under a clear noon sky.
2. **Refraction incident** (`water.frag:551`): `refract(sunRay, N, 1.0/1.33)` — GLSL `refract(I,N,eta)` requires `I` = propagation direction = `-sunRay`; passing `sunRay` with upward `N` gives `dot(I,N) > 0`, wrong refraction branch.
3. **Lambert weight** (`water.frag:594`): `max(dot(N, -sunRay), 0.0)` — with `N` up and `sunRay` up, clamps to **0** exactly where caustics should peak.

## Impact
Water-side caustics produce ~zero output for any above-horizon sun. The #1210 / M38 feature ships disabled-in-practice. The failure is a black/absent caustic — visually indistinguishable from "not yet implemented" and invisible to `cargo test`.

## Suggested Fix (direction only — do NOT flip the refract origin-bias, which is correct)
- `water.frag:540` shadow ray dir `-sunRay` → `sunRay`
- `water.frag:551` `refract(sunRay, …)` → `refract(-sunRay, …)`
- `water.frag:594` `dot(N, -sunRay)` → `dot(N, sunRay)`
- Correct the misleading "light travel" comments at `water.frag:111`, `water.frag:534`, and `draw.rs:687` to "points toward the sun (light-incoming)"

## Sibling (fix in the same change)
`caustic_splat.comp:258-279` carries the identical inversion for the **directional sun** case (`LtoG = normalize(L.direction_angle.xyz)`, `dot(N, -LtoG)`, `refract(LtoG, …)`). Note the point/spot branch (`LtoG = G - Lp`) is correctly signed — only the sun consumer is affected. Both accumulators share `CAUSTIC_FIXED_SCALE`; fix both to keep them on a consistent physical basis.

## Validation
Per the "no speculative Vulkan/shader fixes without RenderDoc or revert" policy: validate with a RenderDoc capture of an exterior water cell at noon — confirm `waterCausticAccum` is non-zero after the fix.

## Completeness Checks
- [ ] **SIBLING**: `caustic_splat.comp:258-279` directional case fixed in the same change; point/spot branch confirmed already-correct (not touched)
- [ ] **UNSAFE**: N/A (shader-only change, no `unsafe`)
- [ ] **DROP**: N/A (no Vulkan object lifecycle change)
- [ ] **CANONICAL-BOUNDARY**: N/A (no `translate_material` / `resolve_pbr` / emitter-params touch)
- [ ] **TESTS**: Shader math not unit-testable; RenderDoc capture (noon exterior water, accumulator non-zero) is the regression gate. Consider a host-side assert/log that `waterCausticAccum` is non-zero on an exterior-noon bench frame.
- [ ] **COMMENTS**: The three "light-travel" comments (`water.frag:111`, `water.frag:534`, `draw.rs:687`) corrected to prevent the next consumer repeating the error.
