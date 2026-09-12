# CONC-D2-01: Ground-cover counter readback copies a buffer the scatter dispatch just wrote, with no COMPUTE→TRANSFER dependency

Labels: medium,sync,renderer,terrain-exterior,bug

**Description**: `GroundCoverPipeline::record_scatter` dispatches `groundcover_scatter.comp`, which writes `counter_buffer` via atomics, then emits exactly one publish barrier and immediately `vkCmdCopyBuffer`s from that same buffer into the per-FIF `counter_readback[frame]`. The barrier's dst scope is `DRAW_INDIRECT | VERTEX_SHADER` / `INDIRECT_COMMAND_READ | SHADER_READ` — confirmed by direct read — it names neither `TRANSFER` stage nor `TRANSFER_READ` access, so the copy's read has no dependency on the compute write.

**Evidence**:
```rust
// groundcover.rs:1231-1239 (confirmed present)
buffer_barrier(
    device, cmd,
    vk::PipelineStageFlags::COMPUTE_SHADER, vk::AccessFlags::SHADER_WRITE,
    vk::PipelineStageFlags::DRAW_INDIRECT | vk::PipelineStageFlags::VERTEX_SHADER,
    vk::AccessFlags::INDIRECT_COMMAND_READ | vk::AccessFlags::SHADER_READ,
);
device.cmd_copy_buffer(cmd, counters.buffer, self.counter_readback[frame].buffer, &[..]);
```
The two sibling edges in the same file are correct by contrast (`groundcover.rs:1186-1193`, `:1306-1314`).

**Impact**: `GroundCoverPipeline::harvest` decodes the readback into `GroundCoverStats` (blade counts, density histogram, extrema) — EXAL ground-cover density tuning reads these numbers directly. Rendering itself is unaffected (the draw uses the correctly-published edge); failure mode is silently wrong telemetry driving tuning decisions, plus sync-validation noise. Verify with `BYRO_VALIDATION=1` on an exterior cell with ground cover active.

**Related**: #4054 (EXAL ground cover), #4056 (EXAL ground cover Phase 3, OPEN).

**Suggested Fix**: Widen the trailing barrier's dst scope to add `TRANSFER`/`TRANSFER_READ`, or emit a second `COMPUTE_SHADER/SHADER_WRITE -> TRANSFER/TRANSFER_READ` edge before the copy. Purely additive — same class of change #2403 made for the skinned-vertex publish mask. Do not ship unvalidated.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_CONCURRENCY_2026-09-11.md`.*
