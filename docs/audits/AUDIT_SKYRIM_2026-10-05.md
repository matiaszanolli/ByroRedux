# Skyrim (SE + LE) Compatibility Audit — 2026-10-05

**HEAD**: `a2c24b16e` · **Baseline**: `docs/audits/AUDIT_SKYRIM_2026-09-29.md` (HEAD `9fcfdc3fc`, 313 commits ago) · **Audited**: Dim 2 (shader-type dispatch + Skyrim material slice), Dim 3 (NPC equip + FaceGen), Dim 4 (multi-master load order + TES5 cell load), Dim 5 (archives + corpus gates) · **Unchanged since baseline (skimmed)**: Dim 1 (BSTriShape packed geometry + SSE reconstruction). Its only commit is `530c9e7aa`, a #5121 clippy rewrite (`chunks_exact(3)` → `as_chunks::<3>()`) that does not change behaviour. Guards were spot-checked green.

Run context: one leg of `/audit-suite --preset comprehensive`. All five dimensions were analysed synchronously in this session, with no sub-agents. Per-dimension scratch notes are at `/tmp/audit/skyrim/dim_{1..5}.md`, and the census scripts are `/tmp/audit/skyrim/{armo_scan,bodt_census}.py`. Constraints: no engine launch, so the Whiterun control bench, the DLC repro and the live render trace were not re-driven. Real-data evidence comes from single named `--ignored` tests and from byte walks of the installed SE masters.

---

## Executive Summary

Skyrim SE's parse and archive layers are still fully green:
- **NIF corpus:** SE 33,468 / 33,468 clean across 8 archives; LE 22,466 / 22,466 clean.
- **Per-block baselines:** both green. SE is OK at 145 types now that #4628 has regenerated the TSV and the gate names corpus-size drift. LE is OK at 146 types.
- **Archives:** v105 `declared_size` matches extract on all 19,443 `Meshes0` entries.
- **`Skyrim.esm` walk:** 590 cells, 18,318 statics (equal to the baseline), 37 worldspaces, 590/590 extended XCLL.
- **DIAL:** QNAM ownership holds (117 MS01 topics), and #5045 now decodes the topic category correctly.
- **SSE packed bone indices:** 19,606 weighted lanes, 0 changed by the importer.

**Two NEW findings (both MEDIUM), both in the Skyrim body/equip data path.**

**SKY-D3-2026-10-05-02:** the per-NPC skin override `NPC_.WNAM` is never decoded. At least 442 vanilla actors wear their race's default body instead of the skin authored for them. They include every `SkinDraugrMale0x` Draugr variant, the Falmer variants and `MQ101Alduin`, the Helgen intro on the MQ101 route.

**SKY-D3-2026-10-05-01:**
- **What is missing:** Skyrim authors the biped-slot mask of 10 `Skyrim.esm` ARMOs, and of every `Skyrim.esm` / `Dawnguard.esm` ARMA (766 + 150), in the older `BODT` sub-record. The parsers read only `BMDT` / `BOD2`, so all of these load with mask `0`.
- **Why it went unnoticed:** two earlier decisions rest on that zero:
  - #3408's "author `BOD2 == 0`" exemption keeps creature skins alive at spawn.
  - `equip.rs`'s #3411 comment ("Skyrim.esm 0 / 766 author bits … the Skyrim-era ARMA record has no per-addon biped-slot field") states the zero as a fact.
- **New consequence this window:** #5034's post-restore `reconcile_worn_gear` builds "what is equipped" from biped-slot occupancy. A zero-mask item never occupies a slot, so every Draugr's hair and beard is hidden on cell return and on save-load. That covers 14 vanilla outfits and 214 directly-outfitted Draugr NPC_ records (SKY-D3-2026-10-05-01).

