## Source Audit
`docs/audits/AUDIT_AUDIO_2026-05-05.md`
M44 audio subsystem

## Severity / Dimension
LOW / Reverb Send & Routing

## Location
`crates/audio/src/lib.rs:420-429`

## Description
Per kira's API design, `with_send` is build-time-only on `SpatialTrackBuilder` (kira spatial_builder.rs:128-134) — there is no per-track `set_send_volume` after construction. Already-playing looping ambients (a cathedral chant, a generator hum) spawned BEFORE a `set_reverb_send_db(-12.0)` call will continue to play with their old (likely silent) send level. The docstring at lib.rs:421-426 captures the spirit ("Already-playing sounds keep their construction-time send level") but doesn't surface the consequence: a reverb-level change toward a populated cell will not retro-apply to the cell's existing ambient layer until those ambients are restarted.

## Evidence
```
# crates/audio/src/lib.rs:420-429
/// **Phase 6**: set the per-new-spatial-track reverb send level
/// in decibels. Already-playing sounds keep their construction-
/// time send level; the change applies to *new* sounds dispatched
/// after the call. ...

# kira-0.10.8/src/track/sub/spatial_builder.rs:128-134
/// Routes this track to the given send track with the given volume.
pub fn with_send(mut self, track: ..., volume: ...) -> Self {
    self.sends.insert(track.into(), volume.into());  // build-time only
    ...
}
```
No `SpatialTrackHandle::set_send_volume(...)` exists in the kira 0.10 API.

## Impact
The crate docstring claim "for short SFX (footsteps, gunshots) the level naturally refreshes as new sounds replace old ones" only works when sounds are short. Long ambients that span the cell-load → interior reverb-flip transition won't bloom. User-perceptible only after AUD-D5-NEW-05 lands and reverb starts firing.

## Suggested Fix
Once the cell-load detector lands (AUD-D5-NEW-05), on a reverb-level-flip event, restart all currently-active looping emitters (stop with a fade, then re-dispatch via the same `dispatch_new_oneshots` path so the new send level takes effect). Or: defer Phase 6 reverb-level dynamics until kira surfaces a per-track `set_send_volume` API. Document the limitation in the next-phase contract — see Future-Phase Readiness.

## Related
AUD-D5-NEW-05.

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
