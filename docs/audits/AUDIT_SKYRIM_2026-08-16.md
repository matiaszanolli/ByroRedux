# Skyrim SE Compatibility Audit — 2026-08-16

**Command**: `/audit-skyrim` (run as part of the `comprehensive` `/audit-suite` sweep)
**Repo HEAD**: `adbc3f77`
**Game data**: `/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/` (present — every dimension had real-data validation available)
**Dedup baseline**: `/tmp/audit/issues.json` (269 OPEN issues) + full scan of `docs/audits/AUDIT_SKYRIM_*.md` (20 prior reports) and the adjacent FNV/FO3/FO4/NIFAL reports from this sweep.

---

## Scope line

All **7 dimensions** were executed by this agent directly (no sub-agents — nested
relay is broken in this repo). Per-dimension scratch notes live in
`/tmp/audit/skyrim/dim_1.md` … `dim_7.md`.

**Un-owned subsystem swept**: `crates/facegen/src/` (989 LOC, `.tri`/`.egm`/`.egt`
parsers on untrusted archive input) was read start-to-end for the panic / OOB /
unchecked-length / unbounded-allocation classes. **Clean — 0 findings** (details
in Dimension 3). Note it is also **unreachable from Skyrim**: its only consumer
is the kf-era `spawn_runtime_head`, while Skyrim takes the pre-baked FaceGeom
NIF path.

**Not exercised**: no engine instance was launched (per the run brief, and no
`byro-dbg` listener was up on 9876 while a parallel audit sweep occupied the
machine). Consequently the **Whiterun BanneredMare control-bench entity/FPS
comparison and `tex.missing` were NOT run this pass**. Everything reported below
is grounded in source + real on-disk archive/ESM corpus measurement.

**Open investigation left alone**: the diagonal double-image / translucency ghost
in Skyrim interiors. No new hard evidence was produced this pass, so no claim is
made about it.

---

## Executive Summary

Skyrim SE remains the renderer control bench, and the parse layer is in excellent
shape: **100.00 % clean** on both mesh archives (22 047 NIFs, 0 truncated, 0
failures, 0 recovered, **zero WARN lines**), and **0 extraction failures across
89 498 files / ~19.1 GiB** of BSA v105. The `BSLightingShaderProperty` wire
reader matches nif.xml field-for-field, and the #838 / #836 / #837 / #1350 /
#2093 / #2094 / #2591 / #2694 regression guards are all intact.

The four findings are all **semantic**, not structural — the bytes are read
correctly, then routed to the wrong destination:

1. **HIGH** — `BSShaderTextureSet` slot 2 is bound as the *glow map* on **4 922
   of 6 253** vanilla non-tint properties whose `SLSF2_Glow_Map` bit is **clear**,
   because `slot_to_role` never reads shader flags. 383 of them also author a
   non-black emissive, so it is live, not latent. A false premise written into
   `shader_flags.rs` ("Skyrim doesn't have an SLSF2 glow bit") is what keeps it
   there.
2. **HIGH** — `expand_leveled_form_id` ignores the TES5 `LVLF` bit `0x04`
   ("Use All"), so the 45 outfit-reachable Use-All armour sets collapse to **one
   piece**. **289 of 5 118** vanilla NPC records — the entire Imperial /
   Stormcloak / hold-guard population — spawn in a single armour piece over bare
   skin.
3. **MEDIUM** — the FNV `BSXFlags` bit-5 carve-out comment claims Skyrim is
   exempt; it is not (BSVER 100 < 130), and the comment will mislead whoever
   fixes the FNV issue.
4. **MEDIUM** — 1 564 vanilla properties author a slot-7 back-lighting map that
   has no canonical role and is dropped.

---

## Dimension roll-call (every dimension, including clean ones)

| # | Dimension | Findings | Verdict |
|---|-----------|---------:|---------|
| 1 | BSTriShape packed geometry + SSE skinned reconstruction | **1** (D1-01, MEDIUM) | Parser clean; the finding is a doc defect on the cell-loader gate |
| 2 | `BSLightingShaderProperty` / `BSEffectShaderProperty` dispatch | **2** (D2-01 HIGH, D2-02 MEDIUM) | Wire reader clean; texture-slot roles are the problem |
| 3 | NPC equip + FaceGen (M41) | **1** (D3-01, HIGH) | `crates/facegen` swept clean; equip chain has the LVLI gap |
| 4 | Multi-master load order + TES5 cell-load regression | **0** | All four regression guards live; `parse_real_skyrim_esm` GREEN |
| 5 | BSA v105 (LZ4) | **0** | 89 498/89 498 files extract; sibling auto-load correct |
| 6 | Specialty blocks + real-data parse | **0** | 100 % clean, zero WARNs; #838 three-arm split intact |
| 7 | NIFAL canonical material translation (Skyrim slice) | **0** | Single boundary, correct ordering, `EmissiveSource::Lighting`, Disney lobe unreachable |

