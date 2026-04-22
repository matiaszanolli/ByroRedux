# M33-05: Oblivion HNAM semantic offsets don't match 56-byte layout

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/537
- **Severity**: HIGH
- **Dimension**: ESM parser
- **Audit**: `docs/audits/AUDIT_M33_2026-04-21.md` § M33-05
- **Status**: NEW (created 2026-04-21)

## Location

- `crates/plugin/src/esm/records/weather.rs:148-154` (HNAM arm)

## Summary

HNAM is 56 B in Oblivion with ~14 f32 lighting-model fields (NOT fog distances). Parser reads first 4 f32 as fog day/night near/far — producing `fog_far=4.0` (tiny) on every Oblivion weather.

Fix with: `/fix-issue 537`
