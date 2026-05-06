## Source Audit
`docs/audits/AUDIT_AUDIO_2026-05-05.md`
M44 audio subsystem

## Severity / Dimension
LOW / Looping & Streaming

## Location
`crates/audio/src/lib.rs:842-845`

## Description
When a looping emitter's source entity is despawned, `prune_stopped_sounds` issues `.stop(Tween::default())` — kira's default tween is 10 ms linear (kira tween.rs:104-112). For short sustained ambients (campfire crackle, generator hum) 10 ms is inaudible. For long-tailed ambients (cathedral choir, distant thunder loop) the abrupt fade can produce a faint click on cell exit.

## Evidence
```rust
// crates/audio/src/lib.rs:842-846
for idx in &to_stop_indices {
    audio_world.active_sounds[*idx]
        .handle
        .stop(Tween::default());  // 10 ms linear, hard-coded
}
```
No way for callers (cell-unload path, scripted cutscene) to specify a longer fade.

## Impact
Possible audible click on long-tail ambient cell-unload. No way to tune per-emitter or globally. Doesn't break correctness but lowers production polish.

## Suggested Fix
Add an `unload_fade_ms: f32` field to `AudioEmitter` (default 10 ms) and read it during the prune sweep. Or: make the prune sweep accept a global "cell-unload fade" parameter from `AudioWorld` (configurable via a method).

## Related
AUD-D3-NEW-03.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
