# REN-D10-03: GpuCamera doc: stale triangle.vert consumer credit, ambiguous position frame

## Description
The "Consumers (#1492)" list credits `triangle.vert` with the absolute reconstruction; since #1496 that moved to `triangle.frag`, which the list omits entirely despite now being the busiest consumer (absolute reconstruction, `camRel` soft-particle rebase, `renderOrigin.w` FSR-reset view). `position` is documented "xyz = world position" where the rest of the block carefully distinguishes frames — it is ABSOLUTE.

## Location
`crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`GpuCamera::render_origin`, `::position`)

## Severity / Domain / Type
low / renderer / documentation

https://github.com/matiaszanolli/ByroRedux/issues/2753

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D10-03).
