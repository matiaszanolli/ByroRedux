# #6919 — INC-2026-11-14-07: A muting movie does not stop entity-emitter sounds dispatched during the hold

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-7). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: MEDIUM
- **Location**: `crates/audio/src/system.rs` (`drain_pending_oneshots` is the only `world_muted` check; `dispatch_new_oneshots` is ungated); `crates/audio/src/world.rs` (`hold_for_movie` / `release_movie_hold`)
- **Changed in**: `crates/audio/src/{system.rs, world.rs}` (commit `6aa314bcf`)

## Description
`hold_for_movie(mute_world = true)` pauses the current `active_sounds` and clears queued one-shots. `audio_system` then calls `dispatch_new_oneshots` every frame with no `movie_hold` check, so any `OneShotSound` + `AudioEmitter` marker raised during the movie (looping ambients, machinery, torches from a cell or streaming load, scripted spawns) starts at full volume. Those sounds were never paused, so `release_movie_hold` does not account for them either.

## Evidence
`audio_system`: `drain_pending_oneshots(&mut audio_world); dispatch_new_oneshots(world, &mut audio_world);` — `world_muted` is read only inside `drain_pending_oneshots`.

## Impact
Ambient loops are audible under a movie that asked to mute the world (BIK-PRES-005). Not observed at runtime: whether the boot start-sequence frame dispatches emitters after the hold is applied.

## Related
BIK-PRES-005

## Suggested Fix
While `world_muted`, dispatch and immediately pause each new handle (so release resumes it), or leave the markers undispatched until release. Add a headless `AudioWorld` test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other direct `play_*` paths that bypass the queue)
- [ ] **TESTS**: A regression test pins this specific fix
