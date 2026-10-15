# #5875: GAME-D7-2026-10-15-01: The `HitEvent` ordering guard silently skips producers it cannot find

- **Labels**: low,bug,gameplay,test-gap,combat
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Location**: `byroredux/src/boot/schedule/mod.rs:138-140` (`hit_event_consumers_run_after_every_producer`)
- **Status**: NEW (test gap in `ee6468fd1`)
- **Description**: The test does `continue` when a producer's name is no longer in the source. A renamed producer therefore leaves the guard green while checking nothing, which is exactly the dart-trap regression the guard exists to catch. A missing consumer name panics; a missing producer name should too.
- **Suggested Fix**: Panic on a missing producer name.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
