# Audio Subsystem Audit (M44) — 2026-10-21

**HEAD**: 4a3e89d56 · **Baseline**: [`AUDIT_AUDIO_2026-10-12.md`](AUDIT_AUDIO_2026-10-12.md) (HEAD `5d31fd637`) · **Audited**: Dim 1 (Spatial Dispatch & Listener: `build_spatial_track` #5738, orphaned-anchored prune #5678, `Attenuation` boundary), Dim 3 (Music & Caches: new script-audio consumers), Dim 4 (Manager & Schedule: new Update-stage producers, REGN gate sites after #5589), Dim 5 (Engine Consumers: `.fuz` voice #5912, ObScript `Say`/`PlayMusic`/`PlaySound`, Papyrus `Sound.Play`, footstep #5347/#5348/#5714, impact gate #5711) · **Unchanged since baseline (skimmed)**: Dim 2 (Send Graph). Its only commit is the doc fix `b0b9bc987` (#5677); the send/filter code is byte-identical.

- **Command**: `/audit-audio` (default delta scope), one leg of `/audit-suite --preset comprehensive`. Solo, no sub-agents; scratch notes in `/tmp/audit/audio/dim_1.md`–`dim_5.md`. No engine launch, and no audio played on the user's device.
- **kira**: `0.12.5`. The feature set is unchanged since the baseline (`Cargo.toml:213`: `cpal`, `cpal-realtime`, `wav`, `pcm`, `ogg`, `vorbis`, `mp3`, `flac`).
- **Tests run** (toolchain 1.96.0):
  - `cargo test -q -p byroredux-audio`: **43 passed, 0 failed, 9 ignored** (38 at the baseline; +5 come from `fuz.rs` and the #5678 orphan selection).
  - `cargo test -q -p byroredux --bin byroredux -- audio dialogue_voice footstep impact scheduler_access combat_anim scripted_sound`: **103 passed, 0 failed, 2 ignored**.
  - `--ignored real_fnv_sounds_decode_through_kira` (device-free, FNV data): **1 passed**.
  - **Not run**: the eight device-gated lifecycle guards (`looping_emitter_survives_natural_duration_and_stops_on_emitter_remove`, `non_looping_emitter_stops_on_emitter_remove_regression_858`, `play_music_looping_survives_track_end`, `play_music_drives_streaming_playback_on_real_ogg`, `emitter_position_follows_the_source_entity_regression_3086`, `anchored_voice_follows_survives_prune_and_stops`, `play_oneshot_queue_drives_real_playback`, `audio_system_full_lifecycle_on_real_fnv_sound`). The suite forbids opening the audio device. Despawn truncation, follow, the orphaned-anchored stop and music looping were verified by tracing the code against the kira 0.12.5 source.
- **Headless-mode boot: PASS** (`audio_world_constructs_without_panic_on_any_environment`, `explicit_headless_world_discards_playback_without_retaining_sound`).
- **Measurements (device-free, read-only)**:
  - **`.fuz` bridge on Fallout 4.** The census in the `fuz.rs` doc covers 40 SE files only. A Python census of the first 200 `.fuz` entries in `Fallout4 - Voices.ba2` (116,101 files in total) found 190 mono 44.1 kHz and 10 stereo 48 kHz streams. Every one is `RIFF/XWMA` with fmt tag `0x0161`, so `fuz_game` correctly includes FO4. I ran four of them through the engine's exact `ffmpeg` argument list (`-f s16le pipe:1`). Each output byte count equals the stream's last `dpds` entry (282,624 / 278,528 / 983,040 / 770,048), so the hand-built WAV header's channel count and rate are correct for both shapes.
  - **Script `PlayMusic` usage**: the text `PlayMusic` appears **13 times in FalloutNV.esm** (`musSCRGoodspringsStinger`, `musSCRVegasStinger` ×3, `MUSVault10106`, `MUSKimbleSpeechHail`, `EndSlideShow`, `Endgame01`, …) and **22 times in Fallout3.esm** (`Endgame01`–`18`, `Tension`, `defaultBattle`, `defaultExplore`). These are raw text hits (SCTX), not a decoded count.
  - **FNV loose music, decoded size.** The `MP3` headers say CBR, so these are estimates. `mus_SCR_GoodspringsStinger.mp3` is 192 kbps / 48 kHz stereo, about 30 s, about 11 MB as kira static PCM (8 B/frame). `mus_SCR_VegasStinger.mp3` is about 77 s, about 30 MB. `mus_endgameslideshow.mp3` is about 72 s, about 28 MB.

## Executive Summary

The baseline's five findings (#5675–#5679) are all closed, and their fixes hold: the impact position is the deepest contact point, the `--sounds-bsa` docs say last-wins, the send-limitation doc is corrected, an orphaned anchored voice is stopped through prune, and the audio docs were refreshed. #5738 (`build_spatial_track`) folded both dispatch loops onto one chain. #5348 and #5347 narrowed the footstep camera skip and the gate. #5711 disarms the physics force-event subscription when there is no impact content. #5912 brought Skyrim+ `.fuz` voice online through an optional host `ffmpeg`. All of these trace clean.

The new risk comes from the **M47.2/M47.3 script-audio producers**, which arrived without passing through the audio crate's API contracts:

1. **ObScript `PlayMusic` turns the whole audio mix into NaN (HIGH, D1-01).** It fakes a non-spatial sound with `Attenuation { min: 1e9, max: 1e9 }`. kira's linear attenuation divides `(d - min) / (max - min)`, so every output frame becomes `0/0 = NaN` for the life of the track. Indoors, the NaN also enters the reverb send's feedback buffers and stays there, which kills every later reverberated sound until restart. The `Attenuation` boundary that #1612 hardened against reversed ranges still accepts the degenerate equal range.
2. **`PlayMusic` also bypasses the music path (MEDIUM, D3-01).** It does a full static PCM decode on the main thread (about 11–30 MB per FNV track, re-decoded on every call, even with no audio device). The track is a spatial sub-track pinned where the listener stood, so it pans as the player moves and takes on the interior reverb and the underwater low-pass. Nothing ever stops it. `PlaySound` (GECK: "non-locational") is likewise played as a positional one-shot at the player body.
3. **Script `Sound.Play` / `PlaySound` / graph `SoundPlay.*` land open-ended keys on the unbounded `SoundCache` (LOW, D3-02).** The skill calls out exactly this shape. They also decode with no `is_active` gate and ignore the attenuation that the SOUN `SNDD` record authors.

| Severity | NEW | Existing (cross-referenced, not re-reported) | Total |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 1 | 0 | 1 |
| MEDIUM | 1 | 0 | 1 |
| LOW | 3 | 0 | 3 |
| **Total** | **5** | — | **5** |

Cross-references (already tracked, not counted): PAR-D2-01 (`.fuz` ffmpeg synchronous, no timeout, under the `VoiceSoundCache` guard, predictable temp file; co-owned), PHYS-D2-2026-10-21-02 (contact-force subscription toggle timing, #5711 follow-up), #3816, #3301, #4747, #5349, #6140 (`lib.rs` over 2000 LOC), #6156 (`.fuz` path-builder duplication). No regressions: #3086, #4146, #5432, #5148, #4743, #5382, #5383, #5410, #5305, #5545, #5556 and #5675 all hold. Archive precedence is still `.rev()`.

## Lifecycle Invariant Matrix (HEAD `4a3e89d56`)

| Invariant | State | Anchor |
|---|---|---|
| Field-drop order `active_sounds → pending_oneshots → oneshots_requested → music → reverb_send → reverb_send_db → listener → manager → multi_listener_warned → underwater → emitter_position_updates` | HOLDS (declaration order re-read) | `crates/audio/src/lib.rs` `AudioWorld` |
| `headless()` never contacts a device; `SUB_TRACK_CAPACITY` 512 / `SEND_TRACK_CAPACITY` 32 | HOLDS | `AudioWorld::new` |
| Sticky listener | HOLDS | `sync_listener_pose` |
| `audio_system` body order listener → follow → underwater → drain → dispatch → prune | HOLDS | `lib.rs:1085-1090` |
| One spatial chain (`build_spatial_track`) for both dispatch loops | HOLDS (#5738; guard `both_dispatch_paths_build_the_filter_through_one_call_site`) | `lib.rs:1253` |
| One unit seam `bu_to_audio_space` | HOLDS | `every_kira_position_site_goes_through_the_unit_seam` |
| `Attenuation` is a valid kira distance window | **DRIFTED**: a reversed range is normalized (#1612), but `min == max` passes through and makes kira divide 0/0 (D1-01) | `lib.rs:954` |
| Despawn truncation: `Emitter` on emitter removal, `Anchored` on `GlobalTransform` loss (#5678) or `stop_sounds_for` | HOLDS by trace; orphan selection pinned default-lane by `only_transformless_anchored_sources_are_prune_orphans` | `prune_stopped_sounds` |
| Music only through the main-track `play_music` slot with `StreamingSoundData` | **DRIFTED**: ObScript `PlayMusic` plays music as a static spatial one-shot (D3-01) | `script_requests.rs:225-261` |
| Open-ended-key consumer never lands on `SoundCache` | **DRIFTED**: script sounds do (D3-02); dialogue voice stays on `VoiceSoundCache` | `scripted_sound.rs`, `combat_anim.rs:409` |
| `SoundCache`/`VoiceSoundCache` → `SoundArchiveProvider` is the single nesting direction | HOLDS (`scripted_sound` goes through `play_oneshot_cached`; `voice_and_combat_sound_paths_share_one_cache_provider_order`) | `combat_anim.rs`, `dialogue_voice.rs` |
| Late exclusive chain `footstep → impact_audio → … → water_audio → audio_system`; `reverb_zone_system` parallel | HOLDS | `boot/schedule/late.rs:110-342` |
| Update producers (`scripted_line_voice_system` :529, `scripted_sound_system` :553, `music_system` :640) enqueue before Late `audio_system` | HOLDS (same-frame drain) | `boot/schedule/update.rs` |
| REGN music redispatch gated on `music_form` change | HOLDS at both remaining sites (`cell_loader/load.rs:776`, `scene/world_setup.rs:762`; #5589 folded the sync interior load into the apply job) | — |
| Footstep lock nesting acyclic | HOLDS (#5305, #5556; #5347 dropped the `WaterContact` edge) | `systems/audio.rs` |
| Archive precedence `.rev()` last-wins | HOLDS | `asset_provider/audio.rs:147` |

## Findings

### AUD-2026-10-21-D1-01: ObScript `PlayMusic` builds a spatial track with `min_distance == max_distance`; kira divides 0/0, so the whole mix goes NaN, and indoors the reverb send latches NaN for the rest of the session
- **Severity**: HIGH
- **Dimension**: Spatial Dispatch & Listener (the `Attenuation` boundary) / Engine Consumers
- **Location**: `byroredux/src/systems/script_requests.rs:218-261` (`MUSIC_RANGE`, `music_system`); `crates/audio/src/lib.rs:948-958` (`Attenuation::distance_range`); kira 0.12.5 `src/track/sub/spatial_builder.rs:355-358`, `src/track/sub.rs:312-323`
- **Status**: NEW (introduced by the M47.3 ObScript `PlayMusic` work, `07bd22a6b`/`6f44874ac`, 2026-10-18; sibling of the closed #1612 reversed-range panic)
- **Description**: `music_system` plays every scripted music track through `play_oneshot(sound, listener, Attenuation { min_distance: 1.0e9, max_distance: 1.0e9 }, volume)`. Its doc says the intent is a level "no listener can be farther from". `distance_range()` normalizes only the order of the two values, so kira receives `1e9..=1e9`. kira's default attenuation function is `Some(Easing::Linear)`, and its spatial stage computes:
  ```rust
  let distance = distance.clamp(self.min_distance, self.max_distance);   // = 1e9
  (distance - self.min_distance) / (self.max_distance - self.min_distance) // = 0.0 / 0.0 = NaN
  ```
  `Easing::Linear.apply(1 - NaN)` returns NaN. `Decibels::interpolate(SILENCE, IDENTITY, NaN)` returns NaN. `as_amplitude(NaN)` falls through both early returns (`== 0.0`, `<= SILENCE`) into `10f32.powf(NaN/20)`, which is NaN. Every frame of the track is multiplied by NaN. The sub-track output is then summed into the main track (NaN + anything = NaN). kira's renderer clamp (`backend/renderer.rs:107`, `f32::clamp`) does not sanitize NaN. When `reverb_send_db > -60` (every interior, `reverb_zone_system` sets -12 dB), `build_spatial_track` also routes the track to the reverb send, where `add_input` feeds the NaN into the reverb's feedback state.
- **Evidence**: The arithmetic was reproduced in f32 (`0/0 → nan`, `amp → nan`). There are 13 `PlayMusic` sites in FalloutNV.esm (Goodsprings and Vegas stingers, `MUSVault10106`, the end slides) and 22 in Fallout3.esm (`Endgame01`–`18`, `Tension`). `MusicTracks::from_index` is installed from `actor_value_names.rs:292`, opcode `0x11ff` is live in `obscript_more/extras.rs`, and FNV's tracks exist loose under `Data\Music\`, so the path plays real content.
- **Impact**: While a scripted track plays (about 30–77 s for the FNV stingers), the device receives NaN for the whole mix. Depending on the backend that is silence or full-scale noise, and every other sound is lost with it. If the call happens in an interior, the reverb send's feedback keeps circulating NaN after the track ends, so every later interior sound routed to reverb (all of them, at -12 dB) poisons the main mix again. The audio stays dead until the process restarts. Nothing logs an error.
- **Related**: #1612 (closed: reversed range panicked kira's render thread; same boundary, other degenerate case); AUD-2026-10-21-D3-01 (why music should never be a spatial one-shot).
- **Suggested Fix**: Harden the boundary: make `distance_range()` guarantee `hi > lo` (for example `hi = hi.max(lo + lo.abs().max(1.0) * f32::EPSILON * 4.0)`), with a unit test on an equal-range `Attenuation`. Then route scripted music through a non-spatial main-track path (D3-01) instead of a sentinel range.

### AUD-2026-10-21-D3-01: ObScript `PlayMusic` (and `PlaySound`'s non-locational semantics) bypass the music path: synchronous full static decode, a positional sub-track pinned at the listener's old spot, and no stop
- **Severity**: MEDIUM
- **Dimension**: Music & SoundCache / Manager & ECS Lifecycle
- **Location**: `byroredux/src/systems/script_requests.rs:222-261`; `crates/scripting/src/obscript_actions.rs:220-235` (`play_sound`); `byroredux/src/systems/scripted_sound.rs:51-66`
- **Status**: NEW
- **Description**: The audio crate has two outputs: spatial one-shots, and the single main-track music slot (`play_music`, `StreamingSoundData`, crossfade, `stop_music`). Scripted music uses neither correctly:
  1. **Decode.** `load_sound_from_bytes` produces a fully decoded `StaticSoundData` (kira static PCM is 8 B/frame) inside an Update exclusive on the main thread. The skill's invariant exists because of this: "never `StaticSoundData` (full PCM decode = OOM on long tracks)". FNV's stingers are about 11–30 MB each. Nothing caches the result, so every `PlayMusic` re-reads and re-decodes. The decode runs before the `AudioWorld` lookup, so a headless session pays it and then discards the sound.
  2. **Placement.** The sound is a `Detached` spatial sub-track at the `AudioListener`'s position at dispatch (or `Vec3::ZERO` with no listener). kira's spatialization strength defaults to 0.75, so as soon as the player moves or turns, the "music" pans toward the spot where they stood. It also picks up the interior reverb send and the underwater low-pass, which `play_music` never applies.
  3. **Lifetime.** A `Detached` one-shot runs to natural end. A cell change, door transition, F9 load or a second `PlayMusic` does not stop it, and several can stack. It is also counted in `oneshots_requested`, the total `combat.status` prints.
  4. **`PlaySound`.** The GECK says `PlaySound` "plays the specified sound, non-locational". The handler queues it from `PLAYER_REFERENCE`, and `scripted_sound_system` plays it at the player body's position with combat's 0.5–24 m window. The listener is the camera, so in third person the sound comes from about 2.6 m away and stays put. The SOUN `SNDD` flags that say "2D" / "Menu Sound" are not decoded (see D3-02).
- **Evidence**: `script_requests.rs:248` `load_sound_from_bytes(bytes)`, then `:258` `audio.play_oneshot(sound, listener, …)`. `SoundSource::Detached` is never stopped by prune or by `stop_sounds_for`.
- **Impact**: After D1-01 is fixed, scripted music still hitches the frame on every call (mp3 decode of tens of MB), holds tens of MB per playing track, sounds positional and reverberant, and leaks across cell and save boundaries. Non-locational script sounds are positional.
- **Related**: D1-01; #3816 (REGN music; FO3/FNV `MUSC` is now decoded into `EsmIndex::music_types` for this consumer); the `play_music` contract in Dim 3.
- **Suggested Fix**: Add a non-spatial main-track layer to `AudioWorld` (a second streaming slot, or `play_music`'s handle with a "do not stop the cell's music" layer, as the GECK describes) fed by `load_streaming_sound_from_bytes`/`from_file`. Stop it on cell transition and on load. Give `PlaySound` a non-spatial one-shot entry point, or honor the SOUN `2D` flag once `SNDD` is decoded.

### AUD-2026-10-21-D3-02: Script sound plays (Papyrus `Sound.Play`, graph `SoundPlay.*`, ObScript `PlaySound`/`PlaySound3D`) put open-ended, script-chosen paths on the unbounded `SoundCache`, decode with no device, and ignore authored attenuation
- **Severity**: LOW
- **Dimension**: Music & SoundCache / Engine Consumers
- **Location**: `byroredux/src/systems/scripted_sound.rs:20-70`; `byroredux/src/systems/combat_anim.rs:409-444` (`play_oneshot_cached`); producers `crates/scripting/src/fragment/vm/natives/forms.rs:39`, `fragment/anim_events.rs:146-163`, `obscript_actions.rs:230`, `obscript_effects.rs:953-959`
- **Status**: NEW (the shape the skill's Dim 3 marks as "the finding"; no issue or audit covers it, and dedup checked #850, #4739 and #5382, all closed)
- **Description**: `scripted_sound_system` resolves any SNDR/SOUN FormID a script names (including multi-`ANAM` variants taken in turn) and plays it through `play_oneshot_cached`, which inserts the decoded `Arc<StaticSoundData>` into the process-lifetime `SoundCache`. That cache has no eviction, and its negative entries are permanent until `clear()`, which nothing calls. Four producers now feed it: Papyrus `Sound.Play`, behaviour-graph `SoundPlay.<edid>` notifies (120 vanilla event names), ObScript `PlaySound` and `PlaySound3D`. Unlike the voice path (#5382, gated on `AudioWorld::is_active` before any archive work), `play_oneshot_cached` extracts and decodes even when the `AudioWorld` is headless and the play is discarded. Its miss log says `"combat sound: '…' not found"` for script sounds. Every script sound also takes combat's hardcoded `{0.5, 24.0}` m window, although FO3/FNV SOUN records author `SNDD` minimum/maximum attenuation distance (×5 and ×100 game units per `wbDefinitionsFNV.pas:7889`) plus `2D` / `Menu Sound` / `Environment Ignored` flags. `SounRecord` carries none of them.
- **Evidence**: `git grep PendingSoundPlays` lists the four producers. `SoundCache` keeps `Option<Arc<_>>` forever. `scripted_sound.rs:66` calls `play_oneshot_cached(world, &path, position)`.
- **Impact**: Over a long Skyrim/FO4 session, every distinct scripted or graph sound stays decoded in RAM, and headless sessions retain them too. Each one is small, but the set is open-ended in the way #5382 fixed for voice. Script sounds play with the wrong range, and 2D/menu sounds play positionally. The log mislabels misses.
- **Related**: #5382 (closed: same issue for voice, fixed with `VoiceSoundCache`); D3-01 (`PlaySound` non-locational).
- **Suggested Fix**: Route script sounds through a bounded cache (generalize `VoiceSoundCache`'s byte-budget LRU, or give script sounds their own instance) and gate the decode on `AudioWorld::is_active`. Pass the path's role into the miss log. Decode `SNDD` attenuation and flags into `SounRecord` and use them in `scripted_sound_system`.

### AUD-2026-10-21-D5-01: A `.fuz` temp-file write failure is reported as `FfmpegMissing` and logged at debug as "no host ffmpeg"
- **Severity**: LOW
- **Dimension**: Engine Consumers (voice)
- **Location**: `crates/audio/src/fuz.rs:228`; consumer `byroredux/src/systems/dialogue_voice.rs:280-292`
- **Status**: NEW (distinct from PAR-D2-01, which covers the synchronous/no-timeout spawn and the predictable temp name)
- **Description**: `std::fs::write(&temp, container.xwma).map_err(|_| FuzDecodeError::FfmpegMissing)?` maps any I/O failure (full or read-only temp dir, permissions) to the variant that means "the optional host tool is absent". The voice loader treats that variant as the expected degradation: `log::debug!` "resolved but no host ffmpeg — playing silent". It returns `None`, and that `None` becomes a `VoiceSoundCache` negative entry, so the line is not retried until the 4096-entry negative cap clears.
- **Evidence**: `fuz.rs:228`, `dialogue_voice.rs:282-288`.
- **Impact**: On a host that has `ffmpeg` but a full `/tmp` (a known failure mode on this machine; see the doctest tmpfs note), every Skyrim/FO4 line goes silent with only a debug line that names the wrong cause.
- **Related**: PAR-D2-01 (same function; fix together).
- **Suggested Fix**: Add a `TempWrite(std::io::Error)` variant (or reuse `FfmpegFailed` with the I/O error text) and log it at warn. Better, pipe the xWMA to ffmpeg's stdin if the demuxer allows it, which also removes PAR-D2-01's temp file.

### AUD-2026-10-21-D3-03: Audio docs lag the new codecs and consumers
- **Severity**: LOW
- **Dimension**: Music & SoundCache / docs
- **Location**: `crates/audio/src/lib.rs:41-45` (module doc), `crates/audio/src/lib.rs:1753` (`SoundCache` doc); `docs/feature-matrix.md:150-166`; `byroredux/src/asset_provider/audio.rs:225`
- **Status**: NEW for the `lib.rs` items. The feature-matrix rows are part of **Existing: #4747** (status docs).
- **Description**:
  - The `8dabcda8d` codec refresh says the workspace "enables WAV/PCM and OGG/Vorbis explicitly". It omits `mp3`, which now carries Oblivion voice (#5526), and `flac`. `load_sound_from_bytes`'s own doc lists all four correctly.
  - The module and `SoundCache` docs still name "dialogue line" as a `SoundCache` use. Voice moved to `VoiceSoundCache` in #5382, and the real open-ended `SoundCache` consumer (script sounds, D3-02) is not mentioned.
  - The feature matrix has no row for script sound plays (`Sound.Play`, `PlaySound`, `PlaySound3D`, graph `SoundPlay.*`) or scripted `PlayMusic`. It does not say that `.fuz` voice depends on an optional host `ffmpeg`, and the music row still reads "Streaming music (OGG, crossfade)".
  - The REGN dispatcher's comment "No `MUSC`/`MUST` or `MSET` runtime exists yet" is now half-true: FO3/FNV `MUSC` is decoded (`misc/music.rs`, consumed by `PlayMusic`). Only Skyrim `MUSC`, `MUST` and `MSET` remain undecoded.
- **Impact**: Readers of the module docs and the status page get a wrong picture of which codecs are load-bearing and which consumers exist.
- **Suggested Fix**: Fold these into the #4747 doc pass. Add `mp3`/`flac` to the module doc, retarget "dialogue line" to script sounds, add the script-audio rows, and narrow the REGN comment to Skyrim `MUSC`/`MUST` + FNV `MSET`.

## Dimension notes

- **Dim 1**: `build_spatial_track` (#5738) is the only sub-track builder, and `oneshot_slot_ready` is the shared prologue. The #5678 orphan sweep collects `Anchored` entities that have no `GlobalTransform` under one `GlobalTransform` read and drops that guard before the `AudioEmitter` query. Entity ids are never reclaimed, so an orphan can never alias a respawn. The only defect is D1-01.
- **Dim 2**: unchanged (doc-only `b0b9bc987`). D1-01 is the first way for a NaN to reach the global reverb send.
- **Dim 3**: `SoundCache`/`VoiceSoundCache` code is unchanged. The new consumers are D3-01 and D3-02.
- **Dim 4**: all lifecycle rows hold. The new Update producers enqueue before the Late drain. The REGN gate holds at both remaining sites.
- **Dim 5**: The footstep changes (#5348 camera-only skip resolved from `PlayerMode` + `ActiveCamera` before any guard; #5347 `!is_grounded` alone; #5714 one surface result per trigger without physics) trace clean, and each has a pinning test. The impact path keeps the #5675 contact position and the #5554 per-impact skip; the #5711 subscription sync is already reported as PHYS-D2-2026-10-21-02. ObScript `Say` cuts the speaker's previous line (`stop_line_voice`) before voicing and times `SayToDone` from the resolved segments. The `.fuz` resolve/decode works on SE (census in `fuz.rs`) and on FO4 (measured above); FO76 is in `fuz_game` but was not measured. The remaining `.fuz` issues are PAR-D2-01 and D5-01.

## Future-Phase Readiness

- **FOOT / IPDS / IPCT**: The mechanism is unchanged and ready (`FootstepConfig`/`ImpactConfig.surface_sounds` keyed by Havok u32). Oblivion, FO3 and FNV terrain now report its LTEX's Havok material (c53d87679, part of #6048), so footsteps on landscape have a real key once content exists. Skyrim/FO4 terrain still reads 0.
- **MUSC / MUST / MSET**: FO3/FNV `MUSC` is now decoded (`EsmIndex::music_types`). That makes CELL `XCMO` (`music_type_form`) routing possible on FNV without new parsing; nothing reads it yet. REGN on FNV targets `MSET` and on Skyrim targets `MUSC`/`MUST`, both still undecoded (#3816). Any consumer must use the streaming main-track path, never the D3-01 shape.
- **Occlusion / per-cell acoustics**: still unstarted. The one global reverb send now has a NaN hazard (D1-01) that a future per-zone send graph would inherit unless the `Attenuation` boundary is fixed first.

Publish with: `/audit-publish docs/audits/AUDIT_AUDIO_2026-10-21.md` (labels `audio`; add `game:fnv` + `game:fo3` on D1-01/D3-01, `scripting` on D3-01/D3-02).
