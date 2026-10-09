# #5382: AUD-2026-10-08-D5-01: Dialogue voice keeps every voiced segment's decoded PCM in `SoundCache` for the life of the process. It is the first open-ended-key consumer, and there is no eviction or `clear()` caller

**Labels**: medium,audio,memory,performance,dialogue,bug,game:fnv
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5382

**Source**: `docs/audits/AUDIT_AUDIO_2026-10-08.md` — `AUD-2026-10-08-D5-01` (HEAD `00f580e09`)

- **Severity**: MEDIUM
- **Dimension**: Engine Consumers / Music & SoundCache
- **Location**:
  - `byroredux/src/systems/dialogue_voice.rs:136-163`: the cache use.
  - `crates/audio/src/lib.rs:1569-1673`: `SoundCache`, which has no eviction.
  - `assets/debug_profiles.toml`: `default_sounds_bsas` now includes `Fallout - Voices1.bsa`.
- **Status**: NEW (introduced by `f8950e7cc`)
- **Description**: `play_line_voice` resolves each response segment through `cache.get_or_load(path, || provider.extract(path))`. Each segment's `StaticSoundData` is a full PCM decode, stored as stereo `f32` frames even for mono sources, and stays in `SoundCache.map` until `clear()`. `git grep` finds no `clear()` caller anywhere in `byroredux/src`. The cache's own doc says the cell-unload path *can* call it, but none does.

  Before this commit, the only consumer was `combat_anim`, with three `&'static str` keys. The voice keyspace is the 105,517-entry Voices1 archive, and every distinct line spoken adds an entry.

  Misses add entries too. On every segment where the quest-prefixed candidate is absent, `get_or_load` inserts a `None` negative entry for it, and both candidates insert one when neither exists. These entries are uncounted by `len()` and `bytes_estimate()`.

  The decode is also synchronous inside the Late exclusive `npc_dialogue_selection` system. It decodes every segment of the line in one frame while holding the `SoundCache` write guard. The cost is unmeasured here, since the suite does not allow an engine launch.

  The cache fill happens before the `AudioWorld` gate. A headless or device-less session therefore decodes and retains voice PCM it will never play, because `play_oneshot` discards only after the decode.
- **Evidence**:
  - **Measured on Doc Mitchell's greeting** (`vcg01_greeting_00107222_1.ogg`, the line pinned by the BSA floor test): 30,418 B Ogg, mono 24 kHz, 120,050 frames, 5.0 s. Decoded, it is 120,050 × 8 B = **960,400 B retained per 5-second segment**, about 0.19 MB per voiced second at this sample rate.
  - **Extrapolation (not measured)**: at that rate, 1,000 distinct ~5 s lines is about 0.96 GB of resident PCM that is never freed.
  - ```rust
    // dialogue_voice.rs:142-151
    let mut cache = world.try_resource_mut::<byroredux_audio::SoundCache>()?;
    for response in 1..=segments {
        let candidates = voice_path_candidates(...);
        let sound = candidates.iter().find_map(|path| {
            cache.get_or_load(path, || provider.extract(path))
        });
    ```
- **Impact**:
  - Resident memory grows without bound with the number of distinct voiced lines heard.
  - It is visible in `ownership_sample` (`bytes_estimate`) but nothing acts on it.
  - A long FNV dialogue-heavy session, or TTW and mod voice packs, can push this into the GB range against a project budget of about 4 GB total.
  - Each first-time line costs a frame hitch for decoding.
- **Related**: #850 / AUD-D6-NEW-09 (`SoundCache` eviction strategy "manual, via `clear`"), #5367, GAME-D2-03 (stop semantics, not memory).
- **Suggested Fix**: Do not cache voice. Decode into a local `Arc<StaticSoundData>` that dies with the kira handle (voice lines are rarely replayed), or route voice through a separate byte-budgeted LRU. Move the decode off the frame thread, or decode only segment 1 synchronously, before taking the `SoundCache` guard. In either case, skip extract and decode entirely when `AudioWorld::is_active()` is false.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SIBLING**: Other `SoundCache` consumers checked for open-ended keys; negative (`None`) entries counted
- [ ] **TESTS**: A regression test pins this specific fix
