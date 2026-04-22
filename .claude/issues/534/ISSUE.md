# M33-02: Cloud-texture sub-record FourCCs never match

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/534
- **Severity**: CRITICAL
- **Dimension**: ESM parser
- **Audit**: `docs/audits/AUDIT_M33_2026-04-21.md` § M33-02
- **Status**: NEW (created 2026-04-21)

## Location

- `crates/plugin/src/esm/records/weather.rs:184-196` (cloud FourCC arms)

## Summary

Parser matches `00TX/10TX/20TX/30TX` but actual FourCCs are `DNAM/CNAM/ANAM/BNAM` (FNV/FO3) or `CNAM/DNAM` (Oblivion). 0/127 vanilla WTHRs have any cloud texture parsed. Must be fixed with M33-03 since DNAM is claimed both ways.

Fix with: `/fix-issue 534`
