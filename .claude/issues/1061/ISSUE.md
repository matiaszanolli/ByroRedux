# Issue #1061: Vulkan MemoryBarrier consolidation — 13 inline barrier sites

**Domain**: renderer
**Status**: FIXED

Added `memory_barrier(device, cmd, src_stage, src_access, dst_stage, dst_access)`
thin wrapper to `descriptors.rs`. Replaced all 13 inline barrier sites across 8 files.
Zero semantic changes — all flags remain identical at call sites.
