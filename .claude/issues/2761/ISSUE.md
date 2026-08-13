# REN-D11-2026-08-12-05: pipeline.rs UI builder comment sizes Vertex at stale 100 bytes

## Description
Comment sizes `Vertex` at 100 bytes; it has been **104** since the vertex colour widened `vec3 → vec4` (`cd2b5fe4`), and `vertex_size_matches_attribute_stride` asserts 104. `vertex.rs`'s own `UiVertex` doc already says 104.

## Location
`crates/renderer/src/vulkan/pipeline.rs` (UI pipeline builder)

## Severity / Domain / Type
low / renderer / documentation

https://github.com/matiaszanolli/ByroRedux/issues/2761

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D11-2026-08-12-05).
