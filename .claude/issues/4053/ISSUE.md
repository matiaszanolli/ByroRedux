# #4053 — Renderer: collapse the TLAS-membership predicate into one policy

**State**: OPEN · **Labels**: renderer vulkan tech-debt 

Split out of #3807 Phase 1, because it is not a ground-cover component and is
larger than that phase assumed.

## What

`collect_static_mesh_draws` decides TLAS membership with an ad-hoc chain
([`byroredux/src/render/static_meshes.rs:251`](../blob/main/byroredux/src/render/static_meshes.rs#L251)):

```rust
let in_tlas = (!is_lod || lod_shadow_caster)
    && !is_decal_mesh
    && material_kind != byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION;
```

Three unrelated exclusion reasons, each added independently. `exal-groundcover.md`
§5 predicted this shape and proposed an `ExcludedFromTlas` marker component as the
fix.

## Why it is not just "add a marker"

Two findings from the #3807 review pass (2026-09-06):

1. **`IsLodTerrain` is no longer a boolean exclusion.** `lod_shadow_caster` lets a
   camera-local subset of LOD blocks into the TLAS as structure shadow casters.
   Collapsing it into a boolean marker would silently change LOD shadowing.
   `exal-groundcover.md` §2 carried the stale "keeps it out of the TLAS" claim
   until this pass; the design was written against the old behaviour.
2. **Ground cover will never carry the marker anyway.** §2's binding constraint
   makes blades a GPU-side point list, never ECS entities, so nothing in the
   ground-cover path flows through this loop. The first entity that would is the
   Phase 4 proxy shell, which is now gated.

So the cleanup is still worth doing — a fourth reason is coming eventually — but
it should be done for its own sake, with its own justification, and not as a
prerequisite anyone believes ground cover needs.

## The risk that makes this its own issue

The failure mode is "something quietly enters or leaves the TLAS". That is
invisible to `cargo test`, and this repo has been bitten by shipping renderer
changes whose failure modes only appear on a GPU. Wants a RenderDoc capture
before and after, on a scene containing all three current exclusion classes,
not a green test run.

## Done when

- One policy function, with each exclusion reason named and its distance/state
  gating preserved exactly.
- Before/after RenderDoc captures showing identical TLAS membership on a scene
  with LOD terrain, decals and fire-refraction material all visible.
