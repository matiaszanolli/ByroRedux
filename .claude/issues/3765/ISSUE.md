# #3765 — SAFE-2026-08-30-D9-01: B-spline dequantised sample path has no finite guard — a NaN `offset`/`half_range` reaches `Transform`, the bone palette and the TLAS

**Labels**: bug, animation, nif-parser, medium, safety, nif

---

- **Severity**: MEDIUM
- **Dimension**: 9 — NIFAL NaN/Inf boundary
- **Location**: `crates/nif/src/anim/bspline.rs` — `dequant`, `dequantize_channel`, `extract_float_channel_bspline`, `extract_transform_channel_bspline`; source fields in `crates/nif/src/blocks/interpolator.rs`
- **Source**: `docs/audits/AUDIT_SAFETY_2026-08-30.md` (`SAFE-D9-01`), HEAD `64f64480`

## Description

`NiBSplineCompTransformInterpolator`'s six quantisation parameters (`translation_offset` /
`translation_half_range` / `rotation_*` / `scale_*`) are read as raw `f32`s off disk with
**no validation**, then fed to

```rust
pub fn dequant(raw: i16, offset: f32, half_range: f32) -> f32 {
    offset + (raw as f32 / 32767.0) * half_range
}
```

A NaN or ±Inf in either parameter makes **every** dequantised control point non-finite, and
the de Boor evaluation propagates it.

The **pose-fallback** branches of this same function are correctly gated by `is_flt_max`,
and the mainline keyframe converters are gated by `is_key_value_sane` (#1443,
`crates/nif/src/anim/keys.rs`) — but the **sampled** branch, which is the whole point of
the block, passes `deboor_cubic`'s output straight into the key structs with **neither
check**.

Rotation happens to be safe **by accident** (the `len_sq > f32::EPSILON` test is false for
NaN, so the quaternion falls back to identity); **translation, scale, and the whole of
`extract_float_channel_bspline` are not.**

## Evidence

- `crates/nif/src/blocks/interpolator.rs` — six consecutive `stream.read_f32_le()?` with no
  `is_finite()` filter before they land on the struct.
- `crates/nif/src/anim/bspline.rs` — `translation_keys.push(TranslationKey { value:
  zup_to_yup_pos(zup), … })` where `zup` comes directly from `deboor_cubic` (re-verified
  verbatim at HEAD); contrast the `else` arm immediately below, which **does** gate on
  `is_flt_max`.
- Same shape for `ScaleKey`, again against a gated `else` arm.
- `keys.push(AnimFloatKey { value: p[0] })` — ungated, while the static fallback returns
  `None` on the sentinel.
- **No downstream gate exists**: grepping `is_finite` across `crates/core/src/animation/`
  hits only `player.rs` ×2 and `stack.rs` — all on *time*, none on sampled values.
  `byroredux/src/systems/animation.rs` writes `transform.translation = pos`
  unconditionally.
- **Reachability**: this is *not* a Skyrim-only path — #1424 (CLOSED) established that
  `NiBSplineCompTransformInterpolator` is reachable on FO3/FNV too (see also
  `feedback_bspline_not_skyrim_only`).

## Impact

A NaN `Transform.translation` propagates to `GlobalTransform`, then into `bone_world`
(`byroredux/src/render/skinned.rs`) and the `GpuInstance` model matrix — i.e. into the
bone-palette SSBO, the skinned-BLAS refit vertices, and the TLAS instance transform.
**A NaN AABB in an acceleration-structure build is undefined behaviour**, and a NaN bone
matrix silently vanishes the whole mesh.

`pose_hash` hashes NaN by bit pattern, so the pose stays **permanently "dirty"** and
re-dispatches the skin compute + BLAS refit every frame — the cost is per-frame CPU/GPU
work as well as UB.

Trigger is corrupt or hostile file data (a mod archive), **not vanilla content**: on vanilla
NIFs an unused channel carries an INVALID (`u32::MAX`) handle, so `channel_slice` returns
`None` and the already-gated pose fallback runs instead. Rated MEDIUM to match this repo's
settled severity for the identical defect class from corrupt NIF floats — #3529, #3048,
#1534, #3132, #3432 are all MEDIUM.

## Suggested Fix

Reject the block **at the boundary** rather than filtering per key: in
`extract_transform_channel_bspline` / `extract_float_channel_bspline`, require the relevant
`offset` + `half_range` pair to satisfy `is_key_value_sane` before building each
`channel_slice`; a failing pair drops that channel to `None`, which **already routes to the
gated pose fallback**.

Belt-and-braces, run the pushed values through the existing `is_key_value_sane`
(`crates/nif/src/anim/keys.rs`) at the three `push` sites so a future arithmetic overflow
inside `deboor_cubic` cannot outrun the input check. Pin with a synthetic-fixture test
alongside `crates/nif/src/anim/tests/sanitize.rs`.

## Related

#772 (the sentinel this path partially honours), #1443 (mainline stream sanitiser),
**#3432 (OPEN — `NiControllerSequence` `duration`/`weight` unsanitised past #3258; direct
sibling, same layer)**, #3529 (CLOSED — NaN quad → NaN LocalBound → NaN BLAS vertices),
#3048 (CLOSED — ±Inf vertex positions into the vertex buffer), #1424 (B-splines reachable
on FO3/FNV).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every sampled (non-fallback) branch in `bspline.rs`, and the `#3432` sibling in the same layer
- [ ] **CANONICAL-BOUNDARY**: The guard belongs at the NIF parser → canonical boundary, not re-derived at render time — do not push a per-game or render-time NaN filter into the shaders/renderer. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix — a synthetic fixture with a NaN `translation_half_range` must yield `None` for that channel, not a NaN key