**Totals — 4 findings: 0 CRITICAL · 2 HIGH · 2 MEDIUM · 0 LOW.** All NEW.

---

## Findings

### SKY-2026-08-16-D2-01: `slot_to_role` binds `BSShaderTextureSet` slot 2 as the glow map without reading `SLSF2_Glow_Map` — 4 922 vanilla properties mis-roled, 383 of them live

- **Severity**: HIGH
- **Dimension**: 2 — shader-type dispatch / texture-slot roles
- **Location**: `crates/nif/src/import/material/slot_role.rs:105-110`; false premise at `crates/nif/src/shader_flags.rs:201` and `:219-221`
- **Status**: NEW
- **Description**: The unified slot table branches only on `(shader_type, slot, model_space_normals)`. For slot 2 it returns `TextureRole::Tint` for the tint family (4/5/6) and `TextureRole::Emissive` for **everything else**. But nif.xml's own `BSShaderTextureSet` documentation names slot 2 three different things selected by *flags*, not by shader type:
  `0: Diffuse … 2: Glow(SLSF2_Glow_Map)/Skin/Hair/Rim light(SLSF2_Rim_Lighting)` (nif.xml:6307-6318).
  The reason the flag is never consulted is written down as fact in the codebase: `crates/nif/src/shader_flags.rs:219-221` states
  `/// Bit 6 — Glow_Map. FO4-specific — Skyrim's glow signal is the texture-set slot-2 presence, not a flag bit.`
  and `:201` repeats "(Skyrim doesn't have an SLSF2 glow bit)". Both are wrong: nif.xml `SkyrimShaderPropertyFlags2` `<option bit="6" name="Glow_Map">Use Glow Map in the third texture slot.</option>` — same bit index, same name, same meaning as FO4. There is no `skyrim_slsf2::GLOW_MAP` constant at all, so no code could consult it even if it wanted to.
- **Evidence**: Corpus survey `crates/nif/examples/_tmp_sk_slotflags.rs` over `Skyrim - Meshes0.bsa` (67 105 pre-FO4 `BSLightingShaderProperty` blocks):
  ```
  non-tint properties with slot 2 authored:                    6 253
    SLSF2_Glow_Map SET   (slot 2 genuinely a glow map):        1 331
    SLSF2_Glow_Map CLEAR (bound as glow map anyway):           4 922
      ... SLSF2_Soft_Lighting set (subsurface mask):           3 561
      ... SLSF2_Rim_Lighting  set (rim mask):                    155
    mis-roled AND emissive_color non-black  → LIVE:              383
  ```
  Live samples (`ec` = `emissive_color`, `em` = `emissive_multiple`):
  ```
  actors\character\facegendata\facegeom\skyrim.esm\0002e519.nif   ty=16 ec=[0.874,0.451,0.0] em=1.72  slot2=EyeBrown_sk.dds
  actors\character\facegendata\facegeom\dawnguard.esm\00017f8f.nif ty=16 ec=[0.886,0.647,0.0] em=1.42 slot2=EyeBrown_sk.dds
  dlc01\effects\icechunk04.nif                                     ty=1  ec=[1,1,1] em=0.75          slot2=IceSpellBacklight.dds
  actors\dlc01\falmervampire\falmervampirewings.nif                ty=11 ec=[0.0588]*3               slot2=White.dds
  dlc01\clutter\chauruscocoon\chauruscocoonexplosion.nif           ty=1  ec=[0.165,0.173,0.137] em=0.5
  ```
  The consumption path has no second gate: `dedicated_shader.rs:164` `TextureRole::Emissive => &mut info.glow_map` → `MaterialInfo::glow_map` (`import/material/mod.rs:1159` `emissive: self.glow_map`) → `byroredux/src/material_translate.rs:136` `glow_map: textures.emissive` → `byroredux/src/render/static_meshes.rs:264,626` `glow_map_index` → `crates/renderer/shaders/triangle.frag:1213-1219`:
  ```glsl
  if (mat.glowMapIndex != 0u) { emissiveMask = glowSample; }
  ...
  emissive = min(emissiveColor * emissiveMult * emissiveMask, vec3(64.0));   // :1376
  ```
  `triangle.frag:1240` even documents that shader type 2 (Glow) is "dispatched by data presence" — i.e. purely on `glowMapIndex != 0u`, with no material-kind or flag check.
