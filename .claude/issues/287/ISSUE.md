# #287: P4-01: AnimationStack re-locked 4x per entity per frame

## Finding
**Severity**: MEDIUM | **Dimension**: ECS Query Patterns | **Type**: performance
**Location**: `byroredux/src/systems.rs:337-471`
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-04-13.md`

## Description
The AnimationStack processing loop acquires and drops the AnimationStack query lock 4 separate times per entity: (1) query_mut for advance_stack, (2) query for sampling channel names, (3) query for accum_root lookup, (4) query for dominant channel extraction. For N animated entities, 4N lock acquire/release cycles per frame.

## Impact
Measurable on scenes with 20+ AnimationStack entities.

## Fix
Merge the three read passes (2, 3, 4) into a single `query::<AnimationStack>()` per entity. Reduces 4N to 2N lock operations.

## Completeness Checks
- [ ] **LOCK_ORDER**: Verify merged query doesn't overlap with Transform write lock
- [ ] **TESTS**: Existing animation tests stay green
