# PERF-D2-2026-09-11-02: `preserve_opaque_gbuffer` is a `PipelineKey` axis with no corresponding `draw_sort_key` slot

Labels: low,performance,renderer,bug

**Description**: `PipelineKey::Blended` has four axes (`src`, `dst`, `wireframe`, `preserve_opaque_gbuffer`), each mapping to a distinct `VkPipeline`, but `draw_sort_key` only carries slots for the first three (wireframe was added by #1806 for exactly this reason). Two draws agreeing on every sorted axis but differing in `preserve_opaque_gbuffer` can interleave and ping-pong `cmd_bind_pipeline`. Narrow in practice: the dominant alpha-blend population takes the depth-primary branch where this wouldn't help; only the additive/`no_sorter` branches would benefit, and their affected population (`is_refractive_glass && (additive || NoSorter)`) is plausible but unmeasured. Sibling of OPEN #2764, which covers the batch-merge-key half of the same field.

**Evidence**:
`crates/renderer/src/vulkan/context/build_and_upload_instances.rs:474-486` vs `byroredux/src/render/mod.rs:752-841`.

**Impact**: Two draws differing only in `preserve_opaque_gbuffer` can interleave in the sorted draw list, causing extra pipeline binds. Not measured; narrow affected population.

**Related**: #2764 (OPEN — the batch-merge-key half of the same field).

**Suggested Fix**: Add `is_refractive_glass(cmd) as u32` after `dst_blend` in the additive and `no_sorter` branches only, not the depth-primary branch.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
