# #289: P2-01: TLAS instance buffer HOST_VISIBLE — GPU reads traverse PCIe on discrete

## Finding
**Severity**: MEDIUM | **Dimension**: GPU Memory | **Type**: performance
**Location**: `crates/renderer/src/vulkan/acceleration.rs:663-669`
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-04-13.md`

## Description
Instance buffer for AS build is host-visible. On discrete GPUs, GPU reads from BAR memory traverse PCIe (10-30x slower than VRAM). At 8192 instances × 64B = 512KB.

## Impact
~0.05-0.1ms per TLAS build. Minimal at current interior sizes but grows with exterior cells (5000+ instances).

## Fix
Double-buffer: write to host-visible staging, cmd_copy_buffer to device-local before AS build. Record the copy in the same command buffer.

## Completeness Checks
- [ ] **DROP**: Verify device-local buffer destroyed before allocator
- [ ] **SIBLING**: Check other host-visible buffers (scene_buffer SSBOs) — those use mapped writes which is correct
