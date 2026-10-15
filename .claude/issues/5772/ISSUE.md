# REN-D8-2026-10-14-01: volumetrics init: host-mapped `moment_buffer`/`occupancy_buffer` locals are outside `partial` when their fill/flush fails

**Labels**: low,renderer,vulkan,memory,bug
**Source**: docs/audits/AUDIT_RENDERER_2026-10-14.md

Found by `/audit-renderer` (docs/audits/AUDIT_RENDERER_2026-10-14.md). Finding ID: `REN-D8-2026-10-14-01`. Status: NEW, confirmed against HEAD `adbd468e2`.

**Location**: `crates/renderer/src/vulkan/volumetrics/init.rs` (combustion moment and occupancy buffer setup, `try_or_cleanup!` blocks)

## Description
`moment_buffer`/`occupancy_buffer` are created, then zero-filled and flushed in a `try_or_cleanup!` block, and only pushed into `partial.combustion_light_moment_buffers`/`combustion_occupancy_buffers` afterwards. If the fill or flush fails, the local is not in `partial`. Pre-existing (unchanged by #5593).

## Evidence
`init.rs` ~l.384-430. Not a leak: `GpuBuffer::Drop` (#656) frees the buffers with a warning. The step is effectively infallible.

## Impact
A debug-build `debug_assert!` in the Drop safety net would panic and mask the `Err` on an effectively unreachable path.

## Suggested Fix
Push each buffer into `partial` before filling it.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
