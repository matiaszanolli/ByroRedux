# Issue #2221: REN-COMPAT-2026-07-28-02: non-transform animation channels have no production sink components — visibility/alpha/UV/morph/flipbook are runtime-dead

**State**: OPEN
**Labels**: bug, animation, renderer, medium

## Body

**Severity:** MEDIUM · **Dimension:** renderer extraction + animation/material integration
**Source:** `docs/audits/AUDIT_RENDERER_2026-07-28.md` — REN-COMPAT-2026-07-28-02
**Status when filed:** NEW — newly consolidated compatibility defect

### Description

Non-transform animation channels parse correctly and then go nowhere. The animation
system updates `Animated*` sink components **only when the target component already
exists**, and no production spawn/import path ever inserts them. Separately, several
sinks have no renderer consumer even if they were populated.

Net effect: transform animation works; almost everything else is runtime-dead.

### Evidence

**No production sink insertion.** `byroredux/src/systems/animation.rs` updates
`AnimatedVisibility`, `AnimatedAlpha`, animated material colors, `AnimatedShaderColor`,
`AnimatedShaderFloat`, `AnimatedUvTransform`, and `AnimatedMorphWeights` through
`get_or_insert_with(|| world.query_mut::<…>())` — a *query* cache, not a component
insert; the write is skipped when the entity lacks the component:

```rust
let q = alpha_q.get_or_insert_with(|| world.query_mut::<AnimatedAlpha>());   // :239
let q = uv_q.get_or_insert_with(|| world.query_mut::<AnimatedUvTransform>()); // :251
let q = shader_q.get_or_insert_with(|| world.query_mut::<AnimatedShaderFloat>()); // :266
let q = morph_q.get_or_insert_with(|| world.query_mut::<AnimatedMorphWeights>()); // :274
```

Every `world.insert(entity, Animated…)` call in the repository sits inside a
`#[cfg(test)]` block (`animation.rs:808`, `:831`, `:950`, `:1124` mark the test module
boundaries; the insertions are at `:918`, `:965-968`, `:1103`). Repo-wide search finds no
spawn/import attachment.

**Incomplete renderer consumers.** `byroredux/src/render/static_meshes.rs` reads only
`AnimatedVisibility` (`:94`) and `AnimatedUvTransform` (`:105`). It does not consume
animated alpha, material colors, shader values, or morph weights.

**Flipbooks have no consumer at all.** `TextureFlipChannel` is parsed
(`crates/nif/src/anim/types.rs:175`) and stored
(`crates/core/src/animation/types.rs:178`), and the type's own docs state renderer
integration is deferred.

### Impact

Fire/lava material motion, visibility controllers, fades, animated emissive/diffuse
effects, UV scrolling, morph targets, and Oblivion/FO3/FNV texture flipbooks parse
successfully yet remain visually inert across all games. Light color/intensity
controllers are the one exception, because they mutate an already-present `LightSource`.

### Suggested Fix

1. During clip attachment/import, insert **only** the sink components the clip's channel
   types actually require (don't blanket-attach all seven).
2. Apply animated material values at extraction, *before* `GpuMaterial` interning — not
   after, or the dedup key won't see them.
3. Implement the texture-rebind consumer for `TextureFlipChannel`.
4. Connect morph weights to an actual deformation path.
5. Add an end-to-end import → system → draw-material test. The current tests are
   helper-only routing tests, which is exactly why the missing insertion went unnoticed.

### Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Animated material values must land before `GpuMaterial`
      interning at the NIFAL boundary, never re-derived at render time — see `/audit-nifal`
- [ ] **SIBLING**: All seven sink types audited, not just the two `static_meshes.rs`
      already reads; check the skinned and particle extraction paths too
- [ ] **TESTS**: End-to-end import → system → draw-material regression test pins the fix
- [ ] Update `crates/core/src/animation/types.rs`'s "renderer integration deferred" note
      once flipbooks have a consumer
