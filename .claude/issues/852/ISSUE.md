## Source Audit
`docs/audits/AUDIT_AUDIO_2026-05-05.md`
M44 audio subsystem

## Severity / Dimension
LOW / Spatial Sub-Track Dispatch

## Location
`crates/audio/src/lib.rs:320-342`

## Description
When the pending queue hits `MAX_PENDING = 256`, `play_oneshot` calls `self.pending_oneshots.remove(0)` — O(n) shift of 256 elements per push. On a no-device-host (the only scenario where the queue can saturate, since `audio_system` drains every frame on an active host), 1000+ enqueues per second is unrealistic, but each saturated push is O(256). Could be O(1) with a `VecDeque` + ring-buffer.

## Evidence
```rust
// crates/audio/src/lib.rs:328-335
if self.pending_oneshots.len() >= MAX_PENDING {
    log::warn!(...);
    self.pending_oneshots.remove(0);  // O(n) shift
}
self.pending_oneshots.push(...);
```

## Impact
Negligible. Saturation only happens on no-device-host with a runaway upstream producer. The warn-log itself is more expensive than the shift. Pure code-quality finding.

## Suggested Fix
Switch to `VecDeque<PendingOneShot>` with `pop_front` on saturation. Drain becomes `pending_oneshots.drain(..)`. One-line type change.

## Related
None.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
