# #6226: AUD-2026-10-21-D3-01: ObScript `PlayMusic` (and `PlaySound`'s non-locational semantics) bypass the music path: synchronous full static decode, a positional sub-track pinned at the listener's old spot, and no stop

**Labels**: medium,audio,scripting,bug,game:fnv,game:fo3

**Source**: docs/audits/AUDIT_AUDIO_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Music & SoundCache / Manager & ECS Lifecycle · **Location**: `byroredux/src/systems/script_requests.rs:222-261`; `crates/scripting/src/obscript_actions.rs:220-235` (`play_sound`); `byroredux/src/systems/scripted_sound.rs:51-66`

**Audit status**: NEW

- **Description**: The audio crate has two outputs: spatial one-shots, and the single main-track music slot (`play_music`, `StreamingSoundData`, crossfade, `stop_music`). Scripted music uses neither correctly:
  1. **Decode.** `load_sound_from_bytes` produces a fully decoded `StaticSoundData` (kira static PCM is 8 B/frame) inside an Update exclusive on the main thread. The skill's invariant exists because of this: "never `StaticSoundData` (full PCM decode = OOM on long tracks)". FNV's stingers are about 11–30 MB each. Nothing caches the result, so every `PlayMusic` re-reads and re-decodes. The decode runs before the `AudioWorld` lookup, so a headless session pays it and then discards the sound.
  2. **Placement.** The sound is a `Detached` spatial sub-track at the `AudioListener`'s position at dispatch (or `Vec3::ZERO` with no listener). kira's spatialization strength defaults to 0.75, so as soon as the player moves or turns, the "music" pans toward the spot where they stood. It also picks up the interior reverb send and the underwater low-pass, which `play_music` never applies.
  3. **Lifetime.** A `Detached` one-shot runs to natural end. A cell change, door transition, F9 load or a second `PlayMusic` does not stop it, and several can stack. It is also counted in `oneshots_requested`, the total `combat.status` prints.
  4. **`PlaySound`.** The GECK says `PlaySound` "plays the specified sound, non-locational". The handler queues it from `PLAYER_REFERENCE`, and `scripted_sound_system` plays it at the player body's position with combat's 0.5–24 m window. The listener is the camera, so in third person the sound comes from about 2.6 m away and stays put. The SOUN `SNDD` flags that say "2D" / "Menu Sound" are not decoded (see D3-02).
- **Evidence**: `script_requests.rs:248` `load_sound_from_bytes(bytes)`, then `:258` `audio.play_oneshot(sound, listener, …)`. `SoundSource::Detached` is never stopped by prune or by `stop_sounds_for`.
- **Impact**: After D1-01 is fixed, scripted music still hitches the frame on every call (mp3 decode of tens of MB), holds tens of MB per playing track, sounds positional and reverberant, and leaks across cell and save boundaries. Non-locational script sounds are positional.
- **Related**: D1-01; #3816 (REGN music; FO3/FNV `MUSC` is now decoded into `EsmIndex::music_types` for this consumer); the `play_music` contract in Dim 3.
- **Suggested Fix**: Add a non-spatial main-track layer to `AudioWorld` (a second streaming slot, or `play_music`'s handle with a "do not stop the cell's music" layer, as the GECK describes) fed by `load_streaming_sound_from_bytes`/`from_file`. Stop it on cell transition and on load. Give `PlaySound` a non-spatial one-shot entry point, or honor the SOUN `2D` flag once `SNDD` is decoded.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every other script-driven music/sound entry point (Papyrus, graph notifies))
- [ ] **TESTS**: A regression test pins this specific fix
