ECS-PERF-07: billboard_system redundantly cycles read+write lock on GlobalTransform when one write lock would suffice|## Description

The system takes `world.query::<GlobalTransform>()` (read) to copy out the camera's GT, drops it, then re-acquires `world.query_mut::<GlobalTransform>()` (write) for the billboard write loop. Two acquisitions of the same storage when one write query would do — the write query exposes `get` for the camera read just as well as `get_mut` for the billboard writes.

## Location

`byroredux/src/systems.rs:755-772`

## Evidence

```rust
let Some(cam_gq) = world.query::<GlobalTransform>() else { return; };
let Some(cam_global) = cam_gq.get(cam_entity).copied() else { return; };
drop(cam_gq);                                                 // release read lock
// ... compute cam_pos, cam_forward ...
let Some(bq) = world.query::<Billboard>() else { return; };
let Some(mut gq) = world.query_mut::<GlobalTransform>() else { return; };  // re-acquire as write
for (entity, billboard) in bq.iter() {
    let Some(global) = gq.get_mut(entity) else { continue; };
    // ...
}
```

## Impact

One extra RwLock acquire/release pair per frame (~50–100 ns) plus one extra Vec allocation in release (see #823). Trivial. Surface area for a future deadlock if the prelude grows another lock acquisition between the read drop and write re-acquire.

## Suggested Fix

Take the write lock first, read camera GT through `gq.get(cam_entity).copied()`, then proceed to the billboard write loop with the same query handle:

```rust
let Some(mut gq) = world.query_mut::<GlobalTransform>() else { return; };
let Some(cam_global) = gq.get(cam_entity).copied() else { return; };
let cam_pos = cam_global.translation;
let cam_forward = cam_global.rotation * -Vec3::Z;

let Some(bq) = world.query::<Billboard>() else { return; };
for (entity, billboard) in bq.iter() {
    if let Some(global) = gq.get_mut(entity) {
        global.rotation = compute_billboard_rotation(billboard.mode, global.translation, cam_pos, cam_forward);
    }
}
```

Caveat: `query_mut::<GlobalTransform>` then `query::<Billboard>` reverses the natural lock-order if a future system tries to hold Billboard before GlobalTransform — but no current system does, and TypeId-sorted ordering is enforced for multi-component queries, not for sequential single-component acquires within one system.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check `particle_system` for similar read-then-write same-storage cycles
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: Verify no other system pre-holds Billboard while taking GlobalTransform (would deadlock with the new acquire order)
- [ ] **FFI**: N/A
- [ ] **TESTS**: Existing billboard tests should still pass — behavior identical

## Source Audit

`docs/audits/AUDIT_PERFORMANCE_2026-05-04_DIM4.md` — ECS-PERF-07