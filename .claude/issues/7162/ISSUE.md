# #7162 — PAR-D2-2026-11-21-07: `bink2_container` maps any `io::ErrorKind::Unsupported` to `VideoError::Unsupported`

Labels: low,import-pipeline,bug

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/video/src/lib.rs` `bink2_container`
- **Status**: NEW (introduced by #7135's signalling)
- **Description**: an OS-level `Unsupported` from `File::open` or `read` (special files, unusual filesystems) is reported as *skipped* rather than *unreadable*, because the #7135 refusal travels in an overloaded `ErrorKind`.
- **Suggested Fix**: a typed refusal (a `bink2::Error` variant or an inner marker type checked with `get_ref().downcast_ref`) instead of matching the kind.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
