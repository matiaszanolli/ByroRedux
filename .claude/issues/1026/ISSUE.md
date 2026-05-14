# Issue #1026

**Title**: F-WAT-05: WaterDrawCommand instance_index relies on no-resort-after-line-1437 — no assertion guards this

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-13.md` — F-WAT-05
**Severity**: MEDIUM (forward-compat trap)
**File**: `byroredux/src/render.rs:1437`

## Issue

`WaterDrawCommand.instance_index = idx as u32` contract relies on `draw_commands` not being re-sorted after line 1437. The `par_sort_unstable_by_key` upstream is well-defined today, but any future code that re-sorts post-1437 would silently desync `instance_index` from the actual GpuInstance SSBO slot.

## Fix

Either: (a) re-derive `instance_index` after the final sort by mapping by entity ID, or (b) add a `debug_assert!` immediately before the GPU upload comparing the recorded `instance_index` against the actual slot.

## Completeness Checks
- [ ] **SIBLING**: Same trap pattern on any other DrawCommand variant?
- [ ] **TESTS**: Synthetic regression injecting a re-sort after 1437 should panic in debug

