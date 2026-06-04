# Investigation — #1459 water caustic sun-direction sign inverted

**Domain:** renderer (shaders / RT caustics)

## Root cause
`sunDirection.xyz` (CameraUBO) and `GpuLight.direction_angle.xyz` both carry the
**to-sun** vector produced by `compute_sun_arc` (`byroredux/src/systems/weather.rs:90-107`):
at noon `[0,+1,~0.15]`. `triangle.frag:3083` consumes it as light-incoming `L`
and fires the directional shadow ray along `+L` — correct. `water.frag` and the
directional branch of `caustic_splat.comp` mislabelled it as light-*travel* and
inverted all three downstream uses (shadow ray dir, `refract` incident vector,
Lambert cosine), so the caustic Lambert weight `dot(N, -sunRay)` clamped to 0 at
overhead sun → caustics ~zero for any above-horizon sun.

## Fix (direction-only; origin-bias untouched)
`crates/renderer/shaders/water.frag`:
- Renamed `sunRay` → `sunDir` (direction TO the sun) for clarity.
- Shadow ray dir `-sunRay` → `sunDir` (fire toward the sun, matches triangle.frag).
- `refract(sunRay, …)` → `refract(-sunDir, …)` (incident = light travel = sun→surface).
- Lambert `dot(N, -sunRay)` → `dot(N, sunDir)`.
- Corrected the struct comment (`:111`) and block comments to "points TO the sun".

`crates/renderer/shaders/caustic_splat.comp` (SIBLING):
- Directional branch: `LtoG = normalize(direction_angle.xyz)` → `-normalize(...)`
  so `LtoG` holds the light-*travel* direction consistent with the point/spot
  branch (`G - Lp`). This single negation fixes all three downstream uses
  (Lambert, normal-flip, `refract`) which were already written against a
  travel-direction `LtoG`. Point/spot branch was already correct — untouched.
- Corrected the misleading "direction of travel" comment.

Both `.spv` recompiled with `glslangValidator -V -I. <shader> -o <shader>.spv`.

## Verification
- `cargo check -p byroredux-renderer`: clean.
- `cargo test -p byroredux-renderer`: 299 passed (incl. SPIR-V reflection tests
  that load the recompiled shaders — UBO layout unchanged, comment-only).
- `cargo test` (workspace): 2785 passed, 0 failed.

## Completeness checks
- [x] **SIBLING**: `caustic_splat.comp` directional case fixed in same change;
  point/spot branch confirmed already-correct and left untouched.
- [x] **UNSAFE**: N/A (shader-only).
- [x] **DROP**: N/A (no Vulkan object lifecycle change).
- [x] **COMMENTS**: three "light-travel" mislabels corrected (`water.frag:111`,
  the caustic block, `caustic_splat.comp` directional branch).
- [ ] **TESTS**: shader math is not unit-testable in this harness; the change is
  a logic sign-flip with cross-shader evidence (triangle.frag uses the opposite
  sign on the *same* vector), not a speculative sync/pipeline change.

## Follow-up (not blocking)
Per the project "no speculative Vulkan/shader fixes without RenderDoc" policy:
recommend a RenderDoc capture of an exterior water cell at noon to confirm
`waterCausticAccum` is non-zero after the fix. Cannot be run in this
(headless, no-GPU) environment.
