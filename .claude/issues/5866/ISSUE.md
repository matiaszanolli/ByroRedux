# #5866: GAME-D2-2026-10-15-02: Script ownership changes never reach the theft rule

- **Labels**: low,bug,gameplay,inventory
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-15.md`

- **Location**: `byroredux/src/cell_loader/references/synth_child.rs:75-77` and `:863-872`; `crates/scripting/src/fragment/effects/deferred.rs:611-620`; `byroredux/src/inventory.rs:890-916`
- **Status**: NEW (from `b2868b848` and `8b3b75d4d`)
- **Description**: REFR `XOWN` is stamped twice: as core `Owned` (read by `transfer_is_theft` / `pickup_loot`) and as scripting `ExplicitOwner`. `SetActorOwner` / `SetFactionOwner` write only `OwnerOverrides`, which only the VM's `GetActorOwner` / `GetFactionOwner` read.
- **Impact**: The "Stolen"/"Added" wording and the `stolen` flag are wrong after a script ownership change. This becomes a real bug once crime has consequences; faction crime gold now exists.
- **Suggested Fix**: Make one ownership read (override, then `Owned`) that both the theft rule and the natives use.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **LOCK_ORDER**: If a RwLock/query scope changes, TypeId-sorted acquisition and the system's declared `Access` row are preserved
- [ ] **TESTS**: A regression test pins this specific fix
