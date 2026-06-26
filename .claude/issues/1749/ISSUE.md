# TD1-004: VulkanContext::new() is a 1025-LOC constructor

_Filed 2026-06-26 as #1749 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1749` for live state)._

**Severity**: LOW · **Dimension**: 1 — Function Complexity
**Location**: `crates/renderer/src/vulkan/context/mod.rs:1427-2451` (1025 LOC)
**Status**: NEW (#1670 tracks a *different* constructor, `App::new`)
**Audit**: TD1-004. Also subsumes the `context/mod.rs` file-size finding (TD1-003) — the file shrinks once `new()` is extracted.

## Description
`VulkanContext::new()` builds the whole Vulkan init chain inline (entry → instance → debug → surface → device → allocator → swapchain → render pass → pipelines → framebuffers → command pool → sync → all optional passes svgf/ssao/taa/bloom/water/volumetrics). 1025 LOC in one function.

## Impact
Hard to follow which sub-result feeds which; any new pass appends another ~30-line block. The 3275-LOC `context/mod.rs` is large mostly because of this constructor + the struct definition.

## Suggested Fix
Extract `build_core_device` / `build_swapchain_and_passes` / `build_pipelines` / `build_optional_passes` helpers along the documented init phases (CLAUDE.md invariant #6); assemble in `new`. Do together with relocating `new()` into a `context/init.rs` submodule (and `impl Drop` into `context/teardown.rs`). Mechanical — preserve order.

## Completeness Checks
- [ ] **DROP**: init/teardown ordering preserved verbatim; Drop still reverse-order correct
- [ ] **TESTS**: engine still boots (`cargo run`); Vulkan validation layers clean in debug
