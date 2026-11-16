# REN-D5-02: no pin that `TextureRegistry::destroy` drains `first_level_tables`

Labels: low, renderer, memory, test-gap, bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/7028

**Severity**: LOW
**Source**: `docs/audits/AUDIT_RENDERER_2026-11-16.md` (renderer audit, delta since 2026-11-09)
**Location**: `first_level_tables: Vec<GpuBuffer>` and `destroy` (`crates/renderer/src/texture_registry/mod.rs`); contrast `crates/renderer/src/vulkan/scene_buffer/destroy_coverage_tests.rs`
**Status**: NEW (validated against HEAD; no matching open issue)

## Description
Same shape as #6521: a missed per-frame buffer family leaks the allocator `Arc`, so shutdown's `Arc::try_unwrap` fails and the device is deliberately leaked. The code is correct today (`destroy` drains the table); nothing would catch a regression.

**Impact**: test gap.

## Suggested Fix
A `production_text` scan asserting every `Vec<GpuBuffer>` field `TextureRegistry` declares is drained in `destroy`.

Related: #6521, M39 `f754eee28`.
## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
