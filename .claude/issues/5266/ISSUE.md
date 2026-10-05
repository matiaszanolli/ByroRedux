# #5266 — GAME-D1-2026-10-05-01: The post-load worn-gear reconcile counts the weapon slot as a missing mesh and shares #5031's one-request queue, so a loaded player's root-less armor imports at random

- **Labels**: medium,gameplay,inventory,save-load,bug
- **Filed from**: `docs/audits/AUDIT_GAMEPLAY_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5266

- **Severity**: MEDIUM (visual and gameplay divergence from saved state; the canonical `EquipmentSlots` are correct)
- **Dimension**: 1 — Inventory & Equipment Model (P3 post-load reconcile, #5034)
- **Location**:
  - `byroredux/src/npc_spawn/loot_appearance.rs:515-534` (`equipped_forms` = occupants `.chain(equipment.weapon)`)
  - `:582-597` (the `missing` set is built from `HashSet` iteration, then passed to `queue_midlife_imports`)
  - `:407-426` (the first request per wearer wins)
  - `:441-463` (a form with no resolved paths is dropped with `continue`)
  - `:1000-1005` (the chained reconcile runs only on a successful import)
  - `crates/plugin/src/equip.rs:185-192` (`resolve_armor_meshes` returns empty for anything that is not `ItemKind::Armor`)
- **Status**: NEW. This is a new call site, added by #5034 (`20717d5b7`, closed), inheriting the #5031 queue shape (open). The event-path half stays #5031's.
- **Trigger**: the player has a weapon equipped plus at least one equipped armor piece that has no spawn-time root. Examples:
  - FNV: a pistol plus mid-life Leather Armor;
  - Skyrim: a sword plus a looted helmet.

  Save, then load (quickload, or `--load` from boot), and switch to third person.
- **Description**:
  - #5034's `reconcile_worn_gear` builds the set of equipped forms from every occupied biped slot **and the weapon slot**. Any equipped form with no live non-intrinsic `NpcEquipmentPart` root goes into the `missing` list.
  - Weapons never get a root. Both spawn arms build `NpcEquipmentPart` only from `armor_to_spawn` (`resumable/prebaked.rs:89-104`, `resumable/runtime.rs:341-356` and `:437-452`), and no mid-life weapon mesh is imported. So the weapon form is in `missing` on every call.
  - `queue_midlife_imports` keeps only the **first** request per wearer (`:422`). It resolves meshes only afterwards, and a WEAP resolves to no paths, so that request is discarded with `continue` (`:458`) and nothing is queued.
  - `missing` is collected from a `std::collections::HashSet` (`RandomState`), so whether the weapon or an armor comes first changes from run to run.
  - The chained reconcile at `:1005` re-runs only after a *successful* import. Once the weapon is picked first, nothing ever runs the reconcile again. The import-failure paths (`:884-893`, `:908-931`) do not chain either.
- **Evidence**:
  - The guard test `load_reconcile_diffs_roots_against_restored_slots` (`:1296-1371`) puts an **armor** form in the weapon slot (`slots.equip_weapon(InventoryIndex(1))` with `install_index(0xCCC, "...gauntlet.nif")`). It never exercises a real WEAP, and it has only one missing form.
  - `grep -rn 'NpcEquipmentPart {' byroredux/src` finds only the armor builders, `player_body.rs` and `loot_appearance.rs:949`.
- **Impact**: after any load with a weapon equipped, the third-person body may show none, some, or all of the saved mid-life armor, and which one it shows varies from run to run. NPCs are mostly unaffected: their restored slots usually match the record outfit, so the weapon is the only missing form and nothing needs importing. An NPC that equipped a new piece mid-life and was then evicted hits the same stall when it returns.
- **Related**: #5031 (the same queue, event path); #5034 (introduced the call site); ECS-2026-10-05-D7-01 (the gear-release `Children` leak).
- **Suggested Fix**: drop the weapon slot from `equipped_forms`, or skip forms that `resolve_armor_meshes` cannot serve *before* choosing the request. Also chain the next reconcile on the failure paths. The #5031 fix (a per-wearer list instead of a single slot) removes both halves; build `missing` in a deterministic order (slot order). Add a test with a real WEAP and two root-less armors.

_Source: `AUDIT_GAMEPLAY_2026-10-05.md` (GAME-D1-2026-10-05-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
