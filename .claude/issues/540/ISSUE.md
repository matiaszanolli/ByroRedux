# M33-08: WLST autodetect collision at 3/6/9-entry Oblivion CLMTs

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/540
- **Severity**: MEDIUM
- **Dimension**: ESM parser
- **Audit**: `docs/audits/AUDIT_M33_2026-04-21.md` § M33-08
- **Status**: NEW (created 2026-04-21)

## Location

- `crates/plugin/src/esm/records/climate.rs:70-89` (WLST autodetect)

## Summary

`len % 12 == 0` heuristic picks 12-byte entries on Oblivion CLMTs with 3/6/9 entries. Vanilla Oblivion.esm has none (histogram: {0,1,2,5,7}) but DLC/mods can trip it. Fix via GameKind dispatch (couples with M33-07).

Fix with: `/fix-issue 540`
