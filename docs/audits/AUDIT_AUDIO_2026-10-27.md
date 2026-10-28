# Audio Subsystem Audit (M44) — 2026-10-27

**HEAD**: fa6be4a6b · **Baseline**: [`AUDIT_AUDIO_2026-10-21.md`](AUDIT_AUDIO_2026-10-21.md) (HEAD `4a3e89d56`) · **Audited**: Dim 1 (Spatial Dispatch & Listener: #6207 span fix, `queue_oneshot`, the #6140 crate split, the non-locational placement from #6226/#6281), Dim 3 (Music & Caches: the `script_music` layer #6226, the bounded script-sound path #6281), Dim 4 (Manager & Schedule: the new Update-stage music/sound producers and the cell-leave stop), Dim 5 (Engine Consumers: ObScript `PlaySound`/`PlayMusic`, Papyrus `Sound.Play`, `SNDD` attenuation, #6228 voice traits, the #6156 voice-path dedup, the #6307 `.fuz` bounds) · **Unchanged since baseline (skimmed)**: Dim 2 (Send Graph). `dsp.rs` is a b2e542eee move of the same send/filter code, and `reverb_zone_system` is untouched.

- **Command**: `/audit-audio` (default delta scope), one leg of `/audit-suite --preset comprehensive`. I ran it solo with no sub-agents. Scratch notes are in `/tmp/audit/audio/dim_1.md`–`dim_5.md`. I did not launch the engine and did not open an audio device.
- **kira**: `0.12.5`. The feature set is unchanged (`Cargo.toml:213`).
- **Tests run** (repo `target/`):
  - `cargo test -q -p byroredux-audio`: **52 passed, 0 failed, 9 ignored** (43 at the baseline).
  - `cargo test -q -p byroredux --bin byroredux -- audio dialogue_voice footstep impact scheduler_access combat_anim scripted_sound script_requests music`: **112 passed, 1 failed, 2 ignored**. The one failure is `scheduler_access_tests::per_frame_driver_functions_stay_within_their_size_pins`, the `render_one_frame` size pin. That is AUDIT_REGRESSION_2026-10-27 REG-2026-10-27-01, not audio.
  - **Not run**: the device-gated lifecycle guards (`looping_emitter_survives_natural_duration_and_stops_on_emitter_remove`, `non_looping_emitter_stops_on_emitter_remove_regression_858`, `play_music_looping_survives_track_end`, `play_music_drives_streaming_playback_on_real_ogg`, `emitter_position_follows_the_source_entity_regression_3086`, `anchored_voice_follows_survives_prune_and_stops`, `play_oneshot_queue_drives_real_playback`, `audio_system_full_lifecycle_on_real_fnv_sound`). The suite rules forbid opening a device. Despawn truncation, follow, the orphaned-anchored stop and music looping were checked by tracing the code against the kira 0.12.5 source instead.
- **Headless-mode boot: PASS** (`audio_world_constructs_without_panic_on_any_environment`, `explicit_headless_world_discards_playback_without_retaining_sound`).
- **Measurements (device-free, read-only)**:
  - **kira's ear model at zero distance** (`kira-0.12.5/src/track/sub.rs:296-344`). The ears sit at ±0.1 m and face `rot_y(∓π/8)·(∓X)`, with the default `spatialization_strength` of 0.75. A source exactly at the listener's head is "behind" both ears: dot = −cos(π/8) = −0.924, so each ear's volume is 0.038 and its gain is 0.25 + 0.75·0.038 = **0.279 (−11.1 dB)**. This holds for any listener orientation. For comparison, a source 1 m ahead gets about −2.7 dB, and a source directly to one side about −0.2 dB at the near ear.
  - **MUSC census** (Python walk over the ESMs). `Fallout3.esm` has 31 MUSC records, 6 of them with a folder `FNAM`: `base\`, `explore\`, `dungeon\`, `public\`, `battle\`, `tranquilitylane\`. `FalloutNV.esm` has 20 MUSC records with the same 6 folders. The FNV records carry `ANAM`; for example `musSCRVegasStinger` is −7.74 dB and `musDocMitchellHouse` is +6.0 (loop). The FO3 records carry no `ANAM`. On disk, `Fallout 3 goty/Data/Music/Battle/` holds `Battle_01.mp3`…

## Executive Summary

The baseline's five findings are all closed (#6207, #6226, #6281, #6307, #6282) and their fixes hold:
- An equal attenuation range no longer reaches kira as a zero span (`distance_range` pushes `max` a few ULPs up).
- Scripted `PlayMusic` streams on its own non-spatial main-track layer and stops at both cell-leave choke points.
- Script sound plays use the bounded `VoiceSoundCache`, check `is_active` before any archive work, and honour `SNDD` attenuation.
- The `.fuz` ffmpeg bridge is bounded in time and size and uses an `O_EXCL` temp file.
- The #6140 split kept every source-scan pin non-vacuous: `production_source_lists_every_module_file` covers all five modules.

The one material defect this round is a **residual of #6226/#6281**. Both fixes make a non-locational sound (ObScript `PlaySound`, or a `2D`/`Menu Sound` SOUN) audible "at the listener" by placing a normal spatial one-shot at the listener's exact position. In kira's ear model that is the quietest place a source can be: about **−11 dB on both ears, folded to mono**. Because the sound is `Detached`, it then pans as the player moves. kira 0.12 can switch spatialisation off per track (`spatialization_strength(0.0)`), and the engine never uses it.

| Severity | NEW | Existing (cross-referenced, not re-reported) | Total |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 1 | 0 | 1 |
| LOW | 2 | 0 | 2 |
| **Total** | **3** | — | **3** |

Cross-references (already tracked, not counted):
- **PAR-D2-01** (AUDIT_PARSERS_2026-10-27): the `.fuz` WAV header is built from the *last* xWMA `fmt ` chunk, and `wav_header` has an unchecked multiply (`crates/audio/src/fuz.rs`).
- Open issues: #3816 (REGN music targets never resolve), #3301 (REGN incidental/ambient loops), #4747 and #5349 (doc-rot).

No regressions: #3086, #4146, #4743, #5148, #5305, #5382, #5383, #5410, #5432, #5545, #5556, #5675, #5678, #6207, #6226 (music half), #6281 and #6307 all hold. Archive precedence is still `.rev()`.

## Lifecycle Invariant Matrix (HEAD `fa6be4a6b`)

| Invariant | State | Anchor |
|---|---|---|
| Field-drop order `active_sounds → pending_oneshots → oneshots_requested → music → script_music → reverb_send → reverb_send_db → listener → manager` (then the flags and the counter) | verified | `crates/audio/src/world.rs:153-211` |
| `AudioWorld::new()` boot-only | verified (single production site) | `byroredux/src/boot/world.rs:207` |
| Sticky listener (never cleared; failed `add_listener` retries) | verified | `system.rs:142-204` |
| Despawn truncation (`Emitter` only; `stop_issued` debounce; retain drops on `Stopped`) | verified by trace (guards `#[ignore]`d) | `system.rs:499-607` |
| Orphaned `Anchored` voice stops via prune on `GlobalTransform` loss (#5678) | verified | `system.rs:510-553`, `world.rs:117-122` |
| Emitter follow change-gated; `Detached` untouched (#3086) | verified | `system.rs:83-102` |
| Shared `build_spatial_track` on both dispatch loops (#5738) | verified | `system.rs:218-239`, :291, :421 |
| `queue_oneshot` → `oneshot_slot_ready` (count before gate, 256 cap, `pop_front`) | verified | `world.rs:360-423` |
| `distance_range` gives `min < max` (#1612, #6207) | verified | `components.rs:53-60` |
| Schedule: footstep → impact → (parallel reverb_zone) → water_audio → `audio_system` (all Late) | verified | `boot/schedule/late.rs:110,135,202,322,346` |
| Script producers `scripted_line_voice` / `scripted_sound` / `music_system` are Update exclusives (same-frame dispatch) | verified | `boot/schedule/update.rs:613,637,726` |
| `stop_script_music` at both cell-leave choke points | verified (pinned by `leaving_a_cell_stops_the_scripted_music`) | `cell_loader/transition.rs` |
| Music on the main track, streaming only, both slots through `play_main_track` | verified | `cache.rs:59-104`, `world.rs:477-518` |

## Findings

### AUD-2026-10-27-D1-01: "Non-positional" sounds are placed at the listener's head, where kira's ear model plays them about 11 dB quieter, folded to mono, and then panning as the player moves
- **Severity**: MEDIUM
- **Dimension**: Spatial Dispatch & Listener (consumer: Engine Consumers)
- **Location**: `byroredux/src/systems/scripted_sound.rs:42-64,115-126` → `byroredux/src/systems/combat_anim.rs:471-493` → `crates/audio/src/world.rs:377-385` → `crates/audio/src/system.rs:218-239`
- **Status**: NEW (residual of #6226 / #6281)
- **Description**: #6226 (f04c3433d) moved ObScript `PlaySound` (GECK: "non-locational") to the `AudioListener` entity's position. #6281 (41b53ab41) did the same for `2D` / `Menu Sound` SOUN records. Both still go through `play_oneshot`, which builds an ordinary spatial sub-track with kira's default `spatialization_strength` of 0.75. kira computes each ear's gain from the direction between that ear (±0.1 m off the head centre) and the source. A source at the head centre lies opposite both ears' facing, so each ear's gain is 0.25 + 0.75·0.038 = 0.279, or −11.1 dB, after `as_mono()`. That holds for any orientation. The distance term is full volume (d < min), so the level loss comes entirely from the ear term. The play is `SoundSource::Detached`, so its position is frozen at dispatch. When the player walks or turns during the sound, it pans off the frozen point. The position is also read in Update from the pre-propagation `GlobalTransform`, so the exact offset, and therefore the gain, depends on sub-frame motion.
- **Evidence**: `kira-0.12.5/src/track/sub.rs:296-344` (`spatialize`: `min_ear_amplitude = 1 - strength`; `(left_ear_direction · normalize(position - left_ear_position) + 1) / 2`). `SpatialTrackBuilder::new()` defaults `spatialization_strength: Value::Fixed(0.75)` (`track/sub/spatial_builder.rs:60`). The engine never calls `spatialization_strength` (`git grep` finds nothing). In the source: `scripted_sound.rs:116-118` (`NON_LOCATIONAL_SOURCE` → `heard_at`) and `:50-51` (`non_positional` → `listener.or(source)`).
- **Impact**: Every ObScript `PlaySound` (FO3/FNV/Oblivion quest and UI cues) and every `2D`/`Menu Sound` record plays roughly 8–11 dB below a nearby positional sound, with its stereo image collapsed, and wanders as the player moves. Fly-cam footsteps are hit the same way: the camera carries both `AudioListener` and the fly-cam `FootstepEmitter` (`byroredux/src/scene.rs:936-944`), and `footstep_system` plays at the emitter's own position (`systems/audio.rs:331`). The existing tests pin only the *position* (`a_non_locational_play_is_heard_at_the_listener`, `a_non_positional_sound_plays_at_the_player`), not the level.
- **Related**: #6226, #6281 (closed; their placement half is what this residual concerns). The skill's Dim 5 contract states the placement rule but does not cover level.
- **Suggested Fix**: Carry a `positional: bool` (or a strength value) on `PendingOneShot` through `build_spatial_track` and set `SpatialTrackBuilder::spatialization_strength(0.0)` for non-positional plays; kira then skips the ear model and applies only distance, which is full volume at the listener. An alternative is a non-spatial main-track one-shot API for `2D` sounds. Either way, add a test that pins the builder setting.

### AUD-2026-10-27-D3-01: Folder-form music types (`DefaultBattle`, `DefaultExplore`, `TranquilityLaneLP`, …) reach `PlayMusic` and fail with a misleading "failed to decode" warning
- **Severity**: LOW
- **Dimension**: Music & SoundCache
- **Location**: `byroredux/src/systems/script_requests.rs:246-266`; `crates/scripting/src/obscript_more/extras.rs:45-52`; `docs/feature-matrix.md:159`
- **Status**: NEW
- **Description**: In both Fallout3.esm and FalloutNV.esm, 6 MUSC records author a folder `FNAM` (`battle\`, `explore\`, `base\`, `dungeon\`, `public\`, `tranquilitylane\`): a directory of variant tracks. Fallout3.esm scripts call `PlayMusic` on `defaultBattle` / `defaultExplore` (2026-10-21 SCTX census). `MusicTracks::from_index` filters out only empty files, so these reach `music_system`. There, the archive lookup misses on `music\battle\`. `find_case_insensitive(Data, "Music\battle\")` filters the empty trailing component and returns the existing `Data/Music/Battle` **directory**. `load_streaming_sound_from_file(dir)` then errors, and the system warns `PlayMusic: 'music\battle\' failed to decode`. Nothing plays, and the log blames the decoder for what is really an unmade variant-selection policy. For SOUN, this exact shape is fenced by `SounRecord::is_folder` / `sound_is_folder` ("any future consumer must gate on it"); the MUSC path has no such gate. The feature matrix marks scripted `PlayMusic` ✓ with no qualifier.
- **Evidence**: The census figures above. `script_requests.rs:254` (`find_case_insensitive(&data, &format!("Music\\{}", track.file))`), `:288` (`.filter(|part| !part.is_empty())`), `:259-261` (the decode warning).
- **Impact**: FO3/FNV scripted battle and explore music cues are silent. Each attempt logs a WARN that sends triage toward the codec.
- **Related**: `sound_is_folder` (`byroredux/src/asset_provider/audio.rs`), #3914 (SOUN folder form), #3816.
- **Suggested Fix**: Gate a trailing-separator `file` before resolution with a once-per-type debug or info line ("folder music type — variant selection not modelled"), mirroring `sound_is_folder`, and qualify the matrix row. Choosing a variant is a separate policy decision, so do not add a fallback.

### AUD-2026-10-27-D1-02: The public `AudioWorld::play_oneshot` has no rustdoc; its contract paragraph sits on the crate-private `oneshot_slot_ready`
- **Severity**: LOW
- **Dimension**: Spatial Dispatch & Listener (doc-rot)
- **Location**: `crates/audio/src/world.rs:338-385`
- **Status**: NEW (dates from eac602644 / #5738; carried through 3a50b3d5b and b2e542eee)
- **Description**: The "Fire-and-forget one-shot dispatch …" paragraph runs straight into "#5738 — the shared `play_oneshot*` prologue …" as one doc block attached to `pub(crate) fn oneshot_slot_ready`. That paragraph documents the #853 drop-on-inactive behaviour and the 256-entry FIFO cap. As a result, the engine's main producer API, `pub fn play_oneshot` (`world.rs:377`), renders with no documentation. `play_oneshot_following` links back to it ("as [`Self::play_oneshot`]"), and `oneshots_requested` refers to "calls to `Self::play_oneshot`".
- **Evidence**: `world.rs:338-359` is a single `///` run ending above `oneshot_slot_ready`; `world.rs:377` has no `///`.
- **Impact**: Documentation only. Producers reading rustdoc lose the contract that a headless world drops the play up front and that the queue sheds its oldest entry at 256.
- **Related**: #6160 (the `queue_oneshot` fold).
- **Suggested Fix**: Move the first paragraph block back onto `play_oneshot` and keep only the #5738 prologue note on `oneshot_slot_ready`.

## Future-Phase Readiness

- **FOOT / IPDS / IPCT** (per-material footstep and impact content): the mechanism is unchanged and live. `surface_material_under` with its mandatory self-exclusion, `select_footstep_sound`, `resolve_impact_material` and the #5711 subscription gate all still hold. The tables still ship empty, so `impact_audio_system` remains a production no-op.
- **MUSC / MUST / MSET** (#3816): FO3/FNV `MUSC` now has a consumer (scripted `PlayMusic`). REGN-driven music still never resolves, and the CELL `XCMO`/`XCMT` and WRLD `default_music` fields are still unread in `byroredux/src` (re-grepped). A CELL-level MUSC caller will need its own change gate (Dim 4), and folder-form MUSC (D3-01) needs a selection policy first.
- **Non-spatial playback**: D1-01 is the first consumer that needs a genuinely 2D one-shot. Whatever fixes it, a per-track strength or a main-track one-shot, should also become the path for future UI/menu sounds and FO4/Skyrim `2D` output models.
- **Occlusion**: not started; no change.

---
Suggested next step: `/audit-publish docs/audits/AUDIT_AUDIO_2026-10-27.md` (labels `audio`; D1-01 adds `game:fnv`/`game:fo3`/`game:oblivion` because ObScript `PlaySound` is the main trigger; D3-01 adds `game:fo3`/`game:fnv`; D1-02 adds `doc-rot`).
