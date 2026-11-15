# #6959: TD2-2026-11-15-06: `Movie::open` / `open_bink2` and `extract_audio` / `bink2_audio` are two copies of one pipeline; this week's three video fixes each had to touch both

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6959
- **Labels**: low,import-pipeline,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Dimension**: 2 · **Severity**: LOW · **Effort**: small · **Labels**: `tech-debt` · **Existing**: none
- **Age**: c678f1065 (2026-11-13, the Bink 2 arm copied the Bink 1 arm); fixes 118afd645 (11-14), aa193bd9e (11-14), 706f95884 and 538992417 (11-15)
- **Location**: `crates/video/src/lib.rs:393-442` (`open`) vs `:444-483` (`open_bink2`); `:199-229` (`extract_audio`) vs `:259-299` (`bink2_audio`)
- **Description**: both open paths do: derive `name` from the file name (the same `path.file_name().map_or_else(..)` line), clamp `(width, height) = size.max(2)`, build the `sync_channel(FRAME_RING)`, spawn the `"movie-decode"` thread, run the body under `guard_decoder`, and on a frame error `log::warn!("video: {reason}")` + `tx.send(Err(reason))` + `return`, then `tx.send(Ok(fit(..)))` or return when the receiver is gone, and finally build the identical eight-field `Self { .. }` literal (also built a third time in `from_decoded`). Both audio paths: open a decoder (warning on failure), compute `pcm_cap`, loop frames, stop on decode error with the same "its audio stops at frame N" warning, call `over_pcm_cap`, then `write_wav`. They already differ without a stated reason: the Bink 2 loop refuses a packet before `decode` grows the buffer (`:271-274`, added by 538992417), the Bink 1 loop extends first and truncates after; the Bink 1 warnings omit the movie `name` that the Bink 2 ones carry. The test `every_reader_thread_runs_under_the_decoder_guard` (`:654`) exists because each new reader thread had to be remembered.
- **Suggested Fix**: `fn spawn_reader(name: String, size: (u32, u32), decode: impl FnOnce(&mut dyn FnMut(Result<Vec<u8>, String>) -> bool) + Send + 'static) -> Result<Movie, VideoError>` taking the per-codec frame producer (the `emit` closure returns `false` when the receiver dropped), and one `Movie::assemble(info, size, rx, reader, audio)`. For audio, one `fn soundtrack(name, cap, next_chunk: impl FnMut(&mut Vec<i16>) -> Step)`; the Bink 1 and Bink 2 arms supply only "decode this frame's packet". Do it before the next codec arm (BIK-DEC-004 and later work add more).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
