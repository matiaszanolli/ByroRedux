# PRDR2-2026-11-03-06: A queued cubemap's reserved slot is never redirected to a cube fallback, so env-mapped surfaces sample an unwritten descriptor during yielded cell loads

Labels: bug, renderer, medium, vulkan, safety

**Severity**: MEDIUM (a live draw samples an unwritten `PARTIALLY_BOUND` descriptor: undefined behaviour and a validation error, on a normal cell-load path)
**Dimension**: Renderer / Texture registry
**Source**: raised by the texture-streaming PRD review round 2 (2026-11-03) as unchecked; traced and confirmed at HEAD `eba979dfb`. Same class as #4885, which fixed only the `recreate_descriptor_sets` path.
**Status**: NEW

## Location
`crates/renderer/src/texture_registry/upload.rs:242` (`enqueue_dds_for_view`):

```rust
if view_kind == TextureViewKind::D2 && matches!(outcome, EnqueueOutcome::Reserved(_)) {
    // redirect the reserved slot's descriptor to the 2D fallback
```

A freshly reserved **Cube** slot gets no redirect, so its binding-1 descriptor (`samplerCube[max_textures]`, `PARTIALLY_BOUND`, `texture_registry/mod.rs:401-405`) stays unwritten until `flush_pending_uploads` runs.

## Description
The reserve-then-flush split (#881) leaves a window by design. The D2 redirect exists because "any `GpuInstance.texture_index` that resolves to this handle before the flush samples the checkerboard instead of an unbound descriptor" (upload.rs:235-241). The window spans frames:
- Yielded cell loads flush only past a threshold: `should_flush_pending_cell_textures` needs `force`, 64 queued uploads (`YIELDED_TEXTURE_UPLOAD_BATCH_MIN`, `cell_loader/references/mod.rs:1028`) or `MAX_UPLOAD_BATCH_BYTES`. Callers: `cell_loader/load.rs:699` and `cell_loader/exterior/persistent.rs:140,153`. Frames render between yields.
- Env-map cubemaps are queued on that path: `asset_provider/texture.rs:795` (`enqueue_cubemap_dds_with_clamp` for secondary texture slots with `cubemap = true`).
- `triangle.frag` samples the cube when `mat.envMapIndex != 0u` (e.g. `:3083`).

So an env-mapped surface spawned during a yielded load reads an unwritten cube descriptor for every frame until the flush. A flush that fails (#1922) leaves the slot unwritten for good.

## Impact
Undefined sampling results (garbage or black reflections, or a fault on stricter drivers), plus a validation-layer error under `BYRO_VALIDATION=1`, on Oblivion/FO3/FNV/Skyrim shiny surfaces during streaming and interior loads.

## Suggested Fix
Mirror the D2 redirect for `Cube` using `cube_fallback_handle`, the same dimension-matched choice `slot_rewrite` makes (`texture_registry/mod.rs:1008-1010`). That handle is `None` until the first cubemap uploads, so also cover that case: create an engine-owned 1×1 cube fallback at registry init, or hold `envMapIndex` at 0 for a slot whose texture is still `None`. Pick one and say why in the commit.

## Completeness Checks
- [ ] **REDIRECT**: a reserved Cube slot's descriptor points at a cube fallback before `enqueue_dds_for_view` returns
- [ ] **NO_FALLBACK_YET**: the first cubemap of a session is covered too
- [ ] **SIBLING**: `drop_released_texture` and `recreate_descriptor_sets` already dimension-match (#4885); confirm nothing else writes D2-only
- [ ] **TESTS**: a registry test that enqueues a cubemap and checks the reserved slot's pending descriptor write targets binding 1 with the cube fallback
- [ ] **VALIDATION**: an env-mapped interior (e.g. an FNV casino interior) loaded with `BYRO_VALIDATION=1` shows no unwritten-descriptor error during the yielded load

