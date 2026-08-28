# #3475 — PERF-2026-08-27b-04: select_interaction_target re-acquires a storage read lock per candidate per frame

**Labels**: low, performance, ecs, gameplay, bug
**Filed**: 2026-08-27 from `docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md`
**HEAD at audit**: `969d81c8`

---

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md` — finding `PERF-2026-08-27b-04`
**Severity**: LOW · **Dimension**: CPU Hot Paths
**Location**: `byroredux/src/interaction.rs:744-770`, with `activation_is_blocked` at `:927-940` and `interaction_bound` at `:942-957`

## Description

`interaction_system` is the first `Stage::Update` exclusive (`byroredux/src/boot.rs:851`) and calls `select_interaction_target` **unconditionally every frame** — correctly, since it drives the HUD prompt, not just the activate edge. `#3059` already removed the per-frame allocation from that path by pooling the candidate map in `InteractionCandidateScratch`. What it did not remove is the per-candidate lock churn: each candidate is then passed through two helpers that reach the world with `World::get::<T>`, which opens a fresh `RwLock` read guard per call rather than reusing a hoisted query.

## Evidence

```rust
let mut targets: Vec<_> = candidates
    .iter()
    .filter(|(entity, _)| !activation_is_blocked(world, **entity))
    .filter_map(|(entity, kind)| {
        let bound = interaction_bound(world, *entity)?;
```

`activation_is_blocked` performs up to two `world.get` calls (`Locked`, then `MG07LabyrinthianDoor`); `interaction_bound` performs up to three (`WorldBound`, `GlobalTransform`, `Transform`). `World::get` (`crates/core/src/ecs/world.rs:358-376`) is not a bare probe — per call it does a `HashMap<TypeId, _>` lookup, constructs a `lock_tracker::TrackedRead` (a thread-local `HashMap<TypeId, LockState>` insert, un-done on drop — always on, in release too, per `crates/core/src/ecs/lock_tracker.rs:8-12`), takes the `RwLock` read, and builds a `ComponentRef`. Every other per-frame consumer in this codebase hoists the query once and iterates (`byroredux/src/systems/bounds.rs:126-137` is the canonical shape).

Also note `targets` is a fresh `Vec` per frame — the one allocation `#3059` left behind on this path.

## Impact

Small and bounded — candidates are `DoorTeleport` plus four scripted-activator component types (`populate_candidates`, `byroredux/src/interaction.rs:876-925`), so tens in an interior and low hundreds across a loaded exterior grid. Derived: a few hundred guard acquire/release pairs per frame, order tens of microseconds. Filed at LOW on magnitude, but flagged because this is the **un-owned gameplay slice** that `_audit-common.md` names as the highest-value coverage gap, the pattern is the one the project has now corrected three times elsewhere (#2149, #3265, #3059), and it will scale with the candidate set as more activator kinds are added.

## Related

#3059 (the same function's allocation half, CLOSED), #3265, #2149; `docs/engine/ecs.md`'s hoisted-query guidance.

## Suggested Fix

Convert `interaction_system` to a factory closure with a persistent `targets` buffer (the shape `make_animation_system` / `make_billboard_system` use), and hoist the five component queries once around the candidate loop, passing `&QueryRead<'_, T>` into `activation_is_blocked` / `interaction_bound` instead of `&World`. Acquire them in the canonical cluster order so the hoist does not introduce a new lock edge.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other `Stage::Update` exclusives that reach the world with per-entity `World::get`)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
