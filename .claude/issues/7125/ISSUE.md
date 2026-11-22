# #7125: PAR-D1-2026-11-20-02: one zero-filled 13 MB audio packet reaches the whole one-hour soundtrack ceiling: ~15.7 s blocking decode and ~1.35 GB RSS (+1.38 GB temp WAV) inside `Movie::open`

**Labels**: low,import-pipeline,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7125

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D1-02 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: Size Discipline
- **Location**: `crates/video/src/lib.rs:191-195,295-321` (`pcm_cap`, `bink2_audio`), `crates/binkaudio/src/decoder.rs:91-123` (`Decoder::decode`, `min_block_bits`)
- **Status**: NEW (residual of the accepted BIK-AUD-1 design: 538992417 capped the ceiling at one hour rather than at what the file could justify)
- **Trigger Input**: a `.bk2` with `frames = 1_000_000`, `fps = 1/1` (duration clamps to 3600 s), one stereo 48 kHz DCT track, and in frame 0 one audio packet of ~13.3 MB of zero bytes stating `decoded_bytes = 1,382,400,000` (any value up to the cap in bytes; here exactly the 691.2 M-sample cap).
- **Description**: the `decode` bound (`blocks * min_block_bits <= 8 * data.len()`) is correct but lets a zero packet expand 104x (7,680 B of PCM per 74 B). The caller's only per-packet limit is the whole-movie `pcm_cap`, so a single packet may consume all of it, synchronously, on the caller's thread: `Movie::open` is called from `VideoPlayer::step` on the frame loop (`byroredux/src/video.rs:263`). Header fields can pick the cap up to `MAX_SOUNDTRACK_SAMPLES` 691.2 M samples.
- **Evidence**: probe `bink_probe abomb 180000` (release build, data = `vec![0; 13_275_008]`, size = 180000 blocks): `decode` returns Ok, 691,200,000 samples in 15.73 s, max RSS 1,350,716 kB, 104.1x the input; `write_wav` then writes 1.38 GB through a `BufWriter` of 2-byte `write_all`s to `temp_dir()` (tmpfs on many systems, i.e. more RAM). 20,000 blocks from 1.47 MB took 1.74 s.
- **Impact**: a hostile mod movie freezes the game for tens of seconds and spends ~1.4-2.8 GB of RAM / tmpfs; it is bounded (no abort on typical hardware), which is why it is LOW. Honest files are unaffected: all 236,690 corpus packets fit a per-frame bound by construction.
- **Related**: BIK-AUD-1 (INC3-1 in the decoder), PAR-D1-01.
- **Suggested Fix**: also bound each packet by the time one frame may hold (`rate * channels * (1/fps + slack)` samples, slack ~2 s) before `Decoder::decode`, so a packet cannot spend the whole-movie budget; decode and stream the WAV per frame instead of holding the PCM `Vec`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
