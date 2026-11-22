# #7139: PAR-D5-2026-11-20-02: Bink 1 soundtrack path ignores the track-ID rule and ends the audio silently on a missing record or a split error; `video.status` reports Played either way

**Labels**: low,import-pipeline,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7139

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D5-02 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Trigger Input**: (a) a Bink 1 file with several audio tracks whose first is not the ID-1 one (the installed Skyrim `BGS_Logo.bik`, 2 tracks, and a mod's `threeweekslater_withSound.bik`, 2 tracks, have more than one); (b) a Bink 1 frame whose `movie.split(frame)` errors or yields no audio record.
- **Location**: `crates/video/src/lib.rs:207-229` (`extract_audio`; `movie.tracks.first()` :210, `.ok().and_then(..)` :220, `Ok(false)` :221) vs `:260-265` (`original_track_index`) / `:269-` (`bink2_audio`); `byroredux/src/video.rs:256-285`, `commands/video.rs:43-45`.
- **Status**: NEW. This entry absorbs Dim 2's PAR-D2-03, whose truncation probe cut a real `BGS_Logo.bik` to 99/90/60/30 %. The soundtrack WAV shrank from 1,221,364 to 1,205,804, 852,524 and 506,924 bytes with no audio-side log line, while the video side warned that frame 146 runs past the file.
- **Description**: BIK-DEC-002 and `docs/engine/video-playback.md` §2 state one rule — play the track with ID 1, else the first — but only the Bink 2 path applies it (`original_track_index`); the Bink 1 path hard-codes `tracks.first()`. Second, the Bink 1 frame callback converts a `split` error into `None` (`.ok()`) and `Ok(false)` ("end quietly") with no log line, while the Bink 2 path logs "its audio stops at frame N" for a decode error and skips (does not stop at) a frame with no packet. So a Bink 1 soundtrack can end early with no diagnostic. In both paths a soundtrack that never started or stopped early still yields `VideoOutcome::Played { ended_early: false }`; the only trace is the crate's `log::warn!` and `soundtrack=no` in the `playing` line, not in `video.status`.
- **Evidence**: `let track = movie.tracks.first()?;` and `let record = movie.split(frame as _).ok().and_then(|(records, _)| records.first().copied()); let Some(record) = record else { return Ok(false) };` vs Bink 2 `let Some(packet) = packet else { return Ok(true) };`. Probe `dim5probe`: every installed Bink 1 movie with audio, including the Skyrim/FO4 two-track files, decode a soundtrack as long as the picture (no silent truncation in the corpus), and the doc states Skyrim's two tracks are sample-identical, so there is no observed misbehaviour today.
- **Impact**: Latent. A mod's multi-track Bink 1 file whose tracks differ plays track 0 whatever its ID, and a corrupt record mid-file silently shortens the sound (the clock then falls back to the wall clock, `movie_clock`). Corpus unaffected.
- **Related**: BIK-DEC-002 (#6896), BIK-DEC-004 (frames are reported Failed, audio is not).
- **Suggested Fix**: Take the track's ID through the same `original_track_index` once the `mercs2_bink` track exposes it (or document that Bink 1 plays track 0), warn on a `split` error / missing record the way `decode_soundtrack` warns for an `Err`, and add a "soundtrack ended early / absent" note to `video.status` (or the `Played` outcome) so a silent play is visible there.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
