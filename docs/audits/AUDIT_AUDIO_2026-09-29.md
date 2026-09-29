# Audio Subsystem Audit (M44) — 2026-09-29

**HEAD**: `9fcfdc3fc` · **Baseline**: [`AUDIT_AUDIO_2026-09-22.md`](AUDIT_AUDIO_2026-09-22.md) (HEAD `ee6d3fb39`) · **Audited**: Dim 1 (Spatial Dispatch & Listener), Dim 3 (Music & SoundCache), Dim 4 (Manager & ECS Lifecycle), Dim 5 (Engine Consumers) · **Unchanged since baseline (skimmed)**: Dim 2 (Send Graph — only a rustfmt reflow of `underwater_mix`; guards spot-checked)

- **Command**: `/audit-audio` (default scope), one leg of `/audit-suite --preset comprehensive`.
  Each dimension was analysed serially in this session, with no sub-agents. Scratch files were
  `/tmp/audit/audio/dim_1.md`–`dim_5.md`, and this report was reconciled against all five.
- **kira**: pinned `0.10` and resolved to `kira-0.10.8` (unchanged).
- **Tests run**:
  - `cargo test -j 4 -p byroredux-audio`: **33 passed, 0 failed, 7 ignored**.
  - `cargo test -j 4 -p byroredux --bin byroredux -- audio combat_anim scheduler_access`: **70 passed, 0 failed, 1 ignored**.
  - Device-free `--ignored` real-data tests, both PASS:
    - `real_fnv_sounds_decode_through_kira` (FNV WAV + OGG decode).
    - `draugr_combat_sound_assets_extract_and_decode_when_available` (the three Skyrim combat WAVs).
  - Four lifecycle guards need an audio device and were **not run**, because this suite runs other audits concurrently and forbids audio-device processes. They were traced in code instead (Dim 4):
    - `looping_emitter_survives_natural_duration_and_stops_on_emitter_remove`
    - `non_looping_emitter_stops_on_emitter_remove_regression_858`
    - `play_music_looping_survives_track_end`
    - `play_music_drives_streaming_playback_on_real_ogg`
- **Headless-mode boot: PASS** (`audio_world_constructs_without_panic_on_any_environment`,
  `explicit_headless_world_discards_playback_without_retaining_sound`).

## Executive Summary

