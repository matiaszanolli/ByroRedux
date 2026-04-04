# Investigation: Issue #25

## Root Cause
BLAS and TLAS creation in acceleration.rs uses `GpuBuffer::create_host_visible`
for result buffers (lines 114, 297) and scratch buffers (lines 135, 319).
These buffers are:
- **Result buffers**: written by GPU (AS build), read by GPU (ray query) — never CPU-accessed
- **Scratch buffers**: written and read by GPU during build only — never CPU-accessed

Using CpuToGpu (HOST_VISIBLE) places these in system RAM on discrete GPUs,
forcing AS traversal to go over PCIe — significant performance penalty.

## Instance buffer
The instance buffer (per-frame CPU write via write_mapped) correctly uses
create_host_visible — it's the one buffer that SHOULD be HOST_VISIBLE.

## Fix
1. Add `GpuBuffer::create_device_local_uninit()` — allocates GpuOnly memory
   without staging (no initial data needed, GPU builds fill them)
2. Change result buffer allocations (lines 114, 297) to use it
3. Change scratch buffer allocations (lines 135, 319) to use it
4. Keep instance buffer as create_host_visible (correct)

## Scope
2 files: buffer.rs (add method), acceleration.rs (4 call sites).
