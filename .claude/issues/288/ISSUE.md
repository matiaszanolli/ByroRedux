# #288: P6-05: sample_blended_transform iterates layers 3x per channel — 270 HashMap lookups/entity

## Finding
**Severity**: MEDIUM | **Dimension**: CPU Allocations (CPU time) | **Type**: performance
**Location**: `crates/core/src/animation/stack.rs:193-281`
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-04-13.md`

## Description
Three full passes over stack.layers per channel name: (1) find max priority, (2) compute total weight, (3) blend transforms. Each pass repeats registry.get() + clip.channels.get() HashMap lookups. For 3 layers × 30 channels = 270 HashMap lookups/entity/frame.

## Impact
CPU-bound on blended animation scenes. String-key hashing dominates.

## Fix
Fuse passes 1 and 2 — find max_priority and accumulate total_weight simultaneously using a running max. Reduces 3N to 2N layer iterations per channel.

## Completeness Checks
- [ ] **TESTS**: Existing animation blending tests must stay green
- [ ] **SIBLING**: Check sample_blended_float/color/bool for same pattern
