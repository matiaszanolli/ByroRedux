# #2495: REN-D9-NEW-03: Stale palette-bound number in the skin_vertices.comp clamp rationale

**State**: OPEN  **URL**: https://github.com/matiaszanolli/ByroRedux/issues/2495  **Labels**: documentation, animation, renderer, low, vulkan

**Severity**: LOW
**Dimension**: 9 — Skinning
**Location**: `crates/renderer/shaders/skin_vertices.comp:141`
**Status**: NEW

## Description
The #651 / SH-6 clamp comment says an unclamped index "would read past `bone_offset + 127` into the adjacent mesh's palette". `MAX_BONES_PER_MESH` was raised to 144 (#1135), so the real boundary is `bone_offset + 143`. The code itself is correct (`min(boneIdx, uvec4(MAX_BONES_PER_MESH - 1u))`); only the prose is stale.

## Evidence
`crates/renderer/src/shader_constants_data.rs:64` → `pub const MAX_BONES_PER_MESH: u32 = 144;`, matching `crates/core/src/ecs/components/skinned_mesh.rs:52`.

## Impact
Documentation only. Flagged because this is a stride/bound comment on a safety clamp, and the M29 failure modes in this dimension are all "two sites drifted and nothing observed it" — a wrong number here is exactly the kind of thing a future reader would trust.

## Related
#651 / SH-6, #1135.

## Suggested Fix
Change `127` to `MAX_BONES_PER_MESH - 1` (avoid re-baking a literal).

## Completeness Checks
- [ ] **TESTS**: N/A (comment-only change)


---

# #2496: REN-D10-NEW-01: #2240's freqScale multiplies water's absolute textured wave UV, amplifying the one un-rebased large-world consumer

**State**: OPEN  **URL**: https://github.com/matiaszanolli/ByroRedux/issues/2496  **Labels**: bug, renderer, low, vulkan

