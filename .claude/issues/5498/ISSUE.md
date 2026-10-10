# #5498: FO4-2026-10-09-D4-01: FO4 `NPC_.TPTA` per-flag Template Actors are never decoded, so every Use-X flag resolves through `TPLT`

**Labels**: bug, esm-plugin, game:fo4, gameplay, legacy-compat, medium

**Source**: `docs/audits/AUDIT_FO4_2026-10-09.md` — finding `FO4-2026-10-09-D4-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM. Actors get the wrong gear, AI, stats and traits on about 26% of vanilla FO4 NPC records. Nothing
  crashes.
- **Dimension**: 4, the ESM decode. The consumer is `npc_spawn` via `ResolvedNpc`.
- **Location**:
  - `crates/plugin/src/esm/records/actor/npc.rs:685-692`: only `TPLT`, into `template_form_id`.
  - `crates/plugin/src/equip.rs:663-718`: `ResolvedNpc::resolve` walks every flag through `template_form_id`.
  - `byroredux/src/npc_spawn.rs:1314-1315`: #5359's `resolved.r#traits.worn_skin`.
- **Status**: NEW. `grep TPTA` across `crates/plugin/src` and `byroredux/src` finds 0 hits.
  `docs/engine/charal-fo4-ruleset.md:535` documents TPTA. #4137 (open, unconsumed template-flag bits) is related but
  distinct.
- **Description**:
  - xEdit FO4 (`wbDefinitionsFO4.pas:10350-10369`) authors `TPLT 'Default Template'` plus
    `TPTA 'Template Actors'`. TPTA is 13 per-flag entries, in this order: Traits, Stats, Factions, Spell List, AI Data,
    AI Packages, Model/Animation, Base Data, Inventory, Script, Def Package List, Attack Data, Keywords.
  - The flag bit order is xEdit's `wbTemplateFlags` (`wbDefinitionsCommon.pas:8429-8451`): bits 0-12 match the TPTA
    member order, and the repo's `TEMPLATE_FLAG_*` constants use the same bits.
  - The FO4 CK Templates tab has a Template Form per flag. A null entry falls back to the default template.
  - The engine decodes only the default template, so each per-flag override is ignored.
- **Evidence**: census (`/tmp/audit/fo4/py/tpta_census.py`), counting records where the flag bit is set and the TPTA entry
  is non-null and differs from `TPLT`, on the flags the engine consumes.
  - **`Fallout4.esm`**:
    - 3,015 `NPC_`, 2,289 with `TPLT`, 2,167 with `TPTA`.
    - **763 records affected**: Traits 464, Inventory 249, Stats 136, Spell List 110, AI Data 47, Factions 43,
      AI Packages 15.
    - Plus 1 record with TPTA entries but no TPLT, which inherits nothing.
  - **DLCs**: DLCRobot 41, DLCCoast 75, DLCNukaWorld 269 (+1 no-TPLT), DLCworkshop03 4 (+2 no-TPLT), DLCworkshop01 1.
    That makes **1,157 records across all seven masters**.
  - Examples:

    | Record | Flag | Correct template (TPTA) | Template used today (TPLT) |
    |---|---|---|---|
    | `EncBoSSoldier07..09PowerArmorLegendary` | Inventory | `EncBoS_PowerArmor_Auto_Template` | `EncBoSSoldierTemplate` (non-PA) |
    | `EncWorkshopNPCMaleGuard04b` | Stats | `…Guard01Template` | `…Farmer01Template` |
    | `POISC_Fisherman` | AI Packages / AI Data | `POIWastelanderMale01` | `LvlSecurityDiamondCity` |
    | `VRWorkshopShared_LvlRaiderMixed` | Traits | `LCharRaiderFaceAndGender` | `LCharRaider` |

  - #5359 compounds this on FO4. `NPC_.WNAM` has no game gate: 1,162 FO4 records author it, all pointing at ARMO
    (SkinNakedDirty 337, SkinSuperMutant 131, SkinSynthGen2Mech 109). It is read off the Traits terminal, which is wrong
    for 464 records.
- **Impact**: templated FO4 actors spawn with:
  - The wrong inventory, e.g. power-armor legendaries without PA.
  - The wrong AI packages and disposition.
  - The wrong stats.
  - The wrong race, skin, gender and face source.
  - The wrong factions.
  - The wrong spell lists.
- **Related**: #2956 and #4093 (template-flag resolution), #4137, #5359, and `charal-fo4-ruleset.md`.
- **Suggested Fix**:
  - Decode `TPTA` into `NpcRecord`: 13 remapped FormIDs, with 0 meaning "use default".
  - Have `walk_inherited_records` pick `tpta[bit]` when it is non-zero, else `template_form_id`, per flag. This is data
    only (no other game authors TPTA), so no game branch is needed.
  - Add a real-data pin on `EncBoSSoldier07PowerArmorLegendary` Inventory.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
