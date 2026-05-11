# Investigation — #936 (NIF-D5-NEW-01)

## Layouts confirmed from nif.xml

`NiBSplineInterpolator` (abstract): `start_time f32, stop_time f32, spline_data_ref(Ref), basis_data_ref(Ref)` = 16 B.

`NiBSplineFloatInterpolator` adds `value f32, handle u32` (8 B).
`NiBSplineCompFloatInterpolator` adds `float_offset f32, float_half_range f32` (8 B). Total = 32 B.

`NiBSplinePoint3Interpolator` adds `value Vec3, handle u32` (16 B).
`NiBSplineCompPoint3Interpolator` adds `position_offset f32, position_half_range f32` (8 B). Total = 40 B.

## Code surface

- `crates/nif/src/blocks/interpolator.rs` already has `NiBSplineCompTransformInterpolator` + the data/basis blocks. New structs slot in next to it.
- `crates/nif/src/blocks/mod.rs:702` dispatches only the transform variant.
- `crates/nif/src/anim.rs`:
  - `extract_float_channel_at` dispatches `NiFloatInterpolator` only → extend with BSpline-comp-float fallback.
  - `resolve_color_keys_at` is the Vec3 sink (`NiColorInterpolator` + `NiPoint3Interpolator → NiPosData`) → extend with BSpline-comp-Point3 fallback.
  - Existing `extract_transform_channel_bspline` is the template for the BSPLINE_SAMPLE_HZ sampling recipe.

## Files touched

1. `crates/nif/src/blocks/interpolator.rs` — two new parsers (~80 LOC)
2. `crates/nif/src/blocks/mod.rs` — import + 2 dispatch arms
3. `crates/nif/src/anim.rs` — float-channel fallback, Point3 color-channel fallback, two new sampler helpers
4. `crates/nif/src/blocks/dispatch_tests.rs` — 3 round-trip tests (float + Point3 + static-handle)
5. `crates/nif/src/anim_tests.rs` — 3 emitter tests (BSpline-comp float, static-key fallback, BSpline-comp Point3)

5 files total — at the scope-check boundary, but each touch is mechanical and bounded.

## Notable correctness fix found during test bring-up

First draft of `extract_float_channel_bspline` early-returned `None` whenever `basis_data_ref` or `spline_data_ref` was NULL. That's wrong for the documented static-handle case (`handle == u32::MAX`, no spline data attached) — the emitter should fall back to a single-key channel at `start_time` carrying the static `value`. Same pattern applied to the Point3 sampler. The two new emitter tests pin this.
