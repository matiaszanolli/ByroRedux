# #7157 — PAR-D2-2026-11-21-02: a soundtrack cut at the movie's running-time cap is not reported as a problem

Labels: low,import-pipeline,bug

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/video/src/lib.rs` `decode_soundtrack` (the `over` branch, ~l.385)
- **Status**: NEW (residual of #7139, closed)
- **Description**: the branch logs a warning and truncates, but it does not call `stop`. So `Audio.problem` stays `None`, and both `video.status` and the sweep's completeness check treat the truncated track as whole.
- **Suggested Fix**: route the branch through `stop(..)`, or document why audio past running time plus slack is safe to drop silently.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / list walker)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21.md`
