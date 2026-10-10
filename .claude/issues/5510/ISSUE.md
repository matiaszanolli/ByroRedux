# #5510: SKY-D3-2026-10-09-01: The ARMA mesh resolver has no female→male biped-model fallback — 172 male-only Skyrim addons render nothing on female wearers, and the skin partition they displace is still hidden (footless female Stormcloaks, shieldless…

**Labels**: bug, esm-plugin, game:fo4, game:skyrim, gameplay, inventory, legacy-compat, medium

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-09.md` — finding `SKY-D3-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: MEDIUM
- **Dimension**: 3 — NPC equip
- **Location**:
  - `crates/plugin/src/equip.rs:226-236`: the `pick_path` closure returns `None` for `Gender::Female` when `female_biped_model` is empty. It is used by both pass 1 (`:281-298`) and pass 2 (`:313-320`).
  - Downstream: `byroredux/src/npc_spawn.rs:1454` (the occupancy `equip()` runs before, and regardless of, mesh resolution), `:1476-1500` (`displaced_mask` → the skin's `hidden_biped_mask`) and `:1572-1580` (`facegen_hidden_mask`).
- **Status**: NEW. This is a pre-existing M41 gap, not a regression. The nearest prior issue is #3416 (CLOSED), the FO3/FNV `MOD3`-not-parsed defect, which was fixed on the non-ARMA arm (`:200-218`), where an empty female slot correctly falls back to `MODL`. The Skyrim+/ARMA arm never got the same rule. Searches for "female biped model fallback ARMA" and "MOD3 female ARMA" across all states return only #3416 and #896.
- **Description**:
  - **The rule.** The Skyrim CK documents the fallback explicitly: "If the biped model for female is not defined, it will use the biped model for male." (`/mnt/data/src/reference/ck-uesp-wiki/(main)/ArmorAddon.wiki:11`).
  - **What the code does instead.** `pick_path` treats an empty `MOD3` as "this addon has no mesh for this wearer". For a female actor the item still occupies its biped bits, because `equipment_slots.equip()` runs from the ARMO mask before any mesh lookup.
    - The item contributes no `ResolvedArmor`.
    - The race skin's `displaced_mask` still hides the skin's dismember partitions under those bits.
    - A closed helm's bits still feed `facegen_hidden_mask`.
  - **Net result.** The gear is invisible and the body region under it is cut away.
- **Evidence**: `female_fallback.py`, a byte walk of SE `Skyrim.esm`.
  - **Male-only ARMAs.** 172 of 766 author `MOD2` with no `MOD3`: 122 actor/creature, 22 shields, 13 helmets/hoods, 9 jewelry, 1 boots, 5 other. Of the 29 shield ARMAs, 22 lack `MOD3` (Iron, Hide, Steel, Banded and Elven among them).
  - **Other plugins.** Dawnguard has 56 / 150, Dragonborn 46 / 165, and FO4 187 / 739 on the same code path (the FO4 rule is unsourced locally).
  - **Default outfits (DOFT, leveled lists expanded).** 261 female NPC_ × ARMO pairs across 101 NPC_ records resolve **no** mesh. In every case the male model exists and is what the engine would show:
    - Shields: 218 pairs, including `ArmorIronShield` (62) and `ArmorHideShield` (59).
    - `ArmorStormcloakBoots`: 14 NPC_. These are `EncSoldierSonsNordF01–03`, `EncGuardSonsF01–03`, `MQ101StormcloakPrisonerFemale` (plus the `02` and `Unaggressive` variants), `MQ101StormcloakCartFemale` (placed), `TreasCorpseCWSonsFemale` (8 placed) and `TreasCorpseGuardRiften02`.
    - `DraugrHelmet03`: 6 female Draugr leveled bases.
    - `ArmorImperialHelmetFull`: 1. As a closed helm, it also hides the FaceGen head partitions.
  - **Why the feet disappear.** `SkinNaked`'s mask includes Feet (`0x80`), so the boots' occupancy hides `NakedFeet`'s partition 37 while the boots draw nothing.
  - **Skins are unaffected.** Every vanilla female WNAM and race skin resolves a female model (81 / 81 pass-1), so no female body vanishes from this path.
- **Impact**:
  - **Visible on every Skyrim session with female armored NPCs.** Female Stormcloak soldiers and Windhelm guards walk on stumps. The female prisoner in the MQ101 opening cart has no feet. Female bandits and soldiers carry no shield, female Draugr lose their helmets, and a female wearer of a full Imperial helm loses face and helmet together.
  - **Undetectable by the current gate.** The equip smoke floor (`m41-equip.sh`) counts `Inventory` / `EquipmentSlots` components, not resolved meshes, so this passes silently.
- **Related**:
  - #3416 (closed): the same rule on the FO3/FNV ARMO arm.
  - #5358: its gate also reads `pick_path`; the fallback must apply before `covered |= …` so that a male-only addon is not mistaken for "no claim".
  - FO4: same code path, flag `game:fo4` for the FO4 auditor.
- **Suggested Fix**:
  - In `pick_path`, fall back to `male_biped_model` when the female slot is empty, mirroring the `:200-218` arm and the CK rule.
  - Add a real-data pin: female `EncSoldierSonsNordF01` resolves `BootsM_1.nif`, and the shields resolve for a female wearer.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
