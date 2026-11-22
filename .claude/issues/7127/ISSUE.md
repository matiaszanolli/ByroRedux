# #7127: PAR-D2-2026-11-20-02: Bink 2 bit reader reads zeros past the end of a packet and nothing checks the over-read: a truncated video packet decodes `Ok` to a different picture, never `Failed`

**Labels**: low,import-pipeline,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7127

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D2-02 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: crates/bink2/src/bits.rs:62 (`peek`, `bit`), crates/bink2/src/lib.rs:226 (`decode_into`) / :413 (`decode_slice`); crates/binkaudio/src/bits.rs (same, audio)
- **Status**: NEW (the zero-fill is a deliberate documented choice — "ours, not probed" — and binkaudio bounded the *size* side in 706f95884, but no end-of-slice `r.pos > 8*data.len()` verdict exists on the video side)
- **Description**: `Bits` returns 0 for every bit past `data`. `decode_into` never compares the final `r.pos` with the packet length, and `Bk2::video` accepts any packet length the (validated) frame index gives. A packet cut short by the encoder/mod therefore finishes decoding the remaining macroblocks from zero bits, returns `Ok(&Frame)`, and `Movie` reports a clean frame instead of `Failed(frame N does not decode)`; contract (c) wants a documented Ok+warn or an Err.
- **Evidence**: probe `short` on the 96x32 probe stream's frame 0 (24-byte packet): full packet -> Y-plane FNV `65961a2b50454325`; packet cut to 4 bytes (flags word only) -> `Ok`, Y-plane `bb2cbe5e5ab51325` (a different picture); cuts 12..23 -> `Ok` and identical to full (the tail is genuinely zero bits). Only cuts below 4 bytes give `Err(ShortPacket)`. Same at threads 1 and 4.
- **Impact**: corrupt/mod Bink 2 files show garbage frames and play to the end as Played. Not a safety issue (0 panics in 900k fuzz decodes).
- **Trigger Input**: KB2g/KB2j video packet whose bitstream is shorter than its macroblocks need (e.g. 4-byte packet for a 96x32 keyframe).
- **Related**: #6855, BIK-DEC-004
- **Suggested Fix**: do NOT simply error on any over-read: the probe shows valid streams may omit trailing zero bits. Measure the corpus first (max over-read bits per slice across the installed `.bk2` set); then warn or `Err(Error::Overrun)` above the measured maximum, and pin it with a test.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
