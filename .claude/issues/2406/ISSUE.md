# TD1-003: impl Drop for VulkanContext — 343 LOC, cognitive complexity 36/25

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2406
**Finding ID**: TD1-003 (source: `docs/audits/AUDIT_TECH-DEBT_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 1 — File / Function / Module Complexity
**Location**: `crates/renderer/src/vulkan/context/mod.rs:3363-3705`
**Status**: NEW

## Description
`impl Drop for VulkanContext` is 343 LOC, cognitive complexity 36/25. It mirrors the reverse-order teardown of `VulkanContext::new` (TD1-002). Lower priority than the constructor split — a flat, explicitly-ordered destroy sequence is the *correct* shape for Vulkan teardown; over-abstracting `Drop` risks hiding ordering bugs.

## Suggested Fix
Do not extract opaque helpers that hide ordering. If split at all, only pair extracted helpers with existing `new_inner`/`destroy` pairs from sibling subsystems (composite/volumetrics/svgf/ssao/bloom/taa). Low priority, opportunistic alongside the `VulkanContext::new` split.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: Engine still boots (`cargo run`) and validation layers remain clean in debug after any split
