# #3765 — SAFE-2026-08-30-D9-01: B-spline dequantised sample path has no finite guard

**Severity**: MEDIUM · **Location**: `crates/nif/src/anim/bspline.rs` — `dequant`, `channel_slice`, `extract_float_channel_bspline`, `extract_transform_channel_bspline`
**Source**: `docs/audits/AUDIT_SAFETY_2026-08-30.md` (SAFE-D9-01)

`NiBSplineCompTransformInterpolator`'s six quantization parameters
(`translation_offset`/`translation_half_range`/rotation/scale) are read as
raw `f32`s off disk with no validation, then fed to `dequant`. A NaN or
±Inf in either parameter poisons every dequantized control point, and the
de Boor evaluation propagates it into the sampled channel — translation,
scale, and the whole of `extract_float_channel_bspline` had neither the
`is_flt_max` gate the pose-fallback branches use nor the `is_key_value_sane`
gate the mainline keyframe converters use (#1443). Rotation was safe only
by accident (`len_sq > f32::EPSILON` happens to be false for NaN). A NaN
`Transform.translation` reaches `GlobalTransform`, the bone-palette SSBO,
the skinned-BLAS refit, and the TLAS instance transform — a NaN AABB in an
acceleration-structure build is undefined behavior, and `pose_hash` hashing
NaN by bit pattern keeps the pose permanently "dirty", re-dispatching the
skin compute + BLAS refit every frame.

## Fix implemented

Exactly the issue's own suggested fix, both parts:

1. **Boundary rejection**: `channel_slice` — the single choke point all
   four callers (the float channel's scalar channel, the transform
   channel's translation/rotation/scale channels) funnel through — now
   requires `offset` and `half_range` to both satisfy `is_key_value_sane`
   before dequantizing. A failing pair returns `None`, which every caller
   already routes to its existing, correctly-gated pose fallback. One
   change covers all four call sites at once.
2. **Belt-and-braces**: the three previously-ungated `push` sites
   (`extract_float_channel_bspline`'s scalar key,
   `extract_transform_channel_bspline`'s translation and scale keys) now
   run the sampled `deboor_cubic` output through `is_key_value_sane` before
   pushing, skipping just that sample (not the whole channel) on failure —
   the sampler interpolates across the gap from its finite neighbors. This
   guards against a pathologically large *but individually finite*
   `offset`/`half_range` pair overflowing to non-finite through
   `deboor_cubic`'s repeated blending even after passing the boundary
   check (pinned directly:
   `bspline_dequant_and_deboor_can_overflow_and_the_guard_catches_it`
   reproduces the overflow with `dequant(32767, f32::MAX, f32::MAX)` and
   confirms `is_key_value_sane` rejects it).

Rotation's existing accidental-safety (`len_sq > f32::EPSILON` false for
NaN) was left as-is — the issue didn't flag it as needing an explicit gate,
and it's already covered transitively by the boundary-level `channel_slice`
fix (a NaN `rotation_offset`/`rotation_half_range` now never reaches
`deboor_cubic` at all).

**SIBLING** (issue's own checklist item): checked #3432 (`NiControllerSequence`
`duration`/`weight` unsanitized), named as the direct sibling in the same
layer — **already CLOSED**, independently fixed. Nothing to do there.

**CANONICAL-BOUNDARY** (issue's own checklist item): the guard lives entirely
in the NIF parser → canonical boundary (`bspline.rs`); no per-game logic, no
render-time NaN filter pushed into the shaders/renderer.

**TESTS** (issue's own checklist item — "a synthetic fixture with a NaN
`translation_half_range` must yield `None` for that channel"): five new
tests in `crates/nif/src/anim/tests/bspline.rs`, at the `channel_slice`
level (the exact function the fix centralizes on, and the same level the
file's existing tests already operate at) —
`bspline_channel_slice_rejects_nan_offset`,
`_rejects_nan_half_range`, `_rejects_infinite_quantization_params` (all
four ±Inf combinations), plus the belt-and-braces overflow test above. The
existing real-fixture tests (`extract_float_channel_at_samples_bspline_comp_float`,
`resolve_color_keys_at_samples_bspline_comp_point3`, the round-trip parser
tests) all still pass unchanged, confirming legitimate finite-parameter
content is unaffected.

Full workspace: `cargo test --no-fail-fast` 7067 passing, 0 failing (+5 new
tests).
