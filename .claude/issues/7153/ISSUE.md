# #7153 — PAR-D1-2026-11-21-01: Bink 1 decodes a frame's whole audio record before the per-frame cap is checked; a record's decoded-size claim reserves ~20x the record

Labels: high,import-pipeline,bug,safety

- **Severity**: HIGH (abort/OOM reachable from a mod file)
- **Dimension**: Size Discipline
- **Location**: `crates/video/src/lib.rs` `extract_audio` (`pcm.extend(decoder.decode_record(record)…)`, ~l.329) and `decode_soundtrack` (the `pcm.len() > soundtrack.frame_cap` check, ~l.375); contrast the Bink 2 pre-check `packet.decoded_bytes as usize / 2 > soundtrack.frame_cap` (~l.446)
- **Status**: NEW. Incomplete fix of #7125 (closed): the commit says "both codecs stop at a frame past it", but only Bink 2 refuses before decoding.
- **Trigger Input**: a loose `Data\Video\*.bik` (revision `i`) with one audio track whose frame-0 audio record is large and zero-filled. The record's leading u32, the decoded-bytes claim, is set near `0x7fff_fff0`.
- **Description**: the frame cap is applied to `pcm.len()` after `decode_record` has returned its whole output. The decoder honours the record's size claim, and a claim of 8192 yields exactly 8192 bytes of WAV. With a huge claim it keeps decoding until it runs off the record ("an audio block reads past its record"). The error is graceful, but the output buffer has already been reserved and filled. `Movie::open` runs this synchronously on the engine's frame loop (`byroredux/src/video.rs`).
- **Evidence**: probe `binkprobe audiohuge 2147483632 <record_bytes>` (release, black-box API use, no dependency source read):

  | record | peak RSS | wall |
  |---|---|---|
  | 4 MiB | 88 MB | 0.37 s |
  | 16 MiB | 341 MB | 1.47 s |
  | 64 MiB | 1,364 MB | 6.1 s |

  The coordinator re-read `extract_audio` and `decode_soundtrack`: the cap check runs after `decode_frame` returns.
- **Impact**: a hostile or broken mod movie freezes the game for seconds and costs memory in proportion to its file size. A few hundred MiB of record reaches the multi-GB range, where an allocation failure aborts the process.
- **Related**: #7125, PAR-D1-02 of 2026-11-20 (the Bink 2 twin, fixed by #7125's pre-check).
- **Suggested Fix**: in the Bink 1 frame closure, read `record[..4]` as the decoded-bytes claim and return `Err` when `claim / 2 > frame_cap`, mirroring the Bink 2 check, before calling `decode_record`. Add a test that drives `extract_audio` with that hostile record, since today's tests drive `decode_soundtrack` only through closures. Re-run the real-data sweep to confirm no vanilla record trips the check (the slack was measured over 58 files with sound).

### MEDIUM

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
