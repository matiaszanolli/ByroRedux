# #6286: GAME-D1-2026-10-21-01: The quest-alias inventory grant appends a plain row per grant, bypassing the #5871 fold

**Labels**: low,gameplay,inventory,quests,bug

**Source**: docs/audits/AUDIT_GAMEPLAY_2026-10-21.md · **Severity**: LOW · **Dimension**: Inventory & Equipment Model (Dim 1, index stability and one row per plain form) · **Location**: `crates/scripting/src/scene/quest_alias.rs:757-763`

**Audit status**: NEW (residual of #5871; the code predates it, `64fed2c49`)

- **Description**: `Inventory::add` (#5871) folds a plain stack into the form's existing row. The skill names scripted `AddItem`, barter, loot and pickup as going through it. The alias-injected inventory (`ALCO`/`COCT` items and Create-In created objects) still calls `inventory.push(ItemStack::new(item, count))`. An alias that grants a form the actor already holds (the player alias included) adds a second plain row for it. The spawn-time builders (`npc_spawn.rs:1514`, `cell_loader/references/attach.rs:272`) also push, but they build fresh inventories, where a duplicate only arises from duplicate CNTO entries.
- **Impact**: Duplicate rows for one form. They are listed twice by the inventory menu and saved that way. Counts stay correct because readers sum rows.
- **Suggested Fix**: Use `inventory.add(...)` at both grant sites. Optionally build the spawn inventories through it too.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every remaining `inventory.push(ItemStack::new(..))` grant site)
- [ ] **TESTS**: A regression test pins this specific fix
