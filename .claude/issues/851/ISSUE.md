## Source Audit
`docs/audits/AUDIT_AUDIO_2026-05-05.md`
M44 audio subsystem

## Severity / Dimension
LOW / Spatial Sub-Track Dispatch

## Location
`crates/audio/src/lib.rs:603-624`

## Description
`drain_pending_oneshots` first checks listener_id (line 604), then takes the pending vec (line 610), then checks the manager (line 619). If the manager is `None` at line 619, the pending entries are dropped (already-taken into a local) without dispatch. The comment at line 620-623 says "Inactive — queue cleared" — but in practice this is unreachable: `audio_system` early-returns at line 542 (`if !audio_world.is_active() { return; }`) when the manager is `None`, so the manager check at line 619 can never trigger inside `drain_pending_oneshots`. Once `audio_system` starts, the manager state cannot change.

## Evidence
```rust
// crates/audio/src/lib.rs:538-550 (audio_system)
if !audio_world.is_active() { return; }   // gates everything
...
drain_pending_oneshots(&mut audio_world);

// crates/audio/src/lib.rs:603-624
fn drain_pending_oneshots(audio_world: &mut AudioWorld) {
    let Some(listener_id) = ... else { return; };
    if audio_world.pending_oneshots.is_empty() { return; }
    let pending = std::mem::take(&mut audio_world.pending_oneshots);  // takes
    ...
    let Some(mgr) = audio_world.manager.as_mut() else {
        return;  // unreachable in practice — pending is dropped here
    };
}
```

## Impact
Dead defensive branch. Wastes a cycle of `mem::take` before realising the take was wasted. Trivial perf cost. Slight mental-model mismatch reading the code: "is this protected for re-init?" — answer is "no, the check above already guarantees manager is Some."

## Suggested Fix
Move the manager check UP, before `mem::take`, OR remove the redundant manager check entirely (it's already guaranteed Some by `audio_system`'s early-return). Cleanup pass; not urgent.

## Related
None.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
