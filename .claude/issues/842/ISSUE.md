## Source Audit
`docs/audits/AUDIT_AUDIO_2026-05-05.md`
M44 audio subsystem

## Severity / Dimension
HIGH / Manager Lifecycle & Spatial Sub-Track Dispatch

## Location
`crates/audio/src/lib.rs:230-245` (manager init); `crates/audio/src/lib.rs:640-661` and `:752-797` (per-dispatch failure paths)

## Description
`AudioManager::new(AudioManagerSettings::default())` inherits `Capacities::default()` from kira, which sets `sub_track_capacity = 128` (kira-0.10.8 manager/settings.rs:25). Each active spatial sound (entity-path one-shot, queue-path one-shot, OR looping emitter) holds one spatial sub-track for the duration of playback. A populated FO3 / FNV interior (Megaton has 929 REFRs; ~30-60 NPCs in a populated bunker; ambient looping per cell) plus a layer of footstep one-shots can reach 128 simultaneous sub-tracks during cell-load bursts. When kira returns `ResourceLimitReached`, the dispatch path logs `warn!` and `continue`s — the sound is silently dropped from that frame's playback.

## Evidence
```
# kira-0.10.8/src/manager/settings.rs:22-32
impl Default for Capacities {
    fn default() -> Self {
        Self {
            sub_track_capacity: 128,
            send_track_capacity: 16,
            ...
        }
    }
}
# kira-0.10.8/src/manager.rs:131-148 (add_spatial_sub_track)
... .insert(track)?  // returns Err(ResourceLimitReached) at cap

# crates/audio/src/lib.rs:752-760 (dispatch_new_oneshots)
let mut track = match mgr.add_spatial_sub_track(...) {
    Ok(t) => t,
    Err(e) => {
        log::warn!("M44 Phase 3: add_spatial_sub_track failed for entity {:?}: {e}", p.entity);
        continue;
    }
};
```

## Impact
Sounds drop silently during cell-load bursts. The user hears a partial soundscape with no obvious error in normal logs (only WARN). Hard to diagnose because the symptom is "some sounds missing," not a crash. Will be triggered by every populated interior cell once Phase 3.5b FOOT records + REGN ambient soundscapes land.

## Suggested Fix
Override capacities at manager init — `AudioManagerSettings { capacities: Capacities { sub_track_capacity: 512, send_track_capacity: 32, ..Default::default() }, ..Default::default() }`. 512 is a comfortable headroom for the worst Bethesda interior cell (FO4 Diamond City Market sits around 400 active emitters in vanilla; 512 also leaves room for the Phase 4 REGN ambient layer). Pin via a module-level `const SUB_TRACK_CAPACITY: usize = 512;` so the cap is one-line-greppable. Add a smoke test that asserts the configured cap exceeds 128 (regression gate against a "simplify back to default" refactor).

## Related
M44 Phase 3.5b (FOOT records), Phase 4 future (REGN).

## Completeness Checks
- [ ] **UNSAFE**: If fix involves unsafe, safety comment explains the invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan/kira objects change, verify Drop impl still correct
- [ ] **LOCK_ORDER**: If RwLock scope changes, verify TypeId ordering
- [ ] **FFI**: If cxx bridge touched, verify pointer lifetimes
- [ ] **TESTS**: Regression test added for this specific fix
