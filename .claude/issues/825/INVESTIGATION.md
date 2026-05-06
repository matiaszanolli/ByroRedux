# Investigation — #825

**Domain**: ecs

## Code path

`make_transform_propagation_system` (`crates/core/src/ecs/systems.rs:41-144`) returns a `FnMut` closure with captured `roots: Vec<EntityId>` and `queue: VecDeque<EntityId>`. Phase 1 iterates every Transform entity (~6 000 on Megaton, ~30 000 on FNV exterior r=3) calling `parent_q.get(entity).is_none()` to identify roots. The root set is nearly static — changes only on top-level spawn/despawn.

## Approach

Audit suggested two fixes:
1. **Architectural**: `RootEntities: Resource<HashSet<EntityId>>` maintained by spawn/despawn hooks
2. **Interim**: cache root set in closure, invalidate on `Transform::len() != last_seen`

Going with the interim. Resource-based fix touches the spawn/despawn API surface across the engine and is overkill for a 250 µs/frame win. The interim is local to the system and preserves correctness through a generation key.

## Cache key

`(Transform::len(), Parent::len(), World::next_entity_id())`

- `Transform::len()` catches insert/remove of Transform.
- `Parent::len()` catches insert/remove of Parent (changes root status of an entity that already had Transform).
- `next_entity_id()` (monotonic) catches the **despawn + spawn in same frame** edge case where t_len/p_len happen to match exactly. Without this, a respawn could leave the cached `roots` containing a dead ID and missing the new one for one frame.

All three are O(1) via `QueryRead::len()` and `World::next_entity_id()` (already public, used by the existing `NameIndex` generation pattern).

## Companion #826

Same anti-pattern in `world_bound_propagation_system` (`byroredux/src/systems.rs:921-935`) — different storage (GlobalTransform) but identical fix shape. Out of scope for this fix per the user's single-issue request; will follow up on `/fix-issue 826`.

## Scope

1 file: `crates/core/src/ecs/systems.rs`
