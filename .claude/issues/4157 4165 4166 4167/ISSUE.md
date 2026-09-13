## #4157 [OPEN] PERF-D6-2026-09-11-01: NiPixelData mipmap array bypasses the amplification guard allocate_vec_sized exists to close
labels: bug, nif-parser, medium, nif, game:oblivion

**Severity**: MEDIUM
**Dimension**: 6 — Allocation Hygiene
**Game Affected**: Oblivion (the only current source of embedded `NiPixelData` textures); mechanism is version-agnostic
**Location**: `crates/nif/src/blocks/texture.rs:210,288`
**Status**: NEW

**Description**: Both `NiPixelData`/`PixelFormatPrelude` mipmap-descriptor loops pre-size with the loose `stream.allocate_vec::<MipMapInfo>(num_mipmaps)` — a 1-byte-per-element bound with no `check_alloc`/`MAX_SINGLE_ALLOC_BYTES` routing at all — for a 12-byte struct (`width`/`height`/`offset`, all `u32`) that `allocate_vec_sized`'s own doc comment names as exactly the case it exists to bound correctly.

**Evidence** (`texture.rs:209-215` and the parallel `:287-292` site):
```rust
let mut mipmaps: Vec<MipMapInfo> = stream.allocate_vec(num_mipmaps)?;
for _ in 0..num_mipmaps {
    let width = stream.read_u32_le()?;
    let height = stream.read_u32_le()?;
    let offset = stream.read_u32_le()?;
    ...
```

**Impact**: A crafted `NiPixelData` block with an inflated `num_mipmaps` can trigger a `Vec::with_capacity` request up to 12× the remaining file bytes, unconstrained by the crate's 256 MB hard cap — an OOM/large-allocation DoS vector on untrusted input, not covered by any of the three `heap_allocation_bounds*.rs` dhat gates.

**Suggested Fix**: `stream.allocate_vec_sized::<MipMapInfo>(num_mipmaps)?` at both call sites — the type has no heap indirection or version-dependent size variability, so the exact `size_of`-based bound applies cleanly.

## Completeness Checks
- [ ] **TESTS**: A dhat allocation-bounds test pins both call sites at the `size_of::<MipMapInfo>()` bound


## #4165 [OPEN] PERF-D6-2026-09-11-02: read_pod_vec_from_cursor lacks the #[must_use] its NifStream twin carries
labels: bug, nif-parser, low, tech-debt, nif

**Severity**: LOW
**Dimension**: 6 — Allocation Hygiene
**Location**: `crates/nif/src/header.rs:426-449`
**Status**: NEW

**Description**: `read_pod_vec_from_cursor` (header parser) lacks the `#[must_use]` attribute its `NifStream` twin (`read_pod_vec`, `crates/nif/src/stream.rs:445`) carries. No live bug — both current call sites (`header.rs:272,296`) correctly bind the result — but the asymmetry means a future caller that forgets to bind the result gets no compiler warning, unlike the stream-side twin.

**Evidence**: `stream.rs:445` carries `#[must_use = "read_pod_vec returns a populated Vec; bind it or call stream.skip() to advance the cursor without reading"]`; the equivalent attribute is absent from `header.rs:426`'s `read_pod_vec_from_cursor`.

**Impact**: None today — both call sites already bind the result. Latent hygiene gap only.

**Suggested Fix**: Add the matching `#[must_use]` attribute to `read_pod_vec_from_cursor`, mirroring its `NifStream` twin.

## Completeness Checks
- [ ] none — trivial attribute addition, no behavioral test applies


## #4166 [OPEN] NIFAL-D7-2026-09-11-01: B-spline rotation channel is missing the belt-and-braces finiteness guard its float/translation/scale siblings got in #3765
labels: bug, animation, nif-parser, high, game:fnv, game:fo3, game:skyrim, nifal

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


## #4167 [OPEN] NIFAL-D9-NEW-01: Animation and Particles remain the only two declared NIFAL boundaries with no completeness/dispatch-coverage guard
labels: bug, medium, nifal, test-gap

**Severity**: MEDIUM
**Dimension**: Completeness
**Tier Violated**: harness-coverage gap — no production tier violated (same classification precedent as the now-closed #2532)
**Game Affected**: all seven (harness-coverage gap, not a per-game data bug)
**Location**: `byroredux/src/material_translate.rs:2243-2244` (the scoping comment names this itself); no equivalent structural guard exists for `byroredux/src/anim_convert.rs::convert_nif_clip`, `byroredux/src/asset_provider/animation.rs::convert_hkx_clip`, or `byroredux/src/systems/particle.rs::apply_emitter_overlays`
**Status**: NEW (successor to closed #2532, which explicitly named Animation as "the next extension" when it fixed Lights and confirmed Collision)

**Description**: #2532 closed with Collision (pre-existing) and Lights (new) both gaining a completeness-style dispatch-coverage guard, and its own closing comment correctly re-scoped the remaining gap to Animation (`convert_nif_clip`/`convert_hkx_clip`) only. That leaves Particles (`apply_emitter_overlays`) unmentioned and still unguarded — the original #2532 body listed it as one of the ~5 boundaries in scope, but neither the issue's fix nor its closing comment addressed it, and no separate issue tracks it.

**Evidence**: `grep -rn "canonical_completeness_harness\|dispatch_coverage" -r crates/nif/src byroredux/src` finds exactly 3 guard modules: `material_translate.rs::canonical_completeness_harness` (Material), `import/collision/mod.rs::dispatch_coverage_tests` (Collision), `import/walk/lights.rs::light_dispatch_coverage_tests` (Lights). None reference `convert_nif_clip`, `convert_hkx_clip`, or `apply_emitter_overlays`. All three existing guards pass.

**Impact**: A silent field-drop or a whole-arm omission in `convert_nif_clip`/`convert_hkx_clip` or in `apply_emitter_overlays` would not be caught by any automated signal today; it depends entirely on a human audit sweep noticing, exactly the gap #2532 itself was filed to close for Collision/Lights.

**Related**: Successor/residual scope of closed #2532 (itself the successor of closed #2214).

**Suggested Fix**: For Animation, add a kitchen-sink-value harness analogous to Material's (both `convert_nif_clip` and `convert_hkx_clip` produce the same canonical `AnimationClip` target and both live in `byroredux`, so no crate-boundary obstacle applies). For Particles, `apply_emitter_overlays`'s failure mode is closer to Lights/Collision's ("a whole overlay field stops being unioned in") than Material's per-field copy, so a structural/revert-and-fail guard is the better fit. Update the Material module's scoping comment once either lands.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: New guards must live at the existing declared boundaries (`convert_nif_clip`/`convert_hkx_clip`, `apply_emitter_overlays`) — never a new parallel boundary. See `/audit-nifal`.
- [ ] **TESTS**: This finding IS a test-gap; closing it means adding the two missing completeness guards


