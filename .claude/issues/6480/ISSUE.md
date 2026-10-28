# PAR-D2-01: `.fuz` decoding builds its WAV header from the *last* xWMA `fmt ` chunk, read even past `data`, while `ffmpeg` decodes the *first*; the header arithmetic on the file's channel count and sample rate is unchecked

Labels: medium, import-pipeline, audio, bug
Source: docs/audits/AUDIT_PARSERS_2026-10-27.md

**Source**: `docs/audits/AUDIT_PARSERS_2026-10-27.md` finding `PAR-D2-01` (MEDIUM)
**Location (re-mapped to current tree)**: `crates/audio/src/fuz.rs`: `parse_xwma` (`fmt = Some(..)` overwritten per `fmt ` chunk, loop does not stop at `data`, ~:172-212) and `wav_header` (`channels * 2` as u16, `sample_rate * u32::from(block_align)`, ~:399-401). Consumer `byroredux/src/systems/dialogue_voice.rs` (~:282)

**Severity**: MEDIUM (silently wrong bytes; a panic in the dev profile). Dimension: Error Semantics. Untrusted input: mod/replaced `.fuz`. 78be43036 (#6307) bounded the `ffmpeg` run but not this path.

## Description
`fuz_to_wav_bytes` passes the raw xWMA to `ffmpeg` and labels the PCM it gets back with `stream.channels` / `stream.sample_rate` from our own `parse_xwma`. The two readers disagree on which chunk they use:
- ffmpeg's `libavformat/xwma.c` requires `fmt ` as the first chunk after `XWMA`, skips unknown chunks, and stops at `data` (from that source; not reproduced here).
- Our walker takes the *last* `fmt ` anywhere in the RIFF (`fmt = Some(..)` is overwritten; the `data` arm does not end the walk).
- The format-tag check (`WAVE_FORMAT_WMAUDIO2`) also reads our chunk, not ffmpeg's.

## Trigger input
A xWMA with a valid leading `fmt ` / `dpds` / `data`, plus a trailing `fmt ` with tag 0x0161:
- `channels` >= 0x8000: `channels * 2` overflows `u16`. That panics in the dev profile (overflow checks on; `[profile.release]` does not set them), on the main thread while the `VoiceSoundCache` write guard is held. In release the value wraps.
- Any other `channels` / `sample_rate`: the WAV header mislabels ffmpeg's real output; the line plays at the wrong speed/channel layout and returns `Ok`.

## Impact
A crash in a dev build when voiced dialogue starts, garbled voice in release. Vanilla `.fuz` files have one `fmt ` chunk and are unaffected.

## Related
#6307, #5912.

## Suggested Fix
Stop the chunk walk at `data` and keep the first `fmt ` (matches ffmpeg); better, take channels and rate from ffmpeg itself (e.g. have it emit a WAV container). Use `checked_mul` in `wav_header` and turn an overflow into `BadXwma`. Add a trailing-`fmt ` regression fixture.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling systems
- [ ] **TESTS**: A regression test pins this specific fix

