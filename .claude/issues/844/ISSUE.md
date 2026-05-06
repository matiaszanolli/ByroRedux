## Source Audit
`docs/audits/AUDIT_AUDIO_2026-05-05.md`
M44 audio subsystem

## Severity / Dimension
LOW / Spatial Sub-Track Dispatch / Looping & Streaming

## Location
`crates/audio/src/lib.rs:817-846`

## Description
When a looping emitter's source entity has lost its `AudioEmitter` component (cell unload, explicit removal), `prune_stopped_sounds` issues a tweened `stop()` on the kira handle each tick until the handle reports `Stopped`. The tween default is 10 ms (kira tween.rs:104-112), so by the time the next prune tick runs (~16 ms later at 60 FPS), the state has flipped and the entry drops. But if the audio system tick-rate is faster than the kira tween rate, the sweep will call `stop()` multiple times, each resetting the tween. kira treats subsequent stops as new fade commands.

## Evidence
```rust
// crates/audio/src/lib.rs:824-846
let mut to_stop_indices: Vec<usize> = Vec::new();
for (idx, s) in audio_world.active_sounds.iter().enumerate() {
    if !s.looping { continue; }
    let Some(entity) = s.entity else { continue; };
    let still_has_emitter = emitter_q...
    if !still_has_emitter {
        to_stop_indices.push(idx);  // marked every tick until Stopped reports
    }
}
...
for idx in &to_stop_indices {
    audio_world.active_sounds[*idx].handle.stop(Tween::default());
}
```
No "stop already issued" flag on `ActiveSound`.

## Impact
Redundant kira commands during the ~10 ms fade window. Wasted CPU on re-walking the active list and re-marking; minor ringbuf traffic. Not a correctness issue (kira's repeated-stop is idempotent in effect). Becomes more visible if a future fade duration is longer (e.g., a 1-second graceful cell-unload fade).

## Suggested Fix
Add a `stop_issued: bool` field to `ActiveSound` and skip the re-stop when set. Drop the entry on the next tick that observes `Stopped`. Trivial change, prevents future-phase regressions if the fade duration is tuned up.

## Related
AUD-D4-NEW-04 (fade duration is hard-coded to `Tween::default()` 10 ms).

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
