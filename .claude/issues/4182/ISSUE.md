# CONC-D2-02: Five HOST-barrier comments assert a false spec premise ("required even for HOST_COHERENT memory")

Labels: low,sync,doc-rot,documentation

**Description**: Three sites emit `HOST/HOST_WRITE -> {VERTEX,FRAGMENT,COMPUTE,DRAW_INDIRECT}` barriers whose comments assert the barrier is required by the Vulkan spec even for `HOST_COHERENT` memory. That premise is backwards: Vulkan's host-write ordering guarantee (Sec 7.9) puts host writes flushed before `vkQueueSubmit` into the first access scope of an implicit dependency covering the whole submitted batch. Every write here is a mapped write performed during recording, strictly before submit, so it's already visible without the barrier. This is a documentation/rationale defect, not a sync defect — **do not remove the barriers**, only correct the comment.

**Evidence**:
Sites: `context/build_and_upload_instances.rs:984-1004`, `context/dispatch_skin_and_cluster.rs` (cluster-cull HOST barrier), `volumetrics.rs:1170-1181`. All buffers involved (`SceneBuffers` SSBOs/UBOs, SVGF/TAA/bloom/composite param UBOs, `VolumetricsParams`) are written via `write_mapped`/`write_mapped_prefix` on host-visible memory earlier in the same `draw_frame` call, before `queue_submit`; no path writes them from a separate host thread concurrently with an in-flight batch.

**Impact**: None at runtime — harmless over-synchronization. The cost is that these load-bearing-for-audits comments state an incorrect rule; a future audit reasoning from the stated premise could add unwarranted HOST barriers elsewhere, or conclude a genuinely missing edge is present when it isn't. No GPU run needed to verify — Vulkan 1.3 spec Sec 7.9 'Host Write Ordering Guarantees' is the reference.

**Related**: #909, #961, #1397 (the folds that produced these comments).

**Suggested Fix**: Comment-only — reword to the accurate rationale (defense against a future non-coherent memory type or post-recording host write) rather than asserting the spec requires it. Do not remove the barriers.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_CONCURRENCY_2026-09-11.md`.*
