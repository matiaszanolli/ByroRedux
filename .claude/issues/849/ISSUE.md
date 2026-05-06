## Source Audit
`docs/audits/AUDIT_AUDIO_2026-05-05.md`
M44 audio subsystem

## Severity / Dimension
LOW / ECS Lifecycle

## Location
`crates/audio/src/lib.rs:555-595`

## Description
When the entity carrying `AudioListener` is despawned, `sync_listener_pose` early-returns at line 557-562 — the `audio_world.listener` handle is NOT reset to `None`. On the next frame, if a NEW entity gets `AudioListener`, line 574 (`audio_world.listener.is_none()`) is FALSE, so we fall through to the `else if` branch (line 592) and update the EXISTING handle's pose. **This is correct (handle reuse, no kira leak), but the contract is non-obvious from reading the code.**

## Evidence
```rust
// crates/audio/src/lib.rs:555-595
fn sync_listener_pose(world: &World, audio_world: &mut AudioWorld) {
    let listener_entity = { ... };  // early-return on no AudioListener
    let pose = { ... };              // early-return on no GlobalTransform
    if audio_world.listener.is_none() { ... add_listener ... }
    else if let Some(handle) = audio_world.listener.as_mut() {
        handle.set_position(pose.0, Tween::default());
        handle.set_orientation(pose.1, Tween::default());
    }
}
```
kira's `listener_capacity = 8` (kira-0.10.8/src/manager/settings.rs:29) — even if despawn-respawn DID create a new handle each cycle, the cap would catch a runaway after 8 cycles. But the actual code reuses the existing handle, so the cap is irrelevant.

## Impact
None today. Future-phase risk: if someone refactors `sync_listener_pose` to "clear `audio_world.listener` when no entity carries the marker," the next respawn would call `add_listener` again — fine the first time, but a bursty churn (debug fly-cam destroy-create loop) could exhaust kira's `listener_capacity = 8`.

## Suggested Fix
Add a doc comment at lib.rs:553 stating "Listener handle is created lazily on first observation and REUSED across AudioListener entity churn — never cleared. This is intentional: prevents listener_capacity exhaustion on rapid entity churn." A one-line guard against future "simplify by clearing on missing entity" refactors.

## Related
AUD-D2-NEW-02 (multi-listener entity case).

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
