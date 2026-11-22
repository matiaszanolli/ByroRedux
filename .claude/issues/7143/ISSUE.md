# #7143: PAR-D6-2026-11-20-03: The movie soundtrack's temp WAV is created non-exclusively under a predictable name

**Labels**: low,import-pipeline,safety,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7143

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D6-03 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Trigger Input**: another local user pre-creates `<tmp>/byroredux-movie-<pid>-<n>.wav` as a symlink to a file the player can write
- **Location**: `crates/video/src/lib.rs:153-157` (`temp_wav_path`), `:256-277` (`write_wav`)
- **Status**: NEW. This is a sibling of the pattern #6307 fixed: the movie writer is new code that never got the fix, so it is not a regression of #6307 itself. The #6307 fix is (`crates/audio/src/fuz.rs::write_temp_xwma`, `create_new` + mode 0600 + pid/clock/counter name)
- **Description**: `write_wav` uses `std::fs::File::create(&wav)`, which follows a planted symlink and truncates its target. The name is `byroredux-movie-{pid}-{counter}.wav`, guessable. The decoded PCM (up to 691 MB) is written with default permissions. The `.fuz` writer was fixed for exactly this; the movie path, added later, did not reuse the fix.
- **Evidence**: no `OpenOptions`/`create_new` in `crates/video/src/lib.rs`; `Drop for Movie` only removes the file on a clean drop, so a crash leaves up to 691 MB in the temp dir.
- **Impact**: hardening on a shared-tmp multi-user host (kernel `protected_symlinks` blunts it on modern Linux); on a single-user desktop only the stale-file leak matters.
- **Related**: #6307
- **Suggested Fix**: share the `create_new` + retry + 0600 helper between `crates/audio/src/fuz.rs` and `crates/video` (or use `tempfile`), so there is one temp-file writer.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
