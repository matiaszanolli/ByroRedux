# NIFAL-D6-NEW-01: synthesize_packed_havok_proxy unions skinned-mesh bind-pose geometry into the compatibility AABB, unlike its Architecture-trimesh sibling

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2531
**Finding ID**: NIFAL-D6-NEW-01

**Severity**: MEDIUM
**Dimension**: Collision · **Tier Violated**: NIFAL translate boundary (canonical-fallback tier — the compatibility-proxy consumer introduced this cycle by `716b7ee9`/`8ee151e0`, not the raw/translate tiers proper)
**Game Affected**: FO4 / FO76 / Starfield — any `RenderLayer::Actor` (CREA — creature) or `RenderLayer::Clutter` placement with packed (`BhkNPCollisionObject`) collision authoring and a skinned render mesh
**Location**: `byroredux/src/cell_loader/spawn.rs:118-135` (`synthesize_packed_havok_proxy`'s mesh filter), contrasted with `byroredux/src/cell_loader/spawn.rs:1680-1687` (the sibling `ArchitectureTriMesh` gate, which requires `mesh.skin.is_none()`)
**Status**: NEW — brand-new code path (landed `8ee151e0`, this delta window). Not a duplicate; sibling of the same-cycle-fixed `#2355` (that issue was "no proxy at all" for Clutter/Actor; this is "proxy built from the wrong pose data" once the sibling fix landed).

## Description
The Architecture trimesh fallback (`synthesize_static_trimesh`) explicitly excludes skinned meshes (`mesh.skin.is_none()`, "never synthesize for animated bodies"). `synthesize_packed_havok_proxy` has no equivalent check:
```rust
let geometry = meshes
    .iter()
    .filter(|mesh| {
        !mesh.material.is_decal
            && !mesh.material.alpha_test
            && mesh.material.material_kind
                != byroredux_renderer::MATERIAL_KIND_FIRE_REFRACTION
            && !mesh.positions.is_empty()
    })
    .map(|mesh| ProxyMeshGeometry { positions: &mesh.positions, ... });
```
`mesh.positions` on a skinned `ImportedMesh` is bind-pose (T-pose/rest-pose) local geometry — the same array GPU skinning deforms at render time, not a runtime-posed shape. Creature (CREA) REFRs on FO4+/FO76/Starfield reach `spawn_placed_instances` through the generic REFR path (`npcs: &HashMap<u32, NpcRecord>` is keyed by NPC_ only — CREA is absent, so it falls through to `spawn_synth_child` → `spawn_placed_instances` with `base_layer = RenderLayer::Actor`), so a creature whose model is a skinned mesh and whose NIF authors only packed Havok gets its collision cuboid built from bind-pose vertex positions.

## Evidence
No test in either commit constructs an `ImportedMesh` with `skin: Some(...)` through this path — both new tests (`packed_proxy_bakes_outer_scale_into_cuboid_extent`, `packed_proxy_is_keyframed_and_parented_to_visual_placement`) use `ImportedMesh::from_geometry(...)`, which defaults `skin: None`. The gap is untested as well as unguarded.

## Impact
A bind-pose T-pose skeleton for many creature/character rigs has limbs splayed far wider than the resting silhouette, so the resulting `Cuboid` half-extents can be substantially oversized relative to the creature's visible footprint — an invisible collision block extending well beyond the rendered model, obstructing movement in open space around the creature. The proxy is `Keyframed` and parented to `placement_root` (not any bone), so it never reflects animated posture — the mis-sizing is permanent for the creature's lifetime, not a spawn-frame transient. Scoped to skinned creature/actor content on FO4+/FO76/Starfield, exactly the population most likely to lack decoded classic collision.

## Related
Sibling of fixed `#2355`.

## Suggested Fix
Either (a) add a `mesh.skin.is_none()` filter to the closure, matching the Architecture precedent, and fall back to each mesh's already-computed `local_bound_center`/`local_bound_radius` (pose-independent, mesh-local) for skinned submeshes instead of dropping the creature to "unresolved"; or (b) use the authored `local_bound_center`/`local_bound_radius` directly for skinned submeshes rather than raw bind-pose vertex positions — preserves the "conservative coarse box" intent without trusting bind-pose extremities as representative.

## Completeness Checks
- [ ] **TESTS**: A regression test constructs an `ImportedMesh` with `skin: Some(...)` through `synthesize_packed_havok_proxy` and confirms it either falls back to the local bound or is excluded
- [ ] **CANONICAL-BOUNDARY**: The fallback fix stays a spawn-time decision (per-REFR, once), not re-derived per-draw
