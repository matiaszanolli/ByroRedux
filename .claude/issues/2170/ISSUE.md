# 2170: D6-01: Skinned-vertex output buffer stores the full 104-byte Vertex when only the 12-byte position lane is ever read

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/2170
**Labels**: bug, medium, performance

---

## Severity
MEDIUM

## Dimension
Skinning & BLAS Cost (Dim 6) — `/audit-performance` 2026-07-25

## Location
`crates/renderer/src/vulkan/skin_compute.rs:402`, `crates/renderer/shaders/skin_vertices.comp:164-202`, `crates/renderer/src/vulkan/acceleration/blas_skinned.rs:82,504`

## Description
Each `SkinSlot` allocates `vertex_count x 104 B` of DEVICE_LOCAL memory, and `skin_vertices.comp` writes all 26 floats per vertex (position, skinned normal, skinned tangent, plus 17 floats of verbatim pass-through: colour RGBA, UV, bone indices/weights, splats, bitangent sign). The only consumer in the codebase is the acceleration-structure build, which reads the buffer as `R32G32B32_SFLOAT` at `vertex_stride = size_of::<Vertex>()` — i.e. touches 12 of every 104 bytes. RT hit shading samples the bind-pose global vertex SSBO, not this slot output; nothing reads the skinned normal/tangent or the pass-through lanes.

Confirmed against current code: `size_of::<Vertex>() == 104` (`crates/renderer/src/vertex.rs:320`, test-pinned).

## Evidence
Exhaustive grep for `output_buffer` across the renderer + binary crates yields exactly four non-doc uses — allocation, destruction, the descriptor write, and the two AS-build call sites (first-sight BUILD, refit UPDATE) — both going through `vertex_format(R32G32B32_SFLOAT)`. The shader's own comment states the pass-through exists because "Phase 3 (vertex shader reads pre-skinned) needs every field present" — but Phase 3 is explicitly deferred, and `create_slot` deliberately omits `VERTEX_BUFFER` usage for exactly that reason (#681/MEM-2-6, closed), so the buffer is provisioned for a consumer that does not exist and cannot be bound without a usage-flag change anyway.

## Impact
Two costs, both scaling with skinned-entity count: (1) VRAM — 8.7x over-allocation per slot; at a conservative 2K verts/sub-mesh and the ~1040 distinct `SkinnedMesh` allocation attempts/frame telemetered on FNV Atomic Wrangler peak, this is ~216 MB of slot output buffers where ~25 MB would serve, against the ~4 GB total budget target. (2) Bandwidth — every non-skipped dispatch writes 104 B/vertex instead of 12 B, the dominant traffic of the skin pass on a moving-crowd frame with the #1195 dirty-gate open. Neither is a correctness risk.

## Related
#681/MEM-2-6 (closed, the paired decision to omit `VERTEX_BUFFER` usage); #1797 (closed, the other unmeasured skin-pass throughput ceiling — both want the same moving-crowd bench); `docs/engine/memory-budget.md`.

## Suggested Fix
Narrow the slot output buffer to a positions-only layout (stride 12-16 B), drop the pass-through writes from `skin_vertices.comp`, and pass the matching `vertex_stride` to both AS-build sites. Keep the change behind the same commit that would otherwise land Phase 3, or explicitly retire Phase 3 in the shader's header comment so the provisioning rationale doesn't silently outlive its plan. Quantify with the existing `skin.coverage`/`gpu_skin_dispatch_ms` hooks on the same moving-crowd bench #1797 needs.

## Completeness Checks
- [ ] **TESTS**: A regression test pins the narrowed vertex stride at both AS-build call sites
- [ ] **SIBLING**: skinned normal/tangent removal checked against every other consumer of the slot output buffer, not just the two AS-build sites