Since the 2026-09-22 baseline, three commits worked through that report's findings:
- `2f8538334` (Enhance audio systems and combat animations).
- `546366364` (integrate `SoundCache` into world and combat systems).
- `3978b5184` (#4700/#4708: the Draugr marker on the prebaked Skyrim path).

All nine baseline issues (#4739–#4747) are still **OPEN**. The current code shows:

**Fixed; the issues can be closed**
- **#4739**: combat sounds now go through the engine `SoundCache`. It has a negative cache, is installed at boot, and `bytes_estimate` is sampled. This confirms the gameplay audit's note.
- **#4740**: the tests now use `AudioWorld::headless()`.
- **#4741**: the dead line-number pointer in `late.rs` is gone.
- **#4742**: the player-swing early return is gone.
- **#4744**: the tautological path pin is replaced by a real-data decode test, and an extract miss now logs a warning.
- **#4745**: every smoke fixture now supplies `--sounds-bsa`, and the gate reads `oneshots_requested`.
- **#4746**: the ripple doc is corrected.

**Partially fixed; residuals re-reported below**
- **#4743**: the Draugr swing is still gated on its animation take (D5-03).
- **#4747**: the live ROADMAP status line still overclaims audio features (D5-04).

**New findings**
- **AUD-2026-09-29-D5-01 (MEDIUM)**: the P3 third-person camera (`a070baaad`) puts the only `FootstepEmitter` on a 180 BU boom. Turning the camera while standing still, or pressing V, plays footsteps. Separately, `footstep_system` has no grounded or swim gate, so swimming and horizontal motion in the air play dirt footsteps.
- **AUD-2026-09-29-D5-02 (LOW)**: the P2 smoke gate's new audio check (`oneshots_requested > 0`) is satisfied by the footstep that `combat.approach` itself produces. It cannot detect a silent combat-sound path.

The crate core has no findings: unit seam, listener, send graph, music slot, `SoundCache` contract, field-drop order and schedule.

**Severity counts (this run)**:

| Severity | NEW | Existing (residual) | Total |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 1 | 0 | 1 |
| LOW | 1 | 2 | 3 |
| **Total** | **2** | **2** | **4** |

No regressions. #4146 (reverb-zone comment drift) is still fixed: `late.rs:162-168` cites no line number and does not claim registration order. The archive precedence guard, `.rev()` with last-listed winning, holds through the `extract_first` refactor (`ec63d2636`).

## Lifecycle Invariant Matrix (HEAD `9fcfdc3fc`)

| Invariant | State | Anchor |
|---|---|---|
| Field-drop order `active_sounds → pending_oneshots → music → reverb_send → reverb_send_db → listener → manager → multi_listener_warned → underwater` | HOLDS. The new `oneshots_requested: u64` sits after `pending_oneshots` and has no `Drop` | `crates/audio/src/lib.rs:391-438` |
| `headless()` never contacts a device; `new()` = `..Self::headless()` + manager + send | HOLDS | `lib.rs:450-526` |
| Capacities 512 / 32 applied in `new()` | HOLDS | `lib.rs:451-458` |
| `AudioWorld::new()` boot-only (single production constructor) | HOLDS | `byroredux/src/boot/world.rs:188` (`SoundCache::new()` at `:189`) |
| Sticky listener (never cleared; kira capacity 8) | HOLDS. `sync_listener_pose` has no diff since baseline | `lib.rs:947-1015` |
| Despawn truncation (tweened stop over `unload_fade_ms`, `stop_issued` debounce, queue sounds exempt) | HOLDS. Code has no diff since baseline; the device-gated guards were not run and the code was traced instead | `lib.rs:1279-1360` |
| `audio_system` body order: listener → underwater → drain → dispatch → prune | HOLDS | `lib.rs:894-911` |
| `footstep_system` Late exclusive before `audio_system` | HOLDS, pinned by `footstep_runs_after_camera_follow_in_late` | `boot/schedule/late.rs:100-109` vs `:244` |
| `submersion_system` → `water_audio_system` → `audio_system` | HOLDS, pinned by `submersion_runs_after_camera_follow_and_before_water_audio` | `late.rs:148,218-229,244` |
| `reverb_zone_system` Late parallel, `audio_system` Late exclusive | HOLDS, pinned by `reverb_zone_is_parallel_and_audio_is_exclusive_in_late` | `late.rs:169-174,244` |
| PostUpdate `make_combat_feedback_system()` → Late drain, same frame | HOLDS | `boot/schedule/post_update.rs:137-140` |
| REGN music change-gated at every call site | HOLDS, 3 sites | `cell_loader/load.rs:704-714,1087-1097`; `scene/world_setup.rs:561-576` |
| `event_cleanup_system` is the last Late exclusive | HOLDS. The new dialogue-selection exclusive sits between `audio_system` and it and touches no audio state | `late.rs:426-441,496` |
| `SoundCache` → `SoundArchiveProvider` nesting has no reverse edge | HOLDS. No path takes `SoundArchiveProvider` then `SoundCache` | `systems/combat_anim.rs:382-399` |

## Findings

### AUD-2026-09-29-D5-01: Footsteps follow the camera, so third-person camera turns and the view toggle play footsteps; swimming and airborne motion also play dirt footsteps
- **Severity**: MEDIUM
- **Dimension**: Engine Consumers
- **Location**:
  - `byroredux/src/scene.rs:970-974`: `AudioListener` and the only `FootstepEmitter` are both on the camera entity.
  - `byroredux/src/systems/character.rs:610-613,699-712`: `THIRD_PERSON_BOOM_BU = 180.0` and `cam_pos = head_pos - forward * 180`.
  - `byroredux/src/systems/audio.rs:123-222`: `footstep_system`.
  - `byroredux/src/app_events.rs:532-539`: the V key calls `toggle_third_person`.
- **Status**: NEW
- **Description**: `footstep_system` adds up the XZ change of each `FootstepEmitter` entity's `GlobalTransform` and fires a footstep every 52.5 BU (`DEFAULT_STRIDE_THRESHOLD_BU`). The only emitter is the active camera. That was the right pose source while the camera was always at the player's eyes.
  - **Third person** (`a070baaad`, 2026-09-28): the camera sits 180 BU behind the head along the look vector. Its XZ position is the head's XZ minus 180·cos(pitch) along the yaw direction, so it moves when the player only looks around.
    - A full yaw turn with the body standing still moves the camera about 1131 BU along an arc. That fires about 21 footsteps. A 90° mouse flick fires about 5.
    - Pitch changes the radius of that arc, so looking up or down also counts.
    - Toggling the view moves the camera 180 BU at once, which fires one footstep (the same shape as the `single_large_jump_fires_one_footstep_only` test).
    - While walking in third person, the steps play at the boom, 2.6 m behind the body.
  - **Either view**: the system never reads the character's locomotion state (`CharacterController.is_grounded`, swimming). Swimming (the WATAL W1 route) and horizontal motion in the air (running jumps, falls) play the dirt-walk footstep at walking cadence.
- **Evidence**:
  - `git grep FootstepEmitter -- byroredux/src ':!*test*'` finds a single insert, at `scene.rs:974`.
  - `camera_look_rotation(yaw, pitch) = Ry(yaw)·Rx(pitch)`, with forward = rot·−Z (`systems/camera.rs:14-16`).
  - `footstep_system` has no `CharacterController`, `PlayerMode` or `PlayerCameraView` read (its Access, `late.rs:103-108`, lists only `FootstepConfig`, `FootstepScratch`, `AudioWorld`, `GlobalTransform` and `FootstepEmitter`).
- **Impact**: audible and reachable in normal play on the character route with a single key. Every mouse turn in third person produces footsteps, and all swimming does too. Workaround: stay in first person and avoid water.
- **Related**: PHYS-D4-2026-09-29-01 in `AUDIT_PHYSICS_2026-09-29.md`. It has the same boom root cause but a different consumer (camera-origin gameplay rays), so this finding cross-references it rather than duplicating it. Also #848, #3652, #4185 (footstep stage placement, closed) and FOOT phase 3.5b.
- **Suggested Fix**:
  - In `PlayerMode::Character`, drive footsteps from the player body (the `PlayerEntity` capsule's `GlobalTransform`) instead of the camera. Keep the listener on the camera.
  - Only accumulate stride while the character is grounded and not swimming. Otherwise re-seed `last_position` and zero the accumulator.
  - Add a unit test: an orbiting camera with a stationary body fires nothing.

### AUD-2026-09-29-D5-02: The P2 gate's `oneshots_requested > 0` check passes on the footstep from `combat.approach`, so it cannot detect a silent combat-sound path
- **Severity**: LOW
- **Dimension**: Engine Consumers
- **Location**:
  - `docs/smoke-tests/p2-melee-core.sh:311-318`: the gate.
  - `crates/audio/src/lib.rs:555-557,584`: one total counter across all producers.
  - `byroredux/src/commands/view.rs:196-203`: `combat.status`.
  - `byroredux/src/boot/world.rs:211-217` with `asset_provider/texture.rs:362-363`: the Skyrim default footstep loads whenever `--sounds-bsa` is present.
- **Status**: NEW. This follows up #4745, whose filed scope is otherwise fixed.
- **Description**: `2f8538334` added `--sounds-bsa "Skyrim - Sounds.bsa"` to `skyrim_se.env` and a gate requiring `oneshots_requested > 0` after the kill. That counter counts every `play_oneshot` call from every producer: footsteps, splashes, ripples and combat.
  - With the new fixture argument, `FootstepConfig.default_sound` is decoded at boot.
  - `combat.approach` places the capsule on a ring 96–144 BU from the target, well past the 52.5 BU stride. `footstep_system` therefore requests at least one one-shot before the first swing, regardless of combat sound.
  - The gate prints "PASS -- audio dispatch requested N one-shots" even if `play_oneshot_cached` never calls `play_oneshot`. That covers a wrong path, a cached miss, or a regression of #4742's early-return class.
- **Evidence**: see Location. The `FootstepEmitter` on the camera was seeded at boot, and `single_large_jump_fires_one_footstep_only` pins the one footstep per repositioning.
- **Impact**: test infrastructure only. The single end-to-end audio gate proves that some sound was requested, not that the combat sound family it was added for is working.
- **Related**: #4745 (fixed as filed; recommend closing it), #4742, AUD-2026-09-29-D5-01 (the footstep producer).
- **Suggested Fix**: either option works.
  - Read `oneshots_requested` just before the lethal swing and require a delta of at least 2 after it (impact + death voice).
  - Or expose per-`FeedbackSound` request counts on `combat.status` and gate on those.

### AUD-2026-09-29-D5-03: The Draugr swing sound still keys off installing the attack take, so strikes during an active take are silent (residual of #4743)
- **Severity**: LOW
- **Dimension**: Engine Consumers
- **Location**:
  - `byroredux/src/systems/combat_anim.rs:176-195`: an active take hits `continue`.
  - `:197-215`: the hit-reaction branch wins the frame.
  - `:217-233`: the swing is pushed only when the attack take is installed.
  - Module doc `:11-13`.
  - NPC strike cadence: `byroredux/src/systems/combat_ai.rs:212` and `byroredux/src/combat.rs:505-511` (`0.45 / weapon.speed`).
- **Status**: Existing: #4743 (residual after `2f8538334`)
- **Description**: `2f8538334` fixed three of #4743's four problems:
  - Impacts are queued for every `HitEvent` on a marked target before the Dead/take ladder (`:129-131`).
  - The killing blow queues Impact and DeathVoice.
  - The swing moved to the marked aggressor, at its own position.

  The swing itself is still pushed only in step 4, when the attack take is installed. A marked Draugr's attack cooldown is `0.45 / speed`, well under 1 s for any weapon speed of 0.5 or more. Its attack take (`2hmattackforwardb`) lasts 2.50 s per `docs/engine/p2-combat-anim-sound-fixture.md`. While that take is active, step 2 `continue`s, so about three of every four strikes in sustained combat make no swing sound. A Draugr that is hit on the same frame it strikes takes the step-3 hit branch and makes no swing sound either. The module doc says without qualification that a marked aggressor "starts its attack take and swing sound at its own position".
- **Evidence**: the line references above. The tests (`npc_aggressor_takes_the_attack_clip`) cover only the first strike from an idle Draugr.
- **Impact**: once combat is sustained, most enemy swings are silent. The enemy's attack rhythm is not audible even though every hit on the player lands.
- **Related**: #4743 (update it with this residual rather than closing it), AUD-2026-09-29-D5-02.
- **Suggested Fix**: push `(actor, FeedbackSound::Swing)` for every `HitEvent` whose aggressor carries the marker, in the same loop that queues impacts (`:129-131`). Keep the take gating for animation only, and correct the module doc.

### AUD-2026-09-29-D5-04: The live ROADMAP status line lists "ambient" and "music" as working, and the feature matrix still marks combat sound ✗ (residual of #4747)
- **Severity**: LOW
- **Dimension**: Engine Consumers
- **Location**:
  - `ROADMAP.md:69-70`: "kira spatial audio: footsteps, ambient, music, per-cell reverb, water routing (M44)".
  - `docs/feature-matrix.md:228`: "Authored attack/hit/death animation + sound | ✗ | P2 remainder".
- **Status**: Existing: #4747 (residual; the ROADMAP rewrite created a new site)
- **Description**: #4747's original sites are fixed:
  - `feature-matrix.md:156` now reads "◐ dispatch mechanism ✓ … awaits #3816".
  - `:148` qualifies the cache.
  - The archived M44 row (`docs/archive/roadmap-history.md:823`) describes the parallel-batch-before-exclusive mechanism.

  Session 92's ROADMAP rewrite added a new "What works today" line that overclaims two features:
  - **Music**: its only production producer is REGN dispatch, which plays nothing on any supported game until #3816.
  - **Ambient**: it has no producer at all. `git grep AudioEmitter\|spawn_oneshot_at -- byroredux/src` finds no production insert, and REGN incidental/loop sounds are ✗ (`feature-matrix.md:157`, #3301).

  In the other direction, the P2 combat tail (Draugr takes plus swing, impact and death-voice one-shots) has shipped on Skyrim since `ec3a18d2f`/`3978b5184`, but row 228 still says ✗.
- **Evidence**: see Location. `feature-matrix.md:156-157` contradicts `ROADMAP.md:69`.
- **Impact**: documentation only. The headline status overclaims two audio features, and the gameplay table underclaims one.
- **Related**: #4747, #3816, #3301.
- **Suggested Fix**:
  - Rewrite the ROADMAP line as "footsteps, water splash/ripple, Skyrim Draugr combat one-shots, per-cell reverb, underwater filter; REGN music mechanism awaits #3816".
  - Change row 228 to "◐ Skyrim Draugr (`systems/combat_anim.rs`)".

## Prior-Finding Disposition (baseline issues, all still OPEN)

| Issue | Baseline ID | State at HEAD | Evidence |
|---|---|---|---|
| #4739 | D3-01 hand-rolled cache | **Fixed**; close | `546366364`: `SoundCache` has a negative cache (`lib.rs:1517-1541`), is inserted at boot (`boot/world.rs:189`), `bytes_estimate` is sampled (`ownership_sample.rs:67-70`), and `play_oneshot_cached` routes through `get_or_load` (`combat_anim.rs:382-399`). Confirms the gameplay audit's note |
| #4740 | D4-01 tests open device | **Fixed**; close | `asset_provider/audio.rs` tests and `underwater_listener_state_persists` use `headless()`. The remaining `new()` sites are device probes only |
| #4741 | D4-02 dead line pointer | **Fixed**; close | `late.rs:236-243` names `camera_follow_system`, and the phantom quote has 0 hits |
| #4742 | D5-01 swing early return | **Fixed**; close | Player swing removed; `if let Some(anim_q)` (`combat_anim.rs:124`); `player_swing_without_marked_draugr_does_not_queue_feedback` |
| #4743 | D5-02 take-keyed sounds | **Partial**; update | See D5-03 |
| #4744 | D5-03 tautological pin | **Fixed**; close | Self-match removed; `draugr_combat_sound_assets_extract_and_decode_when_available` passes; an extract miss warns (`combat_anim.rs:393`) |
| #4745 | D5-04 no `--sounds-bsa` | **Fixed as filed**; close | All five fixtures carry `--sounds-bsa` plus a required-file entry (archives verified on disk); Skyrim adds `Animations.bsa`. Gate weakness is tracked as D5-02 |
| #4746 | D5-05 ripple doc | **Fixed**; close | `systems/audio.rs:1,226-229` |
| #4747 | D5-06 status docs | **Partial**; update | See D5-04 |

Still open and unchanged: #3086 (entity-path position frozen at dispatch), #3816 (MUSC/MUST/MSET/RDMD decode) and #3301 (REGN incidental/loop).

## Future-Phase Readiness

- **FOOT records → per-material footsteps (3.5b)**: not shipped. The footstep system still uses one default sound per game. Fixing D5-01 (moving the pose source to the player body and gating on grounded state) is the natural prerequisite: FOOT dispatch needs the body's ground contact and material, not the camera's.
- **MUSC/MUST/MSET/RDMD (#3816)**: the REGN dispatch mechanism is unchanged, change-gated at 3 sites, and fails closed on every layer. It still plays nothing in production.
- **Occlusion**: none. `Attenuation` is distance-only, and no new code touches it.
- **`SoundCache` eviction**: not needed yet. The only consumer uses three fixed `&'static str` keys. The first consumer keyed on open-ended paths (FOOT, REGN incidental, per-NPC SFX) must bring eviction. If the archive set ever becomes swappable at runtime, it must also call `clear()`, because negative entries are never re-probed and the provider is built only once, at boot.

## Dedup Method

- **Open issues**: checked against `/tmp/audit/issues.json`, pre-fetched by the suite (163 open issues). Titles were searched for audio, sound, footstep, music, reverb, oneshot, third-person, camera and smoke keywords.
- **Closed issues**: `gh issue list --state all --search "footstep in:title"` returned #4185, #3520, #3776, #2163, #1615, #3788, #932 and #848. None is the third-person or locomotion-gate defect.
- **Sibling reports**: checked today's `docs/audits/*_2026-09-29.md`.
  - PHYS-D4-2026-09-29-01 shares D5-01's root cause (the boom) but concerns gameplay rays. It is cross-referenced, not duplicated.
  - The gameplay report's note on #4739 is confirmed: fixed.
  - No sibling report covers the footstep, P2-gate or swing-gating defects.
