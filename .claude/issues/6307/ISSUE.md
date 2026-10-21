# #6307: PAR-D2-01: `.fuz` voice decoding shells out to `ffmpeg` synchronously, with no timeout and no output cap, under the `VoiceSoundCache` write guard; the temp file is non-exclusive and a failed temp write is reported as "ffmpeg missing"

**Labels**: low,bug,audio,import-pipeline,game:skyrim,game:fo4

**Source**: docs/audits/AUDIT_PARSERS_2026-10-21.md · **Severity**: LOW · **Dimension**: Error Semantics · **Location**: `crates/audio/src/fuz.rs` (`fuz_to_wav_bytes`); caller `byroredux/src/systems/dialogue_voice.rs`

- **Location**:
  - `crates/audio/src/fuz.rs:207-260` (`fuz_to_wav_bytes`).
  - Caller: `byroredux/src/systems/dialogue_voice.rs:264-296` (inside `cache.get_or_load`).
- **Status**: NEW (acd577a79, #5912)
- **Trigger Input**: any `.fuz` that makes `ffmpeg` slow or produce very large PCM (a long or crafted `data` chunk), or a temp-dir write failure.
- **Description**:
  - `Command::output()` blocks until `ffmpeg` exits and buffers all of stdout. That buffer is copied again into the WAV (`wav_header`), and kira then decodes it to `f32` frames.
  - `VoiceSoundCache`'s 64 MiB budget only evicts other entries after the insert, so one line has no ceiling.
  - All of this runs inside the loader closure, which holds the cache write guard on purpose for #5383's lock order.
  - The temp path is `temp_dir()/byro-fuz-<nanos>.xwma`, written with `fs::write`, which follows symlinks and truncates; it is not `create_new`.
  - A write failure maps to `FfmpegMissing`, which is logged at debug level as "no host ffmpeg", so a full or read-only temp dir is misreported.
  - The parsers themselves are sound: `parse_fuz` uses checked `lip_end`, and the chunk walk ends on an overflowing size.
- **Suggested Fix**:
  - Cap the stdout read, for example at 64 MiB of PCM through a bounded reader on a spawned child, and add a wall-clock timeout that kills the child.
  - Create the temp file with `create_new` and a random suffix, or pipe the stream on stdin.
  - Give the temp write its own error variant.

**Cross-report overlap**: AUD-2026-10-21-D5-01 (`docs/audits/AUDIT_AUDIO_2026-10-21.md`) files the temp-write → `FfmpegMissing` misreport (the last bullet here) on its own. Fix the two together; if both are filed, drop that bullet from one of them.

## Also reported as AUD-2026-10-21-D5-01 (audio audit, merged here)

**Source**: docs/audits/AUDIT_AUDIO_2026-10-21.md · **Severity**: LOW · **Dimension**: Engine Consumers (voice) · **Location**: `crates/audio/src/fuz.rs:228`; consumer `byroredux/src/systems/dialogue_voice.rs:280-292`

**Audit status**: NEW (distinct from PAR-D2-01, which covers the synchronous/no-timeout spawn and the predictable temp name)

- **Description**: `std::fs::write(&temp, container.xwma).map_err(|_| FuzDecodeError::FfmpegMissing)?` maps any I/O failure (full or read-only temp dir, permissions) to the variant that means "the optional host tool is absent". The voice loader treats that variant as the expected degradation: `log::debug!` "resolved but no host ffmpeg — playing silent". It returns `None`, and that `None` becomes a `VoiceSoundCache` negative entry, so the line is not retried until the 4096-entry negative cap clears.
- **Evidence**: `fuz.rs:228`, `dialogue_voice.rs:282-288`.
- **Impact**: On a host that has `ffmpeg` but a full `/tmp` (a known failure mode on this machine; see the doctest tmpfs note), every Skyrim/FO4 line goes silent with only a debug line that names the wrong cause.
- **Related**: PAR-D2-01 (same function; fix together). Overlaps PAR-D2-01 (AUDIT_PARSERS_2026-10-21), whose description and suggested fix already include this temp-write misclassification.
- **Suggested Fix**: Add a `TempWrite(std::io::Error)` variant (or reuse `FfmpegFailed` with the I/O error text) and log it at warn. Better, pipe the xWMA to ffmpeg's stdin if the demuxer allows it, which also removes PAR-D2-01's temp file.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
