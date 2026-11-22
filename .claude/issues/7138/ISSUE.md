# #7138: PAR-D5-2026-11-20-01: A Bink 1 header with a zero fps denominator is accepted, then played at an invented 25 fps with a 3-second soundtrack cap

**Labels**: low,import-pipeline,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7138

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D5-01 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: Decode-Consumer Wiring
- **Trigger Input**: Bink 1 (`BIKi`) header whose fps divider field is 0 (or whose dividend is 0). Not in any installed file; reachable from a mod's `.bik`.
- **Location**: `crates/video/src/lib.rs:72-81` (`describe`), `:600` (`Movie::advance`), `:221`/`:225` (`pcm_cap` input); contrast `:89` (`bink2_container`).
- **Status**: NEW
- **Description**: `describe` turns a non-positive fps divider into `fps = 0.0` and `duration_secs = 0.0` instead of an error, and `Movie::open` accepts it. The Bink 2 path refuses the same condition (`file.fps_num == 0 || file.fps_den == 0` -> `Unreadable`). Downstream, the consumer substitutes defaults for the missing decoded field: `advance` uses `25.0` when `info.fps <= 0.0`, `byroredux/src/video.rs:269` computes `frames = round(0.0 * 0.0) = 0` for `video.status`, and `extract_audio` computes `pcm_cap(duration_secs = 0, ..)` = (0 + 2 s) x 1.5 = 3 s of PCM, so a movie with a real soundtrack has its audio truncated at 3 s (a warn is logged, the outcome is still `Played`).
- **Evidence**: probe `fps0` over a handcrafted 16x64 Bink 1 header: `fps divider 0` -> `probe: Ok(MovieInfo { fps: 0.0, duration_secs: 0.0, .. })` and `Movie::open: ok, info { fps: 0.0, duration_secs: 0.0 }`. (Header offsets: width/height/fps at 0x14/0x18/0x1c/0x20 as the parser reads them; confirmed by the parsed values.) Source: `let fps = if self.info.fps > 0.0 { self.info.fps } else { 25.0 };`.
- **Impact**: A damaged or hostile Bink 1 file plays at a made-up rate with its frame total reported as 0 and (if it has sound) 3 s of audio; no Failed/Skipped is reported, so BIK-DEC-001 ("at the file's frame rate") is silently replaced by a consumer default. Not reachable from vanilla content.
- **Related**: BIK-DEC-001, BIK-RPT-001; the Bink 2 refusal at `bink2_container`.
- **Suggested Fix**: Refuse `fps.0 == 0 || fps.1 == 0` in `describe`'s callers (`probe` / `Movie::open`) as `VideoError::Unreadable`, the same condition `bink2_container` already refuses, and drop the `25.0` fallback.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
