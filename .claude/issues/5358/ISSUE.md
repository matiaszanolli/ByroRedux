# #5358: SKY-D3-2026-10-05-01: Skyrim's `BODT` biped-slot sub-record is never decoded (10 ARMOs, 916 ARMAs) — #5034's post-restore gear reconcile now hides every Draugr's hair and beard on cell return and on load

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5358
- **Labels**: medium,bug,game:skyrim,legacy-compat,esm-plugin,gameplay,inventory
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-10-05.md` (SKY-D3-2026-10-05-01)

_From `docs/audits/AUDIT_SKYRIM_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: MEDIUM
- **Dimension**: 3 — NPC equip + FaceGen (Skyrim ARMO/ARMA data through the shared equip mechanism)
- **Location**:
  - Decode gap:
    - `crates/plugin/src/esm/records/items.rs:540`: the ARMO arm reads `BMDT` / `BOD2` only.
    - `crates/plugin/src/esm/records/misc/equipment.rs:103`: the ARMA arm reads `BMDT` / `BOD2` only.
  - Consumers:
    - `byroredux/src/npc_spawn/loot_appearance.rs:507-562` (`reconcile_worn_gear`).
    - `byroredux/src/cell_loader/reference_state.rs:337`, reached from `cell_loader/references/synth_child.rs:97` for every NPC with an `Inventory`, and from `restore_resident` on load.
  - False premises in comments:
    - `byroredux/src/npc_spawn.rs:1478-1490` (#3408).
    - `crates/plugin/src/equip.rs:250-266` (#3411).
- **Status**: NEW.
  - `gh` search "BODT" across all issue states: nothing.
  - #3408 (CLOSED) measured the same 10 ARMOs but misread them as authoring `BOD2 == 0`.
  - GAME-D1-2026-10-05-01 is a different `reconcile_worn_gear` defect (the weapon counted as a missing root).
- **Description**:
  - **Two sub-record versions.** Skyrim carries the biped body template in two versions:
    - `BOD2`: biped flags u32 + armor type u32.
    - `BODT`: biped flags u32 + general flags u8 + 3 pad, with an optional armor-type u32.
  - **Nothing reads `BODT`.** Both are xEdit's `wbBODTBOD2`, and the first u32 is the biped mask in both. The plugin decodes only `BOD2` (`ARMO`: `biped_flags = r.u32_or_default()` under `b"BOD2"`; `ARMA`: `b"BOD2" if is_skyrim_or_later`). `BODT` falls through to `_ => {}`, so every BODT-authored record loads with `biped_flags == 0`.
  - **ARMO consequence.** A real slot claim is read as "claims no region". The #3408 retain exemption (`authored_biped_mask == 0`) was built on that misreading. It keeps those meshes at spawn, so the gap stayed invisible until something else derived "equipped" from slot occupancy.
  - **#5034 is that something.** `reconcile_worn_gear` builds `equipped_forms` from `EquipmentSlots.occupants` ∪ `weapon`. `EquipmentSlots::equip` (`crates/core/src/ecs/components/inventory.rs:226`) iterates the set bits of the mask, so a zero-mask item never enters `occupants`. Every non-intrinsic root whose form is zero-mask therefore gets `expected_visible = false` and is stamped `NpcAppearanceHidden`.
  - **When the reconcile runs.**
    - From `reference_state::restore`, for every actor whose row was parked at unload. `capture` keeps every victim that has an `Inventory`, so in practice every NPC.
    - From the save-load path.
- **Evidence**: byte walk of the installed SE masters (`/tmp/audit/skyrim/bodt_census.py`; 24-byte TES5 headers, compressed bodies inflated).

  | Plugin | ARMO `BOD2` / `BODT` | ARMA `BOD2` / `BODT` |
  |---|---|---|
  | `Skyrim.esm` | 2,752 / **10** | 0 / **766** |
  | `Update.esm` | 156 / 0 | 20 / **10** |
  | `Dawnguard.esm` | 171 / 0 | 0 / **150** |
  | `HearthFires.esm` | 5 / 0 | 2 / 0 |
  | `Dragonborn.esm` | 741 / 0 | 165 / 0 |

  - **The 10 BODT ARMOs.** None has a zero mask:
    - `SkinDraugr`, `SkinSkeever`, `SkinSabrecat`, `SkinFrostbiteSpider`, `SkinFrostbiteSpiderCold`, `SkinSlaughterfish`: `0x04` (Body).
    - `SkinDraugrHair01/02`: `0x02` (Hair).
    - `SkinDraugrBeard01/02`: `0x10`.
  - **Which ones reach the reconcile.** The skins are intrinsic: RACE.WNAM on 7 races. `SkinFrostbiteSpiderCold` is reached only through NPC_ WNAM, which is never decoded (SKY-D3-2026-10-05-02). The reconcile filters intrinsic roots out. The hair and beard ARMOs are ordinary outfit items:
    - 14 OTFTs `INAM` them, for example `DraugrHair01Beard01` and `Draugr02Helmet01Beard01Outfit`.
    - 214 `NPC_` records point `DOFT` straight at those outfits, plus 14 direct TPLT children.
  - **Resulting chain:** the hair/beard root is stamped non-intrinsic `NpcEquipmentPart` (`resumable/prebaked.rs:96`). Its form id is in `Inventory` but not in `occupants`. `reconcile_worn_gear` then hides it.
- **Impact**:
  - **Draugr hair and beards.** Skyrim's most common dungeon enemy loses its hair and beard meshes the first time a crypt is re-entered, and after any quicksave/quickload inside one. The reconcile is deliberately silent and leaves no log. It never reverses itself, because the event path only acts on equip transitions and the mask stays 0.
  - **Creature skins.** The skins' real `0x04` Body claim cannot displace or be displaced, so the #2094 partition-hiding logic is inert for those 7 races. Not measured.
  - **ARMA addons.** The `equip.rs` #3411 rule's documented premise is false. With `BODT` decoded, it would skip 80 redundant same-slot addons on 75 `Skyrim.esm` ARMOs, which today all stack:
    - 68 of the 80 are `KhajiitRaceVampire` circlets. Each `Circlet0xArgonianAA` lists `KhajiitRaceVampire` among its additional races, so a Khajiit vampire wears both the Argonian and the Khajiit circlet mesh.
    - Others include `ArchmageHood_OrcAA` alongside `ArchmageHoodAA` on Orcs, `GagAA`, and the skeleton/troll naked variants.
  - Which addon the vanilla engine shows when two claim one slot is unsourced (DNAM priorities are equal on the Orc hood pair). Do not guess it.
- **Related**: #3408 (CLOSED; workaround built on the misread), #3411 / #3357 (ARMA multi-addon rules), #5034 (introduced the reconcile consumer), GAME-D1-2026-10-05-01 (same function, weapon gap), #2094 (occupancy filter).
- **Suggested Fix**:
  1. Decode `BODT` beside `BOD2` in both `parse_armo` and `parse_arma`: the first u32 is the biped mask, and the armor type is present only in the 12-byte form. Cite xEdit `wbBODTBOD2`, add a fixture for each width, and add a real-master census guard (10 ARMO / 766 ARMA BODT in `Skyrim.esm`).
  2. Correct the #3408 and #3411 comments.
  3. Re-measure what the zero-mask exemption still guards (likely nothing on vanilla Skyrim).
  4. Decide the same-slot ARMA winner rule from a sourced engine reference, not first-wins by assumption.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
