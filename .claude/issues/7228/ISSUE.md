# #7228: GAME-2026-11-22-05: `EquippedAmmo` is an additive column, so a load cannot clear a live arrow choice

**Labels**: medium,gameplay,inventory,save-load,bug,game:oblivion,game:skyrim

From `docs/audits/AUDIT_GAMEPLAY_2026-11-22.md` (report ID **GP-05**), audited at HEAD 2b40f76b6.

- **Severity**: MEDIUM
- **Dimension**: 1 (equipment model) / 7 (what a load restores); the registration kind belongs to `/audit-save`
- **Domain**: inventory · **Game**: Oblivion, Skyrim (the profiles with a `BowModel`)
- **Trigger**: Equip an arrow type, then load in-process a save written while the player had no `EquippedAmmo` row (before their first arrow equip), whose inventory still carries that arrow type.
- **Location**: `byroredux/src/save_io/registry.rs:97-102,340` (`register_component::<EquippedAmmo>`, additive); `byroredux/src/save_io/live_load.rs:864-876` (the post-`apply_deltas` player block re-derives only `EquippedWeapon`); `byroredux/src/bow.rs:120-141` (`equipped_ammo`, `set_equipped_ammo`, `reconcile_equipped_ammo`)
- **Status**: NEW (found independently by Dims 1 and 7)
- **Description**: The row is created lazily by the first `toggle_equipped_ammo`; neither `attach_to_player` nor the boot player build stamps one. A save from before that has no row. `apply_deltas` removes a live row on a saved absence only for `register_replacing_component` columns (`crates/save/src/driver.rs:369-375`), and `EquippedAmmo` is additive, unlike its siblings `ReadiedSpell`, `SkillUseLevel`, `Sneaking` and `Dead`. The registry comment's "cleared by writing None" covers only a clear made in the saving session. `reconcile_equipped_ammo` clears only a form the inventory no longer carries. This is the shape of #3488 (`EquippedWeapon`) and #5027 (`Dead`).
- **Evidence**: `rg EquippedAmmo byroredux/src/save_io` hits only the registry and tests. `equipped_ammo_round_trips` checks a saved `None` and a saved `Some` through cold `restore_world`, where an absent row cannot occur.
- **Impact**: The loaded game fires, and the menu shows as equipped, an arrow type the saved game never chose. It would otherwise refuse with "no arrows". Re-toggling the slot fixes it.
- **Related**: #7055 (BOW-AMM-002), #3488, #5027
- **Suggested Fix**: Register `EquippedAmmo` with `register_replacing_component`, keeping the `None` writes. Add a live-overlay test where the live player holds `Some` and the snapshot has no row. Dims 1 and 7 disagree on whether the registration kind enters the registry fingerprint. Check it against the shape guard before deciding whether a `FORMAT_MAJOR` bump is needed.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other procedures / other transients / other inventory paths)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
