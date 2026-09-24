# #4812 — GAME-D1-2026-09-24-01: Templated NPCs read `DOFT` and the leveled-expansion level from the placed record, not the template — 2,716 Skyrim placed actors spawn without their outfit

**Labels**: high,gameplay,inventory,bug,game:skyrim
**Filed from**: docs/audits/AUDIT_GAMEPLAY_2026-09-24.md

From `docs/audits/AUDIT_GAMEPLAY_2026-09-24.md` (HEAD `aabd99a05`).

- **Severity**: HIGH
- **Dimension**: 1 — Equipment model (also Dim 2, corpse loot)
- **Location**:
  - `byroredux/src/npc_spawn.rs:1158` (`let npc = resolved.shell;`)
  - `:1175` (`let actor_level = effective_actor_level(npc);`)
  - `:1223` (`if let Some(otft_fid) = npc.default_outfit`)
  - For contrast, the correct reads: `npc_spawn.rs:275` (`CharacterLevel` from `resolved.stats`) and `crates/plugin/src/equip.rs:871` (spells from `resolved.stats`)
- **Status**: NEW. The bug predates #4457: the baseline read `npc.default_outfit` the same way. But #4457 claims no consumer at the population boundary still reads raw shell fields, and these two do. That makes #4457 incomplete.
- **Description**: `build_npc_equip_state` takes the carry list (`CNTO`) from the Use Inventory terminal, `resolved.inventory`. It reads two other values off `resolved.shell`:
  - **The outfit.** xEdit places `DOFT` in the Inventory group (`wbDefinitionsTES5.pas:8450`). UESP says TPLT 0x100 "Use inventory" covers the "Inventory tab, including all outfits and geared-up item". The outfit must therefore come from the terminal.
  - **The level used to expand leveled outfits and carry lists.** The stats row (TPLT 0x02) covers level. So one actor reports its `CharacterLevel` and resolves its spell lists from the Use Stats terminal, but expands its leveled gear at the shell's level.
  - `resolved.shell.default_outfit` is the only production reader of an NPC outfit. `inventory.rs:514` is the player seed.
- **Evidence**: census over the raw masters, re-run by the merge pass:
  - Skyrim.esm has 2,490 shells with Use Inventory set. In 775 of them, the shell has no `DOFT` while its terminal has one. There are 0 cases where both have a `DOFT` and the two differ, which is consistent with pure inheritance.
  - After LVLN resolution, 740 shells lose their outfit. 516 of these are placed, across **2,716 ACHRs**. The most-placed are `CWFortSiegeImperial` ×321, `LvlDraugrAmbushMelee1HMale` ×140, `LvlDraugrAmbushMelee2HMale` ×83, `LvlBanditMissile` ×78, `LvlBanditMeleeAny` ×52 and `LvlGuardImperial` ×50. Other examples: `LvlDwarvenCenturion` → `EncDwarvenCenturion03`, and `dunAnsilvundDraugrAmbushMelee2HAggro` → `EncDraugr05Melee2HEbonyHeadM02`.
  - The shell's level differs from its Use Stats terminal's level on Skyrim for 622 records (535 with a leveled `CNTO`), and on FNV for 643 (639).
- **Impact**:
  - Skyrim leveled bandits, draugr, guards and siege soldiers render with only the race skin (visible content missing), and their corpses lack the gear.
  - On Skyrim and FNV, hundreds of templated NPCs expand their leveled carry lists at a level that disagrees with their own `CharacterLevel`.
- **Trigger**: Skyrim; any bandit camp, draugr crypt or `CWFortSiege*` cell.
- **Related**: #1658 (closed; CNTO only); #4457; #4232; #4696; #4137.
- **Suggested Fix**:
  - Read `resolved.inventory.default_outfit`, and the sleep outfit if one is ever consumed.
  - Compute `actor_level` from `effective_actor_level(resolved.stats)`.
  - Add a test with a TPLT shell that has no `DOFT` over a terminal that has one, and a shell-vs-template level mismatch.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both NPC spawn paths, sibling interaction kinds, other parked `ReferenceState` facts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (no guard held across `world.get`/physics queries)
- [ ] **SAVE**: If a saved shape changes, `FORMAT_MAJOR` is bumped and no `serde(default)` is added (#4465)
- [ ] **TESTS**: A regression test pins this specific fix
