# REN-D11-2026-08-12-01: refractive glass's mesh-ID write is masked off, invisible to its own caustic gate

**Severity**: HIGH
**Dimension**: 11 — Pipeline/RenderPass
**Location**: `crates/renderer/src/vulkan/pipeline.rs` (`create_blend_pipeline`, `preserve_opaque_gbuffer`); `crates/renderer/src/vulkan/context/draw.rs` (`is_refractive_glass`/`is_caustic_source`); `crates/renderer/shaders/caustic_splat.comp`

## Description

`c615f8de` (2026-08-11) added `preserve_opaque_gbuffer`, which masks off the mesh-ID write (attachment 3) for refractive glass. `is_caustic_source` is literally `is_refractive_glass` — the same predicate. So the population tagged `INSTANCE_FLAG_CAUSTIC_SOURCE` is exactly the population whose mesh-ID write is now discarded. Producer and consumer sets are provably disjoint.

## Evidence

```rust
fn is_caustic_source(cmd: &DrawCommand) -> bool { is_refractive_glass(cmd) }
PipelineKey::Blended { …, preserve_opaque_gbuffer: order_dependent_glass }
```
`pipeline.rs:654` masks attachment 3 to `no_write` when `preserve_opaque_gbuffer`. Three live docs (`triangle.frag`'s `stableSurfaceId` comment, `gbuffer.rs::MESH_ID_FORMAT`, `shader-pipeline.md`) still describe the old contract.

## Impact

The glass-side caustic pass (#321/M22, live since `91638ec4`) receives zero splats on every frame in every cell, while still paying its full dispatch cost. Water-side caustics unaffected. Compounds with a second, independent mechanism filed separately (the CPU gate never requires alpha-blend while the GPU gate requires it) — fixing either alone leaves the pass dark.

## Suggested Fix

Either keep mesh-ID writable for the glass pipeline and solve "caustics through walls" in the splat shader's depth/geometry gate, or retire the alpha-draw mesh-ID representation with an explicit source list — then update all three docs together.

## Related

`883f57cd`, #321, #992, #2468

Filed from `docs/audits/AUDIT_RENDERER_2026-08-12b.md` (finding REN-D11-2026-08-12-01, Cluster B).
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2745
