## Source Audit
`docs/audits/AUDIT_CONCURRENCY_2026-05-05.md`

## Severity / Dimension
MEDIUM / Thread Safety

## Location
`crates/audio/src/lib.rs:320-342, 538-550, 603-624`

## Description
**Trigger Conditions**: Engine launched on a host where `AudioManager::new()` fails (CI / headless server / broken sound driver), and a system runs that calls `AudioWorld::play_oneshot` — currently `byroredux/src/systems.rs::footstep_system` on a player entity that has a `FootstepEmitter` AND a non-`None` `FootstepConfig.default_sound`.

`play_oneshot` always pushes a `PendingOneShot` into `pending_oneshots` (line 336), regardless of whether the manager is active. `audio_system` runs the early-return `if !audio_world.is_active() { return; }` at line 542-544 BEFORE calling `drain_pending_oneshots`, so the queue never drains when audio is inactive. Bounded at 256 entries via FIFO drop-oldest (line 327-335), so this is a steady-state cap, not unbounded growth — but those 256 entries (each holds an `Arc<StaticSoundData>` clone) pin one strong refcount on each cached sound and ~2.5 KB of `Vec<PendingOneShot>` state for the lifetime of the engine. Long-term the queue can also block hypothetical audio-recovery flows (the docstring at line 313-316 hints at "future re-init" replay, but pinning 256 stale entries from earlier gameplay would replay them all the moment audio comes online).

## Evidence
```rust
// crates/audio/src/lib.rs:538-550
pub fn audio_system(world: &World, _dt: f32) {
    let Some(mut audio_world) = world.try_resource_mut::<AudioWorld>() else {
        return;
    };
    if !audio_world.is_active() {
        return;  // ← early-return BEFORE drain_pending_oneshots
    }

    sync_listener_pose(world, &mut audio_world);
    drain_pending_oneshots(&mut audio_world);  // ← never reached when inactive
    ...
```

## Impact
~12 KB pinned memory + an `Arc` strong-count per cached sound on no-audio hosts after ~8 seconds of footstep activity. No functional bug in the steady state. CI runs on a headless host might accumulate a queue of stale one-shots if a long-running test fires footsteps.

## Suggested Fix
In `play_oneshot`, short-circuit the push when `self.manager.is_none()` (cheap check). Alternatively, in `audio_system`, drain (and discard) the pending queue on every tick when inactive so a later activation doesn't replay stale events. The first option is simpler.

## Related
The 256-cap path in `play_oneshot` was added precisely to bound this scenario; this finding tightens it from "bounded leak" to "no leak".

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
