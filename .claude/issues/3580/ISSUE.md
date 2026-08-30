# CONC-D5-2026-08-30-01: `combat_approach_line_of_sight_reaches` holds `PhysicsWorld` across `RapierHandles` — live lock cycle, `lock-order-check` CI job RED at HEAD

**Issue**: #3580
**Labels**: bug, ecs, high, physics, concurrency
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D5-2026-08-30-01 (HIGH, D5 · RwLock Patterns — Resource<->Storage & Physics Step).

**Introduced by `5c8a1581`** ("Fix #3422, Fix #3424, and gate combat.approach on line of sight (#3423)").

## This is empirically confirmed, and CI is RED at HEAD

Reproduced independently twice — by the dimension agent and by the publish orchestrator:

```
$ BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins
test result: FAILED. 1642 passed; 5 failed; 17 ignored     (all ragdoll::tests::*)

$ BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins -- --skip combat_approach_line_of_sight
test result: ok. 1645 passed; 0 failed
```

> **Note for anyone triaging this**: a sibling `/audit-runtime` sweep on the same day reported this as "CONTRADICTED — does not reproduce", having run only `cargo test -p byroredux-physics` and the 18 ragdoll tests **in isolation**. That is an isolation artifact: the ragdoll tests pass individually because the cycle only closes once `combat_approach_line_of_sight_reaches` has *also* run in the same process and recorded its edge. Run the whole `-p byroredux --bins` binary, as CI does, and it fails. Do not close this on the isolated result.

## Description

`combat_approach_line_of_sight_reaches` (`byroredux/src/commands/view.rs:168-215`) binds the `PhysicsWorld` resource read guard to a named local, so the guard is alive for the whole function body. It then acquires the `RapierHandles` **storage** and the `ActorColliderOwner` storage **underneath it**. That records `PhysicsWorld -> RapierHandles` and `PhysicsWorld -> ActorColliderOwner` in `lock_tracker`'s single TypeId graph — which keys storages *and* resources together, since `World::resource` / `try_resource` go through the same `lock_tracker::TrackedRead/Write` (`crates/core/src/ecs/world.rs:708,738`).

Two long-standing canonical edges complete the ring:

- **`RapierHandles -> GlobalTransform`** — `collect_newcomers` (`crates/physics/src/sync.rs:821-844`) and `push_kinematic` (`:1055-1063`) both hold `RapierHandles` while acquiring `GlobalTransform`. `docs/engine/ecs.md:602-604` *documents* this as the process-wide order.
- **`GlobalTransform -> PhysicsWorld`** — `ragdoll_writeback_system` (`byroredux/src/ragdoll.rs:491-495`) holds the `GlobalTransform` write guard while taking `PhysicsWorld`.

