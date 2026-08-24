# 3250: ECS-2026-08-24-01: two ComponentRef<Transform> read guards held simultaneously in fragment.rs

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_ECS_2026-08-24.md` (ECS-2026-08-24-01)

## Description

`World::get::<T>()` returns a `ComponentRef<'_, T>` that owns its `RwLockReadGuard` for its whole lifetime. Both `Effect::SetVehicle` and `Effect::TetherToHorse` take two such guards on `Transform` simultaneously — the recursive-read pattern the just-landed `#2386` fix in `lock_tracker.rs` (commit `5428e872`) added a hazard warning for. `condition.rs:528-537` already documents and applies the fix for the same pattern one file over.

## Location
- `crates/scripting/src/fragment.rs:916-917` (`Effect::SetVehicle`)
- `crates/scripting/src/fragment.rs:942-946` (`Effect::TetherToHorse`)

## Evidence

```rust
let relative_pose = vehicle.and_then(|vehicle| {
    let actor_transform = world.get::<Transform>(actor)?;
    let vehicle_transform = world.get::<Transform>(vehicle)?;   // guard #2 while #1 live
    ...
```

## Impact

(1) Real today: every `SetVehicle`/`TetherToHorse` fragment effect now emits `log::warn!` on each call, unbounded — floods the log on repeated cinematic ticks. (2) Latent: `std::sync::RwLock` on Linux is write-preferring; a recursive read can park behind a queued writer. Mitigated today because `quest_fragment_dispatch` and siblings are all `add_exclusive` — promoting any to parallel would turn this into a real hang.

## Related

Not a duplicate of #3130 or #3142 (different mechanisms).

## Suggested Fix

Copy out of the guards immediately (`Transform` is `Copy`): `let actor_t = world.get::<Transform>(actor).map(|t| *t)?;` before acquiring the second, matching `condition.rs:534`.

## Completeness Checks
- [ ] **LOCK_ORDER**: RwLock scope change preserves TypeId-sorted acquisition
- [ ] **SIBLING**: Same fix pattern as `condition.rs:534`
- [ ] **TESTS**: A regression test pins this specific fix
