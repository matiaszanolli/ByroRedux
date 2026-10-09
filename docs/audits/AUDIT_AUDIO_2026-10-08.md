# Audio Subsystem Audit (M44) — 2026-10-08

**HEAD**: 00f580e09 · **Baseline**: [`AUDIT_AUDIO_2026-10-05.md`](AUDIT_AUDIO_2026-10-05.md) (HEAD `a2c24b16e`) · **Audited**: Dim 1 (Spatial Dispatch & Listener — `b4f08089b` #3086), Dim 4 (Manager & ECS Lifecycle — crate struct/pass order + `late.rs`), Dim 5 (Engine Consumers — `6915fe783` #5305, `f8950e7cc` #5367 Phase V dialogue voice) · **Unchanged since baseline (skimmed)**: Dim 2 (Send Graph), Dim 3 (Music; the `SoundCache` impl is unchanged, but its first open-ended consumer landed and is audited under Dim 5)

- **Command**: `/audit-audio` (default delta scope), one leg of `/audit-suite --preset comprehensive`. Solo, no sub-agents; scratch in `/tmp/audit/audio/dim_1.md`–`dim_5.md`.
- **kira**: `0.10.8`. `f8950e7cc` changed the workspace dep to `{ version = "0.10", features = ["wav", "ogg"] }`. That is a no-op, because kira's `default` already includes `cpal, mp3, ogg, flac, wav` and `default-features` is not disabled. The commit message's "kira gains the ogg feature" is inaccurate. Not filed.
- **Tests run**:
  - `cargo test -j 4 -p byroredux-audio`: **33 passed, 0 failed, 8 ignored**. The eighth ignored test is the new device-gated #3086 guard `emitter_position_follows_the_source_entity_regression_3086`.
  - `cargo test -p byroredux --bin byroredux -- audio combat_anim scheduler_access footstep dialogue_voice` (toolchain 1.96.0): **79 passed, 0 failed, 1 ignored**.
  - Not run: the five device-gated lifecycle guards (including #3086's), because the suite forbids opening devices. They are covered by tracing the code.
- **Measurement (device-free)**: extracted `sound\voice\falloutnv.esm\maleuniquedocmitchell\vcg01_greeting_00107222_1.ogg` from FNV `Fallout - Voices1.bsa` with `bsa_extract_one`, then parsed the Vorbis ident header and the last Ogg granule position.
  - Compressed: 30,418 B, mono, 24 kHz, 120,050 frames (5.002 s).
  - Decoded by kira into `Frame { f32, f32 }`: **960,400 B**, a 31.6× expansion.
- **Headless-mode boot: PASS** (`audio_world_constructs_without_panic_on_any_environment`, `explicit_headless_world_discards_playback_without_retaining_sound`).

## Executive Summary

Three commits since the baseline change audio behaviour.

1. **`b4f08089b` closes #3086.** `sync_emitter_positions` is the new pass 2 of `audio_system` (`crates/audio/src/lib.rs:966-986`).
   - Each entity-backed `ActiveSound`'s sub-track now follows its source's `GlobalTransform` through `bu_to_audio_space`.
   - The follow is change-gated on `last_position`, which both dispatch paths seed.
   - Queue sounds and music are untouched.
   - The code trace matches the contract, so this is not a regression. However, the new kira position site sits outside the unit-seam source guard (D1-01).
2. **`6915fe783` (#5305)** moves the footstep body gate ahead of every guard. It is behaviour-identical, and the residuals #5347, #5348 and #5349 are unchanged.
3. **`f8950e7cc` (#5367 Phase V)** adds the fourth `play_oneshot` caller, `dialogue_voice::play_line_voice`. It is also the first `SoundCache` consumer keyed on open-ended paths, which this skill names as the finding to look for: there is no eviction and nothing calls `clear()` (D5-01). It also takes `SoundArchiveProvider → SoundCache` in the reverse of `combat_anim`'s order (D5-02).

Per the gameplay-audit dedup, GAME-D2-02, GAME-D2-03 and GAME-D2-04 are not re-reported. D5-03 covers only the audio-backend side of the voice path.

| Severity | NEW | Existing (residual, not re-reported) | Total |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 2 | 0 | 2 |
| LOW | 3 | 8 | 11 |
| **Total** | **5** | **8** | **13** |

The existing residuals are #5347, #5348, #5349, #5148, #4743, #4747, #3816 and #3301. All are open and unchanged; see the disposition table.

No regressions:
- #3086: fixed.
- #4146: the reverb ordering comment is unchanged.
- Archive precedence: still `.rev()`. Voices1 is listed last, and FNV `Update.bsa` carries only 3 `sound\fx\…` entries, so there is no voice overlap.
- REGN change gates: untouched.

## Lifecycle Invariant Matrix (HEAD `00f580e09`)

| Invariant | State | Anchor |
|---|---|---|
| Field-drop order `active_sounds → pending_oneshots → oneshots_requested → music → reverb_send → reverb_send_db → listener → manager → multi_listener_warned → underwater → emitter_position_updates` | HOLDS. The new trailing `u64` has no drop semantics | `crates/audio/src/lib.rs` `AudioWorld` |
| `headless()` never contacts a device; capacities 512 / 32 | HOLDS (no diff) | `lib.rs` |
| Sticky listener | HOLDS (no diff) | `sync_listener_pose` |
| `audio_system` body: listener → **emitter follow (new)** → underwater → drain → dispatch → prune | HOLDS | `lib.rs:935-940` |
| `ActiveSound::track` kept alive and pushed to `active_sounds` on both paths; `last_position` seeded at dispatch | HOLDS | `lib.rs` drain (`~1176-1180`) and dispatch (`~1335-1339`) |
| Despawn truncation (tweened stop, `stop_issued` debounce, queue sounds exempt) | HOLDS by trace. `prune_stopped_sounds` has no diff; the device guards were not run | `lib.rs` |
| `footstep_system` / `water_audio_system` / `reverb_zone_system` / `audio_system` Late order | HOLDS. Pins pass (`footstep_runs_after_camera_follow_in_late`, `reverb_zone_is_parallel_and_audio_is_exclusive_in_late`) | `boot/schedule/late.rs:103`, `:152-226`, `:250` |
| Dialogue voice enqueue relative to `audio_system` | **New**: `npc_dialogue_selection` (Late exclusive, `late.rs:437`) is registered *after* `audio_system` (`:250`), so a voiced line drains on the next frame. The one-frame latency is harmless, and the Goodbye close (`close_after = now + voice_seconds`) trails the audio by about one frame | `late.rs` |
| `SoundCache` ↔ `SoundArchiveProvider` nesting has a single direction | **DRIFTED**. See D5-02 | `combat_anim.rs:383-389` vs `dialogue_voice.rs:138-142` |
| `SoundCache` has no open-ended consumer without eviction | **DRIFTED**. See D5-01 | `dialogue_voice.rs:142-151` |
| Footstep lock nesting is acyclic (ECS-2026-10-05-D1-01) | HOLDS since `6915fe783`. The gate is resolved before the guards | `systems/audio.rs:170-186` |

## Findings

### AUD-2026-10-08-D5-01: Dialogue voice keeps every voiced segment's decoded PCM in `SoundCache` for the life of the process. It is the first open-ended-key consumer, and there is no eviction or `clear()` caller
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

### AUD-2026-10-08-D5-02: The dialogue voice path nests `SoundArchiveProvider → SoundCache`, the reverse of `combat_anim`'s `SoundCache → SoundArchiveProvider`
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

### AUD-2026-10-08-D5-03: Voice lines ride the fire-and-forget queue path. The position is frozen at line start, a transform-less NPC voices from the world origin, and the backend exposes no handle to stop a line
- **Severity**: LOW
- **Dimension**: Engine Consumers / Spatial Dispatch
- **Location**:
  - `byroredux/src/systems/dialogue_voice.rs:178-195`.
  - `crates/audio/src/lib.rs:602-630` (`play_oneshot`), `:966-976` (follow pass skips `entity == None`).
- **Status**: NEW (audio-backend half only. The gameplay-side stop-on-topic-change/close/death is GAME-D2-03 and is not re-reported)
- **Description**:
  - `play_line_voice` schedules all segments of a line through `AudioWorld::play_oneshot` at the NPC's `GlobalTransform` sampled once, with `with_start_delay` chaining. That is the queue path: `entity: None` by contract. So:
    - The #3086 follow pass skips every voice segment. A line of tens of seconds, with chained segments, from an NPC that walks while talking (force-greet approach, followers, or a package resuming after Goodbye) stays anchored where the line began.
    - `world.get::<GlobalTransform>(npc).map(..).unwrap_or_default()` places the voice at the world origin `(0,0,0)` instead of skipping playback when the NPC has no transform.
    - Queue sounds have no stop path at all (`prune_stopped_sounds` only truncates entity-backed sounds). A line keeps playing across a door or cell transition at stale world coordinates. Interior and exterior coordinate spaces overlap, so the stale position can land near the new listener.
  - The crate gives GAME-D2-03's eventual fix nothing to call. `play_oneshot` returns no token, and the voice's `ActiveSound`s are indistinguishable from footsteps.
- **Evidence**:
  ```rust
  let position = world.get::<GlobalTransform>(npc).map(|t| t.translation).unwrap_or_default();
  …
  audio.play_oneshot(at, position, byroredux_audio::Attenuation::default(), 1.0);
  ```
- **Impact**:
  - Today the effect is minor, because NPCs mostly stand still while talking.
  - The latent part is structural: stopping, following or ducking voice requires a crate API change.
- **Related**: GAME-D2-03, #3086 (closed; the follow covers the entity path only), D5-01.
- **Suggested Fix**: Add a `play_oneshot_following(entity, …)` variant, or an `Option<EntityId>` tag on `PendingOneShot`, so that voice segments become entity-backed. The follow pass then tracks the NPC, and a `stop_sounds_for(entity, fade)` API gives GAME-D2-03 its hook. Skip playback, rather than defaulting to the origin, when the NPC has no `GlobalTransform`.

### AUD-2026-10-08-D4-01: The `npc_dialogue_selection` Access row does not declare the voice path's audio resources, and the #5307 source-scan guard does not follow `apply_selection → play_line_voice`
- **Severity**: LOW. The system is exclusive, so the under-declaration has no scheduling effect today.
- **Dimension**: Manager, ECS Lifecycle & Schedule
- **Location**:
  - `byroredux/src/boot/schedule/late.rs:437-493`: the Access row.
  - `byroredux/src/boot/schedule/mod.rs:1008-1018`: the guard.
  - `byroredux/src/systems/dialogue_voice.rs:98-195`.
- **Status**: NEW (introduced by `f8950e7cc`)
- **Description**: `play_line_voice`, reached from `apply_selection`, acquires:
  - `AudioWorld` (write) and `SoundCache` (write);
  - `SoundArchiveProvider` and `LoadedPluginSet` (read);
  - the `GlobalTransform` component (read).

  None appears on the row. #5307 fixed exactly this class for the fragment queues and explicitly argued that an under-declaration "would make a parallel-lane promotion of this row look safe when it is not". Its guard scans `npc_dialogue_selection_system_inner`, `apply_spoken_info_fragment`, `apply_fragment_guard_free` and `mark_scene_actor_bindings_dirty`, but not `apply_selection` or `play_line_voice`, so it stays green.
- **Evidence**: `grep -n "AudioWorld\|SoundCache\|SoundArchiveProvider\|LoadedPluginSet" byroredux/src/boot/schedule/late.rs` finds no hit in the `make_npc_dialogue_selection_system` row.
- **Impact**: An `access_report` built on this row under-reports conflicts with `audio_system` (exclusive, write `AudioWorld`) and `make_combat_feedback_system` (writes `SoundCache`).
- **Related**: #5307 (closed), D5-02.
- **Suggested Fix**:
  - Add `.writes_resource::<AudioWorld>()`, `.writes_resource::<SoundCache>()`, `.reads_resource::<SoundArchiveProvider>()`, `.reads_resource::<LoadedPluginSet>()` and `.reads::<GlobalTransform>()`.
  - Extend the source-scan tuple list with `(NPC_DIALOGUE_SRC, "apply_selection")` and the `dialogue_voice.rs` `play_line_voice` body.

### AUD-2026-10-08-D1-01: Neither new crate surface has a default-lane guard. The #3086 follow site is outside the unit-seam scan, and `with_start_delay` is untested
- **Severity**: LOW (test gap; the code is correct by trace)
- **Dimension**: Spatial Dispatch & Unit Seam
- **Location**:
  - `crates/audio/src/lib.rs:978-981`: `let position = bu_to_audio_space(gt.translation); … active.track.set_position(position, …)`.
  - `crates/audio/src/lib.rs:1513-1519`: `with_start_delay`.
  - `crates/audio/src/tests.rs:1433-1459`: the unit-seam guard.
- **Status**: NEW
- **Description**:
  - **Unit seam.** `every_kira_position_site_goes_through_the_unit_seam` pins the two listener sites and the two `add_spatial_sub_track` sites. `b4f08089b` added a third kind of kira position site, the per-tick emitter follow. Reverting its `bu_to_audio_space` to raw `gt.translation` would leave the guard green and silently place every moving emitter 70× too far away (inaudible past about 43 cm, which is the failure mode #3178 guarded). The only test of the follow, `emitter_position_follows_the_source_entity_regression_3086`, is `#[ignore]`d (device) and asserts the counter, not the units.
  - **`with_start_delay`.** It is the new crate API behind multi-segment voice and has no test of any kind. Device-free assertions are possible for:
    - the `StartTime::Delayed` setting;
    - shared `frames` (`Arc::ptr_eq`);
    - negative-delay clamping.
- **Evidence**: `grep -n "with_start_delay\|sync_emitter_positions" crates/audio/src/tests.rs` finds only doc and test-struct initialisers. No assertion covers either.
- **Impact**: A regression in either site would be silent in CI.
- **Related**: #3086 (closed), #3178.
- **Suggested Fix**:
  - Add the needle `"let position = bu_to_audio_space(gt.translation)"` (whitespace-squeezed) to the unit-seam test.
  - Add a headless unit test for `with_start_delay`'s settings and shared frames.

## Prior-Finding Disposition

| Issue | Baseline ID | State at HEAD | Evidence |
|---|---|---|---|
| #3086 (closed) | entity-path frozen position | **Fixed** (`b4f08089b`) | Trace in Dim 1. The device guard was not run here (suite rule). Test gap: D1-01 |
| #5347 (open) | 10-05 D5-01 swim gate duplicates `is_grounded` | **Unchanged** | `6915fe783` moved the `WaterContact` read out of the guard scope (`systems/audio.rs:178-185`) but kept the redundant operand. The test is unchanged |
| #5348 (open) | 10-05 D5-02 non-body emitters muted | **Unchanged** | `if entity != body \|\| gated` (`systems/audio.rs:227-232`) |
| #5349 (open) | 10-05 D5-03 stale doc sites | **Unchanged** | `scene.rs:1241` still names `setup_camera_and_lights` (0 definitions); `components.rs:1757` still says "will own this in future" |
| #5148, #4743 (open) | 09-29 D5-02 / 09-21 D5-02 | **Unchanged** | No diff to `combat_anim.rs` production code or `p2-melee-core.sh` |
| #4747 (open) | status docs | **Unchanged** | `ROADMAP.md:86` still says "footsteps, ambient, music". `docs/feature-matrix.md` has no dialogue-voice row for the Phase V that shipped |
| #3816, #3301 (open) | MUSC/REGN | **Unchanged** | No REGN dispatch diff |
| #4739–#4746 | 09-21 | Closed | `gh` reports CLOSED |

## Future-Phase Readiness

- **FOOT (3.5b)**: unchanged. #5348 still mutes per-NPC emitters, and ground material is not plumbed.
- **MUSC/MUST/MSET/RDMD (#3816)**: the mechanism is unchanged and fails closed.
- **Occlusion**: none.
- **Voice (Phase V and the Skyrim `.fuz` V2)**: the backend needs three things before it scales:
  1. a non-cached or LRU decode path (D5-01);
  2. an entity-backed, stoppable one-shot (D5-03, the GAME-D2-03 prerequisite);
  3. a single `SoundCache`/provider lock order (D5-02).

  `.fuz` (FUZE + lip + XMA2) also needs an XMA2 decoder. kira/symphonia has none.
- **`SoundCache` eviction**: now **needed**. See D5-01.

## Dedup Method

- **Open issues**: `/tmp/audit/issues.json` (113, reused). Titles were searched for audio, sound, footstep, music, reverb, oneshot, splash, swing, kira, emitter, voice, dialogue and flycam. The matches were #5367, #5347–#5349, #5148, #5066, #4747, #4743, #3816 and #3301. None covers voice caching, the cache/provider order inversion, voice spatial/stop semantics at the backend, the dialogue Access row's audio resources, or the new guard gaps.
- **Closed**: #3086, #4739 and #4746 were confirmed CLOSED via `gh`. #3086's fix was verified in code. #5307 is closed, and its guard scope was checked (D4-01).
- **Sibling report (2026-10-08)**: GAME-D2-02 (load-order slot to `--esm`), GAME-D2-03 (voice not stopped) and GAME-D2-04 (FO3 profile has no sound archives) were excluded as instructed. D5-03 is limited to the crate-API and spatial side.

Publish with `/audit-publish docs/audits/AUDIT_AUDIO_2026-10-08.md`. Use labels `audio` plus `memory`/`performance` (D5-01), `concurrency` (D5-02, D4-01), `dialogue` (D5-01, D5-03) and `test-gap` (D1-01). Add `game:fnv` on D5-01, because FNV Voices1 is the only voiced profile today.