**Every other `PhysicsWorld` site in the tree** acquires storages *before* the resource and drops them first: `collect_newcomers`->`register_newcomers`, `push_kinematic`, `pull_dynamic`, `apply_buoyancy`, `dump_awake_fallers` (#2136), `spawn_collider_census_report` (#3266), `probe_walkable_floor_near`, `interaction::target_has_line_of_sight`, `combat.rs`'s melee swing, `release_victim_rapier_bodies`. **This function is the only inversion.**

And its own comment describes the discipline it does not follow:

```rust
// byroredux/src/commands/view.rs:184-185
// Same lock discipline as the swing: resolve body ownership before
// touching PhysicsWorld-adjacent component storages.
```

The swing (`byroredux/src/combat.rs:159-172`) resolves ownership first and takes `PhysicsWorld` after.

## Evidence

```
byroredux/src/commands/view.rs
175	    let Some(physics) = world.try_resource::<byroredux_physics::PhysicsWorld>() else {
176	        return true;
177	    };
...
184	    // Same lock discipline as the swing: resolve body ownership before
185	    // touching PhysicsWorld-adjacent component storages.
186	    let (excluded_body, owners) = match world.query::<byroredux_physics::RapierHandles>() {
...
198	    let Some(hit_body) = physics
199	        .cast_ray(camera_pos, direction, distance, excluded_body)
...
212	    let hit_root = world
213	        .get::<byroredux_physics::ActorColliderOwner>(collider_entity)
```

Detector output (verbatim, two distinct rings reported), from `crates/core/src/ecs/lock_tracker.rs:411`:

```
ECS cross-thread deadlock risk (lock-order cycle): attempted acquisition of
`byroredux_physics::world::PhysicsWorld` while holding
`byroredux_core::ecs::components::global_transform::GlobalTransform` ... cycle:
PhysicsWorld -> RapierHandles -> GlobalTransform -> PhysicsWorld

ECS cross-thread deadlock risk (lock-order cycle): attempted acquisition of
`byroredux_physics::world::PhysicsWorld` while holding
`byroredux_core::ecs::components::hierarchy::Children` ... cycle:
PhysicsWorld -> RapierHandles -> RigidBodyData -> Parent -> Children -> PhysicsWorld
```

## Impact

1. **CI is red.** The dedicated `lock-order-check` job (`.github/workflows/ci.yml:108-121`, `BYRO_LOCK_ORDER_CHECK: 1`, `cargo test --workspace`) fails at HEAD. **Every subsequent concurrency regression is masked while it stays red** — that job is the project's only dynamic ABBA proof for exercised paths.
2. **Real ABBA risk on the next scheduling change.** `PhysicsWorld` is currently reached from many storage guards in the *safe* direction (`FollowBehavior`/`GuardBehavior`/`TravelBehavior`/`EscortBehavior` -> `PhysicsWorld` in `systems/{follow:139->255, guard:144->229, travel:184->268, escort:205->363}.rs`; `Transform`/`Parent`/`Children`/`GlobalTransform` -> `PhysicsWorld` in `ragdoll.rs`). Each of those becomes a cycle the instant a `PhysicsWorld`-held storage acquisition exists — **this one edge opens all of them at once.**
3. `combat.approach` is a `DebugDrainSystem` (Late exclusive) command: running it in a debug engine build with the detector on aborts the session (the #2388 precedent).

A genuine *hang* needs two threads holding overlapping guards in the opposing orders; today the three sites are separated by scheduler stages (`Stage::Physics` parallel batch / `Stage::Late` exclusives), so the **deadlock is latent — but the detector abort and the red CI job are live**.

## Related

#313, #2675 (reachability-based cycle detection), #2136, #2404, #3303, #3423 (the commit that introduced it), `docs/engine/ecs.md:596-628`. Sibling finding CONC-D5-2026-08-30-02 covers the second `PhysicsWorld -> storage` edge; CONC-D5-2026-08-30-03 covers the unwritten rule.

## Suggested Fix

Move the `RapierHandles` snapshot (lines 186-196) and the `ActorColliderOwner` resolution (212-215) **out** from under the `PhysicsWorld` guard: resolve `excluded_body` / `owners` first, then scope the `physics` guard to the `cast_ray` call alone —

```rust
let hit_body = { let physics = world.try_resource::<PhysicsWorld>()?; physics.cast_ray(...) };
```

— exactly as `combat.rs:159-172` and `interaction.rs:790-805` already do. The `None` case must still `return true`. Then delete or repoint the now-accurate comment at 184-185.

## Completeness Checks
- [ ] **LOCK_ORDER**: The `PhysicsWorld` guard scope is narrowed such that no component storage is acquired while it is held; TypeId-sorted acquisition preserved
- [ ] **SIBLING**: CONC-D5-2026-08-30-02 (`ragdoll_writeback_system`'s `LocalBound`/`WorldBound` under the same guard) fixed in the same pass, or the class stays open
- [ ] **TESTS**: `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bins` must go green — that is the pin. Confirm the `lock-order-check` CI job passes on the fix branch
- [ ] **DOCS**: `docs/engine/ecs.md`'s canonical order table gains a `PhysicsWorld` entry (CONC-D5-2026-08-30-03) so the rule this broke is written down
