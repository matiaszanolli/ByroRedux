# SF-2026-08-20-D6-01: water.vert still documents WaterParams.noise_falloff.yzw as reserved after 148780cb put Starfield roughness in .z

**Issue**: #3229 · https://github.com/matiaszanolli/ByroRedux/issues/3229
**Finding ID**: SF-2026-08-20-D6-01
**Filed**: 2026-08-20 (comprehensive audit suite)

Filed from `docs/audits/AUDIT_STARFIELD_2026-08-20.md` § SF-2026-08-20-D6-01 (Dimension 6 — GPU struct / shader lockstep, Starfield slice).

**Severity**: LOW · **Type**: documentation (doc-rot) · **Status**: NEW
**Location**: `crates/renderer/shaders/water.vert:107` — the `noise_falloff` doc comment, contradicted by `crates/renderer/shaders/water.frag:95-97` and `crates/renderer/src/vulkan/water.rs:105-107`

## Scope note — this is the *remainder* of the drift, not the part #3204 covers

`GpuWaterParams` lane-doc drift was filed for `uv_offset.zw` and `absorption.w` as
**#3204** (TD3-2026-08-20-02), both on `crates/renderer/src/vulkan/water.rs`.
This issue is the **third, separate site** that #3204 does not touch: the
`noise_falloff` comment in **`water.vert`**. Verified at `bb0b92f2` — the Rust
`noise_falloff` doc is *correct*, `water.frag`'s is *correct*, only `water.vert`'s is
stale. Fixing #3204 would not fix this.

## Description

The three copies of the `WaterParams` std140 record agree on **layout** (22 vec4 slots,
verified field-by-field; the `size_of::<GpuWaterParams>() == 352` pin at
`vulkan/water.rs:151` still holds and both `.spv` blobs were recompiled in the same
commit as their sources, `1a428278`). They disagree on **contract documentation**:

```glsl
// crates/renderer/shaders/water.vert:107
// x = authored Skyrim noise-falloff distance; yzw reserved.
vec4 noise_falloff;
```

All three of `yzw` are live:

```glsl
// crates/renderer/shaders/water.frag:95-96
// x = authored Skyrim noise-falloff distance; y = Blend Normals gate;
// z = Starfield surface roughness; w = Skyrim Specular Radius.
```

```rust
// crates/renderer/src/vulkan/water.rs:105-106
/// x = authored Skyrim noise-falloff distance; y = Blend Normals gate;
/// z = Starfield surface roughness; w = Skyrim Specular Radius.
```

`.z` was claimed by `148780cb` (Starfield roughness) in this delta. The producer
`byroredux/src/render/water.rs:287-292` fills all four lanes:

```rust
noise_falloff: [
    mat.noise_falloff,
    if mat.blend_normals { 1.0 } else { 0.0 },
    mat.roughness.clamp(0.0, 1.0),
    mat.specular_radius.max(0.0),
],
```

## Impact

No runtime effect. It matters because `feedback_shader_struct_sync` makes these three
copies a **lockstep contract**: `GpuWaterParams` is 352 B against a 64 KiB UBO with ~64
bytes of real headroom, so "there are three spare floats right here" is an active
invitation to reuse live lanes. The next person needing a free slot reads `water.vert`
and takes `.z` — silently killing Starfield water roughness.

## Suggested Fix

Copy `water.frag:95-96`'s wording into `water.vert:107`. One-line change.

## Related

- **#3204** (TD3-2026-08-20-02) — the sibling lane-doc drift on `uv_offset.zw` / `absorption.w` in `vulkan/water.rs`. **Fix both in one pass**; between them they cover all three declaration sites.
- **#3124** (REN-D15-01) — `GpuWaterParams` has three declaration sites and no lockstep guard. A guard that compared *comments* as well as layout would have caught this class.
- **#3206** (TD3-2026-08-20-03) — `GpuWaterParams` appears in no file under `docs/`.
- **#2763** — the same class on `water.vert`'s `GpuInstance` comment.
- `148780cb` — the commit that claimed `.z`.

## Completeness Checks
- [ ] **SIBLING**: all three `GpuWaterParams` declaration sites re-read in the same pass (`vulkan/water.rs`, `water.vert`, `water.frag`) — see #3204
- [ ] **TESTS**: consider extending the #3124 lockstep guard to compare lane documentation, not only layout
