## Source Audit
`docs/audits/AUDIT_SAFETY_2026-05-05.md`

## Severity / Dimension
LOW / Memory Safety / Audio Lifecycle

## Location
- `crates/audio/src/lib.rs:817-870` — `prune_stopped_sounds` only stops looping sounds when their entity loses `AudioEmitter`. Non-looping handles are dropped on `PlaybackState::Stopped` only.
- `byroredux/src/cell_loader.rs:258-262` — `unload_cell` calls `world.despawn(eid)` on every cell-owned entity. No coordination with `AudioWorld::active_sounds`.

## Description
`unload_cell` despawns every cell entity (which removes ALL component rows including `AudioEmitter`). For **looping** active sounds, `prune_stopped_sounds` notices the missing `AudioEmitter` and issues `handle.stop(Tween::default())`. For **non-looping** active sounds (footsteps, weapon fire, dialogue), the prune path takes no action — kira keeps decoding and mixing the sound until natural termination (typically 50 ms - 3 s for short SFX).

This is **not a memory leak**: `ActiveSound` is bounded by playback duration and self-prunes on `Stopped`. EntityIds are never recycled in this engine (`world.rs:113-116` per #372 / #36), so `s.entity == Some(stale_eid)` is safely a no-op when `prune_stopped_sounds` reaches it (despawn already removed all components, the `emitter_q.remove(entity)` call is idempotent).

The audible effect: a footstep that lands at the last frame of cell A's tick finishes playing through cell B's first ~150 ms. Spatial position is stale (the `_track` is anchored at the despawned entity's last `GlobalTransform`, not updated thereafter). For exterior cell streaming this would be unnoticeable; for fast-travel interior-to-interior transitions this could surface as a faint cross-cell SFX bleed.

## Evidence
```rust
// crates/audio/src/lib.rs:826-840 — only looping path checks emitter presence
for (idx, s) in audio_world.active_sounds.iter().enumerate() {
    if !s.looping {
        continue;                          // ← non-looping sounds skip the despawn check
    }
    let Some(entity) = s.entity else { continue; };
    let still_has_emitter = emitter_q
        .as_ref().map(|q| q.get(entity).is_some()).unwrap_or(false);
    if !still_has_emitter {
        to_stop_indices.push(idx);
    }
}
```

## Impact
Audible only on cross-cell transitions during active SFX playback. No memory growth, no leak, no GPU coupling. Footstep system runs in `Stage::Update` and dispatch happens in `Stage::Late` of the same frame, so the exposure window per cell unload is at most one playback duration (~3 s ceiling).

## Suggested Fix
(sketch — do NOT ship without observing the actual audible regression first; per `feedback_speculative_vulkan_fixes.md`): widen the looping-only branch in `prune_stopped_sounds` to ALSO stop non-looping sounds whose `entity` no longer has `AudioEmitter`. Three-line change:
```rust
for (idx, s) in audio_world.active_sounds.iter().enumerate() {
    let Some(entity) = s.entity else { continue; };  // queue-driven entries unaffected
    let still_has_emitter = emitter_q.as_ref()
        .map(|q| q.get(entity).is_some()).unwrap_or(false);
    if !still_has_emitter {
        to_stop_indices.push(idx);
    }
}
```
This makes cell unload truncate every cell-owned active sound regardless of looping. Side note: the symmetric question of "queue-driven (`entity == None`) one-shots that out-live their cell" doesn't apply — no entity, no despawn coupling, they always run to natural termination, which is the intended semantics of `play_oneshot`.

## Related
- The looping path at `lib.rs:826-846` was added in M44 Phase 4. Extending it to non-looping is a single-condition relaxation.
- No GitHub issue.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
