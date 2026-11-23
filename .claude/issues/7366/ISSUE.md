# #7366 — CONC-2026-11-23-07

Snapshot of the issue as filed; GitHub is authoritative for current state.

Filed from `docs/audits/AUDIT_CONCURRENCY_2026-11-23.md` (finding CON-07).

- **Severity**: LOW
- **Dimension**: Worker Threads and Thread-Safety Bounds (temp-file lifetime of the `movie-decode` / `.fuz` decode path)
- **Location**: `crates/video/src/lib.rs:387-460` (`decode_soundtrack`, `create_temp_file(soundtrack.stem, "wav")` at `:397`), `crates/core/src/atomic_file.rs:124-150` (`create_temp_file`) and `:199-229` (`TempPath`), `crates/audio/src/fuz.rs:295-302` (`write_temp_xwma`), `byroredux/src/video.rs:327` (the WAV is played by path)
- **Status**: NEW. Dedup: #7158 and #7172 (closed) cover the panic-unwind and early-return paths, #7143 (closed) the exclusive creation, #7354 (closed) reclaims only `.save_<slot>.ess.<pid>.<n>.tmp`. `gh issue list --state all --search "soundtrack WAV temp crash orphan"` and `"temp file leak crash reclaim"` returned no match for a hard-death sweep of these two stems.
- **Description**: `TempPath::drop` removes the file on a return or an unwinding panic, which is what #7158/#7172 pinned (`the_soundtrack_wav_is_removed_on_every_way_out`). It cannot run when the process dies without unwinding: SIGSEGV/SIGKILL, an OOM kill, a power cut, a Vulkan-driver crash, or the rayon abort-on-panic class this dimension polices. The movie WAV is the exposed one: it is created before the first frame, written whole by `decode_soundtrack`, and then stays on disk for the whole playback because the audio crate opens it by path (`byroredux/src/video.rs:327`, `play_audio`). The #7354 fix made the same observation for save staging ("a disk-space leak of one save per crash") and added `disk::reclaim_stale_staging_files` to `SaveState::new`; the two temp stems produced through the same helper got no equivalent.
- **Evidence**: `grep -rn "byroredux-movie\|byro-fuz\|SOUNDTRACK_STEM\|create_temp_file" crates byroredux tools` finds the creators and no reader of the stems other than the tests; nothing lists `std::env::temp_dir()` at boot. The names carry the pid (`{stem}-{pid}-{nanos}-{counter}.{ext}`), so a sweep can tell a dead owner from a live one. The soundtrack is capped at `MAX_SOUNDTRACK_SECS` = 3600 s of 48 kHz stereo s16 (about 1.4 GB), typical vanilla intro movies are a few MB to tens of MB.
- **Trigger Conditions**: the process dies without unwinding while a movie is open or playing (or inside a `.fuz` decode, a window of milliseconds to seconds). Every such death leaves one file per open movie.
- **Impact**: disk (or RAM, on a tmpfs `TMPDIR`) held until the next reboot or manual cleanup; no correctness effect. Same class and rationale as #7354 but for the temp directory.
- **Verification Path**: unit test with a fake temp dir is not possible (`create_temp_file` reads `std::env::temp_dir()`); the test is a sweep helper taking the directory, driven by a fixture of `byroredux-movie-<deadpid>-1-0.wav` and `byroredux-movie-<livepid>-...` files. No GPU needed.
- **Related**: #7354, #7158, #7172, #7143
- **Suggested Fix**: a boot-time sweep next to `reclaim_stale_staging_files` that removes `byroredux-movie-*.wav` / `byro-fuz-*.xwma` in `temp_dir()` whose embedded pid is not alive (or whose mtime is older than a day), best-effort and logged with a count. On Windows the same helper should also be checked for the open-handle case: `Movie::drop` removes the WAV while kira may still hold the file open after `stop_movie_sound()` (the removal error is swallowed by `TempPath::drop`); not verified on Windows.
- **Dimension agent ID**: CON-D7-01

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other record-time latches, other temp stems, other exclusive rows)
- [ ] **TESTS**: A regression test pins this specific fix
