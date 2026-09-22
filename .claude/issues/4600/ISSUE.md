# SAFE-D4-2026-09-21-02: Three SAFETY / `# Safety` texts restate a pre-refactor invariant

**Labels**: low, safety, renderer, doc-rot, documentation

Filed via /audit-publish from docs/audits/AUDIT_SAFETY_2026-09-21.md.

**Severity**: LOW · **Dimension**: 4 — Unsafe-block discipline (truth of stated invariants)
**Location**:
- `crates/renderer/src/vulkan/frame_upscaler.rs`: `record_native_blit`'s `# Safety` doc (~:691-700) and its first inner `// SAFETY:` (~:746-748)
- `crates/renderer/src/vulkan/context/skinned_blas_refit.rs`: the skin-dispatch `// SAFETY:` (~:500)
- `crates/renderer/src/vulkan/scene_buffer/upload.rs`: `upload_terrain_tiles`' staging `// SAFETY:` (~:1049-1052)

**Status**: NEW
**Verified against**: HEAD `f97775ca8`

## Description

- **`record_native_blit`.** Its `# Safety` says "`scene_color` must be in `SHADER_READ_ONLY_OPTIMAL` (composition's output layout)", and the inner SAFETY says "scene composition left `scene_color` shader-readable". Since #3572 the function takes `source_layout`, and the TAA path legitimately passes `GENERAL`. This stale contract is part of why SAFE-D1-2026-09-21-01 (#4592)'s wrong argument reads as plausible.
- **Skin dispatch.** The SAFETY says "Each `dispatch` binds the compute pipeline + slot set at the COMPUTE bind point". Since #4205 the pipeline is bound once per batch through `SkinComputePipeline::bind`, and `slot_dispatch_does_not_rebind_the_pipeline_per_entity` (`skin_compute.rs`) pins that `dispatch` no longer binds it.
- **Terrain staging.** The SAFETY says "GpuTerrainTile is #[repr(C)] with u32-only fields matching std430". Since #4057 the struct also has `f32` lanes: `cover_affinity0` / `cover_affinity1: [f32; 4]` and `cell_origin_xz: [f32; 2]`.

## Evidence

The quoted texts at the locations above, compared with the code they describe:
- `record_native_blit(…, source_layout, output_layout)`;
- `SkinComputePipeline::bind` / `dispatch`;
- `GpuTerrainTile` in `scene_buffer/gpu_types.rs`.

## Impact

- At the second and third sites this is documentation only: the property the code actually relies on still holds. One pipeline bind precedes the dispatches, and `GpuTerrainTile` is a `#[repr(C)]` POD whose size `gpu_terrain_tile_is_160_bytes` pins.
- At the first site the stale text hides a live bug (SAFE-D1-2026-09-21-01 (#4592)).

## Related

- SAFE-D1-2026-09-21-01 (#4592)
- #3572, #4205 and #4057 (closed): the changes these texts did not follow.
- #3597 and #3583 (closed): earlier instances of this class.

## Suggested Fix

Restate each text against the current code:
- **Blit:** the `source_layout` / `output_layout` contract.
- **Skin loop:** one lazy pipeline bind per batch, plus a per-slot descriptor bind.
- **Terrain copy:** a `#[repr(C)]` POD with `u32` and `f32` lanes.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D4-2026-09-21-02)

## Completeness Checks
- [ ] **UNSAFE**: each restated SAFETY names the invariant the block relies on today
- [ ] **SIBLING**: the `// SAFETY:` comments at `record_native_blit`'s three call sites re-read against the restated contract
