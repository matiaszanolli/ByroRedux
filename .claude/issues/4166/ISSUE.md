# NIFAL-D7-2026-09-11-01: B-spline rotation channel is missing the belt-and-braces finiteness guard its float/translation/scale siblings got in #3765

URL: https://github.com/matiaszanolli/ByroRedux/issues/4166
Labels: bug, animation, nif-parser, high, game:fnv, game:fo3, game:skyrim, nifal

---

**Severity**: HIGH
**Dimension**: Animation
**Tier Violated**: no-fabrication (a poisoned/NaN transform value reaching the canonical `AnimationClip` is exactly the class of defect the tier's finite-guard doctrine exists to prevent)
**Game Affected**: FO3/FNV and Skyrim+ (any title whose NIF content authors `NiBSplineCompTransformInterpolator` — not Skyrim-only, per project note `feedback_bspline_not_skyrim_only`)
**Location**: `crates/nif/src/anim/bspline.rs:367-401` (`extract_transform_channel_bspline`'s rotation branch), contrast with the sibling finiteness guards at lines 248 (float), 341 (translation), 408 (scale)
**Status**: NEW

**Description**: Commit `9ce6b7a5` (Fix #3765) added an `is_key_value_sane` check on the sampled `deboor_cubic` output at three of the four B-spline sub-channel call sites — float, translation, and scale — as a belt-and-braces layer beyond the already-added `channel_slice` guard. The commit message and fixture comments both enumerate this as covering "all four" sub-channels, but the rotation branch was left unguarded: `deboor_cubic`'s raw `[w,x,y,z]` output is fed directly into the quaternion-normalize arithmetic with no finiteness check on either the pre- or post-normalize values, and the result is unconditionally pushed to `rotation_keys`.

This is not simply the same NaN-input case the other three guard against — the normalize arithmetic introduces its own, distinct overflow path that a direct `is_key_value_sane(p[i])` check (mirroring translation/scale) would not fully catch on the pre-normalize values: a per-component value that individually passes `is_key_value_sane` (finite, `< 3.0e38`) can still overflow to `Infinity` when squared in `w*w` (anything with `|w| > ~1.84e19` squares past `f32::MAX`), making `len_sq` infinite; `1.0 / sqrt(Infinity) = 0.0`; and `Infinity * 0.0 = NaN`. So a rotation control point that is individually "sane" by the exact predicate used elsewhere can still poison the pushed `RotationKey` through the normalize step alone.

Coincidental partial protection: if all four raw components happen to be NaN at once, `len_sq` is NaN, `NaN > f32::EPSILON` is false, and the `else` branch substitutes an identity quaternion — so the purely-NaN-input case is accidentally caught. The Infinity-input case (and the "individually sane but squares-to-Infinity" case) is not.

**Evidence** (`bspline.rs:367-391`):
```rust
let p = deboor_cubic(cps, n_cp, BSPLINE_ROT_STRIDE, u);
let [mut w, mut x, mut y, mut z] = [p[0], p[1], p[2], p[3]];
let len_sq = w * w + x * x + y * y + z * z;
if len_sq > f32::EPSILON {
    let inv = 1.0 / len_sq.sqrt();
    w *= inv; x *= inv; y *= inv; z *= inv;
} else {
    w = 1.0; x = 0.0; y = 0.0; z = 0.0;
}
rotation_keys.push(RotationKey { time: t, value: zup_to_yup_quat([w, x, y, z]), tbc: None });
```
No `is_key_value_sane` call anywhere in this branch, unlike the sibling translation branch immediately above it (`if zup.iter().all(|v| is_key_value_sane(*v)) { ... }`).

**Impact**: A crafted or corrupted B-spline-compressed rotation channel (large-magnitude quantized control points, or a `half_range` large enough to make squared components overflow during normalize) can still poison a bone's rotation with NaN despite #3765 having shipped as a fix for "the" B-spline finiteness gap. Downstream blast radius: `GlobalTransform` → skinned mesh vertex positions → BLAS refit / TLAS build with a NaN-containing AABB — undefined behavior per Vulkan's acceleration-structure build contract (potential VkDevice loss or GPU crash), not merely a visual glitch.

**Related**: Sibling/parent of #3765 (this is the one channel that commit's own fix left out); same failure family as #1443 (`is_key_value_sane`'s origin).

**Suggested Fix**: Add the same `is_key_value_sane` check the translation/scale branches use, but on the post-normalize `[w,x,y,z]` (not just the pre-normalize `p[]`, since the overflow can be introduced by the normalize arithmetic itself) — skip the sample (mirroring the "skip just that sample" behavior the other three branches already have) if any component fails.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: This is inside `crates/nif/src/anim/bspline.rs`, upstream of `byroredux/src/anim_convert.rs::convert_nif_clip` (`Quat::from_xyzw`, no sanitization) → the canonical `AnimationClip`; per-game logic stays at the NIFAL parser→canonical boundary, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A test analogous to `bspline_dequant_and_deboor_can_overflow_and_the_guard_catches_it` but driving the quaternion-normalize path specifically pins this the same way #3765 pinned its three siblings

