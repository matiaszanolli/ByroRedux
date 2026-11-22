# #7172 — PAR-D2-2026-11-21b-01: the soundtrack WAV's guard ends at `keep()`, and the leak fix is untested where it is used

Labels: low,import-pipeline,bug,test-gap

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/video/src/lib.rs`: `Audio.wav: Option<PathBuf>`, `discard_wav` (the only manual `remove_file` left), `open_unguarded` / `open_bink2` (`.inspect_err(|_| discard_wav(&audio))?`), and `decode_soundtrack`'s tests
- **Status**: NEW (residual of #7158, closed)
- **Description**: `decode_soundtrack` hands back a bare `PathBuf` once the samples are kept. A panic between that point and `Movie::streaming` taking the audio unwinds through `guard_open` and leaves the WAV; `discard_wav` covers only the explicit `Err` arm. No panic trigger is known. Separately, no video test checks that a panicking frame closure, an `Err`, or a zero-sample track leaves no `byroredux-movie-*` file; only the core `TempPath` test exists.
- **Suggested Fix**: carry the `TempPath` in `Audio` and in the `Movie` until playback ends, and drop `discard_wav`. Add a video-level leak test over the three exits (matching this process's file-name prefix).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / cache key)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21b.md`
