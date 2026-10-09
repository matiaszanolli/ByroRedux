# #5383: AUD-2026-10-08-D5-02: The dialogue voice path nests `SoundArchiveProvider → SoundCache`, the reverse of `combat_anim`'s `SoundCache → SoundArchiveProvider`

**Labels**: medium,audio,concurrency,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5383

**Source**: `docs/audits/AUDIT_AUDIO_2026-10-08.md` — `AUD-2026-10-08-D5-02` (HEAD `00f580e09`)

- **Severity**: MEDIUM. There is no runtime deadlock today, for two reasons. `SoundArchiveProvider` is never write-locked after boot, so the inversion is read-vs-write on two locks where one side is read-only. The two callers also run in different stages (PostUpdate exclusive and Late exclusive). However, this is a lock-order contract violation that the type-keyed detector reports as a cycle. It would become HIGH once a test exercising both paths lands in the `BYRO_LOCK_ORDER_CHECK` lane, as ECS-2026-10-05-D1-01 did.
- **Dimension**: Engine Consumers
- **Location**:
  - `byroredux/src/systems/dialogue_voice.rs:138-142`: the provider read is held, then the cache write is taken.
  - `byroredux/src/systems/combat_anim.rs:383-389`: the cache write is held, then the provider read is taken inside the `get_or_load` loader.
- **Status**: NEW (introduced by `f8950e7cc`)
- **Description**: This skill's invariant for `play_oneshot_cached` is that the loader reads `SoundArchiveProvider` while the `SoundCache` write guard is held, and that no path may take them in the reverse order. `play_line_voice` takes `let provider = world.try_resource::<SoundArchiveProvider>()?` first and then `world.try_resource_mut::<SoundCache>()?` while still holding it, which is the reverse.

  The global lock-order graph (`crates/core/src/ecs/lock_tracker.rs` `record_and_check`) is keyed on type, ignores read vs write, and is process-wide. It records:
  - `SoundArchiveProvider → SoundCache` on every voiced line;
  - `SoundCache → SoundArchiveProvider` on every combat-sound cache miss.

  Whichever comes second closes the cycle and panics under `BYRO_LOCK_ORDER_CHECK=1`. That happens in any session, or any test process, that both loads a combat sound for the first time and plays a voiced line.

  `play_line_voice` also runs under `npc_dialogue_selection_system_inner`'s `LoadedCellIndex` read guard, which the shadowing at `npc_dialogue.rs:478-481` keeps alive (#5066, open). It re-reads `LoadedCellIndex` at `dialogue_voice.rs:100` as a recursive read, so the voice path also adds `LoadedCellIndex → SoundArchiveProvider/SoundCache/AudioWorld/GlobalTransform` edges.
- **Evidence**:
  ```rust
  // dialogue_voice.rs:138-142
  let provider = world.try_resource::<SoundArchiveProvider>()?;
  ...
  let mut cache = world.try_resource_mut::<byroredux_audio::SoundCache>()?;   // Provider → Cache
  // combat_anim.rs:383-389
  .try_resource_mut::<byroredux_audio::SoundCache>()
  .and_then(|mut cache| cache.get_or_load(path, || {
      let provider = world.try_resource::<…SoundArchiveProvider>()?;          // Cache → Provider
  ```
- **Impact**:
  - The concurrency/ECS lock-order lane will go red as soon as a scenario covers both, as was seen five times before in *lock_order_lane_blind_spots*.
  - If the provider ever gains a runtime writer (archive hot-swap, which is also the event that would need `SoundCache::clear()`), this becomes a real ABBA between the two systems.
- **Related**: #5066 (the `LoadedCellIndex` guard span), ECS-2026-10-05-D1-01 / #5305 (the same detector class, fixed for footsteps), D5-01. The shared fix site is the same block.
- **Suggested Fix**: Mirror `combat_anim`. Take `SoundCache` first and read the provider inside the `get_or_load` loader. Alternatively, extract all candidate bytes under the provider guard, drop it, and then take the cache. Add a lock-order-lane test that runs `play_oneshot_cached` and `play_line_voice` in one process.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SIBLING**: Every `SoundCache` / `SoundArchiveProvider` acquisition site uses the single cache→provider order
- [ ] **TESTS**: A regression test pins this specific fix