**Severity**: LOW
**Dimension**: 10 — Camera-Relative Precision
**Location**: `crates/renderer/shaders/water.frag:221` (`sampleScrollingNormal`, textured branch); scale sourced at `:415`
**Status**: NEW (introduced by `6d40f6bf` / #2240, landed 2026-08-05, after the 2026-08-03 audit)

## Description
`sampleScrollingNormal` has two branches. The procedural branch was fixed under #1997 to rebase its hash input origin-relative: `vec2 uv = (uvBase - originOffset) * scale + scroll * time;` (relative, correct). The textured branch deliberately stays **absolute** so the wrapping sampler has no seam at a render-origin crossing: `vec2 uv = uvBase * scale * freqScale + scroll * time;` (absolute). #2240 inserted `freqScale = push.misc.y / 0.6` (WATR-authored `wave_frequency`, **unclamped**) into that product. `uvBase` is `vWorldPos.xz`, up to ~176k on MarkarthWorld. With the default `uv_scale_a = 1/256` and the default `wave_frequency = 0.6` (`freqScale == 1.0`) nothing changes from the pre-#2240 magnitude (~687, f32 ULP ≈ 6.1e-5 ≈ 1/16 texel on a 1024² normal map). But any WATR authoring `wave_frequency > 0.6` scales the UV magnitude — and therefore the quantization step — proportionally, with no upper bound. At `freqScale ≈ 3.3` the ULP reaches ~1/4 texel.

## Evidence
`:415` `float freqScale = push.misc.y / 0.6;` (no clamp) feeding `:221`. The companion in-code precision comment at `:183-193` documents the hazard for the procedural branch only and explicitly says the textured branch keeps its "absolute (wrapping) UV".

## Impact
Visual only, and only for textured water (Skyrim/FO4 WATR with a bound normal map) in a worldspace far from the origin *and* with an authored `wave_frequency` above the 0.6 default — the wave normal map stair-steps/aliases instead of resolving smoothly. Invisible near the origin and unreachable from `cargo test`; needs a large-world capture to confirm the practical magnitude. I did **not** verify what vanilla Skyrim actually authors for `wave_frequency`, so the real-content blast radius is unconfirmed — reporting the mechanism, not a claimed observed artifact.

## Related
#1997 (procedural-branch rebase), #2240 / `6d40f6bf` (the `freqScale` addition), #1502 (original water precision bound).

## Suggested Fix
Subtract the *tile-integral* part of the origin so the wrapping sampler is unaffected but the magnitude collapses: `vec2 o = floor(originOffset * scale * freqScale); vec2 uv = uvBase * scale * freqScale - o + scroll * time;`. Separately consider clamping `freqScale` to a sane authored range at the CPU packing site (`byroredux/src/render/water.rs:107`).

## Completeness Checks
- [ ] **TESTS**: A large-world capture confirms the practical magnitude before/after the fix (no game data → document as needs-verification)


---

# #2497: REN-D10-NEW-02: caustic_splat.comp is the one CameraUBO re-declarer still missing #2164's renderOrigin.w payload note

**State**: OPEN  **URL**: https://github.com/matiaszanolli/ByroRedux/issues/2497  **Labels**: documentation, renderer, low, vulkan

**Severity**: LOW
**Dimension**: 10 — Camera-Relative Precision
**Location**: `crates/renderer/shaders/caustic_splat.comp:76`
**Status**: NEW (incomplete application of the fix for prior finding L-10 / #2164)

## Description
#2164 fixed the "w unused" documentation trap at `draw.rs`, `water.vert:83` and `cluster_cull.comp:69` — all three now read "w = FSR one-frame-reset flag (NOT padding — #2164/L-10)". The fourth `CameraUBO` re-declarer, `caustic_splat.comp`, was missed and still reads:
```glsl
vec4 renderOrigin;   // #markarth-precision — camera-relative render origin (added to inv_view_proj world reconstruction below). Keeps CameraUBO == sizeof(GpuCamera).
```
— no mention of `w` at all, and the trailing "Keeps CameraUBO == sizeof(GpuCamera)" reads as "this field is here for padding parity", exactly the reading #2164 set out to eliminate.

## Evidence
`grep -n "w unused" water.vert cluster_cull.comp draw.rs` → 0 hits (fixed); `caustic_splat.comp:76` carries neither the corrected wording nor a `w` description.

## Impact
Documentation only. Same latent trap class as the tracked `VolumetricsParams::render_origin.w` overload (#1928): a future author reading only this site could repurpose `w` and silently break the FSR reset-flag contract that `triangle.frag:582` (`clamp(renderOrigin.w, 0.0, 1.0)`) depends on.

## Related
Prior L-10 / #2164; #1928 / REN-D10-01.

## Suggested Fix
Copy `cluster_cull.comp:69`'s wording verbatim into `caustic_splat.comp:76`.

## Completeness Checks
- [ ] **TESTS**: N/A (comment-only change)


---

# #2506: REN-D14-2026-08-07-02: EMA decay pass still floors while the deposit stochastically rounds (#2239 half-fix)

**State**: OPEN  **URL**: https://github.com/matiaszanolli/ByroRedux/issues/2506  **Labels**: bug, renderer, low, vulkan

**Severity**: LOW
**Dimension**: 14 — Caustics
**Location**: `crates/renderer/shaders/caustic_splat.comp`, the `pc.decayOnly == 1u` block
**Status**: NEW (residual of the fix for #2239)

## Description
#2239 identified that the parked-camera EMA drove dim caustics to zero because the per-tap deposit truncated sub-ULP values every frame, and fixed it by stochastically rounding the deposit. The *paired* operation — the decay pass — was not changed and still truncates: `uint(float(v) * pc.decayFactor)` discards a mean 0.5 fixed-point ULP per texel per frame. That is a constant additive drain, so the EMA's steady state is `A* = (D - 0.5) / (1 - decay)` instead of `D / (1 - decay)`, short by `0.5 / (1 - decay)` fixed-point units. At `CAUSTIC_DECAY_MAX = 0.995` that is 100 units ≈ `100/65536 = 0.0015` luminance; any pool texel whose true per-frame deposit is below 0.5 ULP still collapses to exactly zero no matter how many frames pass, reproducing the #2239 symptom on the decay side.

## Evidence
```glsl
if (pc.decayOnly == 1u) {
    uint v = imageLoad(causticAccum, pixel).r;
    imageStore(causticAccum, pixel, uvec4(uint(float(v) * pc.decayFactor), 0u, 0u, 0u));
    return;
}
```
contrasted with the deposit path, which does dither:
```glsl
if (pc.decayFactor > 0.0) {
    float fracPart = depositF - float(fv);
    ...
    if (fracPart > ditherThreshold) { fv += 1u; }
}
```

## Impact
Bounded erosion of the dim outskirts of a parked-camera caustic pool (hard-edged, slightly-too-small pool; sub-0.0015-luminance caustics vanish entirely). Much smaller than the pre-#2239 unbounded collapse, and only while parked.

## Related
#2239, commit `4279c195`; `AUDIT_RENDERER_2026-08-02.md` REN-D14-02.

## Suggested Fix
Apply the same PCG-hash stochastic rounding to the decay `imageStore` (round `v * decayFactor` up when its fraction exceeds a per-(texel, frame) threshold), so the multiply is unbiased in expectation like the deposit now is.

## Completeness Checks
- [ ] **TESTS**: N/A shader-side; document the fix rationale inline mirroring the deposit-path pattern


---
