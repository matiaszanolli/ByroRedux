# #6292: ECS-2026-10-21-06: `ArchetypeIdleSeen.unmatched` grows for the whole session

**Labels**: low,ecs,animation,bug

**Source**: docs/audits/AUDIT_ECS_2026-10-21.md · **Severity**: LOW · **Dimension**: 4 Resources & World-Level State · **Location**: `byroredux/src/systems/archetype_idle.rs:35-48`

- **Location**: `byroredux/src/systems/archetype_idle.rs:35-48` (the set and `note_unmatched`), `:56-63` (the only `clear`, taken when `AnimArchetypeOverrides` changes)
- **Status**: NEW
- **Description**: Every FO4 actor that has no archetype entry or no clip is inserted into a `HashSet<EntityId>` so it is not re-examined each frame. Entity ids are never recycled and the set is cleared only when a script changes the archetype overrides. It therefore keeps every unmatched actor ever streamed in, despawned ones included.
- **Impact**: About 8 bytes per unmatched actor spawn, unbounded over a long streaming session. The per-frame cost is still an O(1) probe. The finding is the pattern: a per-entity fact kept in a global map that despawn cannot reclaim.
- **Suggested Fix**: `retain` the set against the live `AnimationTarget` storage when its `structural_generation` changes, or replace it with a sparse `ArchetypeIdleUnmatched` marker component.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
