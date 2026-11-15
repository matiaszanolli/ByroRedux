# #6920 — INC-2026-11-14-08: `music_system` and `music.status` take `MusicCatalog` / `MusicController` / `AudioWorld` in opposite orders

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-8). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: MEDIUM
- **Location**: `byroredux/src/music/mod.rs` (`music_system`; `status_lines`)
- **Changed in**: `byroredux/src/music/mod.rs` (commits `45b3b0a64`, `3981afb82`)

## Description
`music_system` takes `MusicCatalog` (W) → `MusicController` (W) → `AudioWorld` (W) → `SoundArchiveProvider` (R), then reads `Dead`, `GlobalTransform`, `FactionRanks`, `AiCombatState` and more while all four are held. `status_lines` takes `MusicController` (R) → `AudioWorld` (R) → `MusicCatalog` (R) with the controller guard still alive. That creates the edges `MusicCatalog→MusicController` / `MusicController→MusicCatalog` and `MusicCatalog→AudioWorld` / `AudioWorld→MusicCatalog`.

Both callers are exclusive (`music_system` via `add_exclusive`; `status_lines` from the debug drain), so no deadlock is reachable today. But `crates/core/src/ecs/lock_tracker.rs` (`BYRO_LOCK_ORDER_CHECK=1`) panics on the second observation of a cycle-closing edge regardless of overlap, and the audio code otherwise keeps "one resource lock at a time" (`systems/audio.rs`, #5556) to avoid AudioWorld-to-many edges.

## Impact
A debug run with the detector on panics when `music.status` / `cell.owners` runs after any music frame; CI's ABBA job fails once a test exercises both. Any existing `X → AudioWorld` edge would close a real cycle — the class just fixed in 4c1d54cc0.

## Suggested Fix
Take the catalog before the controller in `status_lines` (or drop the controller guard first); in `music_system`, compute the component-derived inputs first and take `AudioWorld` last.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (run the detector: `BYRO_LOCK_ORDER_CHECK=1`)
- [ ] **TESTS**: A regression test pins this specific fix (a detector-on test that runs `music_system` then `status_lines`)
