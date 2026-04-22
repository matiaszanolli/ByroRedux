# M33-01: NAM0 parse gate rejects FO3 + Oblivion

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/533
- **Severity**: CRITICAL
- **Dimension**: ESM parser
- **Audit**: `docs/audits/AUDIT_M33_2026-04-21.md` § M33-01
- **Status**: NEW (created 2026-04-21)

## Location

- `crates/plugin/src/esm/records/weather.rs:133` (NAM0 arm)

## Summary

NAM0 arm gates on `>= 240` but FO3 + Oblivion NAM0 is 160 B (10 groups × 4 TOD slots). All 27 FO3 + 37 Oblivion WTHRs fall back to zero sky colours. 12 of 63 FNV records also hit the 160-B variant.

Fix: dispatch stride by on-disk size; probably add `GameKind` to `parse_wthr`.

Fix with: `/fix-issue 533`
