# FNV-CELL-8: CLMT TNAM zero-sentinel doesn't guard per-byte corruption

- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/530
- **Severity**: LOW
- **Dimension**: Cell loading
- **Audit**: `docs/audits/AUDIT_FNV_2026-04-21.md`
- **Status**: NEW (created 2026-04-21)

## Location

`byroredux/src/scene.rs:249-265`

## Summary

TNAM fallback filter uses OR-of-four-bytes, not per-byte range check. Corrupt modded CLMT with `[0,0,0,0x80]` passes the filter → garbage TOD hour. Vanilla FNV all pass.

Fix: require each byte in `1..=144` (1 unit = 10 min, 144 = 24h).

Fix with: `/fix-issue 530`
