# Skyrim (SE + LE) Compatibility Audit — 2026-10-09

**HEAD**: 3bcf6c8e8 · **Baseline**: `docs/audits/AUDIT_SKYRIM_2026-10-08.md` (HEAD `00f580e09`, 81 commits ago) · **Audited**: Dim 1 (BSTriShape packed geometry, #5389 only), Dim 3 (NPC equip + FaceGen, including the Skyrim AI-package data on the spawn path), Dim 4 (load order + TES5 cell load, including the streaming-area commits) · **Unchanged since baseline (skimmed)**: Dim 2 (shader-type dispatch: its only window commit, `3031d8976` (#5281), is a pure-forward refactor), Dim 5 (archives + corpus: no window commits; gates re-run green).

Run context:
- This is one leg of `/audit-suite --preset streaming-deep`, run solo with no sub-agents.
- The area emphasis was `byroredux/src/{streaming,npc_spawn,cell_loader}/` as it applies to Skyrim.
- Scratch notes are in `/tmp/audit/skyrim/dim_{1..5}.md`.
- Census scripts (Python, byte walks of the installed masters) are in `/tmp/audit/skyrim/`: `esmwalk.py`, `bodt_census.py`, `arma_gate.py`, `arma_prio.py`, `necro.py`, `female_fallback.py`, `arma_mod3_count.py`, `fg_census.py`, `sm_heads.py`.
- The NIF probe crate is `/mnt/data/tmp/skyrim-audit-probe`. Its two binaries are a skin-decline census and a dismember-partition dump.

Constraints:
- **No engine launch.** The Whiterun control bench, the DLC repro, door transitions and the live render trace were not re-driven.
- **`--ignored` plugin tests ran only by exact name** (two Skyrim tests, each at about 1 GB RSS).

---

## Executive Summary

**Parse, archive and skinning layers are still fully green at HEAD:**
- **NIF corpus:** SE 33,468 / 33,468 clean across 8 archives; LE 22,466 / 22,466 clean.
- **SSE packed bone indices:** 19,606 weighted lanes, 0 changed.
- **Unit runs:** nif `--lib` (skyrim / bs_tri_shape / sse) 160 passed. plugin `--lib` (bodt / arma / worn_skin / wnam) 19 passed. Bin `npc_spawn|equip|skin` (rustc 1.96) 180 passed.
- **Exact-name real-data tests:** `skyrim_bodt_body_templates_decode_their_biped_masks` and `skyrim_npc_wnam_skin_census_matches_the_byte_walk` both pass.

**#5389 (`cf9dec258`) is safe on Skyrim.** The commit's census was FO4-only. Its check also covers zero-weight lanes, so it can decline a whole weight set. It was re-measured over every skinned Skyrim NIF:
- SE: 26,886 skinned meshes, 0 declines.
- LE: 21,318 skinned meshes, 0 declines.

**The baseline's two MEDIUM findings landed today and are verified on real data.**
- **#5358 (BODT):** Skyrim.esm decodes BODT on 10 ARMOs and 766 ARMAs.
  - The fix also repairs **all 2,762 LE ARMOs**: LE `Skyrim.esm` authors no `BOD2` at all, so before the fix every LE armor had mask 0. The commit text counts SE only.
  - The fix activates the #3411 same-slot gate on Skyrim: 156 ARMO × race × gender skip events, 136 of them Khajiit-vampire circlet pairs.
  - I checked every non-Khajiit skip:
    - All but one are same-partition alternatives (`archmagehoodm_1` vs `_orc_1`, both partition 131 with 439 vertices).
    - The one complement-shaped pair is `SkinSkeletonNecro*` × `RigidSkeletonRace`. No vanilla actor reaches it: all 13 users are `SkeletonNecroRace`, where only the necro addon matches.
    - I dropped this concern.
- **#5359 (WNAM):** of the 664 authored WNAMs, 663 resolve a race-matching addon. The one exception is `EncSprigganSwarm`, with 0 placements.

**New: two MEDIUM findings, both Skyrim data semantics on the NPC spawn path.**
- **SKY-D3-2026-10-09-01: the ARMA resolver has no female-to-male biped-model fallback.**
  - The CK wiki documents this fallback.
  - 172 of 766 Skyrim.esm ARMAs ship only a male model, including 22 of 29 shields and `ArmorStormcloakBoots`.
  - Female NPCs lose that gear entirely, and the skin partition underneath is still hidden. Female Stormcloak soldiers, guards and the MQ101 prisoners render with **no feet**, and female bandits and soldiers carry no shield.
- **SKY-D3-2026-10-09-02: the Skyrim force-greet dialect only recognises templates.**
  - `PackRecord::force_greet` looks only at a package's own procedure tree.
  - 334 of 339 Skyrim.esm force-greet packages are template instances with no tree of their own, and 320 of those author their own topic.
  - All 334 classify as `NotAForceGreet`. The "exactly 5" census and the #5376 premise "vanilla lists no ForceGreet-tree package on NPC_ defaults" (in fact 41 packages, 87 NPC_ edges) both rest on that template-blind count.

**Streaming-area commits checked for Skyrim reach (no new Skyrim defect):**
- **#5423 layout gate:** Skyrim's terrain LOD layout is `Combined`, so it takes the descent path and never runs the legacy index scan. The climate rung order puts the WRLD `CNAM` link first.
- **#5387 prune:** `FalloutLegacy` only; it never touches Skyrim.
- **#5424 default weather:** the per-cell XCCM path. On TES5 that field is a REGN, which is #5463 (open).
- **#5421 REGN WNAM:** a field rename only.
- **#5374 inherit-all:** gated on `GameKind::Oblivion`.
- **The `.btr` WATER plate:** skipped, so `spawn_lod_water_plane` stays the only distant-water source. No double draw.

**Cited, not re-filed:**
- **FO3-2026-10-09-D3-01** (PNAM "Use LOD Data" ignored) applies to Skyrim's city child worlds. There is no Skyrim-specific difference beyond the world list it already names.
- **Others:** ECS-2026-10-09-D7-01 (persistent CELL stamp), PHYS-D2-2026-10-09-01 (the live Skyrim SE `sap_axis` panic) and REN-D11-2026-10-09-01 (FrostmereCrypt contrast 2.0; storm weathers).
- **#5405** (open) still reproduces after #5385's previous-sibling fix: 6 Skyrim parent groups still strand 66 direct members. Details are under Dimension 4.

**Totals:** 2 NEW (0 CRITICAL, 0 HIGH, 2 MEDIUM, 0 LOW) · 0 regressions · matched-existing (open): #4256, #5408, #5462, #5463, #5464, #5405, #5015, #4913, #5288.

---

## Dimension Findings

### Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction

**0 findings.**

**Window commit `cf9dec258` (#5389):** a new `decline_unbounded_packed_indices` helper (`crates/nif/src/import/mesh/skin.rs:612`) bounds three producers by bone count — the BSTriShape inline arrays, the SSE `SseSkinGlobalBuffer` payload, and classic `NiSkinData`.
- **The risk:** the check tests every lane, including zero-weight lanes, and declines the whole weight set (bind pose) on the first out-of-range index. Skyrim was not part of the commit's census.
- **Probe result (`/mnt/data/tmp/skyrim-audit-probe`):** runs `import_nif` over every skinned NIF and counts the decline warning.
  - SE `Meshes0` + `Meshes1`: 6,902 skinned files, 26,886 skinned meshes, **0 declines**.
  - LE `Meshes.bsa`: 5,478 skinned files, 21,318 skinned meshes, **0 declines**.
- **No remap was reintroduced.** `widen_packed_bone_indices` is unchanged, and the real-data lane test still reports 19,606 lanes with 0 changed.
- **The dispatch invariants still hold:**
  - `BSLODTriShape` → `NiLodTriShape::parse` (`blocks/mod.rs:484`).
  - `BSMeshLODTriShape` → `BsTriShape::parse_lod` (`:489`).
  - `BSTreeNode` is at `:355` and `BSPackedCombined[Shared]` at `:760`.

### Dimension 2 — Shader-Type Dispatch + Skyrim Material Slice

**Skimmed. 0 new; 1 matched-existing.**

- **`3031d8976` (#5281):** a pure-forward-twin collapse in `material_translate.rs` with no semantic change.
- **`7d1de4ad5` (#5210):** Starfield CDB documentation only.
- `crates/nif/src/blocks/shader/` and `crates/nif/src/import/material/` have no diff in the window.
- **#4256** (`ImportedMaterial.shader_type` read from the raw tier, `byroredux/src/cell_loader/spawn/mesh_instance.rs:317`) is still OPEN.

### Dimension 3 — NPC Equip + FaceGen (M41) and Skyrim AI-Package Data on the Spawn Path

**2 NEW (MEDIUM); 3 matched-existing (#5408, #5462 open; #5288 transition-adjacent, not re-checked).**

Window commits:
- `479414ffe` (#5358) and `edb5fbdfe` (#5359): both verified, see the Executive Summary.
- `42aab4c09` (#5391) and `4e58f241d` (#5376): gameplay-owned. The Skyrim data claim inside `4e58f241d` is SKY-D3-2026-10-09-02.

Real-data verification of #5358 (`bodt_census.py`):

| Plugin | ARMO BODT / BOD2 | ARMA BODT / BOD2 |
|---|---|---|
| SE `Skyrim.esm` | 10 / 2,752 | 766 / 0 |
| SE `Update.esm` | 0 / 156 | 10 / 20 |
| SE `Dawnguard.esm` | 0 / 171 | 150 / 0 |
| SE `Dragonborn.esm` | 0 / 741 | 0 / 165 |
| **LE `Skyrim.esm`** | **2,762 / 0** | **766 / 0** |

- **General flags:** BODT carries `(ARMO) Non-Playable` (10 SE / 191 LE) and `(ARMA) Modulates Voice` (3). The fix drops both.
  - This is consistent with the BOD2 path. There, Non-Playable moves to the record header and is also unconsumed.
  - The CK wiki marks Modulates Voice "Not used".
  - Not filed.

#### SKY-D3-2026-10-09-01: The ARMA mesh resolver has no female→male biped-model fallback — 172 male-only Skyrim addons render nothing on female wearers, and the skin partition they displace is still hidden (footless female Stormcloaks, shieldless female soldiers)
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

#### SKY-D3-2026-10-09-02: `PackRecord::force_greet` reads only the package's own procedure tree, so 334 of the 339 Skyrim.esm force-greet packages (every concrete template instance) classify `NotAForceGreet`; the "exactly 5" census and #5376's "vanilla lists no ForceGreet-tree package on NPC_ defaults" premise rest on that blind spot
- **Severity**: MEDIUM
- **Dimension**: 3 — Skyrim AI-package data on the spawn path (runtime owners: `/audit-gameplay`, `/audit-scripting`)
- **Location**:
  - `crates/plugin/src/esm/records/misc/pack.rs:424-468`: `force_greet` looks for the leaf in `self.procedures` only.
  - `:723` and `:953`: `parse_skyrim_procedures` fills the pack's own tree only, and no post-pass inherits a tree from the template.
  - `byroredux/src/commands/quest.rs:897-904`: `dialogue.forcegreet` rejects `NotAForceGreet`.
  - `byroredux/src/npc_spawn/ai_package.rs:276-278`: the false premise. The comment was added in window commit `4e58f241d`.
  - `crates/plugin/tests/parse_real_esm.rs:5335-5390`: `force_greet_skyrim_tree_dialect_floor`, census 5.
  - `docs/engine/dialogue-trees.md:161-176`: "Census: exactly 5 `ForceGreet` packs".
- **Status**: NEW.
  - `force_greet` landed in `287214103` (#5367, before the baseline) and the false comment in `4e58f241d` (window).
  - Open force-greet issues (#5429, #5430, #5452, #5471) cover FO3 verification, the kill switch, KCC movers and doc rot — none of them this.
  - `gh` "ForceGreet PKCU template" across all states returns nothing.
- **Description**:
  - **How Skyrim stores force-greets.** A Skyrim concrete package names its behaviour through `PKCU`'s template FormID and carries only data-input values (`PDTO` topic, location and so on). The procedure tree lives on the template.
  - **The codebase already knows this:** `ai_package.rs:1104-1106` builds the Sandbox fixture as "a template *instance* — what an NPC's PKID list actually names. Carries no procedures of its own", and `AmbientBehavior::from_package` chases `package_template_form_id` for Sandbox and Patrol (`:806-810`, `:974-978`).
  - **Where it breaks.** `force_greet` does not chase the template. It finds a leaf only on the 5 template packs themselves, and returns `NotAForceGreet` for every package an NPC or alias actually uses.
  - **Why the topic fallback never runs.** The method's "template inheritance" fallback (`:457-463`) is unreachable for an instance, because the leaf lookup fails first.
- **Evidence**: `fg_census.py`, a byte walk of SE `Skyrim.esm`.
  - **The population.** 5,961 PACKs; 5,758 carry no procedure tree of their own. 339 PACKs have a `ForceGreet` leaf in their own or their `PKCU` template's tree.
  - **Own-tree packs (5):** the templates `ForceGreet`, `ForceGreetFromSitting`, `ForceGreetWaitSitting`, `OrcGuardOutsideForcegreetPackage` and `dunWhiteRiverWatch_WatchmanForcegreetTemplate`. The dt2 gate uses only these (`0x000BBAA4`, `0x0003C1C4`).
  - **Template instances (334):** their templates are `ForceGreet` (302), `ForceGreetFromSitting` (24), `ForceGreetWaitSitting` (4), `OrcGuardOutside…` (3) and the White River Watch template (1). **320 of the 334 author their own type-0 `PDTO` topic.**
  - **On NPC_ default packages:** 41 of the instances sit on NPC_ `PKID` lists, for 87 NPC_ edges. Examples: `VigilantofStendarrForcegreetDaedric` ×39, `CartDriverOnCartForcegreet` ×6, `TG04EECGuardForcegreetPackage` ×2, `GuardRiftenKeepPrisonJailerFG` ×2, `HousecarlWhiterunGreetPlayer`, `MQPaarthurnaxForceGreetNormal`, `MarkarthMuseumForcegreetPackage`, `DA01FINAraneaForcegreetPackage` and `MS01ForswornThreatenForcegreetPackage`.
  - **This falsifies the ai_package.rs comment** that no vanilla NPC_ default lists a ForceGreet-tree package.
- **Impact**:
  - **The feature works only on templates.** The Skyrim half of #5367 Phase 4 ("Landed 2026-10-08") is live for the 5 templates only. `dialogue.forcegreet` refuses all 334 authored concrete Skyrim force-greets, Paarthurnax's and the carriage drivers' included.
  - **Future installers inherit the blind spot.** The quest/alias installers the design says "adopt the same bridge" would get `NotAForceGreet` for every quest-alias force-greet.
  - **The scope decision rests on the false count.** The decision not to wire the Skyrim tree dialect into ambient selection was justified by the 0-reference census. The real reach is 41 packages and 87 NPC_ edges.
  - **The gates cannot catch it.** The real-data test pins a floor of 5, and dt2 exercises only own-tree packs.
- **Related**:
  - #5367 (closed) and #5376 (closed): the FO3/FNV census correction that made the same "0 references" mistake for procedure 15.
  - ESM-2026-10-09-D2-02: PKDD Dialogue Type misread. That is the FO3/FNV arm of the same gate set.
- **Suggested Fix**:
  - Give `force_greet` the template that `from_package` already resolves (`package_template_form_id` chased through the index). Find the leaf on the template, and read the topic from the instance's own data inputs at the leaf's `PKC2` indexes.
  - Correct the ai_package comment, `dialogue-trees.md` §4 and the real-data floor to the 339 / 334 / 320 census.
  - Add an instance pack to dt2, for example `MQPaarthurnaxForceGreetNormal` or a `CartDriverOnCartForcegreet` carrier.
  - Whether ambient selection should then install Skyrim force-greets is a separate gameplay decision. It needs the instance's conditions modeled first, the same gate #5376 applies to FO3/FNV.

#### Still open (re-verified, not re-filed)
- **#5408** (SKY-D3-2026-10-08-01, the PNAM-only head fallback): `race.rs` has no RACE `HEAD` arm, and `prebaked.rs` is unchanged in the window.
- **#5462** (SKY-D3-2026-10-08-02, the `is_child_race` pin): `npc_spawn/tests.rs:2971` still asserts `!is_child_race(GameKind::Skyrim, Some(0x04))`.

### Dimension 4 — Multi-Master Load Order + TES5 Cell Load (+ streaming-area commits)

**0 new; 3 matched-existing (#5463, #5405, #5015).**

Window commits on the paths:
- **`5b68792b7` (#5393/#5395):** the voice owner is now resolved through `GlobalFormIdResolver`. Skyrim `.fuz` voice is out of scope by design (`dialogue_voice.rs` module doc; #5464 tracks the container claim).
- **`67afa4b95` (#5374):** the `wrld.rs` inherit-all stamp is gated `GameKind::Oblivion`, so it does not affect Skyrim.

Streaming-area commits traced for Skyrim reach (all correct for Skyrim):
- **`b3e679dba` (#5423):** `terrain_lod_layout(Skyrim) = Combined`, so `stream_lod_blocks` takes the descent path, and `reconcile_lod_rings` never scans the legacy quad index.
  - `resolve_exterior_climate` puts the WRLD `CNAM` link (with the PNAM-gated parent chain) first.
  - The region rung is inert on TES5: there is no REGN `CNAM`, per #5421.
  - The Oblivion rungs are game-gated.
- **`302394f94` (#5424):** the per-cell `XCCM` re-resolve now shares `resolve_default_weather`. On TES5, `XCCM` is a REGN and every vanilla occurrence is interior, so this is **#5463** (open). `resolve_cell_climate`'s doc still asserts the Skyrim CLMT premise.
- **`fbf1bed74` (#5387):** `select_authored_lod_quads` is reached only under `FalloutLegacy`.
- **`bd052048a` (#5421):** a field rename only.

The `.btr` `WATER` plate is skipped (`terrain_lod_btr.rs:161-186`), so distant water comes from `spawn_lod_water_plane` alone.

Two suite findings also apply to Skyrim and are cited, not re-filed:
- **FO3-2026-10-09-D3-01** (WRLD PNAM "Use LOD Data" ignored) is the Skyrim city-child-world case: `WhiterunWorld` and the others look up `meshes\terrain\<child>\…`, which does not exist.
- **ECS-2026-10-09-D7-01** covers the Tamriel persistent CELL.

**#5405 (open) still reproduces after #5385.** `846e4a6dd` re-read `SNAM` as the previous sibling and links one head per parent group.
- **Re-measure** (`sm_heads.py`, Skyrim.esm): of 116 parent groups, 6 still strand members — 66 direct members, plus their subtrees.
  - `DungeonNode`: 50 members, 8 heads.
  - `BQBranchNodeSHARES`: 9 members, 4 heads.
  - `Root`: 24 members, 3 heads.
  - `CompanionsRadiantNode` (stranding `CompanionsReconNode`), `AssaultActorEvent` (stranding `WIAssautRememberNode`), and one unnamed group of 21 (stranding `CWActorDialogueNode`, `GenericScenes`, `CaravanScenes`, …).
- **Recommend:** add this re-measure to #5405. Its "41 nodes" count was taken under the old next-sibling reading.

**#5015** (XRGD corpse poses unconsumed) has no window commit.

### Dimension 5 — Archives + Corpus Gates

**Skimmed (no window commits). 0 findings.**

All runs used rustc 1.96, release, with `CARGO_TARGET_DIR=/mnt/data/tmp/skyrim-audit-target`:
- **`parse_rate_skyrim_se`:** 33,468 / 33,468 clean. Meshes0 18,862; Meshes1 13,847; `_ResourcePack` 149; Creation Club 231 / 266 / 65 / 4; Animations 44.
- **`parse_rate_skyrim_le`:** 22,466 / 22,466 clean.
- **`packed_sse_indices_match_partition_palette_expansion_on_real_data`:** 19,606 lanes, 0 changed.

---

## Shader-Type Coverage Matrix

Parse and import are unchanged since 2026-09-22. The render column is unchanged since 2026-10-05; #5281 is non-semantic.

| Variant | Numeric type(s) | Parse | Import (→ `ImportedMesh.material`) | Render |
|---|---|---|---|---|
| `None` (no trailing data) | 0, 2, 3, 4, 8, 9, 10, 12, 13, 15, 17, 18, 19, 20 | Complete (0 bytes; #4252 pin) | N/A (kind carried as `material_kind`) | Default lit; kinds 2..=20 protected from the glass classifier |
| `EnvironmentMap` | 1 | Complete | Complete (`env_map_scale`) | Consumed; alchemy glass → glass (#4392) |
| `SkinTint` | 5 | Complete | Complete | Consumed; #4423 tint alpha weight |
| `HairTint` | 6 | Complete | Complete | Consumed |
| `ParallaxOcc` | 7 | Complete | Complete | Consumed |
| `MultiLayerParallax` | 11 | Complete | Complete | Consumed |
| `SparkleSnow` | 14 | Complete | Complete | Consumed |
| `EyeEnvmap` | 16 | Complete | Complete | Consumed |
| FaceTint (no payload) | 4 | Complete | `material_kind = 4` keys the FaceGen tint override (#4421) | Consumed |

Residual: #4256.

---

## Cell-Load Regression Status

- **TES5 walk:** no window commit touches STAT / REFR / LIGH resolution, the LAND scale or the CELL sub-record arms (the only cell-walker change is the Oblivion-gated `wrld.rs` stamp).
- **Real-master tests:** two Skyrim plugin real-data tests passed by exact name (BODT census, WNAM census). `parse_real_skyrim_esm` was not re-run.
- **Multi-master:** the #3813 ordered fold is unchanged. `load_order.rs` gained only the voice-owner name helper's use. The DLC repro was not re-driven.
- **Actor-appearance path:**
  - #5358 and #5359 are closed and verified.
  - Open: #5408 (race base head parts), SKY-D3-2026-10-09-01 (female fallback) and #5015 (XRGD).
- **Whiterun BanneredMare control bench:** INCOMPLETE, not FAILED, because there was no engine launch. The reference is ROADMAP's Bench-of-record. `/audit-runtime` owns the live baseline.
- **Live exterior context (from the suite):**
  - Today's #5482 closed the Tamriel "black void" as a presentation-contrast issue.
  - REN-D11-2026-10-09-01 notes the toe still crushes at contrast > ~1.556 (Skyrim storm weathers, FrostmereCrypt).
  - PHYS-D2-2026-10-09-01 is the live SE grid 0,0 `sap_axis` panic.

---

## Totals

| Severity | New | Regression | Matched-existing (open) |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 2 (SKY-D3-2026-10-09-01, SKY-D3-2026-10-09-02) | 0 | #4256, #5408, #5015, #5405 |
| LOW | 0 | 0 | #5462, #5463, #5464 |

Also still open and outside this skill's scope: #4913 (Skyrim `.btt` tree LOD, exterior-owned) and #5288 (door-transition double archive open, performance-owned).

| Dimension | New | Matched-existing |
|---|---|---|
| 1 — BSTriShape / SSE recon | 0 | 0 |
| 2 — Shader-type / material slice | 0 (skimmed) | 1 (#4256) |
| 3 — NPC equip + FaceGen / AI-package data | 2 MEDIUM | 2 (#5408, #5462) |
| 4 — Load order + cell load + streaming | 0 | 3 (#5463, #5405, #5015) |
| 5 — Archives + corpus | 0 (skimmed) | 0 |

Suggested: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-10-09.md`. Label every finding `game:skyrim` + `legacy-compat`, then add:
- **SKY-D3-2026-10-09-01:** `medium`, `bug`, `inventory`, `esm-plugin`, plus `game:fo4` (same code path; 187 male-only FO4 ARMAs).
- **SKY-D3-2026-10-09-02:** `medium`, `bug`, `ai`, `dialogue`, `esm-plugin`.
