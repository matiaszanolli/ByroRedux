# #7128: PAR-D2-2026-11-20-04: the main-thread open path (`Movie::open` / `probe`: container parse, `VideoDecoder::new`, both soundtrack decodes) runs outside `guard_decoder`

**Labels**: low,import-pipeline,safety,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7128

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D2-04 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW (hardening; no panic reproduced)
- **Dimension**: Error Semantics
- **Location**: crates/video/src/lib.rs `Movie::open`, `open_bink2`, `extract_audio`, `bink2_audio`; call site byroredux/src/video.rs:263 (main thread, `step` -> `start`)
- **Status**: NEW (BIK-DEC-004 guards only the reader thread; SKILL notes unguarded main-thread callers generally)
- **Description**: `spawn_reader`/`guard_decoder` cover the frame loop. `Movie::open` itself — `mercs2_bink::Movie::parse` + `VideoDecoder::new` (third-party 0.1.0), `extract_audio` (`AudioDecoder::decode_record`), `bink2_audio` (`binkaudio::Decoder::decode`, `Bk2::audio`) — runs on the engine's main thread with no `catch_unwind` (grep: only texture_streamer, terrain_height, cdb, pre_parse, fog_volumes, video reader use one). A panic in any of them aborts the process instead of ending the movie `Failed`.
- **Evidence**: none reproduced: 3,400 mutated Bink 1 `open` calls, 100k binkaudio packets and 120k container files gave 0 panics. The exposure is structural: the soundtrack decode is the longest-running stream-driven code on the main thread, and `mercs2_bink` is outside the audit's source rules.
- **Impact**: latent; a future decoder change or a mercs2_bink panic on a hostile `.bik` kills the engine, not the movie.
- **Trigger Input**: any `.bik`/`.bk2` reaching `Movie::open` (mod-supplied Video folder).
- **Suggested Fix**: run `Movie::open`'s body under the same `catch_unwind` -> `VideoError::Decode` conversion (or decode the soundtrack inside the reader thread's guard); add a hygiene scan next to `spawn_reader_is_the_only_reader_thread`. (Also: if `spawn_reader(..)?` fails after `extract_audio` succeeded, the temp WAV is leaked — trivial.)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
