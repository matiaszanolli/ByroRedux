# TD3-002: bindings.glsl cites stale gpu_material_size_is_260_bytes test (real: _300_bytes)

_Filed 2026-06-26 as #1755 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1755` for live state)._

**Severity**: MEDIUM (lockstep-drift bait on the shader contract) · **Dimension**: 3 — Stale Documentation
**Location**: `crates/renderer/shaders/include/bindings.glsl:59-60`
**Status**: NEW · **Audit**: TD3-002

## Description
The shader-struct-sync comment reads: "…the size of this struct (300 B …) is pinned by `gpu_material_size_is_260_bytes` on the Rust side (the test name is historical / kept for grep continuity; it asserts 300)."

## Evidence
There is **no** `gpu_material_size_is_260_bytes` test. The real test is `gpu_material_size_is_300_bytes` (`crates/renderer/src/vulkan/material.rs:1202`); all 5 material.rs cross-refs already name it correctly. The comment's stated reason ("kept for grep continuity") is factually wrong — the test was renamed to track the size, not frozen at 260.

## Impact
This is the single-source-of-truth shader-side `GpuMaterial` declaration (post-`218b425b` split). The struct-sync invariant tells contributors to update the test in lockstep with the struct, then points them at a dead grep.

## Suggested Fix
Replace `gpu_material_size_is_260_bytes` → `gpu_material_size_is_300_bytes`; delete the "test name is historical / kept for grep continuity" clause (line 56-57 already records "300 B — was 260 B" correctly).

## Completeness Checks
- [ ] **SIBLING**: bindings.glsl GpuInstance/GpuCamera size cites are correct (verified: 112/336 OK)
- [ ] **TESTS**: `gpu_material_size_is_300_bytes` exists and asserts 300
