# #1805: D2-NEW-04: Static-mesh hot loop pays a redundant GlobalTransform re-probe and a late IsFxMesh gate

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D2-NEW-04)
**Location**: `byroredux/src/render/static_meshes.rs:147,175,280`

## Description
Two skip-ordering inefficiencies in the per-entity draw enumeration: the #1377 hoist
probes `tq.get(entity).is_none()` (`:147`) then re-fetches the same component at
`:175`, two storage lookups per drawn entity where one binding would do; and the
`IsFxMesh` skip fires only after ~12 optional-component gets and the frustum-sphere
test, all discarded for FX entities.

## Suggested Fix
Replace the two-step probe with a single
`let Some(transform) = tq.get(entity) else { continue; };`, and hoist the `fx_q`
gate immediately after the visibility skip.

---

# #1806: D2-NEW-05: draw_sort_key omits the wireframe pipeline axis — sort key and PipelineKey no longer in lockstep

**Severity**: LOW
**Source**: `AUDIT_PERFORMANCE_2026-07-02.md` (D2-NEW-05)
**Location**: `byroredux/src/render/mod.rs:192-240`; `crates/renderer/src/vulkan/pipeline.rs:114,121`
(`Opaque { wireframe }`, `Blended { …, wireframe }`)

## Description
#869 added `wireframe` to `PipelineKey`, making it a batch-merge and pipeline-bind
boundary, but `draw_sort_key` (a 10-tuple in both the alpha and opaque branches) was
never extended with a matching slot — neither branch references `cmd.wireframe`. A
wireframe draw interleaved among fill draws lands mid-run, splitting the instanced
batch and forcing extra pipeline binds.

## Impact
Near-zero on shipped content (`NiWireframeProperty` is essentially absent from real
assets) — a lockstep/hardening gap that becomes real if wireframe content (debug
modes, mods) ever coexists with fill geometry.

## Suggested Fix
Fold `wireframe` into an existing u8 slot (e.g. pack with `two_sided`), and extend
the sort-key/merge-axis lockstep test.
