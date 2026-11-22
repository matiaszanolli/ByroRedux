# #7158 — PAR-D2-2026-11-21-03: the soundtrack temp WAV leaks when decoding panics

Labels: low,import-pipeline,bug

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/video/src/lib.rs` `decode_soundtrack` (`create_temp_file` before the loop; `remove_file` only on the `Err` and zero-sample arms)
- **Status**: NEW (residual of #7143 / #7128, closed)
- **Description**: a panic in `decode_record`, `binkaudio::Decoder::decode` or the frame closure unwinds to `guard_open`, which correctly returns `Decode`. The 0600 WAV, up to ~1.4 GB, stays in the temp dir. Verified by reading; no panic trigger is known.
- **Suggested Fix**: a drop guard that removes the path unless it is disarmed on success.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
