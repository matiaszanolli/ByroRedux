# #5872: GAME-D4-2026-10-15-06: A `ProcessTrapHit` raised from a spoken dialogue line is dropped

- **Labels**: low,bug,gameplay,combat
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Location**: `byroredux/src/boot/schedule/late.rs:527` (the dialogue-fragment block declaring the `HitEvent` write at `:750`); `event_cleanup_system` at `:821`
- **Status**: NEW (no vanilla trigger confirmed)
- **Description**: The Late dialogue path can flush a trap hit into `HitEvent`, and `event_cleanup_system` clears it before any consumer runs.
- **Suggested Fix**: Queue it for the next frame's Update flush instead of inserting it in Late.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