- **Impact**: Every glowing-eye NPC in Skyrim (vampires, Dawnguard/Dragonborn variants — shader type 16 `EyeEnvmap` with an authored orange/yellow emissive) has its emissive multiplied by a **skin-tint mask** instead of being emitted unmasked. Ice/spell effect meshes with `IceSpellBacklight.dds` and the Falmer vampire wings are affected the same way. The remaining 4 539 mis-roled properties are latent purely because their `emissive_color` is `[0,0,0]` — exactly the "one authored value away" condition #2694 called out when it fixed the *tint* half of this same slot. Blast radius is the whole `slot_to_role` boundary, which is shared with the REFR `XTXR` texture-overlay path (`byroredux/src/cell_loader/spawn/mesh_instance.rs`), so an overlay onto slot 2 inherits the same wrong role.
- **Related**: #2694 (fixed slot 2 for the tint family on the same table), #2695 (unified the two tables), #2580 (a *different* wrong parenthetical in the same file — Alpha_Test, not Glow_Map), Dimension 3 (the 3 561 soft-lighting hits are the Skyrim FaceGeom head corpus), the sweep-wide `slot_to_role`-has-no-`bsver`-gating cross-finding.
- **Suggested Fix**: Add `skyrim_slsf2::GLOW_MAP = 0x0000_0040` (and `SOFT_LIGHTING = 1<<25`, `RIM_LIGHTING = 1<<26`) to `shader_flags.rs`, correct the two false doc claims, and widen `slot_to_role`'s input to carry the already-available `shader_flags_2` so slot 2 resolves `Glow_Map → Emissive`, `Rim_Lighting`/`Soft_Lighting` → a new mask role (or `None`, which is at least not a fabrication) rather than defaulting to `Emissive`. The overlay path already recovers `shader_type` from `ImportedMaterial`; carry the flag word alongside it the same way.

---

### SKY-2026-08-16-D3-01: `expand_leveled_form_id` never reads TES5 `LVLF` bit `0x04` — 289 vanilla NPCs spawn wearing one piece of a four-piece armour set

- **Severity**: HIGH
- **Dimension**: 3 — NPC equip + FaceGen (M41)
- **Location**: `crates/plugin/src/equip.rs:363-373`; flag capture at `crates/plugin/src/esm/records/container.rs:87,189`
- **Status**: NEW
- **Description**: `expand_leveled_form_id` decides single-pick vs multi-pick with one test:
  ```rust
  let multi_pick = lvli.flags & 0x02 != 0;
  ```
  `lvli.flags` is the raw `LVLF` byte, and `container.rs:87` documents it as `"(bit 0: calculate from all levels, bit 1: calculate for each item)"` — the **FO3/FNV/Oblivion** two-bit layout. TES5 adds two more bits, and bit `2` (`0x04`) is the one that means *add every entry* (xEdit `wbDefinitionsTES5.pas` names it `Use All`). It is never read. A Use-All list therefore falls into the single-pick arm:
  ```rust
  let pick = eligible.iter().max_by_key(|e| e.level) …
  ```
  Every entry in a vanilla Use-All outfit list carries `level = 1`, and Rust's `Iterator::max_by_key` returns the **last** element on ties — so exactly one item, the final table row, is equipped.
- **Evidence**: `crates/plugin/examples/_tmp_sk_lvli.rs` against real `Skyrim.esm`:
  ```
  LVLI reachable from an OTFT:                        121
    ... LVLF 0x04 set, 0x02 clear:                     45   (37 %)
  OTFTs containing >= 1 Use-All LVLI:               40 / 481
  NPC_ whose default outfit is one of those:       289 / 5118
  ```
  The lists are unambiguous armour *sets*, not variant rolls:
  ```
  OTFT 000E77DF -> LVLI 000ABF49 flags=0x04
      lvl=1 000A6D7F ArmorStormcloakBoots
      lvl=1 000A6D7B ArmorStormcloakCuirass
      lvl=1 000A6D7D ArmorStormcloakGauntlets
      lvl=1 000A6D79 ArmorStormcloakHelmetFull      <- only this one is equipped
  OTFT 0009F7EF -> LVLI 0009F7EC flags=0x04
      ArmorImperialLightBoots / LightCuirass / LightGauntlets / HelmetOfficer
  OTFT 000D33C6 -> LVLI 000D33C7 flags=0x04
      ArmorGuardCuirassWhiterun / ArmorStormcloakBoots / ArmorGuardShieldWhiteRun / …
  OTFT 00057A29 -> LVLI 000ABF3C flags=0x04
      ArmorImperialLightBoots / ArmorImperialStuddedCuirass / ArmorImperialLightGauntlets
  ```
  Note the flag-name attribution ("Use All") is from xEdit's TES5 definitions, which are not vendored in this repo; the *behavioural* claim above does not depend on it — a level-1 homogeneous {boots, cuirass, gauntlets, helmet} list is a set by inspection, and the code demonstrably takes one row from it.
