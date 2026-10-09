# #5460: SAVE-D5-2026-10-08-02: `reconcile_worn_gear` is called twice back-to-back in the load drain

**Labels**: low,save-load,bug,tech-debt
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5460

**Source**: `docs/audits/AUDIT_SAVE_2026-10-08.md` — `SAVE-D5-2026-10-08-02` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `save_io.rs` 1933-1934 call `reconcile_worn_gear(world, player)` twice in a row.

- **Severity**: LOW
- **Dimension**: Live Load-Apply & Frame Boundary
- **Data-Loss Class**: none
- **Location**: `byroredux/src/save_io.rs:1932-1934` (merge slip in `6b494d002`).
- **Status**: NEW
- **Description**: The #5255 edit inserted a new `reconcile_worn_gear` line above the existing one. The second call is idempotent: visibility already agrees, and `queue_midlife_imports` skips a wearer that already has a pending import. The cost is a second `NpcEquipmentPart` scan and subtree walks per load. The new `load_clears_both_player_gear_handoff_queues` test calls the helper directly, so the drain wiring stays unpinned (#5060).
- **Suggested Fix**: Delete one call. Fold the helper into #5060's source-order pin.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
