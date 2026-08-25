# CONC-2026-08-25-01: pull_dynamic holds GlobalTransform+Transform together, closing a live Transform<->GlobalTransform lock cycle

## Description

Discovered while verifying #3260's fix: running the `byroredux` binary's `systems::character::` test module under `BYRO_LOCK_ORDER_CHECK=1` (all tests, default multi-threaded runner) surfaces a **separate, live 2-edge lock cycle** — `Transform → GlobalTransform → Transform` — distinct from #3260's 3-edge `CharacterController` cycle (already fixed by `98eea9b3`).

`pull_dynamic` (`crates/physics/src/sync.rs:1120-1122`) acquires `Parent`, `GlobalTransform`, and `Transform` all as read locks in the same block, held together until the block ends at `:1153`:

```rust
let parent_q = world.query::<Parent>();
let global_q = world.query::<GlobalTransform>();
let transform_q = world.query::<Transform>();
```

That establishes `GlobalTransform → Transform`. `make_transform_propagation_system` (`crates/core/src/ecs/systems.rs:78-84`) already establishes the canonical reverse, `Transform → GlobalTransform` (documented head of the canonical chain, `docs/engine/ecs.md:597`). Composing the two closes the cycle.

Both are read-only acquisitions, but the tracker's model (module doc, `lock_tracker.rs:12-24`) is intentionally conservative about read/read cycles: a writer-preferring `RwLock` can park new readers behind a pending writer, so a read/read cycle across threads is still a real deadlock risk under contention, not just a same-kind-lock false alarm.

**Reachable in production**, not test-only: `ground_character_body_at` (door/cell-transition arrival, `byroredux/src/systems/character.rs:760`) calls `physics_sync_system(world, 0.0)` → `pull_dynamic`, on every door transition and live cell load.

## Evidence

Isolated, the two `door_arrival_*` tests in `byroredux/src/systems/character.rs` pass clean:
```
BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux \
  systems::character::tests::door_arrival_grounds_the_capsule_on_the_destination_floor \
  -- --exact --test-threads=1
# ok
```
Run as part of the full `systems::character::` module (default parallel runner, so the global lock-order graph has already recorded the reverse edge from another test/thread by the time these run):
```
BYRO_LOCK_ORDER_CHECK=1 cargo test -q -p byroredux --bin byroredux systems::character::
# door_arrival_grounds_the_capsule_on_the_destination_floor --- FAILED
# door_arrival_with_no_probe_hit_falls_back_to_the_authored_height --- FAILED
# panic: ECS cross-thread deadlock risk (lock-order cycle):
#   Transform -> GlobalTransform -> Transform
```
Same root cause as #3260, same reason CI doesn't already catch it (`BYRO_LOCK_ORDER_CHECK=1` is set in only two CI jobs, neither of which drives `pull_dynamic` through a character-mode door transition).

## Location

`crates/physics/src/sync.rs:1120-1122` (the three-query block in `pull_dynamic`); reverse edge at `crates/core/src/ecs/systems.rs:78-84` (`make_transform_propagation_system`).

## Suggested Fix

Same shape as #3260's fix: snapshot the `GlobalTransform` reads `pull_dynamic` needs (parent-global lookups) into locals and drop `global_q` before the loop reads from `transform_q`, or restructure so `global_q` and `transform_q` are never live in the same block. Add a regression test mirroring `camera_follow_does_not_close_character_lock_cycle` that recreates the `Transform → GlobalTransform` edge on one simulated thread, then drives `pull_dynamic` and asserts no panic under `BYRO_LOCK_ORDER_CHECK=1`.

## Completeness Checks
- [ ] **LOCK_ORDER**: `GlobalTransform → Transform` edge in `pull_dynamic` broken
- [ ] **TESTS**: regression test pinning the ordering, `BYRO_LOCK_ORDER_CHECK=1`-gated like #3260's

## Related
#3260 (same class, `CharacterController`-involving 3-edge cycle, fixed in `98eea9b3`), #2135 (documents a *different* edge in the same function — `RapierHandles`/`RigidBodyData` vs `Transform` write), #2675 (detector strengthened to catch N-length cycles), #313 (original lock-order graph).

