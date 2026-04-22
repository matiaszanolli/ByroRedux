# M33-04: FNAM arm body empty — FNV/FO3 fog never parses

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/536
- **Severity**: CRITICAL
- **Dimension**: ESM parser
- **Audit**: `docs/audits/AUDIT_M33_2026-04-21.md` § M33-04
- **Status**: NEW (created 2026-04-21)

## Location

- `crates/plugin/src/esm/records/weather.rs:157-161` (FNAM arm with empty body)

## Summary

FNAM arm body is empty; comment claims "fallback when HNAM is absent" but FNV/FO3 have **no** HNAM — they have 24 B FNAM with fog data. Every FNV/FO3 weather falls back to `fog_day_far=10000.0` default.

Fix with: `/fix-issue 536`
