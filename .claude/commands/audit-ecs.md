---
description: "Deep audit of the ECS — storage backends, queries, world, systems, resources"
---

# ECS Audit

Read `_audit-common.md` and `_audit-severity.md` for shared protocol.

## Dimensions

### 1. Storage Correctness
- SparseSetStorage: swap-remove fixes sparse pointer for moved entity
- SparseSetStorage: insert into existing entity overwrites (no duplicate)
- PackedStorage: binary_search maintains sort invariant after insert/remove
- PackedStorage: insert at correct position, remove shifts correctly

### 2. Query Safety
- RwLockReadGuard lifetime doesn't outlive the query
- RwLockWriteGuard is exclusive (no double write-lock)
- query_2_mut / query_2_mut_mut: locks acquired in TypeId order
- Same-type double-lock: panics with clear message (not deadlock)
- QueryRead/QueryWrite Deref impls don't expose unsound references

### 3. World Integrity
- TypeMap: downcast_ref/downcast_mut always correct (TypeId matches)
- Lazy storage init: first insert creates storage, queries before insert return None
- Entity ID monotonic (no reuse, no overflow handling — document limitation)
- find_by_name: no string comparison in scan (FixedString integer equality only)

### 4. Resource Safety
- resource() panics with type name on missing resource
- try_resource() returns None (no panic)
- ResourceRead/ResourceWrite Deref impls type-safe
- Resources usable from systems (interior mutability via &self)

### 5. System & Scheduler
- Blanket impl for Fn(&World, f32) correct (no ownership issues)
- Mutations from system N visible to system N+1 in same run()
- Empty scheduler runs without panic
- system_names() returns correct order

### 6. Unsafe Code Review
- World::get() uses raw pointer to extend lifetime — is the safety argument valid?
- Any other unsafe blocks: document safety invariants

### 7. Streaming, Scripting & New Component Lifecycles
- M40 streaming (`byroredux/src/streaming.rs`): cell-load attaches components, cell-unload removes them — verify no orphaned components after a load/unload cycle
- M41 NPC spawn (`byroredux/src/npc_spawn.rs`): ACHR REFR → entity dispatch is idempotent (same REFR FormId never spawns twice)
- Scripting events (`crates/scripting/src/events.rs`): transient marker components (ActivateEvent, HitEvent, TimerExpired) are removed by `event_cleanup_system` (Late stage) — verify single-frame lifetime
- ScriptTimer (`crates/scripting/src/timer.rs`): `timer_tick_system` decrements per-frame, fires TimerExpired marker on hit — verify no negative-time accumulation
- Animation controller (`crates/core/src/animation/controller.rs`): controller component lifecycle vs AnimationPlayer — verify no dangling clip references after unload
- DebugDrainSystem (`crates/debug-server/src/system.rs`): Late-stage exclusive — verify no World mutation outside drain (per-client TCP threads must enqueue commands, not mutate)

## Process

1. Read each file in `crates/core/src/ecs/`
2. Run `cargo test -p byroredux-core` to verify all tests pass
3. Check each dimension
4. Save report to `docs/audits/AUDIT_ECS_<TODAY>.md`
