# M33-11: WTHR DATA comment contradicts its own field offsets

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/543
- **Severity**: LOW
- **Dimension**: Doc drift
- **Audit**: `docs/audits/AUDIT_M33_2026-04-21.md` § M33-11
- **Status**: NEW (created 2026-04-21)

## Location

- `crates/plugin/src/esm/records/weather.rs:163-174` (DATA arm comment)

## Summary

Comment says "bytes 13-14: lightning color" while code assigns `classification = sub.data[13]`. Coupled with M33-06 — fix in the same commit once correct byte offset is pinned.

Fix with: `/fix-issue 543`
