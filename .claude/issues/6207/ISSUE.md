# #6207: AUD-2026-10-21-D1-01: ObScript `PlayMusic` builds a spatial track with `min_distance == max_distance`; kira divides 0/0, so the whole mix goes NaN, and indoors the reverb send latches NaN for the rest of the session

**Labels**: high,audio,bug,game:fnv,game:fo3

**Source**: docs/audits/AUDIT_AUDIO_2026-10-21.md · **Severity**: HIGH · **Dimension**: Spatial Dispatch & Listener (the `Attenuation` boundary) / Engine Consumers · **Location**: `byroredux/src/systems/script_requests.rs:218-261` (`MUSIC_RANGE`, `music_system`); `crates/audio/src/lib.rs:948-958` (`Attenuation::distance_range`); kira 0.12.5 `src/track/sub/spatial_builder.rs:355-358`, `src/track/sub.rs:312-323`

**Audit status**: NEW (introduced by the M47.3 ObScript `PlayMusic` work, `07bd22a6b`/`6f44874ac`, 2026-10-18; sibling of the closed #1612 reversed-range panic)

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

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every `Attenuation { .. }` literal and every `play_oneshot` caller)
- [ ] **TESTS**: A regression test pins this specific fix
