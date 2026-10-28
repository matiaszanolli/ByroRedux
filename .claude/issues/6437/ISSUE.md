# AUD-2026-10-27-D1-01: "Non-positional" sounds are placed at the listener's head, where kira's ear model plays them ~11 dB quieter, folded to mono, then panning as the player moves

Labels: medium, audio, bug, game:fnv, game:fo3, game:oblivion
Source: docs/audits/AUDIT_AUDIO_2026-10-27.md

**Report**: `docs/audits/AUDIT_AUDIO_2026-10-27.md` · **Finding**: AUD-2026-10-27-D1-01 · **Severity**: MEDIUM · **Dimension**: Spatial Dispatch & Listener (consumer: Engine Consumers)
**Status**: NEW (residual of #6226 / #6281)
**Location**: `byroredux/src/systems/scripted_sound.rs:42-64,115-126` -> `byroredux/src/systems/combat_anim.rs:471-493` -> `crates/audio/src/world.rs:377-385` (`play_oneshot`) -> `crates/audio/src/system.rs:218-239` (`build_spatial_track`)
**Games**: FO3 / FNV / Oblivion (ObScript `PlaySound`, `2D` / `Menu Sound` SOUN)

## Description
#6226 (f04c3433d) moved ObScript `PlaySound` (GECK: "non-locational") to the `AudioListener` entity's position. #6281 (41b53ab41) did the same for `2D` / `Menu Sound` SOUN records. Both still go through `play_oneshot`, which builds an ordinary spatial sub-track with kira's default `spatialization_strength` of 0.75. kira computes each ear's gain from the direction between that ear (+-0.1 m off the head centre) and the source. A source at the head centre lies opposite both ears' facing, so each ear's gain is 0.25 + 0.75*0.038 = 0.279, or **-11.1 dB**, after `as_mono()`, for any listener orientation. The distance term is full volume (d < min), so the level loss comes entirely from the ear term. The play is `SoundSource::Detached`, so its position is frozen at dispatch: when the player walks or turns during the sound, it pans off the frozen point. The position is also read in Update from the pre-propagation `GlobalTransform`, so the exact offset (and gain) depends on sub-frame motion.

## Evidence
- `kira-0.12.5/src/track/sub.rs:296-344` (`spatialize`: `min_ear_amplitude = 1 - strength`; `(left_ear_direction . normalize(position - left_ear_position) + 1) / 2`). `SpatialTrackBuilder::new()` defaults `spatialization_strength: Value::Fixed(0.75)` (`track/sub/spatial_builder.rs:60`).
- The engine never calls `spatialization_strength` (`grep -rn spatialization_strength crates byroredux` finds nothing; re-verified at HEAD).
- Source: `scripted_sound.rs` `NON_LOCATIONAL_SOURCE` -> `heard_at`, and `emitter_position` (`non_positional` -> `listener.or(source)`).

## Impact
Every ObScript `PlaySound` (FO3/FNV/Oblivion quest and UI cues) and every `2D`/`Menu Sound` record plays roughly 8-11 dB below a nearby positional sound, with its stereo image collapsed, and wanders as the player moves. Fly-cam footsteps are hit the same way: the camera carries both `AudioListener` and the fly-cam `FootstepEmitter` (`byroredux/src/scene.rs:936-944`), and `footstep_system` plays at the emitter's own position. Existing tests pin only the *position* (`a_non_locational_play_is_heard_at_the_listener`, `a_non_positional_sound_plays_at_the_player`), not the level.

## Related
#6226, #6281 (closed; their placement half is what this residual concerns). The audit skill's Dim 5 contract states the placement rule but not level. Future-phase: D1-01 is the first consumer needing a genuinely 2D one-shot (UI/menu sounds, FO4/Skyrim `2D` output models).

## Suggested Fix
Carry a `positional: bool` (or a strength value) on `PendingOneShot` through `build_spatial_track` and set `SpatialTrackBuilder::spatialization_strength(0.0)` for non-positional plays; kira then skips the ear model and applies only distance (full volume at the listener). Alternative: a non-spatial main-track one-shot API for `2D` sounds. Add a test that pins the builder setting.

## Completeness Checks
- [ ] **SIBLING**: `play_oneshot_following` and every other `queue_oneshot` caller (footstep, impact, dialogue voice) checked for the same listener-position placement
- [ ] **DROP**: Field-drop order in `AudioWorld` (`world.rs:153-211`) unchanged if a new field/track type is added
- [ ] **TESTS**: A regression test pins `spatialization_strength(0.0)` (or the main-track path) for non-positional plays

