# #5410: AUD-2026-10-08-D5-03: Voice lines ride the fire-and-forget queue path. The position is frozen at line start, a transform-less NPC voices from the world origin, and the backend exposes no handle to stop a line

**Labels**: low,audio,dialogue,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5410

**Source**: `docs/audits/AUDIT_AUDIO_2026-10-08.md` — `AUD-2026-10-08-D5-03` (HEAD `00f580e09`)

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

## Completeness Checks
- [ ] **SIBLING**: Other queue-path `play_oneshot` callers reviewed for the same origin-default / no-stop behaviour
- [ ] **TESTS**: A regression test pins this specific fix
