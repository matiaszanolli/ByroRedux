# #5062 — CONC-D2-2026-09-29-01: ReSTIR reservoir clear publishes to FRAGMENT `SHADER_WRITE` only; sync validation reports READ_AFTER_WRITE at every draw

**Labels**: high,sync,vulkan,renderer,bug

**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-09-29.md`
**Severity**: HIGH
**Dimension**: Compute → AS → Fragment Chains (TRANSFER → FRAGMENT)

## Location
- `crates/renderer/src/vulkan/restir.rs` — `ReservoirBuffers::begin_frame` (the `after` barrier: `TRANSFER_WRITE → SHADER_WRITE`, `TRANSFER → FRAGMENT_SHADER`)
- called from `crates/renderer/src/vulkan/context/begin_frame_recording.rs`
- `crates/renderer/shaders/include/bindings.glsl` — `ReservoirCurrBuffer` (set 1 binding 16, no `writeonly`), written in `crates/renderer/shaders/triangle.frag`

## Description
`begin_frame` clears this frame's reservoir slot with `vkCmdFillBuffer`, then publishes the clear with `TRANSFER_WRITE → SHADER_WRITE`, `TRANSFER → FRAGMENT_SHADER`. The buffer is declared read-write in GLSL, so Synchronization Validation classifies the main pass's use of binding 16 as `FRAGMENT_SHADER_SHADER_STORAGE_READ`, which the barrier's dst access does not cover. The fill → store WAW ordering is correct as written; the underlying data hazard is probably nil (the shader only stores), but the validator sees a non-`writeonly` storage block.

## Evidence
CI job 109545444434 (run 36609043348 @ `9fcfdc3fc`; also on 36574364470 @ `8b334c102`), 20 errors across both FIF slots:
```
[ SYNC-HAZARD-READ-AFTER-WRITE ] … vkCmdDrawIndexed(): Hazard READ_AFTER_WRITE for VkBuffer 0x1b900000001b9[] …
type: VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, binding #16 index 0. Access info (usage:
SYNC_FRAGMENT_SHADER_SHADER_STORAGE_READ, prior_usage: SYNC_COPY_TRANSFER_WRITE, write_barriers:
SYNC_FRAGMENT_SHADER_SHADER_STORAGE_WRITE|…, command: vkCmdFillBuffer, seq_no: 2, reset_no: 2).
```
```rust
let after = [buffer_barrier(self.curr_buffer(frame))
    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
    .dst_access_mask(vk::AccessFlags::SHADER_WRITE)];
```
Code arrived in `186234944` (ReSTIR light identity) while the lane could not reach a device.

## Impact
Every frame, every scene. The `vulkan-validation` lane stays permanently red, so a real hazard introduced later is invisible in it. If any fragment path ever loads from `reservoirsCurr` (e.g. future in-frame spatial reuse), that load would be a real RAW with no memory dependency.

## Related
CONC-D2-2026-09-29-02 (#5064) (the other red), #4987 (lane can close once both are fixed), #2152.

## Suggested Fix
Add `vk::AccessFlags::SHADER_READ` to the `after` barrier's dst access. Also declare the block `writeonly` if nothing reads it. Confirm on the next lane run that binding #16 goes quiet.

Validated at HEAD 9fcfdc3fc: `restir.rs` `after` barrier still `.dst_access_mask(vk::AccessFlags::SHADER_WRITE)` only; `ReservoirCurrBuffer` still declared without `writeonly`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SHADER**: If `bindings.glsl` changes, the standalone GpuInstance/bindings copies stay in lockstep and SPIR-V is recompiled
- [ ] **TESTS**: A regression test pins this specific fix
