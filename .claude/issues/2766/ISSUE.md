# REN-D12-2026-08-12-03: indirect_call_count overcounts on dispatch_direct early return

## Description
`indirect_call_count` is incremented (`+= 2` on the split branch, `+= 1` otherwise) even when `dispatch_direct` returned early without recording — missing mesh, or no per-mesh buffers (the #1370 global-only distant-LOD case). Makes the post-batch GPU-draw metric an upper bound rather than the actual count, on the `global_bound == false` path only.

## Location
`crates/renderer/src/vulkan/context/geometry_pass.rs` (`dispatch_direct` + its three call sites)

## Severity / Domain / Type
low / renderer / bug

https://github.com/matiaszanolli/ByroRedux/issues/2766

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D12-2026-08-12-03).
