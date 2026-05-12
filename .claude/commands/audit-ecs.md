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
- AnimationClipRegistry (`crates/core/src/animation/registry.rs`): #790 dedupes by lowercased path so cell streaming doesn't grow it unboundedly. Without case-folding interning, one full keyframe set leaks per cell load — observable as steady RAM growth across exterior streaming
- DebugDrainSystem (`crates/debug-server/src/system.rs`): Late-stage exclusive — verify no World mutation outside drain (per-client TCP threads must enqueue commands, not mutate)
- AudioWorld resource (`crates/audio/src/lib.rs`, M44): `audio_system` runs Late-stage; `OneShotSound` markers are removed by `prune_stopped_sounds` once the kira playback transitions to `PlaybackState::Stopped` — verify no infinite-marker leak path. `AudioListener` / `AudioEmitter` lifecycle: spatial sub-track handle drop must precede listener handle drop (kira invariant)

### 8. ECS Hot-Path Performance Invariants (2026-05-04 batch — regression guards)
- `lock_tracker::held_others` Vec collection is `cfg(debug_assertions)`-gated (#823 ECS-PERF-01). Re-enabling for release rebuilds ~100 small allocs/frame for a no-op
- `NameIndex.map` is refilled in place via `HashMap::clear` + reinsert (#824 ECS-PERF-02). The `HashMap::new() + std::mem::swap` pattern costs ~3 ms cell-stream-in spike — a regression test should pin the in-place refill
- `transform_propagation_system` (now in `byroredux/src/systems/animation.rs` post-Session-34 split) caches the root entity set keyed on `(Transform::len, Parent::len, next_entity_id)` (#825 ECS-PERF-03). Recomputing roots every frame is the ~250 µs/frame regression at Megaton scale
- `animation_system` (now in `byroredux/src/systems/animation.rs`) hoists `events` / `seen_labels` scratches out of the per-entity loop and uses `clone` (not `mem::take`) so capacity persists (#828 ECS-PERF-06). Per-iteration allocation is the regression pattern. Helpers `ensure_subtree_cache`, `write_root_motion`, `apply_bool_channels` factored out by `2bdbc36`; `write_lazy!` macro collapses 5 color-target match arms — drift in any of these helpers risks DRY-undo
- `footstep_system` (`byroredux/src/systems/animation.rs` or `systems/audio.rs`) writes to the `FootstepScratch: Resource` (#932 PERF-CPU-02) using `mem::take` + restore pattern preserving Vec capacity across frames. Per-frame `Vec::new` is the regression
- `World::despawn` poisoned-lock panic uses a `type_names` side-table to name the offending component (#466 E-03). Removing the side-table means panic messages lose the type name — bisecting takes 10× longer

## Process

1. Read each file in `crates/core/src/ecs/`
2. Run `cargo test -p byroredux-core` to verify all tests pass
3. Check each dimension
4. Save report to `docs/audits/AUDIT_ECS_<TODAY>.md`
