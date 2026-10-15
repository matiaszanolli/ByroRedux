# #5871: GAME-D1-2026-10-15-02: Barter and `AddItem` append a new row per transfer

- **Labels**: low,bug,gameplay,inventory
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Location**: `crates/scripting/src/fragment/effects/inventory.rs:65-79` and `:243-268`; `byroredux/src/barter.rs:456-480` (two transfers per click)
- **Status**: NEW
- **Description**: Items without an instance are never merged into an existing row, so gold and goods fragment into one row per trade on both sides. The rows are saved and never compacted, and the inventory menu (`inventory.rs:1314-1345`) lists each one.
- **Suggested Fix**: Merge into the first row with the same form, no instance and `count > 0`, or into a zeroed row of that form. Never remove rows.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