- **Impact**: The Imperial Legion, Stormcloak and hold-guard populations — 5.6 % of `Skyrim.esm`'s NPC records and by far the most frequently encountered NPC classes — spawn in a single armour piece. They are not naked (the #2093 `RACE.WNAM` skin layer covers the uncovered biped bits), so the symptom presents as "guards wearing gauntlets over bare skin", which reads as an art/mesh bug rather than an equip bug and has therefore been easy to misattribute. The inverse error is also present: the 58 outfit-reachable lists that *do* set `0x02` get multi-picked, when TES5 `0x02` means "repeat the roll `count` times" — over-equipping. FO3/FNV/Oblivion `LVLF` has no Use-All bit, so this is Skyrim(+FO4)-scoped.
- **Related**: #2093 / SKY-D3-NEW-01 (race default skin), #2094 / SKY-D3-NEW-02 (occupancy filter — it is this finding's downstream, and behaves correctly), #896 (LVLI dispatch in outfits), #2955 (`effective_actor_level`)
- **Suggested Fix**: Add the TES5 `LVLF` bits to `container.rs` (`0x04` Use All, `0x08` Special Loot) and make `expand_leveled_form_id` treat `flags & 0x04` as the multi-pick trigger; keep `0x02` on the documented "over-equip approximation" path or drop it to single-pick now that the real multi-pick bit is handled. Guard with a fixture LVLI carrying `flags = 0x04` and four equal-level entries asserting all four expand.

---

### SKY-2026-08-16-D1-01: the `BSXFlags` bit-5 carve-out comment places Skyrim on the wrong side of its own predicate

- **Severity**: MEDIUM
- **Dimension**: 1 — geometry / cell-loader import gate
- **Location**: `byroredux/src/cell_loader/references/import.rs:68-79` (comment) vs `:89` (code); sibling at `byroredux/src/cell_loader/partial.rs:55-66`
- **Status**: NEW (the *behaviour* is the already-filed FNV-2026-08-16-D1-01 — see the blast-radius section below; this finding is the doc defect only)
- **Description**: The comment above the gate reads, verbatim:
  ```
  //   * Oblivion / FO3 / FNV (BSVER < FALLOUT4): bit 5 = `EditorMarker`. …
  //   * Skyrim / FO4 / FO76 / Starfield (BSVER >= FALLOUT4):
  //     bit 5 = `MultiBoundNode` (Bethesda re-purposed it). …
  ```
  The predicate it documents is `bsx & 0x20 != 0 && bsver < bsver::FALLOUT4`. `bsver::SKYRIM_SE = 100` and `bsver::SKYRIM_LE = 83`, `bsver::FALLOUT4 = 130` (`crates/nif/src/version.rs:391,393,401`) — Skyrim is `< FALLOUT4` and therefore sits in the **drop** branch, not the exempt one the comment names it in. nif.xml agrees with the code and against the comment: `Bit 5 : EditorMarkers present, bEditorMarker(Skyrim)` (nif.xml:4305) — the flag keeps its EditorMarker meaning on Skyrim explicitly.
- **Evidence**: `crates/nif/src/version.rs:393` `pub const SKYRIM_SE: u32 = 100;` and `:401` `pub const FALLOUT4: u32 = 130;`. Corpus confirmation via `crates/nif/examples/_tmp_sk_bsx.rs` (which filters `scene.bsver >= 130` and still finds 977 bit-5 Skyrim NIFs, i.e. every one of them is inside the drop window).
- **Impact**: No runtime effect on its own, but it is directly load-bearing for the fix of a live HIGH defect. Anyone repairing FNV-2026-08-16-D1-01 who reads this comment will conclude Skyrim was already carved out and leave 687 Skyrim meshes deleted. This is precisely the error class `feedback_audit_findings` and #414/#1879 exist to prevent.
- **Related**: FNV-2026-08-16-D1-01 (the behavioural defect, already filed), *AUDIT_POSITIONING_DECALS_2026-04-13.md* PD-01 (origin of the whole-NIF drop premise)
- **Suggested Fix**: Correct the comment to name the actual split (`Oblivion / FO3 / FNV / **Skyrim**` vs `FO4+`), or — preferably, since it is the same fix — cull the marker *child* rather than the whole NIF and delete the era split entirely.

---

### SKY-2026-08-16-D2-02: 1 564 vanilla properties author a slot-7 back-lighting map with no canonical role, and nothing tracks the loss

