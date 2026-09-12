# PERF-D2-2026-09-11-03: `build_instance_map` heap-allocates a fresh `Vec<Option<u32>>` per frame — the one per-frame render buffer with no scratch amortization

Labels: low,performance,renderer,bug

**Description**: Every neighboring per-frame buffer on this path (`gpu_instances_scratch`, `batches_scratch`, `indirect_draws_scratch`, `terrain_tile_scratch`, etc.) is a persistent field taken via `mem::take` and restored — the #243 amortization convention. `instance_map` is the outlier: allocated fresh and dropped every frame, sized to the full draw-command count.

**Evidence**:
`crates/renderer/src/vulkan/context/begin_frame_recording.rs:149-157`, allocation at `acceleration/predicates.rs:310`.

**Impact**: ~32 KB malloc/free pair per frame at the FO4 baseline (3949 draw commands); microseconds, reported because the violated convention is explicit and enforced everywhere else on the same function.

**Related**: #243 (the amortization convention this violates).

**Suggested Fix**: Add `instance_map_scratch` to `VulkanContext`, give `build_instance_map` an `out: &mut Vec<_>` variant, thread it through `BeginFrameOutput` as a borrow or taken/returned scratch.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_PERFORMANCE_2026-09-11.md`.*
