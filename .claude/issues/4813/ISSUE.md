# #4813 — GAME-D5-2026-09-24-01: The REFR/ACHR "Initially Disabled" header flag (0x800) is never decoded — quest-gated items, containers, doors and actors spawn live, and hostile actors now attack

**Labels**: high,gameplay,ai,esm-plugin,bug
**Filed from**: docs/audits/AUDIT_GAMEPLAY_2026-09-24.md

From `docs/audits/AUDIT_GAMEPLAY_2026-09-24.md` (HEAD `aabd99a05`).

- **Severity**: HIGH
- **Dimension**: 5 (actors) and 2 (items, containers, doors)
- **Location**:
  - `crates/plugin/src/esm/cell/walkers.rs:866`: only `RECORD_FLAG_DELETED` (0x20) is tested.
  - `crates/plugin/src/esm/cell/mod.rs:396-445`: `PlacedRef` has no flags field.
  - `byroredux/src/cell_loader/references/mod.rs:500-509` (inverted-XESP skip only) and `:624` (script `ReferenceEnableState` only).
  - `byroredux/src/interaction.rs:1222-1235`: the #4698 filter reads only the ledger.
- **Status**: NEW. #349 (XESP) and #3278/#4698 (script `Disable()`) are closed and do not cover the ref's own flag. `cell/mod.rs:680-700` documents the flag only as an unknown for XESP *parents* (#471).
- **Description**:
  - A ref's disabled state at spawn comes from two sources only: the inverted-XESP heuristic and the script-disable ledger. The authored initial state (xEdit flag bit 11, `wbDefinitionsTES5.pas:3112`, the same bit on every game) is dropped.
  - Initially disabled items and containers therefore render, collide and, since #4697, show a Take prompt and can be taken or looted. Initially disabled doors open and teleport.
  - Initially disabled actors go through the full NPC job. Since #4414, `stamp_combat_disposition` arms them and `faction_hostility_system` starts combat on sight.
- **Evidence**: census of refs with 0x800 and no XESP, whose state is therefore unambiguous.

  Non-actor refs:

  | Master | Items | Containers | Doors |
  |---|---|---|---|
  | FNV | 105 | 34 | 10 |
  | FO3 | 8 | 13 | 5 |
  | Oblivion | 92 | 43 | 33 |
  | Skyrim | 169 | 249 | 22 |
  | FO4 | 40 | 29 | 33 |

  Examples: FO3 `WeapUniqueMissLauncher` and `MQ04RollerSkate`; Skyrim `MQ106DragonParchment`; FO4 `BoSM02_InitiateClarkeKey`; Oblivion `FGD01BrenusAstisJournal`.

  Actor refs whose base AIDT is Aggressive or worse:

  | Master | Refs | Very Aggressive |
  |---|---|---|
  | FNV | 935 | 111 |
  | FO3 | 101 | 52 |
  | Skyrim | 212 | 54 |
  | FO4 | 579 | 305 |

  Examples: the FNV `Vault11c` ceiling turrets, `NellisGenerator` explosive ants and the HooverDamIntPowerPlant Legion; the Skyrim HelgenKeep01 actors (14), `YsgramorsTomb01` wolf spirits, the GoldenglowEstate TG02 reinforcements, and the MS10 pirates in DawnstarWindpeakInn.
- **Impact**:
  - Content the game hides until a quest stage enables it is present from the first load.
  - Unique weapons, quest items and keys can be taken early, and keys open their locks early.
  - Hidden hostile actors attack the player or other NPCs.
- **Trigger**: FNV `Vault11c`; Skyrim `YsgramorsTomb01`, `HelgenKeep01` or Bleak Falls (MQ106); FO3 MQ04; the FO4 BoS quest cells.
- **Related**: #349, #471, #3278, #4698, #4697, #4414; GAME-D4-2026-09-24-01 (same parser site); GAME-D2-2026-09-24-04. `/audit-esm` owns the decode half.
- **Suggested Fix**:
  - Carry the record-header flags on `PlacedRef`.
  - Seed 0x800 refs that have no enable parent as disabled, through the existing `placement_disabled` branch (identity and scripts only), keyed like `ReferenceEnableState` so a scripted `Enable()` overrides it.
  - Resolve XESP children against the parent's real flag (#471's two-pass plan).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both NPC spawn paths, sibling interaction kinds, other parked `ReferenceState` facts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (no guard held across `world.get`/physics queries)
- [ ] **SAVE**: If a saved shape changes, `FORMAT_MAJOR` is bumped and no `serde(default)` is added (#4465)
- [ ] **TESTS**: A regression test pins this specific fix