- **Severity**: MEDIUM
- **Dimension**: 2 — texture-slot roles
- **Location**: `crates/nif/src/import/material/slot_role.rs:148-155`; role enum at `:36-54`
- **Status**: NEW (quantification of a deliberate, documented gap)
- **Description**: Slot 7's arm returns `Some(TextureRole::Specular)` only when `model_space_normals` is set, and `None` otherwise — deliberately, per the function's own doc at `:79-81` ("No canonical role exists yet. Slot 7 on type 11 is a back-lighting map; `MaterialTextureSet` has no back-lighting role and no shader consumes one, so inventing a mapping would be fabrication"). That reasoning is correct and should not be overridden by fabricating a role. What is missing is the scale: the drop is not confined to type 11, and it is not rare.
- **Evidence**: `crates/nif/examples/_tmp_sk_slotflags.rs` over `Skyrim - Meshes0.bsa`: **1 564** non-type-11 properties author slot 7 with `model_space_normals` clear and `SLSF2_Back_Lighting` (bit 27) set. The dominant cluster is the ice-cave architecture set — every `meshes\dungeons\caves\ice\*` piece carries `textures\dungeons\caves\IceCaveSubsurfacetint01.dds` in slot 7. nif.xml:6315 names the slot `7: Back Lighting Map (SLSF2_Back_Lighting)`. Contrast with the deliberate drops that *are* empirically empty: tint-family slot 4/5 (0 occurrences, #1350's premise confirmed) and slot 6 on non-type-11 (1 occurrence, a `tempassets\testroof2delete.nif` dev leftover).
- **Impact**: Skyrim's ice caves (and every other back-lit surface) render without their authored subsurface/back-lighting term. Not a correctness regression — the data has never had a destination — but it is quantified visible content that the material pipeline silently discards with no tracking issue.
- **Related**: SKY-2026-08-16-D2-01 (same table, same "flags are the real discriminator" root), `/audit-nifal` Dimension 7 (canonical role set)
- **Suggested Fix**: Feature work, not a patch: add a `BackLighting` role to `MaterialTextureSet` + `GpuMaterial` and a wrap-lighting term in `triangle.frag`, gated on `SLSF2_Back_Lighting`. Until then, at minimum count the drops so the loss is visible in `nif_stats` rather than silent.

---

## Blast-radius assessments (confirmed cross-findings — NOT re-filed)

### FNV-2026-08-16-D1-01 (`BSXFlags` bit 5 drops the whole NIF) — **Skyrim IS affected**

Stating it explicitly as instructed: **yes**. The gate is `bsver < FALLOUT4`, and
Skyrim SE (100) / LE (83) are both inside it.

Measured with `crates/nif/examples/_tmp_sk_bsx.rs` over `Skyrim - Meshes0.bsa` +
`Skyrim - Meshes1.bsa` (22 047 NIFs parsed):

```
pre-FO4 NIFs with BSXFlags bit 5 set:            977
  ... pure markers (0 meshes, 0 colliders):      290   (correctly dropped)
  ... carrying real geometry / collision:        687   (SILENTLY DELETED)
```

The largest deletions, by triangle count:

| NIF | meshes | tris | colliders | BSX |
|---|---:|---:|---:|---|
| `effects\fxtg09nocturnalbirds.nif` | 611 | 57 129 | 0 | 0x221 |
| `effects\fxtg09nocturnalbirdsreverse.nif` | 176 | 20 488 | 0 | 0x221 |
| `dungeons\imperial\customfx\impforthallcollapsefx01.nif` | 37 | 17 416 | 1 | 0x2AB |
| `effects\fxda16barrier.nif` | 5 | 15 040 | 0 | 0xA3 |
| `effects\fxlabyrinthianbarrier.nif` | 4 | 12 032 | 0 | 0xA3 |
| `dlc01\dungeons\castle\animated\cascoffinpuzzle\cascoffinpuzzle01.nif` | 53 | 11 394 | 2 | 0x2B |
| `dlc02\furniture\craftingstaffworkbench01.nif` | 8 | 8 100 | 1 | 0xAB |
| `dlc02\dungeons\apocrypha\animated\apoextendinghallway\apoextendinghallway01.nif` | 16 | 5 671 | 3 | 0x2B |
| `dlc02\dungeons\apocrypha\animated\apobendinghallway\apobendinghallway01.nif` | 13 | 5 264 | 9 | 0x2B |
| `dungeons\nordic\exterior\dragonmound\dragonmoundbase.nif` | 5 | 5 085 | 1 | 0xAB |
| `traps\largetrap01dwe{90,180}.nif`, `largetrap02dwe180.nif` | 3 | 4 288 | 2 | 0x2B |

So the fix for the FNV issue must cover Skyrim in the same change — including the
Dragonborn Apocrypha animated hallways, Dawnguard's Castle puzzle set, the Dwemer
trap machinery, and every dragon mound.

### `slot_to_role` has no `bsver` gating — which rules the table actually encodes, and on what evidence

Confirmed: `slot_to_role(shader_type: u32, slot: u32, model_space_normals: bool)`
(`crates/nif/src/import/material/slot_role.rs:90`). Skyrim is the game the table
was measured on, and every arm carries its Skyrim occupancy evidence in a
comment. Re-verified independently this audit against `Skyrim - Meshes0.bsa`
(67 105 properties). The precise ruleset every other game is currently being
measured against is:

| slot | rule encoded | stated evidence | independently re-measured this audit |
|---|---|---|---|
| 0 | `BaseColor`, unconditional | — | — |
| 1 | `Normal`, unconditional | — | — |
| 2 | `Tint` for types 4/5/6, else `Emissive` | #2694: `*_sk.dds` on 3158/3158 FaceTint, 16/16 populated HairTint | **rule is incomplete — see D2-01**: 4 922/6 253 non-tint slot-2 properties have `SLSF2_Glow_Map` clear |
| 3 | `Detail` for type 4, else `Height` | #2694: `MaleHeadDetail_Rough01.dds` on 3149/3158 FaceTint | **holds** — `SLSF1_Facegen_Detail_Map` set with `shader_type != 4`: **0** occurrences |
| 4 / 5 | `None` for types 4/5/6, else `Environment` / `EnvironmentMask` | #1350: types 5/6 declare no slot 4/5 | **holds** — tint-family with slot 4 or 5 authored: **0** occurrences |
| 6 | `InnerLayer` for type 11, else `None` | #2693: slot 6 non-empty on 607/607 type-11 (nif.xml's prose contradicts its own field table; the field table won) | **holds** — slot 6 on non-11/non-4: **1** (a dev leftover) |
| 7 | `None` for type 11; else `Specular` iff MSN; else `None` | #2742: 390/390 slot-7-bearing SkinTint properties are MSN | **holds for the MSN half**; the no-MSN half discards 1 564 back-lighting maps — see D2-02 |

Five of the seven rules survive independent re-measurement on Skyrim; slot 2 does
not, and slot 7 has a quantified hole. Any per-game work keyed off this table
should treat slots 0/1/3/4/5/6 as trustworthy Skyrim baselines and slot 2/7 as
in flux.

### ESM-2026-08-16-D7-02 (`health_actor_value_key` special-cases Skyrim) — Skyrim impact: **none today, but do not "fix" it blindly**

`EsmIndex::health_actor_value_key` (`crates/plugin/src/esm/records/index.rs:598-604`)
returns the constant `SKYRIM_HEALTH_ACTOR_VALUE = 24` for Skyrim. Both sides of
the pipeline use that same call — the producer
(`crates/plugin/src/esm/records/actor_value_derive.rs:139` in
`derive_skyrim_actor_values`) and the consumer
(`byroredux/src/npc_spawn.rs:103`, which stamps `ActorVitals { health }`) — so
Skyrim NPC health is **internally consistent and working today**. `24` is also
the correct TES5 built-in actor-value enum index for Health.

Caution for whoever closes ESM-D7-02: the codebase's other AV consumer is the
CTDA evaluator, which keys `ActorValues` directly off the condition's `param_1`
(`crates/scripting/src/condition.rs:419-431`, "whatever space `ActorValues` is
keyed in — a direct lookup, no FormIdPool"). Whether TES5 `GetActorValue`
`param_1` carries an **enum index** or an **AVIF FormID** is not documented
anywhere in this repo. If it is the enum index, `24` is the *correct* key and
swapping Skyrim onto `AVHealth`'s FormID (`0x000003E8`) would break condition
lookups rather than fix them. Establish that before changing the Skyrim arm.

---

## Shader-Type Coverage Matrix (Skyrim `BSLightingShaderType`)

Parse = wire fields read; Import = trailing payload reaches `MaterialInfo`;
Render = a `triangle.frag` branch consumes it.

| # | Name | `ShaderTypeData` variant | Parse | Import | Render |
|---:|---|---|:--:|:--:|:--:|
| 0 | Default | `None` | ✓ | ✓ | ✓ (base PBR path) |
| 1 | Environment Map | `EnvironmentMap { env_map_scale }` | ✓ | ✓ | ✓ (`inst.envMapIndex`) |
| 2 | Glow Shader | `None` | ✓ | ✓ | ✓ (data-presence: `mat.glowMapIndex`) — **see D2-01** |
| 3 | Parallax | `None` | ✓ | ✓ | ✓ (`mat.parallaxMapIndex` POM) |
| 4 | Face Tint | `None` | ✓ | ✓ (normalised to SkinTint in `dedicated_shader.rs`) | ✓ (`materialKind == 5`) |
| 5 | Skin Tint | `SkinTint { skin_tint_color, skin_tint_alpha: None }` | ✓ | ✓ | ✓ (`materialKind == 5`) |
| 6 | Hair Tint | `HairTint { hair_tint_color }` | ✓ | ✓ | ✓ (`materialKind == 6`) |
| 7 | Parallax Occ | `ParallaxOcc { max_passes, scale }` | ✓ | ✓ | ✓ (shares the POM path) |
| 8 | Multi-Index Snow | `None` | ✓ | ✓ | — no dedicated branch |
| 9 | World Multitexture | `None` | ✓ | ✓ | — |
| 10 | World Map 1 | `None` | ✓ | ✓ | — |
| 11 | Multi-Layer Parallax | `MultiLayerParallax { thickness, refraction_scale, inner_layer_texture_scale, envmap_strength }` | ✓ | ✓ | ✓ (`materialKind == 11`) |
| 12 | Tree Anim | `None` | ✓ | ✓ | — |
| 13 | World Map 2 | `None` | ✓ | ✓ | — |
| 14 | Sparkle Snow | `SparkleSnow { sparkle_parameters }` | ✓ | ✓ | ✓ (`materialKind == 14`) |
| 15 | World Map 3 | `None` | ✓ | ✓ | — |
| 16 | Eye Envmap | `EyeEnvmap { eye_cubemap_scale, left/right_eye_reflection_center }` | ✓ | ✓ | ✓ (`materialKind == 16`) — **emissive mask wrong, D2-01** |
| 17 | Cloud | `None` | ✓ | ✓ | — |
| 18 | World Map 4 | `None` | ✓ | ✓ | — |
| 19 | World LOD Multitexture | `None` | ✓ | ✓ | — |

The `_ => Ok(ShaderTypeData::None)` fall-through was checked against nif.xml's
full `cond="Shader Type == N"` set for `#NI_BS_LTE_FO4#` (nif.xml:6619-6634):
the arms `{1, 5, 6, 7, 11, 14, 16}` are exactly complete, so **no type over-reads
and no type under-reads**. FO76's `BSShaderType155` numbering stays confined to
`parse_shader_type_data_fo76`; the two enums do not cross-contaminate at the
parse layer (the *downstream* `material_kind` leak is #2579, already OPEN, and
its blast radius is FO76-only).

`BSEffectShaderProperty` (`crates/nif/src/blocks/shader.rs`) parses
`soft_falloff_depth`, `greyscale_texture`, `lighting_influence`,
`env_map_min_lod` and the falloff start/stop angle+opacity set. `env_map_min_lod`
remains an unconsumed dead-end — **Existing: #2582** (SKY-D2-04, still OPEN); all
8 116 vanilla blocks author `0`, so nothing is lost today. Not re-filed.

---

## Cell-Load Regression Status

| Guard | Result |
|---|---|
| `parse_real_skyrim_esm` (real `Skyrim.esm`) | **GREEN** — `590 cells, 18113 statics, 37 worldspaces`; `SolitudeWinkingSkeever` found with 981 refs |
| Skyrim 92-byte `XCLL` → `directional_fade` | **GREEN** — `590/590 cells with XCLL, 590 with Skyrim extended fields`; Skeever `ambient=[0.318,0.294,0.224] fog_near=250.0 fog_far=5000.0` |
| TES5 compressed-record decompression | **GREEN** (implied — cells with compressed bodies resolve in the above walk) |
| `.STRINGS` per-plugin wiring (#1553) | **LIVE** — `install_strings_guard` inside the per-plugin loop, RAII-scoped (`load_order.rs:198`) |
| ESL / light-master FormID decode (#1554) | **LIVE** — `flags & 0x0200` → separate `GlobalSlot::Light` counter (`reader.rs:724`, `load_order.rs:183-190`) |
| Deleted-REFR tombstones (#1660) | **LIVE** — `RECORD_FLAG_DELETED = 0x0000_0020` tested at `cell/walkers.rs:641` |
| Interior EDID spot-probe (16 cells) | **GREEN** — BanneredMare 696 REFRs, BleakFallsBarrow01 2512, HelgenKeep01 2731, BluePalace 2120, Dragonsreach 1440, Jorrvaskr 725, Ustengrav01 1042, … |
| `Skyrim - Meshes0.bsa` parse sweep | **100.00 %** — 18 862 NIFs, 0 truncated, 0 failures, 0 recovered, **0 WARN lines** |
| `Skyrim - Meshes1.bsa` parse sweep | **100.00 %** — 3 185 NIFs, same |
| BSA v105 full extraction (16 archives) | **100 %** — 89 498/89 498 files, ~19.1 GiB |
| Whiterun control-bench entity/FPS | **NOT RUN** (no engine launched — see scope line) |

Open question referred to `/audit-esm`, not filed here: `EsmIndex.cells` holds
**590** named interiors for `Skyrim.esm`. That number could not be ground-truthed
against an independent source this pass, and the walker's EDID-keyed insert
(`crates/plugin/src/esm/cell/walkers.rs:554`, `if is_interior && !editor_id.is_empty()`)
is the shared mechanism `/audit-esm` owns under the 2026-08-13 scope split.

---

## Disproved / rejected candidates

Recorded so they are not re-investigated:

1. **`VF_UVS_2` / `VF_LAND_DATA` / `VF_INSTANCE` mid-vertex misalignment.** Initially suspected: the packed-vertex parser skips these bits with a *trailing* `consumed < vertex_size_bytes` skip, which would corrupt every field after them if they sit mid-vertex. **Disproved** — nif.xml's own `BSVertexData` / `BSVertexDataSSE` structs (nif.xml:2107-2141) declare **no field at all** for those three bits, so the parser's sequential order is identical to the spec's. Documented deferral under #336; not a defect.
2. **`slot_to_role` slot 3 mis-routes FaceGen detail maps into the POM path on non-FaceTint materials.** The authoritative discriminator is `SLSF1_Facegen_Detail_Map` (bit 10, "Use a face detail map in the 4th texture slot"), not the shader type. **Disproved empirically** — 0 occurrences of that flag with `shader_type != 4` across 67 105 vanilla properties. The #2694 shader-type proxy is exact on this corpus.
3. **`SLSF2_Multi_Layer_Parallax` set without shader type 11 would strand slot 6.** **Disproved** — 0 occurrences.
4. **Tint-family meshes with a stray slot-4/5 string binding a spurious env cube (#1350's stated risk).** **Disproved on vanilla** — 0 occurrences; the guard is defensive only, as documented.
5. **`crates/facegen` carries the `crates/hkx` unvalidated-`u32`-into-`Vec::with_capacity` abort class.** **Disproved** — both `EgtFile::parse` and `EgmFile::parse` cap every file-driven count (`MAX_TEXTURE_DIM 4096`, `MAX_MORPHS 1024`, `MAX_VERTICES 1<<20`), `checked_mul` the area, and gate every allocation behind an **exact** `bytes.len() != needed` equality. `apply_morphs` is `min()`-clamped on both axes with non-finite skips. No `unsafe`, and no `unwrap`/`expect`/`panic!` outside `#[cfg(test)]`.
6. **`BSLightingShaderProperty` Skyrim field order drift.** **Disproved** — `parse_skyrim` (`shader.rs:895-971`) matches nif.xml:6584-6634 field for field, including the `onlyT="BSLightingShaderProperty"` shader-type `u32` that precedes `NiObjectNET.Name` (nif.xml:3361) and the absence of `root_material` / `smoothness` / wetness / `grayscale_to_palette_scale` / `fresnel_power` on the `#NI_BS_LT_FO4#` band.
7. **Skyrim vanilla content reaching the Disney/Burley lobe.** **Disproved by reachability** — `PBR_BSDF` is packed only from `material.is_pbr` (`byroredux/src/cell_loader.rs:239-240`); `is_pbr` initialises `false` (`crates/nif/src/import/material/mod.rs:1336`) and its only writers are in `byroredux/src/asset_provider/material.rs` (`:974`, `:1085`), all BGSM/BGEM/`.mat`-driven. Skyrim ships none.
8. **Skyrim `SLSF1`/`SLSF2` constant drift.** **Disproved** — every constant in `crates/nif/src/shader_flags.rs:85-145` matches its nif.xml bit index. (The *documentation* about the FO4 sibling's bit 6 is wrong — that is D2-01 — but no Skyrim constant is misvalued.)
9. **Sibling archive auto-load re-narrowed, starving distant LOD.** **Disproved** — `numeric_sibling_paths` (`byroredux/src/asset_provider/archive.rs:368-401`) expands `…0.bsa → …1..9.bsa` with the `…10` guard intact, and the `.btr`/`.bto` corpus lives in `Skyrim - Meshes1.bsa` (9 584 + 1 078 files), reachable from a bare `--bsa "Skyrim - Meshes0.bsa"`.
10. **`BSLODTriShape` folded back into `BsTriShape` (#838 regression).** **Disproved** — three distinct dispatch arms at `crates/nif/src/blocks/mod.rs:473`, `:478`, `:489`.
11. **`next_regular: u8` / `next_light: u16` load-order slot counters overflow.** Real but unreachable — the CLI cannot pass 255 regular or 4 096 light plugins. Not filed.
12. **The Skyrim interior-ghosting investigation.** No new hard evidence found; deliberately left untouched per the run brief.

---

## Evidence artifacts

Scratch corpus tools written for this audit (kept, matching the existing
`_tmp_*` example convention in this repo):

- `crates/nif/examples/_tmp_sk_bsx.rs` — BSXFlags bit-5 blast radius over the mesh archives
- `crates/nif/examples/_tmp_sk_slotflags.rs` — texture-slot role vs `SLSF1`/`SLSF2` flag disagreement survey
- `crates/nif/examples/_tmp_sk_bsasweep.rs` — full-archive BSA v105 extraction sweep
- `crates/plugin/examples/_tmp_sk_lvli.rs` — `LVLF` flag distribution over the OTFT-reachable LVLI closure

Per-dimension notes: `/tmp/audit/skyrim/dim_1.md` … `dim_7.md`.
Raw sweep log: `/tmp/audit/skyrim/meshes0_warn.log`.

---

Next step: `/audit-publish docs/audits/AUDIT_SKYRIM_2026-08-16.md`