Other Skyrim-facing work in the window checks out clean:
- **#5057 early-Z for material kinds 1–16:** every `discard` in `triangle.frag` sits under an excluded arm, and the shader never writes `gl_FragDepth`.
- **LSCR model cover:** SNAM/RNAM/XNAM widths match xEdit TES5, and RNAM is converted from degrees.
- **Starts Unconscious (#5017):** correctly gated off TES5. xEdit's TES5 ACHR flag list has no bit 13, and the census guard pins 0 in `Skyrim.esm`.
- **XRGD decode (260afc33f):** the data half of #5015, which stays open.
- **Strings-archive discovery fix (`c7e53a20a`):** works for a bare relative `--esm Skyrim.esm` launch.

**Skyrim-relevant findings filed today by sibling audits (cited, not re-filed):**
- SCR-D2-01 HIGH / #5152: a spoken-line SetStage never runs the new stage's quest fragment (MS01 route).
- SCR-D5-01: the INFO Say Once flag is not honoured.
- ESM D2-01: the INFO `DATA` layout is wrong.
- ECS D5-01: the LSCR turntable advances with dt = 0.
- PERF-D7-01: a door transition opens the archives twice for the LSCR model cover.
- UI-D1-01: #4757 is closed but unfixed (HUD Menu name).
- UI-D3-01: Skyrim catalog labels.
- GAME-D1-2026-10-05-01: `reconcile_worn_gear` counts the weapon as a missing mesh. Same function as this audit's finding, different defect.

**Totals: 2 NEW** (0 CRITICAL, 0 HIGH, 2 MEDIUM, 0 LOW) · 0 regressions · 4 matched-existing (open) · 1 prior matched-existing now resolved (#4628).

---

## Dimension Findings

### Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction

**Unchanged since 2026-09-29; guards spot-checked. 0 findings.**

- **Only commit:** `530c9e7aa` (#5121). It is a clippy rewrite in `import/mesh/{bs_tri_shape,normal,skin}.rs`. `as_chunks::<3>().0` drops the remainder exactly as `chunks_exact(3)` does, so behaviour is unchanged.
- **Packed-bone-index path:** `widen_packed_bone_indices` (`crates/nif/src/import/mesh/skin.rs:553`) is still the only path, and `remap_bs_tri_shape_bone_indices` has 0 hits.
- **Dispatch:** `BSLODTriShape` → `NiLodTriShape::parse` (`blocks/mod.rs:484`) and `BSMeshLODTriShape` → `BsTriShape::parse_lod` (`:489`) are intact.
- **Specialty arms:** present — BSTreeNode `:355`, BSPackedCombined[Shared] `:760`, BSLagBoneController `:910`, BSProceduralLightningController `:912`.
- **Tests:**
  - `-p byroredux-nif --lib` with the Dim 1 + Dim 2 filter set: 348 passed.
  - `packed_sse_indices_match_partition_palette_expansion_on_real_data`: ok (19,606 lanes, 0 changed).

### Dimension 2 — Shader-Type Dispatch + Skyrim Material Slice

**0 new; 1 matched-existing.**

No commits touched `crates/nif/src/blocks/shader/` or `shader_tests/`, so the wire decode and `parse_shader_type_data` are unchanged. Window commits on this dimension's paths, and their effect on Skyrim:
- **`c2b67d81e` (#4441):** doc-only.
- **`e80f7e854` (#5012):** BGSM spec-disabled roughness. Vanilla Skyrim authors no BGSM.
- **`235a90ba2` (#4912):** `.btr` / terrain-LOD clamp on the texture-only translate path. Owned by /audit-exterior. Ordinary meshes are unaffected.
- **`5fe978b1d` / `d1ec1c57f`:** spawner-guard plumbing in `material_translate.rs`.
- **`abb92d0b5` / `6a512d1b5` / `bd25d805b`:** docs.

**Off-path, Skyrim-material relevant: `3c197ed8c` (#5057).** It admits `material_kind` 0..=16 to early fragment tests. I re-verified this against `triangle.frag`:
- The six discards are:
  - alpha-test `aThresh > 0` (`:459`);
  - the implicit pure-blend discard (`:473`);
  - effect-shader 101 (`:1204`);
  - fire-refraction 103 (`:1245/1266/1286`).
- The certificate excludes `alpha_threshold != 0` and `alpha_blend`.
- There is no `gl_FragDepth`.

It is sound for every Skyrim lighting-shader type. Types 17–20 (Cloud, LODLandNoise, MultiTexLandLODBlend, Dismemberment) stay late-test. That only costs performance, so it is not filed.

Checklist re-verified:
- **`PBR_BSDF` producers:** only `pack_imported_material_flags` (where `is_pbr` comes from BGSM only) and `cornell.rs`. That gives 0 vanilla Skyrim instances.
- **Glass-classifier guard:** `helpers.rs:171` (`2..=20` and `!from_bgsm`) is intact.
- **`GpuMaterial` size:** still 432 B (`material_tests.rs:66`).
- **Emissive source:** Skyrim maps to `EmissiveSource::Lighting` (`dedicated_shader.rs:443`).
- **`env_map_scale_consumed`:** the latches are present in `legacy_properties.rs`.

#### SKY-D7-2026-09-11-02 — `ImportedMaterial.shader_type` never crosses the NIFAL boundary
- **Severity**: MEDIUM
- **Status**: Existing: #4256 (OPEN). Unchanged: `byroredux/src/cell_loader/spawn/mesh_instance.rs:317` still reads `material.shader_type` from the raw tier.

### Dimension 3 — NPC Equip + FaceGen (M41)

**2 NEW (both MEDIUM); 1 matched-existing.**

Window commits:
- `7ead491d7` (#5061): shared `AppearanceProviders`.
- `0df2f88e0` (#5194): skinned BLAS swap-on-success.
- `20717d5b7` (#5034): `reconcile_worn_gear`.
- `c20dcdb04` (#5047): the player reads its TPLT terminals. The vanilla Skyrim player has no TPLT.
- `964717fcc` (#5049).
- `f78e018ac` (#5028): gear release.
- `37db35cca` (#5091): splits `resumable.rs` into `resumable/{mod,prebaked,runtime}.rs`.
- `257e973d2` (#4938).
- `3c08cfe50` (#5017): FO4 / FO76 / SF only.
- `8f7acf3a7`: FaceGen docs.

Re-verified:
- The prebaked arm still runs Skeleton → Facegen → Armor → Finalize. `skip_missing_facegen` (`resumable/prebaked.rs:31`) still jumps straight to `Armor(0)`.
- The #3409 displaced-head-partition hide is at `prebaked.rs:216-249`.
- The #4421 FaceTint-only override is unchanged.

Tests: the bin crate run with filter `npc_spawn load_order reference_state loading_screen material_translate glass_classification scene::nif_loader loot_appearance` passed 287 (rustc 1.96). The plugin `--lib` run with filter `esm::cell esm::reader load_screen dialogue equip actor::` passed 415.

#### SKY-D3-2026-10-05-01: Skyrim's `BODT` biped-slot sub-record is never decoded (10 ARMOs, 916 ARMAs) — #5034's post-restore gear reconcile now hides every Draugr's hair and beard on cell return and on load
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

#### SKY-D3-2026-10-05-02: Skyrim's per-NPC skin (`NPC_.WNAM`) is never decoded — 442 vanilla actors, including Alduin, every Draugr variant skin and the Falmer variants, wear their race's default skin instead
- **Severity**: MEDIUM
- **Dimension**: 3 — NPC equip + FaceGen (Skyrim body source)
- **Location**:
  - `crates/plugin/src/esm/records/actor/mod.rs:1948`: the only skin decode, on `RACE` (`RaceRecord.default_skin`). The `NPC_` parser has no `WNAM` arm; `grep 'b"WNAM"' crates/plugin/src` finds no NPC hit.
  - `byroredux/src/npc_spawn.rs:1280`: `if let Some(skin_fid) = race.default_skin`. The race skin is the only intrinsic body layer.
- **Status**: NEW.
  - `gh` searches across all states for "worn armor", "WNAM skin", "Alduin", "SkinDraugrMale" and "default skin NPC" find no match.
  - #2093 (CLOSED) added only the RACE `WNAM` decode.
  - Sibling reports dated 2026-09-2x / 2026-10-0x do not mention it.
- **Description**:
  - **What WNAM is.** A Skyrim NPC_ can override its race's body. In the CK it is the **Skin** field on the Traits tab, and the "Use Traits" template flag governs inheriting it. Source: `ck-uesp-wiki` *Creating a Stable*, Step 04: "below the Race drop down box, click on the Skin drop down box … the horse wears the armor, the armor wears the addon, the addon points to your custom mesh". On disk it is `NPC_.WNAM` → ARMO.
  - **What the engine does instead.** It never reads the field. Every NPC's intrinsic skin layer is its race's `WNAM`, so an authored per-actor body (its mesh and texture set, through the ARMO → ARMA chain) is replaced by the race default.
- **Evidence**: byte walk of `Skyrim.esm`.
  - 664 of 5,118 NPC_ records author `WNAM` on the shell.
  - Resolving each NPC's skin through its Use-Traits chain (ACBS template flag `0x0001` + `TPLT`, as the CK does) gives **442** NPC_ records whose effective skin ARMO differs from their race's `WNAM`. Another 1,118 records template off an LVLN and were not resolved statically, so 442 is a floor.
  - The six Alduin records are `AlduinBase`, `MQ101Alduin` (the Helgen intro on the MQ101 route), `MQ106Alduin`, `MQ206Alduin`, `MQ206AncientAlduin` and `MQ304Alduin`.

  Largest groups:

  | Skin ARMO | NPC_ records |
  |---|---|
  | `SkinDraugrMale05` / `02` / `04` / `03` / `07` / `01` | 73 / 66 / 66 / 51 / 34 / 7 (297 total) |
  | `SkinFalmer01`–`06` | ~34 |
  | Horse hides (Black, Palomino, BlacknWhite, Grey, …) | ~20 |
  | `SkinWolfBlack` / `SkinWolfSummon` | 16 / 4 |
  | `SkinSkeletonNecro*` (3 variants) | 12 |
  | `dunLabrynthianDraugrArmorFX` | 8 |
  | `skinDragonAlduin` | 6 |
  | `SkinFrostbiteSpiderCold` | 5 |
  | `SkinMammothBranded`, `SkinMagicAnomaly` | 3, 3 |
- **Impact**:
  - The Draugr population's authored body variety collapses to the single `SkinDraugr`.
  - Alduin, the main-quest antagonist, renders with the generic dragon race skin.
  - Falmer variants, black wolves, branded mammoths, necromancer skeletons and the Labyrinthian FX draugr all lose their authored look.
  - The bug is silent: the race skin still resolves, so every equip guard and the m41 smoke stay green.
  - Mechanism, not just data: modded NPC skin overrides, the standard way Skyrim mods re-skin actors, are dropped the same way.
- **Related**: #2093 (RACE `WNAM`), #3408 / SKY-D3-2026-10-05-01 (the creature skins' `BODT` masks), #4092 / #4812 (Use-Traits / TPLT terminal resolution), #4457 (`ResolvedNpc`).
- **Suggested Fix**:
  1. Decode `NPC_.WNAM` (remapped FormID) onto the NPC record.
  2. Resolve it through the Use-Traits terminal of `ResolvedNpc`, the same terminal race and gender come from.
  3. In `build_npc_equip_state`, prefer it over `race.default_skin` as the intrinsic skin layer.
  4. Add a real-data guard: a `SkinDraugrMale05` Draugr resolves that ARMO, and Alduin resolves `skinDragonAlduin`.

### Dimension 4 — Multi-Master Load Order + TES5 Cell Load

**0 new; 1 matched-existing.**

Window commits:
- **`260afc33f`:** decodes XRGD → `PlacedRef::ragdoll_pose`. Starts Unconscious bit 13 is gated to FO4 / FO76 / SF only. `GameKind` is now threaded through `parse_refr_group` / `parse_wrld_*`.
- **`7ab87c0fb` (#5045 / #5037):** the Skyrim+ DIAL category is now read from byte 1; 3,061 Skyrim DLBRs decode; the reachable-topic list follows branches.
- **`c7e53a20a`:** an empty `Path::parent()` now counts as cwd, so a bare `--esm Skyrim.esm` finds `strings\skyrim_english.*` again.
- **`dca2bb401` / `de6bdb381` / `717f39a82`:** Oblivion / FO3 / FNV corpse rules. Not Skyrim.

Off-path:
- **`9b099b31f`:** `remap_fid_or_sentinel`. Skyrim EFID is still remapped; only Oblivion's char4 EFID is gated.
- **`e60911864`:** the LSCR model cover. The Skyrim inline pose decode was checked against xEdit TES5:
  - SNAM is f32 scale;
  - RNAM is 3×i16 degrees and is converted to radians before `euler_zup_to_quat_yup_refr`;
  - XNAM is 3×f32 translation;
  - ONAM is 2×i16.

  The runtime parts are covered by ECS D5-01 and PERF-D7-01.

Real data:
- `parse_real_skyrim_esm`: ok (590 / 18,318 / 37; Winking Skeever 981 refs).
- `ms01_eltrys_authored_placement`: ok. 117 MS01 topics, `category=Topic`, `quest_refs` populated.
- `skyrim_xrgd_poses_match_the_census_and_nothing_is_unconscious`: ok.

#### SKY-D4-2026-09-29-01 — Skyrim authored corpse poses (XRGD) not applied
- **Severity**: MEDIUM
- **Status**: Existing: #5015 (OPEN). The data half landed in 260afc33f: it decodes poses but notes the bone ids are not unique and the positions are parent-relative. Nothing poses the skeleton yet.

### Dimension 5 — Archives + Corpus Gates

**0 new; 1 matched-existing; 1 prior matched-existing resolved.**

Window commits:
- **`8c16fec42` (#4628, CLOSED):** regenerates `skyrim_se.tsv` for the ±86 BSDynamicTriShape ↔ BSTriShape archive swap. `run_baseline` now checks the TSV's `total=` header before the per-type comparator and names any drift.
- **`097f51b48` (#5227):** doc reattachment.

There are no codec, layout, naming or sibling changes. The `.btt` registration (#4913) did not move the NIF count, which is correct because `.btt` is a non-NIF format.

Runs:
- `parse_rate_skyrim_se`: 33,468 / 33,468 across 8 archives (Meshes0 18,862, Meshes1 13,847, _ResourcePack 149, CC 231 / 266 / 65 / 4, Animations 44).
- `parse_rate_skyrim_le`: 22,466 / 22,466.
- `per_block_baseline_skyrim_se`: OK (145). **#4628 is verified resolved.**
- `per_block_baseline_skyrim_le`: OK (146).
- `byroredux-bsa --lib`: 110 passed.
- `declared_size_matches_extract_across_bsa_versions`: Meshes0 19,443 / 0 mismatches.
- `archive_siblings`: 10 passed.

#### SKY-D5-2026-09-29-01 — ROADMAP compat matrix stale (33,424 / 7, no LE row)
- **Severity**: LOW
- **Status**: Existing: #5098 (OPEN). `ROADMAP.md:241` is unchanged.

---

## Shader-Type Coverage Matrix

The parse code is unchanged since 2026-09-22. The only change this window is the early-Z admission (#5057), noted in the Render column.

| Variant | Numeric type(s) | Parse | Import (→ `ImportedMesh.material`) | Render |
|---|---|---|---|---|
| `None` (no trailing data) | 0, 2, 3, 4, 8, 9, 10, 12, 13, 15, 17, 18, 19, 20 | Complete (0 bytes; #4252 pin) | N/A (kind carried as `material_kind`) | Default lit; kinds 2..=20 protected from the glass classifier (`helpers.rs:171`); kinds 0..=16 early-Z eligible (#5057), 17–20 late-test |
| `EnvironmentMap` | 1 | Complete | Complete (`env_map_scale`) | Consumed; alchemy glass → glass (#4392); early-Z eligible when opaque |
| `SkinTint` | 5 | Complete | Complete | Consumed; #4423 tint alpha weight |
| `HairTint` | 6 | Complete | Complete | Consumed |
| `ParallaxOcc` | 7 | Complete | Complete | Consumed |
| `MultiLayerParallax` | 11 | Complete | Complete | Consumed; ice preserved through the glass classifier |
| `SparkleSnow` | 14 | Complete | Complete | Consumed |
| `EyeEnvmap` | 16 | Complete | Complete | Consumed |
| FaceTint (no payload) | 4 | Complete | `material_kind = 4` keys the FaceGen tint override (#4421) | Consumed |
| `Fo76SkinTint` etc. | FO76 table | N/A for Skyrim (separate `parse_shader_type_data_fo76`) | — | — |

Residual: #4256. The discriminator `shader_type` is still not canonical (`mesh_instance.rs:317`).

---

## Cell-Load Regression Status

- **TES5 walk**:
  - `parse_real_skyrim_esm` passes: 590 cells, 18,318 statics (equal to 09-29), 37 worldspaces.
  - Winking Skeever: 981 refs; 590/590 cells with extended XCLL.
- **Multi-master**: no window commit touches `load_order.rs` beyond the strings-discovery fix. The #3813 ordered fold is unchanged. The DLC repro (`Dawnguard.esm` / `Forelhost01`) was not re-driven (no engine launch).
- **Reference state**:
  - Starts Dead and Initially Disabled are unchanged.
  - Starts Unconscious is correctly absent on TES5.
  - XRGD is decoded but not applied (#5015).
  - Every respawned NPC with an `Inventory` now runs `reconcile_worn_gear` on restore. This is where SKY-D3-2026-10-05-01 bites.
- **DIAL**: QNAM → `quest_refs` is populated, and the category decodes (`Topic`). Sibling-owned residuals: ESM D2-01 (INFO DATA) and SCR-D5-01 (Say Once).
- **Whiterun BanneredMare control bench: INCOMPLETE, not FAILED.**
  - There was no engine launch in this suite leg.
  - Reference: ROADMAP Bench-of-record, 5,777 entities, 83.8 FPS / 11.93 ms (`ROADMAP.md:156`, HEAD `a37fcba3c`).
  - #5235 refreshed the `skyrim_se` draw rows after #5057's early-fragment admission.
  - No window commit touches STAT/REFR/LIGH resolution or LAND scale for that cell.
  - `/audit-runtime` owns the live baseline.

---

## Totals

| Severity | New | Regression | Matched-existing (open) |
|---|---|---|---|
| CRITICAL | 0 | 0 | 0 |
| HIGH | 0 | 0 | 0 |
| MEDIUM | 2 (SKY-D3-2026-10-05-01, SKY-D3-2026-10-05-02) | 0 | 3 (#4256, #5095, #5015) |
| LOW | 0 | 0 | 1 (#5098) |

| Dimension | New | Matched-existing |
|---|---|---|
| 1 — BSTriShape / SSE recon | 0 (skimmed) | 0 |
| 2 — Shader-type / material slice | 0 | 1 (#4256) |
| 3 — NPC equip + FaceGen | 2 MEDIUM | 1 (#5095, headless Skyrim player; `prebaked.rs:31` unchanged) |
| 4 — Load order + cell load | 0 | 1 (#5015) |
| 5 — Archives + corpus | 0 | 1 (#5098); #4628 resolved |

Suggested: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-10-05.md`. Label both findings with `medium`, `bug`, `game:skyrim`, `legacy-compat` and `esm-plugin`. Add `inventory` to SKY-D3-2026-10-05-01 and `character` to SKY-D3-2026-10-05-02.
