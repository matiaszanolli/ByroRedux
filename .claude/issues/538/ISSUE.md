# M33-06: classification byte offset likely off by two

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/538
- **Severity**: HIGH
- **Dimension**: ESM parser
- **Audit**: `docs/audits/AUDIT_M33_2026-04-21.md` § M33-06
- **Status**: NEW (created 2026-04-21)

## Location

- `crates/plugin/src/esm/records/weather.rs:163-174` (DATA arm)

## Summary

Parser reads classification from DATA byte 13. Byte-level evidence puts WTHR_PLEASANT (0x01) at byte 11 for `NVWastelandClear*`. FO3/Oblivion histograms dominated by 0xFF (nonsensical all-flags-set). Byte-sample 3+ known-classification WTHRs before pinning new offset.

Fix with: `/fix-issue 538`
