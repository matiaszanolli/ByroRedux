## Source Audit
`docs/audits/AUDIT_AUDIO_2026-05-05.md`
M44 audio subsystem

## Severity / Dimension
MEDIUM / Listener Pose Sync

## Location
`crates/audio/src/lib.rs:556-564`

## Description
`sync_listener_pose` does `q.iter().next()` against the `AudioListener` query. When more than one entity carries the marker (mod scenario, debug fly-cam swap, third-person camera transition leaving the old camera marker in place), iteration order determines which entity drives the listener pose. Per CLAUDE.md "fly-cam swap" workflow this is silent and happens during gameplay, not just at startup.

## Evidence
```rust
// crates/audio/src/lib.rs:556-564
let listener_entity = {
    let Some(q) = world.query::<AudioListener>() else { return; };
    let Some((entity, _)) = q.iter().next() else { return; };
    entity
};
```
No counter, no warning when iteration would produce more than one candidate. The crate docstring at lib.rs:445-446 acknowledges "At most one entity should carry this. If multiple do, the audio system uses whichever one comes first" — the policy is documented but not warned-on.

## Impact
After a fly-cam destroy-then-spawn cycle, the audio listener may stay attached to the OLD entity (despawned) — actually, the listener handle is reused (see AUD-D6-NEW-08), so this is fine. But during the brief window where two `AudioListener` entities coexist (third-person cutscene transition), spatial attenuation will use whichever wins iteration. No `warn!` to diagnose.

## Suggested Fix
Add a one-shot warning at the start of `sync_listener_pose`: count the iterator, log `warn!("multiple AudioListener entities found ({n}); using first")` the first time the count exceeds 1, then debounce so it doesn't spam per-frame. A `static AtomicBool` or a flag on `AudioWorld` works.

## Related
AUD-D6-NEW-08 (listener-handle reuse on entity churn).

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
