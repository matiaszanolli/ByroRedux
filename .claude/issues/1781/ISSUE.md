# SKY-D4-NEW-01: stale deleted-REFR tombstone doc comment (left by the #1660 fix)

**Issue**: #1781
**Severity**: LOW (doc-rot; no runtime impact)
**Dimension**: Multi-Master Load Order (ESM / cell loading)
**Labels**: low, import-pipeline, legacy-compat, documentation
**Source audit**: `docs/audits/AUDIT_SKYRIM_2026-06-28.md`
**Status (as filed)**: NEW — drift introduced by the #1660 fix landing 2026-06-26

## Description
The doc comment on `merge_cell_references` in `crates/plugin/src/esm/cell/mod.rs`
still asserts deleted-REFR tombstones "(the 0x20 Deleted flag) aren't captured by
the parser yet." True at the 2026-06-23 baseline; false since `2dc43106`
(2026-06-26) added the `RECORD_FLAG_DELETED` (`0x0020`) skip in the REFR walk.
The fix landed in `walkers.rs`; the neighbouring `mod.rs` comment was not updated.

## Evidence
- `crates/plugin/src/esm/cell/mod.rs` — comment line "… tombstones (the 0x20
  Deleted flag) aren't captured by the parser yet …".
- `crates/plugin/src/esm/cell/walkers.rs` — `const RECORD_FLAG_DELETED: u32 =
  0x0000_0020;` + the `if header.flags & RECORD_FLAG_DELETED != 0 { … skip }` in
  the REFR walk.
- Tests `deleted_refr_tombstone_is_skipped` + `non_deleted_refr_still_places`
  (`crates/plugin/src/esm/cell/tests/refr.rs`), both pass.
- `git log 2dc43106` → "Fix #1730 #1660: FO3 36-byte XCLL warn + skip
  deleted-REFR tombstones".

## Impact
None at runtime. Risk is a future reader / stale-premise audit re-filing #1660.

## Related
- #1660 (SKY-D4-01) — the original tombstone gap this comment describes. Fix
  shipped in `2dc43106` but the commit `Fix #1730 #1660` auto-closed only #1730,
  leaving #1660 stale-OPEN. Should be closed; this doc fix is the cleanup tail.

## Suggested Fix
One-line doc edit at `crates/plugin/src/esm/cell/mod.rs` — state tombstones are
now skipped at the walker level (`walkers.rs`, `RECORD_FLAG_DELETED`) and cite
#1660 as resolved.

## Completeness Checks
- [ ] **SIBLING**: grep `crates/plugin/src/` for any other "aren't captured" /
  "not captured yet" tombstone wording (confirmed only stale instance; re-verify)
- [ ] **TESTS**: comment-only fix; behaviour already pinned by
  `deleted_refr_tombstone_is_skipped`
