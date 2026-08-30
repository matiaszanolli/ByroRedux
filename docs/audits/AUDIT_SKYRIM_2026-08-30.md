# Skyrim SE Compatibility Audit — 2026-08-30

**Scope**: BSTriShape packed geometry, `BSLightingShaderProperty` shader-type
dispatch, NPC equip + FaceGen (`crates/facegen`), multi-master load order,
BSA v105, specialty blocks, NIFAL Skyrim slice.

**Status**: COMPLETE — all 7 dimensions.

## Executive Summary

Skyrim SE is the engine's renderer **control bench**: cell load and rendering
both work, so this pass is regression coverage plus the Skyrim-specific
geometry / shader / equip risk surface. Everything measured against the
installed `Skyrim Special Edition/Data/` corpus — 32 709 NIF entries across
`Skyrim - Meshes0/1.bsa` and 1 188 821 ESM records across 10 plugins — rather
than inferred.

**The parsers are in excellent shape.** 32 709 / 32 709 files parse clean,
0 `NiUnknown` blocks, 0 `vertex_desc` offset drift over 99 203 `BsTriShape`
blocks, 0 `data_size` mismatches, and every shipped `BSLightingShaderType`
dispatches to the field count nif.xml declares. Six named-NPC equip guards,
the creature-race guard and the load-order guard all pass on real data.

**The one real hole is downstream of the parser**: 20.4 % of imported Skyrim
meshes reach the renderer with a *fabricated* constant up-normal, because the
importer has tangent synthesis but no normal synthesis. That silently
includes every NPC face in the game.

### Findings

| ID | Sev | Dim | Summary |
|---|---|---|---|
| SK-D1-01 | HIGH | 1 | 19 657 / 96 123 imported meshes (20.4 %) get a fabricated `[0,1,0]` normal; no `synthesize_normals` exists |
| SK-D3-01 | HIGH | 3 | Consequence of the above on the FaceGen path: all 3 201 vanilla head/mouth meshes render flat-shaded |
| SK-D2-01 | HIGH* | 2 | Disney/Burley lobe proven unreachable on vanilla Skyrim — *regression guard, measured clean* |
| SK-D4-01 | MEDIUM | 4 | Deleted (0x20) tombstone honoured for placements only; 9 DLC-deleted base records merge live |
| SK-D3-02 | MEDIUM | 3 | `crates/facegen`'s `.egt` and `.tri` parsers have zero consumers workspace-wide |
| SK-D2-02 | LOW | 2 | `fo4_slsf2` module doc mis-attributes Skyrim SLSF2 bit 21 as `Cloud_LOD` |
| SK-D4-02 | LOW | 4 | `FileHeader::light_master` doc understates ESL reach on a stock AE install |
| SK-D1-02 | LOW | 1 | `VF_UVS_2` / `VF_LAND_DATA` cursor assumption is unverifiable — exposure measured at exactly zero |
| SK-D5-01 | LOW | 5 | Audit skill mislocates Skyrim's `.btr`/`.bto` LOD meshes and mis-states the sibling rule |
| SK-D6-02 | LOW | 6 | Skill's object-LOD description is stale; a live rustdoc link points at a deleted constant |
| SK-D6-01 | — | 6 | LOD band ladder **verified clean** against the game's own `Ultra.ini` |

**Totals: 0 CRITICAL, 3 HIGH (one of which is a green regression guard),
2 MEDIUM, 4 LOW.** Dimension 7 produced **no findings**.

`*` SK-D2-01 is recorded at HIGH because it is the guard protecting a
whole-material-class rendering regression; it is currently **green**.

### Stale candidates dropped

**Five** candidate findings were dropped after measuring them against current
code/data (detail in the per-dimension sections):

1. "21 140 `BSDynamicTriShape` produce 0 triangles → silent render failure" —
   true at block level, fully recovered at import.
2. "17 926 shapes flag `VF_TANGENTS` but carry no inline tangents" — recovered
   from the skin-partition buffer.
3. "4 368 `.btr` `BSSubIndexTriShape` have an empty attribute mask" — the mask
   is `0x001`; they import correctly.
4. `apply_morphs`' Z-up/Y-up coordinate-frame doc **is** wrong, but the code
   path is unreachable for Skyrim (pre-baked FaceGen track) — handed to
   `/audit-fnv` / `/audit-oblivion` rather than claimed here.
5. "132 level-32 `.btr` quads ship but the Skyrim ladder can never select
   them" — checked against the game's shipped `Ultra.ini`, which authors no
   `fBlockLevel2Distance`; vanilla Skyrim does not stream them either. The
   engine is correct (SK-D6-01).

### Skill-vs-code discrepancies

* The skill's D1 checklist cites `legacy_properties.rs` and
  `dedicated_shader.rs` as the two `alpha_property_consumed` gate sites, with
  `walker.rs` holding only a stale comment. **Confirmed accurate.**
* The skill's D4 repro (`Dawnguard.esm`'s MAST list is 2 entries) is
  **confirmed by measurement**.
* **Dimension 5**: the skill says `.btr` meshes live in `Textures8.bsa`.
  Measured: **all 9 584 `.btr` and 1 078 `.bto` files are in
  `Skyrim - Meshes1.bsa`**; `Textures8.bsa` holds 242 ordinary weapon/clutter
  textures. The skill also states the sibling rule as `<stem>2..<stem>9`,
  which is the FNV (no-trailing-digit) case only — Skyrim's zero-based
  archives take a separate `…0 → …1..…9` arm. The **code is right**, the
  skill text is not. (SK-D5-01)
* **Dimension 6**: the skill describes `.bto` object LOD as "level-4 quads
  only, `OBJECT_LOD_RADIUS_CELLS = 16`". That constant **no longer exists**;
  the flat ring was replaced by a 4/8/16 band ladder, and 334 of Skyrim's
  1 078 `.bto` quads (31 %) are level 8 or 16. (SK-D6-02)
* Unlike the FNV and FO3 checklists — both caught chasing collision shapes
  their game ships zero of — **no Skyrim checklist item measured to zero
  against shipped content**. Every mechanism named is exercised. The nearest
  miss is VWD (5 943 flagged records, consumer unbuilt), which the skill
  already frames correctly as forward scope and which is deliberately not
  re-filed.

### Deliberately not re-reported

* **#1832** (mass=0 Dynamic-family Havok bodies reclassified Static,
  `ae083d69`) — settled; not re-investigated.
* **Renderer ghosting** (diagonal double-image in Skyrim interiors) — open,
  needs RenderDoc. No source-level speculation offered.
* **VWD full-model culling** (#1731 / #3307) — forward scope, premise
  already re-measured 2026-08-29.
* **Skyrim CHARAL ruleset unwired** — known; no new consequence found.

---

# Dimension 1 — BSTriShape Packed Geometry + SSE Skinned Reconstruction

Corpus: `Skyrim - Meshes0.bsa` + `Skyrim - Meshes1.bsa`, 32 709 NIF entries
(`.nif`/`.bto`/`.btr` per `corpus::is_nif_entry`), 99 203 `BsTriShape` blocks,
96 123 `ImportedMesh` results. All measurements 2026-08-30 via
`crates/nif/examples/_tmp_sk_a0830_{census,import,tan,one}.rs`.

## Verified clean (regression guards held)

| Guard | Measured |
|---|---|
| Parse rate | 32 709 / 32 709 files, **0 parse failures, 0 `NiUnknown` blocks** |
| `vertex_desc` offset drift (`check_vertex_desc_offsets`, #2578) | **0 mismatches** over 99 203 shapes |
| `data_size` vs `vertex_size_quads × nv + nt×6` (#359/#621) | **0 mismatches**; the `#2598` unsafe-override path never fires |
| `VF_UVS_2` / `VF_LAND_DATA` / `VF_INSTANCE` (deferred decoders) | **0 / 0 / 0** occurrences — the deferred-extraction gap is not reachable on vanilla Skyrim SE |
| `BsTriShapeKind` split | Plain 71 303, Dynamic 21 140, SubIndex 6 760, MeshLOD 0 (FO4-only, as designed) |
| SSE skinned reconstruction (#559 / #3355) | 26 886 skinned meshes, **0 with zero indices**; 5 801 shapes ship `num_vertices == 0` and all recover |
| `BSDynamicTriShape` (#341/#2322/#2318) | all 21 140 blocks have `data_size == 0` + `VF_VERTEX` clear; all 21 139 importable ones recover geometry from the partition buffer + external position/bitangent-X lanes — **0 empty imports** |
| Alpha cascade gate (#1201/#1202) | `info.alpha_property_consumed` set at `material/mod.rs:1542`, consulted at `dedicated_shader.rs:347` and `legacy_properties.rs:116`; `walker.rs:129` holds only the stale comment the skill describes. Premise confirmed. |

Three candidate findings were **dropped as stale after measurement**:
1. "21 140 `BSDynamicTriShape` produce vertices but 0 triangles → silent
   render failure" — true at block level, fully recovered at import
   (`decode_sse_shape_buffer`). Not a defect.
2. "17 926 shapes set `VF_TANGENTS|VF_NORMALS` but carry no inline tangents"
   — those are the Dynamic/skinned shapes whose tangents live in the
   partition buffer; `sse_recon` recovers them.
3. "4 368 `.btr` `BSSubIndexTriShape` have an all-zero attribute mask →
   no mesh" — the mask is `0x001` (VERTEX only), stride 4 quads; they
   import 64 verts / 96 indices each.

## SK-D1-01 (HIGH) — 19 657 of 96 123 imported Skyrim meshes (20.4 %) render with a fabricated constant `[0,1,0]` up-normal; there is no normal-synthesis fallback anywhere in the importer

`crates/nif/src/import/mesh/bs_tri_shape.rs:83-88` fills `normals` with
`vec![[0.0, 1.0, 0.0]; positions.len()]` whenever neither the inline shape
nor the SSE partition buffer authored `VF_NORMALS`, and
`crates/nif/src/import/mesh/sse_recon.rs` does the same inside the packed
decoder. The importer has `synthesize_tangents` / `synthesize_tangents_yup`
(`import/mesh/tangent.rs:176,384`) but **no `synthesize_normals`** — grep over
`crates/nif/src` returns zero hits. So the one geometric quantity the shading
path cannot do without is the only one with no synthesis path.

Measured breakdown of the 19 657 (all normals exactly `[0,1,0]`):

| Shape name | Count | What it is |
|---|---|---|
| `Land` / `land` | 9 584 | distant-terrain LOD (`.bto`/`.btr`); exactly matches the 9 584 `BSLightingShaderProperty` blocks with `shader_type == 18` (LOD Landscape Noise) |
| `<unnamed>` | 6 001 | `BSSubIndexTriShape` LOD sub-meshes, attrs `0x001` (position only) |
| `MaleHead*` / `FemaleHead*` / `*Mouth*` | 3 201 | **every vanilla FaceGen head + mouth mesh** under `meshes\actors\character\facegendata\` |
| skin patches under `armor\*`, `clothes\*` | ~870 | `SkinHands`, `HandFemale3rd`, `shoes_skin`, … |

Verified this is authentic authoring, not a mis-parse: `MaleHeadNord` in
`facegeom\skyrim.esm\000b4c4f.nif` has `vertex_desc = 0x0046200021000045`,
attrs `0x462` = UVS|COLORS|SKINNED|FULL_PRECISION, `vertex_size = 20` and
`raw_bytes = 17 960 = 898 × 20` — UV(4) + color(4) + skin(12) accounts for
every byte, so the file genuinely ships no normal lane. Same for
`HeadMaleBig:0` in `armor\glass\glassstatic.nif`
(`data_size 49 056 == 20×1 590 + 2 876×6`, exact).

Consequence, by class:
* **FaceGen heads** — the real Creation Engine recomputes head normals on the
  CPU *after* the FaceGen morph pass, which is precisely why the data is not
  shipped. Substituting a constant world-up normal means every NPC face in
  the engine is lit as a flat upward-facing plane: no facial form, brightness
  keyed only to the sun's elevation. This is the D3 content class.
* **Distant-terrain LOD** — Skyrim's LOD pipeline supplies terrain normals via
  a per-quad LOD normal map, so an up-normal + a correctly bound `_n` LOD
  texture may be recoverable; but the `.btr` sub-mesh at index 5 of
  `dlc2apocryphaworld.4.-18.22.btr` carries **no shader property at all**
  (only the `Land` shape at index 1 has one), so 6 001 of those sub-meshes
  have no texture set to fall back on either.

Fix shape: add a geometric `synthesize_normals(positions, indices)` (area-
weighted face-normal accumulation) mirroring the existing
`synthesize_tangents_yup` contract, and call it at the two fabrication
sites *before* the `[0,1,0]` fill, gated on a non-empty index list. That
would also un-block the `normals_authored && uvs_authored` tangent branch
(#2817) for the same 20 206 tangent-less meshes.

Not a duplicate: no open issue covers normal fabrication (searched the
300-issue baseline for `normal`/`facegen`/`tangent`/`head`; #3177 and #3176
are Z-up tangent-synthesis defects, #3464/#3526 are Starfield FaceGen).

## SK-D1-02 (LOW) — `min_vertex_bytes` and `vertex_desc_offset_mismatches` silently disagree about `VF_UVS_2` / `VF_LAND_DATA` cursor placement, and no vanilla content can expose it

Both helpers assume the two undecoded attribute bits reserve *trailing*
bytes. `check_vertex_desc_offsets` exists specifically to make that
assumption falsifiable — and it does its job, but the corpus contains **zero**
shapes with either bit set (measured above), so the warn has never fired and
never can on vanilla Skyrim SE. This is a documentation-accuracy note, not a
defect: the code comments already state the assumption is unverified. Worth
recording that the audit *measured* the exposure at zero rather than
inferring it.

---

# Dimension 2 — BSLightingShaderProperty / BSEffectShaderProperty Shader-Type Dispatch

Corpus: 85 104 `BSLightingShaderProperty` + 8 116 `BSEffectShaderProperty`
blocks across `Skyrim - Meshes0/1.bsa` (32 709 files, 0 parse failures,
0 `NiUnknown`). Spec cross-checked against
`/mnt/data/src/reference/nifxml/nif.xml` lines 1400-1423 (`BSLightingShaderType`),
6371-6440 (`SkyrimShaderPropertyFlags1/2`), 6581-6637 (`BSLightingShaderProperty`).

## Shader-type coverage — measured

| Type | nif.xml name | Occurrences | `ShaderTypeData` arm | Trailing floats read | nif.xml `cond` |
|---|---|---|---|---|---|
| 0 | Default | 47 850 | `None` | 0 | — |
| 1 | Environment Map | 6 726 | `EnvironmentMap` | 1 | ✓ `Shader Type == 1` |
| 2 | Glow Shader | 1 395 | `None` | 0 | — |
| 3 | Parallax | 11 | `None` | 0 | — |
| 4 | Face Tint | 3 158 | `None` | 0 | — |
| 5 | Skin Tint | 1 631 | `SkinTint` | 3 (Color3) | ✓ |
| 6 | Hair Tint | 10 817 | `HairTint` | 3 (Color3) | ✓ |
| 7 | Parallax Occ | 0 | `ParallaxOcc` | 2 | ✓ (unexercised on vanilla) |
| 11 | MultiLayer Parallax | 662 | `MultiLayerParallax` | 5 | ✓ |
| 14 | Sparkle Snow | 19 | `SparkleSnow` | 4 (Vector4) | ✓ |
| 16 | Eye Envmap | 3 251 | `EyeEnvmap` | 7 | ✓ |
| 18 | LOD Landscape Noise | 9 584 | `None` | 0 | — |
| 8,9,10,12,13,15,17,19,20 | — | 0 | `None` | 0 | — |

Variant totals reconcile exactly: `None` 61 998 = 47 850 + 1 395 + 11 +
3 158 + 9 584. Every arm reads the field count nif.xml declares, and the
9 `ShaderTypeData` variants (incl. FO76-only `Fo76SkinTint`) are the
complete set. `parse_shader_type_data_fo76` is reached only on
`bsver == 155` (`BSShaderType155` numbering, type 4 = Color4 skin tint,
type 5 = Color3 hair tint) and cannot cross-contaminate the Skyrim arm —
`BSLightingShaderProperty::parse` dispatches `parse_fo76` / `parse_fo4` /
`parse_skyrim` on disjoint BSVER bands.

**No over-read**: 0 truncated files, 0 `NiUnknown` blocks, 0 realignment
recoveries over the whole corpus — an off-by-one on any `None` type
would surface as a `block_size` realignment on the 61 998 blocks that
take that path.

## Flag decode — verified against nif.xml, bit by bit

Every constant in `crates/nif/src/shader_flags.rs::skyrim_slsf1` /
`skyrim_slsf2` matches nif.xml: SLSF1 bits 4, 5, 12, 15, 16, 22, 26, 27,
30, 31; SLSF2 bits 4, 6, 17, 20, 21, 25, 26, 27, 30. Measured SLSF1 bit
occupancy corroborates the semantics — bit 10 (`Facegen_Detail_Map`)
fires on exactly 3 158 blocks, matching `shader_type == 4` (Face Tint)
one-for-one; bit 21 (`FaceGen_RGB_Tint`) on exactly 1 631, matching
`shader_type == 5` (Skin Tint). SLSF2 bit 1 (`LOD_Landscape`) fires on
exactly 9 584, matching `shader_type == 18`. Three independent
correlations, no drift.

`BSShaderTextureSet` slot routing is shader-type-aware and single-sourced
(`import/material/slot_role.rs::slot_to_role`, #2695), so Face Tint's
detail(TS4)/tint(TS7) slots do reach canonical roles.

## SK-D2-01 (HIGH, regression guard MEASURED CLEAN) — the Disney/Burley lobe is provably unreachable on vanilla Skyrim

`MAT_FLAG_PBR_BSDF` (`crates/renderer/src/vulkan/material.rs:623`, bit 5)
is packed only from `ImportedMaterial::is_pbr`, which the import layer
hard-sets to `false` (`import/material/mod.rs:1437`, `import/types.rs:667`)
and which is flipped `true` at exactly four sites, all inside the
**external-material-file merge** (`asset_provider/material.rs:963`,
`:1295`, and the two `cell_loader.rs` BGSM/BGEM paths) — i.e. only when a
`.bgsm` / `.bgem` / `.mat` / CDB record resolves.

Measured reachability on the Skyrim mesh universe: **0 of 85 104
`BSLightingShaderProperty` blocks and 0 of 8 116 `BSEffectShaderProperty`
blocks carry a non-empty `NiObjectNET.name`** — so no vanilla Skyrim
shader property can even name an external material file, and
`material_path_from_name`'s `.bgsm`/`.bgem` capture has nothing to
capture. The lobe stays off for 100 % of vanilla content; only a mod that
authors a BGSM path into a Skyrim shader property's name flips it, which
is the one legitimate path. Guard confirmed by measurement rather than
inference.

## SK-D2-02 (LOW) — `fo4_slsf2`'s module doc mis-attributes Skyrim SLSF2 bit 21 as `Cloud_LOD`

`crates/nif/src/shader_flags.rs`, `fo4_slsf2` module doc and the
`ANISOTROPIC_LIGHTING` constant doc both claim bit 21 carries "three
different semantics on the same bit across games (FO4:
Anisotropic_Lighting, Skyrim: Cloud_LOD, FO3/FNV: Alpha_Decal)". nif.xml
`SkyrimShaderPropertyFlags2` says bit 20 is `Cloud_LOD` and **bit 21 is
`Anisotropic_Lighting`** — the same semantic FO4 has. Only FO3/FNV
diverges (bit 21 = `Alpha_Decal`).

The **constants are correct** (`skyrim_slsf2::CLOUD_LOD = 0x0010_0000`
bit 20, `skyrim_slsf2::ANISOTROPIC_LIGHTING = 0x0020_0000` bit 21) and
the #414 conclusion the doc supports ("a legacy `is_decal_from_shader_flags`
that tests `flags2 & 0x0020_0000` must not run on a Skyrim+/FO4
property") still holds — only for a two-way, not three-way, reason.
Doc-rot on a comment block whose whole purpose is to be the reference
for cross-game flag work; a future reader trusting it would look for a
Skyrim `Cloud_LOD` behaviour at the wrong bit.

## Unexercised on vanilla (recorded, not findings)

* `BSEffectShaderProperty::env_map_min_lod` — nif.xml range `0:16`;
  **max observed = 0** across all 8 116 blocks. The field parses but no
  vanilla content varies it.
* `BSLightingShaderType` 7 (`Parallax Occ`) — 0 occurrences; nif.xml
  itself annotates it "Unimplemented." The `ParallaxOcc` arm is
  correct-by-spec but untestable against vanilla Skyrim.

---

# Dimension 3 — NPC Equip + FaceGen (M41), incl. `crates/facegen`

## Real-data guards — all green

`cargo test -p byroredux --bin byroredux -- --ignored` against the installed
Skyrim SE data dir (2026-08-30):

| Guard | Result |
|---|---|
| `bannered_mare_npcs_resolve_a_full_equip_state_on_real_skyrim_data` (#3361) | **pass** — all 6 named NPCs (Saadia 0x13BA2, Hulda 0x13BA3, Brenuin 0x13BA7, Mikael 0x1A670, Sinmir 0x813B5, AmaundMotierreEnd 0x4E64F) reach non-empty `Inventory`, occupy ≥1 biped slot, and get a torso mesh from the race skin |
| `bannered_mare_outfits_keep_every_inam_entry_on_real_skyrim_data` (#3356) | **pass** |
| `creature_race_npcs_keep_their_skin_mesh_on_real_skyrim_data` (#3408) | **pass** — the `BOD2 == 0` creature-race occupancy-retain regression stays fixed |
| `helmeted_npcs_get_a_facegen_hide_mask_on_real_skyrim_data` (#3409) | **pass** |
| `real_skyrim_load_order_preserves_categories_and_resolves_archive_strings` | **pass** (see Dim 4) |
| `real_skyrim_esm_ambient_packages_now_resolve_for_previously_blind_npcs` | **pass** |

Equip-chain wiring verified in code: `RACE.WNAM` default skin equips as the
lowest-priority layer (#2093), the post-loop occupancy filter drops
displaced queued meshes (#2094), `humanoid_body_paths` correctly returns
`&[]` for `Skyrim | Fallout4 | Fallout76 | Starfield` (the `upperbody.nif`
pre-scan is Oblivion/FO3NV-only), and `hide_skin_partitions` runs on the
head phase as well as the armour phase (#3409). `BsDismemberSkinInstance`
routes into the skinning pipeline at five sites in
`crates/nif/src/import/mesh/skin.rs` plus `sse_recon.rs`.

## FaceGen path — measured, 1:1

Skyrim is on the **pre-baked** track (`GameKind::uses_prebaked_facegen`,
mutually exclusive with `has_runtime_facegen_recipe` and pinned by a test),
so `EgmFile` / `apply_morphs` are never reached for this game. Measured
against the installed archives:

* `meshes\actors\character\facegendata\facegeom\**` — **3 158** NIFs across
  `Skyrim - Meshes0/1.bsa`
* `textures\actors\character\facegendata\facetint\**` — **3 158** DDS across
  `Skyrim - Textures0..8.bsa`

Exact 1:1, and both match the **3 158** `BSLightingShaderProperty` blocks
with `shader_type == 4` (Face Tint) and the **3 158** with SLSF1 bit 10
(`Facegen_Detail_Map`) measured in Dim 2 — a four-way correlation. The
tint DDS is wired: `prebaked_facegen_tint_path` →
`RuntimeNpcState.tint_path` → `load_nif_bytes_with_skeleton(..., tint_path, ...)`
at `npc_spawn/resumable.rs:1205-1229`, gated on the file actually resolving.

## SK-D3-01 (HIGH, inherits SK-D1-01) — every Skyrim FaceGen head renders with a fabricated flat up-normal

The pre-baked path is the *only* head source for Skyrim, and the measurement
in Dimension 1 lands squarely on it: **3 201 imported meshes under
`meshes\actors\character\facegendata\` come out of `import_nif` with every
normal equal to `[0,1,0]`**, because the head's `NiSkinPartition` global
buffer clears `VF_NORMALS` and the importer's only fallback is a constant
fill. Confirmed non-mis-parse on `facegeom\skyrim.esm\000b4c4f.nif`:
`MaleHeadNord` has `vertex_desc = 0x0046200021000045` (attrs UVS | COLORS |
SKINNED | FULL_PRECISION), `vertex_size = 20`, `raw_bytes = 17 960 = 898 × 20`
— UV(4) + colour(4) + skin(12) accounts for every byte, no normal lane
exists. The sibling `MaleEyesHumanLightBlue` in the same file *does* carry
`VF_NORMALS | VF_TANGENTS` (attrs `0x55a`), so a head and its eyes shade
under two different regimes.

Consequence: the six Bannered Mare NPCs — and every NPC in the game — have
correct silhouettes, correct skin textures, correct tint, and completely
flat facial shading. This is the concrete visual payoff of SK-D1-01; the fix
is the same (`synthesize_normals` from positions + indices), and it belongs
to whichever issue lands first. Filed here separately only because the
audience is different: D1 owns the importer defect, D3 owns "NPC faces are
unlit".

## SK-D3-02 (MEDIUM) — `crates/facegen`'s `.egt` and `.tri` parsers have zero consumers anywhere in the workspace

`crates/facegen` (1 394 LOC incl. tests) exports four public surfaces.
Grepping the whole tree outside the crate itself:

| Export | External consumers |
|---|---|
| `EgmFile` / `EgmMorph` | `byroredux/src/npc_spawn/resumable.rs:997` (Oblivion / FO3NV runtime-recipe track only) |
| `apply_morphs` | same, `:1024-1025` |
| `half_to_f32` | `crates/nif/.../decode_half_float_tests.rs` (a bit-for-bit parity test, #2599) |
| **`EgtFile` / `EgtMorph`** | **none** |
| **`TriHeader`** | **none** |

`egt.rs` (234 LOC) parses the full FaceGen texture-morph table
(`FREGT003`, 50 morphs × 256×256×3) and nothing reads it; the crate's own
module doc says "Phase 3c consumes the EGT compositor output", but
`resumable.rs`'s Phase 3b/3c log line covers FGGS+FGGA *geometry* morphs
only — there is no compositor. `tri.rs` (154 LOC) is a self-declared
header-only stub whose body parse is deferred to "M47-tier work", and even
its header is unread. Both are exercised solely by
`crates/facegen/tests/parse_real_facegen.rs`, i.e. they are tested but not
used.

This matters beyond dead weight: `crates/facegen` has **no other owner in
this audit suite**, so an unconsumed parser here is invisible to every other
gate. Either wire the EGT compositor (the runtime-recipe games need it for
per-NPC complexion) or mark both modules explicitly deferred in the crate
doc so the next reader does not assume Phase 3c shipped.

Not a Skyrim-blocking gap — Skyrim's pre-baked track needs neither file —
but it is the crate's coverage answer, and it is a real one.

## Dropped as stale

`apply_morphs`'s "Coordinate frame" doc claims EGM deltas and base vertices
share a frame because "the Z-up→Y-up conversion happens at the placement-root
level, not at the vertex level". That is false for the current importer —
`extract_bs_tri_shape` / `sse_recon` both run `zup_to_yup_pos` per vertex —
so the hook at `resumable.rs:1024` adds Z-up deltas to Y-up positions.
**Not reported as a Skyrim finding**: the hook is reachable only from
`has_runtime_facegen_recipe()` games (Oblivion, FO3/FNV); Skyrim never enters
it. Handing it to `/audit-fnv` / `/audit-oblivion` rather than claiming it
here.

---

# Dimension 4 — Multi-Master Load Order + TES5 Cell-Load Regression

Measured against the installed Skyrim SE `Data/` (7 `.esm` + 3 `.esl`,
2026-08-30) with `crates/plugin/examples/_tmp_sk_a0830_lo.rs` — a scoped
header + record-header walker, one plugin at a time.

## Measured plugin census

| Plugin | MB | localized | light_master | records | compressed | VWD | deleted |
|---|---|---|---|---|---|---|---|
| `Skyrim.esm` | 238.2 | ✓ | — | 869 688 | 44 153 | 4 945 | 0 |
| `Update.esm` | 24.7 | ✓ | — | 16 388 | 4 227 | 146 | 97 |
| `Dawnguard.esm` | 61.7 | ✓ | — | 95 719 | 15 870 | 454 | 154 |
| `HearthFires.esm` | 3.8 | ✓ | — | 18 037 | 463 | 27 | 16 |
| `Dragonborn.esm` | 61.7 | ✓ | — | 178 716 | 41 552 | 312 | 12 |
| `ccBGSSSE001-Fish.esm` | 1.4 | ✓ | — | 6 061 | 633 | 52 | 4 |
| `ccBGSSSE025-AdvDSGS.esm` | 0.8 | ✓ | — | 3 012 | 99 | 0 | 0 |
| `_ResourcePack.esl` | 0.2 | ✓ | **✓** | 374 | 0 | 7 | 0 |
| `ccBGSSSE037-Curios.esl` | 0.0 | ✓ | **✓** | 152 | 0 | 0 | 0 |
| `ccQDRSSE001-SurvivalMode.esl` | 0.1 | ✓ | **✓** | 674 | 2 | 0 | 0 |

**Totals: 1 188 821 records, 106 999 compressed, 5 943 VWD-flagged, 283 Deleted.**

## Verified clean (regression guards held)

* **ESL / light-master decode (#1554)** — 3 of 3 `.esl` files report
  `light_master = true`; 7 of 7 `.esm` report `false`. `allocate_global_slot`
  routes the former to `GlobalSlot::Light` (12-bit sub-index in the `0xFE`
  space, capped at `0x0FFF`) and the latter to `0x00..=0xFD`, both with
  overflow checks. Measurement also **falsifies the "third-party only"
  reading of `FileHeader::light_master`'s doc**: a stock AE install ships
  three ESL-flagged plugins, one of them (`_ResourcePack.esl`) part of the
  base game, and `_ResourcePack.esl` carries a **3-entry MAST list**
  (`Skyrim.esm`, `Update.esm`, `HearthFires.esm`) — so the ESL path really
  does have to remap references into full-byte master slots on vanilla data.
* **Repeatable `--master` (#561 / #2583)** — `Dawnguard.esm`'s real MAST
  list is exactly `["Skyrim.esm", "Update.esm"]`, confirming the skill's
  repro. `build_remap_for_plugin` consumes `header.master_files` per plugin
  inside a single forward pass over `plugin_paths`, with `slots` filled
  before any dependent references it.
* **`.STRINGS` wired into the multi-plugin path (`db5bb149` / #1553)** —
  `install_strings_guard` is called **inside** the per-plugin loop
  (`load_order.rs:368`), keyed on that plugin's own `header.localized`, and
  the RAII guard is bound to a loop-local so it drops before the next
  plugin. All 10 installed plugins report `localized = true`, so every one
  of them needs its own tables — a last-plugin-only regression would be
  visible on 100 % of this install.
* **Compressed-record decompression** — 106 999 compressed records across
  the set, 44 153 in `Skyrim.esm` alone; the shipped
  `parse_real_skyrim_esm` / `merge_real_dawnguard_partial_cells_preserves_skyrim_landscape`
  guards exercise them.
* **`group_content_end` on header-only GRUPs** — `Skyrim.esm` contains
  empty (24-byte) GRUPs of types 0, 5, 8 and 9; the accessor returns
  exactly the current position for each, i.e. an empty content range rather
  than an underflow. (This tripped my probe's first draft, not the engine.)
* **VWD** — 5 943 records carry `FLAG_VISIBLE_WHEN_DISTANT`. Parsed and
  exposed via `RecordHeader::is_visible_when_distant()`, consumer still
  unbuilt; per the skill this is **forward scope (#3307), not a regression**
  and is deliberately not re-filed.

## SK-D4-01 (MEDIUM) — the Deleted (0x20) tombstone is honoured only for placements; 9 DLC-deleted **base** records merge live on vanilla Skyrim

`RECORD_FLAG_DELETED` is tested at exactly one site in the whole crate —
`crates/plugin/src/esm/cell/walkers.rs:715`, inside the REFR/ACHR/ACRE
placement walk. No parser under `crates/plugin/src/esm/records/` tests bit
`0x20`, so a base record a later plugin marks Deleted is merged by
`EsmIndex::merge_from` under plain last-write-wins and **replaces the
master's live record with the DLC's tombstoned copy**.

Measured on the shipped DLC set (excluding NAVM, which has no consumer, and
REFR/ACHR, which `walkers.rs` already skips):

| Plugin | Type | FormID (raw) | `data_size` | header flags |
|---|---|---|---|---|
| `Update.esm` | STAT | `0006CD7C` | 153 | `0x04010820` |
| `Dawnguard.esm` | STAT | `000BD6A5` | 198 | `0x00000020` |
| `Dawnguard.esm` | **NPC_** | `0007932F` | 220 | `0x00040020` |
| `Dawnguard.esm` | IDLE | `000FDC30` | 210 | `0x00000020` |
| `Dawnguard.esm` | IDLE | `000F6CBB` | 192 | `0x00000020` |
| `Dawnguard.esm` | SMQN | `000F2199` | 147 | `0x00000020` |
| `Dragonborn.esm` | **SPEL** | `0010E38C` | 307 | `0x00000020` |
| `Dragonborn.esm` | INFO | `000CEFBE` | 20 | `0x00000020` |
| `Dragonborn.esm` | EXPL | `000F3A8C` | 163 | `0x00000020` |

Every one has a **non-empty payload** (20–307 bytes), so these are not
zeroed stubs the merge would harmlessly absorb — they are full override
records. And every raw FormID has top byte `0x00`, i.e. **master index 0 =
`Skyrim.esm`**: all nine are DLC overrides of base-game records that the DLC
then deletes. Correct behaviour is to drop the base-game record from the
merged index; actual behaviour is to keep it, carrying the DLC's stale
content. The visible cases are `Dawnguard.esm`'s deleted `NPC_ 0007932F`
(still spawnable / still resolvable by the equip chain) and
`Dragonborn.esm`'s deleted `SPEL 0010E38C`.

The associated doc over-claims: `crates/plugin/src/esm/cell/mod.rs:1131`
states deleted records "never appear in `over` at all", which is true of
REFRs and reads — in a file named `cell/mod.rs` — as though the tombstone
story is complete. It is complete for placements only.

Scope is genuinely small (9 records, plus 68 NAVM the engine cannot consume
yet), which is why this is MEDIUM and not HIGH; but the mechanism is
general, and a mod load order with real conflict resolution will hit it at
far higher volume than vanilla does. The fix is one flag test in the record
walk, mirroring `walkers.rs:715`, plus a removal signal through
`merge_from` analogous to `CellData::deleted_refs` (#2370).

## SK-D4-02 (LOW) — `FileHeader::light_master`'s doc says no vanilla plugin is ESL-flagged

`crates/plugin/src/esm/reader.rs:538` — "No vanilla Skyrim SE / FO4 /
Starfield master is ESL-flagged; this is for third-party ESL mods and
ESL-flagged CC content." True of *masters*, but a stock Anniversary install
ships `_ResourcePack.esl` (base-game, 374 records, 3 masters) alongside the
CC `.esl` files, so the ESL decode path is on the vanilla critical path,
not a mod-only concern. Worth one sentence so nobody treats the ESL branch
as untested-by-construction on a clean install.

## Checklist items that measured to zero / near-zero (recorded)

None of this dimension's checklist items were vacuous on Skyrim data — in
contrast to the FNV/FO3 collision-shape case the coordinator flagged. The
closest is VWD: 5 943 flagged records exist and the consumer does not, but
the skill already states that correctly as forward scope.

## Control-bench guard

Not re-measured: the Whiterun BanneredMare entity-count / FPS bench needs a
live engine run, which `/audit-runtime` owns this cycle and which this audit
is explicitly barred from launching. The static half of the guard — that the
six named NPCs resolve a full equip state on real `Skyrim.esm` — is green
(Dimension 3).

---

# Dimension 5 — BSA v105 (LZ4)

## Full-archive extraction sweep — measured, clean

Every file in every installed Skyrim SE archive extracted one at a time
(`crates/bsa/examples/_tmp_sk_a0830_sweep.rs`, `RUST_LOG=warn`):

| Archive | files | ok | fail | decompressed |
|---|---|---|---|---|
| `Skyrim - Meshes0.bsa` | 19 443 | 19 443 | 0 | 2 000.0 MB |
| `Skyrim - Meshes1.bsa` | 14 242 | 14 242 | 0 | 518.7 MB |
| `Skyrim - Textures0..8.bsa` | 31 952 | 31 952 | 0 | 15 420.0 MB |
| `Skyrim - Voices_en0.bsa` | 75 408 | 75 408 | 0 | 1 720.5 MB |
| `Skyrim - Sounds.bsa` | 6 198 | 6 198 | 0 | 1 467.0 MB |
| `Skyrim - Animations.bsa` | 8 979 | 8 979 | 0 | 115.3 MB |
| `Skyrim - Misc / Interface / Shaders` | 14 419 | 14 419 | 0 | 181.5 MB |
| `_ResourcePack.bsa` + 4 CC + `MarketplaceTextures` | 2 277 | 2 277 | 0 | 2 398.0 MB |
| **Total (23 archives)** | **173 918** | **173 918** | **0** | **≈24.8 GB** |

**0 extraction failures and 0 decompression-size-delta warnings.** The
`#622 / SK-D2-04` post-decompression sanity check (`decompressed.len() !=
original_size` → `log::warn!`) did not fire once across 173 918 files, so
Skyrim SE's LZ4 frames round-trip exactly — no padding deltas at all on this
title.

## Format handling — verified against the code

* **Version gate** — `open.rs:35` accepts only 103 / 104 / 105 with an
  explicit error otherwise.
* **Folder-record size** — 24 bytes for v105 (`hash:u64, count:u32,
  _padding:u32, offset:u64`) vs 16 for v103/v104 (`hash:u64, count:u32,
  offset:u32`), selected at `open.rs:138` and the offset width at `:169`.
* **Codec dispatch** — `extract.rs:142`, `self.version >= BSA_V_SKYRIM_SE`
  → `lz4_flex::frame::FrameDecoder`, else `ZlibDecoder`; both wrapped in
  `safety::inflate_bounded` so `original_size` is an enforced ceiling
  rather than a capacity hint (#3410).
* **Embedded-name flag** — driven by the archive-level `0x100` bit alone,
  matching openmw; the speculative per-file bit-31 override was removed in
  #3367 after measuring it set on **zero** files across every installed
  vanilla archive. Sweep above re-confirms: nothing regressed.
* **Compression-flag priority** — `is_compressed = compressed_by_default !=
  entry.compression_toggle` (XOR), i.e. the per-file `0x40000000` bit
  *toggles* the archive default rather than overriding it. Correct per the
  format, and the 0-failure sweep is the evidence: a priority inversion
  would feed raw bytes to a decompressor (or vice versa) on whichever side
  disagrees, and Skyrim's mesh archives mix both.
* **Zero-based sibling auto-load (`821a425b`)** —
  `numeric_sibling_paths` has a dedicated `Some('0')` arm (guarded against
  `…10`) that strips the trailing `0` and offers `…1`..`…9`, so
  `Skyrim - Textures0.bsa` correctly drags in `Textures1..Textures8`.
  Verified by reading the arm and by the file inventory below.

## SK-D5-01 (LOW, skill-vs-data discrepancy) — the audit skill mislocates Skyrim's distant-LOD meshes

The Dimension 5 checklist states the zero-based sibling auto-load matters
because *"distant-LOD diffuse in `Textures7.bsa` and `.btr` meshes in
`Textures8.bsa` drag in from a zero-based base archive"*. Measured file
inventory:

| Archive | `.btr` | `.bto` | `.dds` |
|---|---|---|---|
| `Skyrim - Meshes0.bsa` | 0 | 0 | 0 |
| `Skyrim - Meshes1.bsa` | **9 584** | **1 078** | 0 |
| `Skyrim - Textures7.bsa` | 0 | 0 | 7 084 |
| `Skyrim - Textures8.bsa` | 0 | 0 | 242 |

* The first half is right: `Textures7.bsa` is where the distant-terrain
  diffuse lives (`textures\terrain\tamriel\tamriel.<level>.<x>.<y>.dds`),
  and it is only reachable through the sibling expansion.
* The second half is wrong: **every one of Skyrim's 9 584 `.btr` and 1 078
  `.bto` distant-LOD meshes is in `Skyrim - Meshes1.bsa`, not
  `Textures8.bsa`.** `Textures8.bsa` holds 242 ordinary weapon/clutter
  textures (`textures\weapons\volendrung\volendrung.dds`, …).

Also worth pinning: the skill describes the sibling rule as
"`<stem>2.bsa`..`<stem>9.bsa`", which is only the *no-trailing-digit* (FNV)
case. Skyrim's zero-based archives take the `…0` arm and expand `…1`..`…9`;
under the skill's stated rule `Skyrim - Textures0.bsa` would look for
`Skyrim - Textures02.bsa` and find nothing. The **code is correct**; the
skill text is not.

Recording this per the standing rule that a skill premise disagreeing with
measured data gets reported explicitly. No code change needed — this is a
correction to `.claude/commands/audit-skyrim/SKILL.md`.

Bonus correlation from the same inventory: 9 584 `.btr` files ↔ 9 584
`BSLightingShaderProperty` blocks with `shader_type == 18` (LOD Landscape
Noise, Dim 2) ↔ 9 584 `Land`/`land` meshes with fabricated normals (Dim 1).
One `Land` shape per `.btr`, exactly.

---

# Dimension 6 — Specialty Blocks + Distant LOD + Real-Data Rendering

## Full mesh-corpus sweep — measured, clean

Every NIF entry (`.nif` / `.bto` / `.btr`) in **all 23 installed Skyrim SE
archives**, parsed and imported:

```
files = 33 468      meshes = 97 998
no_indices = 0      skinned = 27 778   skinned_no_indices = 0
NiUnknown blocks = 0   truncated = 0
realignment / recovery / truncation WARNs = 0
```

Consistent with the ROADMAP's 33 424-across-7-archives figure (this sweep
includes a few non-mesh archives that also carry NIFs). **The
"0 realignment WARNs" baseline holds.**

## Block-dispatch guards — all intact

Measured occurrences across `Skyrim - Meshes0/1.bsa` (97 distinct block
types, `NiUnknown = 0`):

| Block | Count | Dispatch (`blocks/mod.rs`) | Guard |
|---|---|---|---|
| `BSLODTriShape` | 43 | `:473` → `NiLodTriShape::parse` | **#838 intact** — routed through `NiTriBasedGeom`, NOT `BsTriShape` |
| `BSMeshLODTriShape` | 0 (FO4-only) | `:478` → `BsTriShape::parse_lod` | distinct body, correctly separate |
| `BSSubIndexTriShape` | 6 760 | `:489` → `parse_sub_index` | segmentation payload decoded (#404) |
| `BSDynamicTriShape` | 21 140 | `:505` → `parse_dynamic` | #341 / #2322 |
| `BSLagBoneController` | 221 | `:880` | **#837 intact** — no `block_size` WARN burst |
| `BSProceduralLightningController` | 37 | `:881` | #837 intact |
| `BSTreeNode` | 55 | `:344` | SpeedTree wind bones |
| `BSDistantObjectLargeRefExtraData` | 1 522 | — | single `bool` per nif.xml |
| `BSMultiBoundNode` / `BSValueNode` / `BSOrderedNode` / `BSRangeNode` | 18 124 / 791 / 252 / 44 | — | unwrapped by the import walker |
| `BSPackedCombined[Shared]GeomDataExtra` | **0** | `:743` | FO4-only; **Skyrim ships none** |

Any proposal to fold `BSLODTriShape` into `BsTriShape` would over-read all
43 blocks — guard restated and re-verified.

## Distant-LOD inventory — measured

`Skyrim - Meshes1.bsa` is the sole source of Skyrim's baked LOD geometry
(zero in `Meshes0.bsa`, zero in any texture archive):

| ext | level 4 | level 8 | level 16 | level 32 | total |
|---|---|---|---|---|---|
| `.btr` (terrain) | 7 153 | 1 825 | 474 | **132** | 9 584 |
| `.bto` (objects) | 744 | 248 | 86 | 0 | 1 078 |

## SK-D6-01 (VERIFIED CLEAN, against the game's own INI) — the Skyrim LOD band ladder matches `Ultra.ini` exactly

The measured presence of 132 level-32 `.btr` quads that
`LodBandLadder::for_game(Skyrim)` can never select looked like a gap: the
Skyrim ladder is `SKYRIM_ULTRA_REFINE_BU = [60_000, 90_000]` (two
boundaries → `coarsest_level() = LOD_LEVELS[2] = 16`), so the quadtree
descent starts at level 16 and level 32 is unreachable.

Checked against the shipped preset files in the game root rather than
guessing:

| INI key | `Ultra.ini` | code constant |
|---|---|---|
| `fBlockLevel0Distance` | 60000 | `SKYRIM_ULTRA_REFINE_BU[0]` = 60_000 ✓ |
| `fBlockLevel1Distance` | 90000 | `SKYRIM_ULTRA_REFINE_BU[1]` = 90_000 ✓ |
| `fBlockLevel2Distance` | **absent** | ladder correctly one band shorter ✓ |
| `fBlockMaximumDistance` | 250000 | `ULTRA_MAX_DISTANCE_BU` = 250_000 ✓ |

Bethesda's LOD generator emits the full quadtree including level 32;
**vanilla Skyrim's own Ultra preset never descends to it either**, because
the preset authors no `fBlockLevel2Distance`. The 132 level-32 quads are
dead data in the shipped game, not a streaming gap in this engine. The
`lod_bands.rs` comment ("Skyrim's ladder is one band shorter — it authors
no `fBlockLevel2Distance`") is now backed by the primary source. Recording
this as a *verified* guard so a future audit does not re-open it.

## SK-D6-02 (LOW) — the audit skill's object-LOD description is stale, and a live doc link points at a deleted constant

The Dimension 6 checklist describes `.bto` object LOD as *"level-4 quads
only, `OBJECT_LOD_RADIUS_CELLS = 16`"*. Neither half survives:

* `OBJECT_LOD_RADIUS_CELLS` **no longer exists** anywhere in the tree. The
  flat ring was replaced by the quadtree band ladder in `lod_bands.rs`,
  which for Skyrim streams levels **4 / 8 / 16** out to
  `max_cells = 250 000 / 4096 ≈ 61` cells.
* Skyrim's shipped `.bto` set is 744 level-4 / 248 level-8 / 86 level-16
  quads — a level-4-only reader would drop 334 of 1 078 quads (31 %).

The stale name also survives as a **rustdoc intra-doc link to a deleted
item**: `byroredux/src/cell_loader/placement_lod.rs:74` documents
`PLACEMENT_LOD_RADIUS_CELLS` as *"Mirrors
[`super::object_lod::OBJECT_LOD_RADIUS_CELLS`]"*. That target is gone, so
the link is broken and the stated relationship is false — the placement
scheme (FO3/FNV `.lod`) is still a flat 16-cell ring while the baked-`.bto`
scheme (Skyrim/FO4) moved to the ladder. One-line doc fix plus a correction
to `.claude/commands/audit-skyrim/SKILL.md`.

## VWD — forward scope, deliberately not re-filed

5 943 records across the installed plugin set carry
`FLAG_VISIBLE_WHEN_DISTANT` (Dim 4). `RecordHeader::is_visible_when_distant()`
parses it; nothing consumes it to cull a full-detail model under a `.bto`
stand-in. Per the skill (#1731 / #3307, premise re-measured 2026-08-29) this
is forward scope, not a regression, and the per-cell grid is level-4-only —
so a radius-decoupling design must keep full REFRs under level-4 quads and
never under level-8/16. **Not re-filed**, and the "effectively unbuildable"
framing is not repeated.

## Recorded, not a finding

`NiPSysBlock` accounts for 23 205 blocks in the Skyrim corpus. It is the
deliberate opaque catch-all for particle-modifier types with no structured
decoder (contrast `NiUnknown = 0`), so nothing is silently lost at the
dispatcher — but the particle domain, not this audit, owns whether those
23 205 blocks should be structured. Noted for `/audit-nif`.

---

# Dimension 7 — NIFAL Canonical Material Translation (Skyrim slice)

**No findings.** Every invariant the checklist names holds, and three of them
are now backed by corpus measurement rather than code reading.

## Structural invariants — verified

| Invariant | Evidence |
|---|---|
| `translate_material` is the single canonical boundary | one lowering site; `byroredux/src/render/static_meshes.rs:347` explicitly documents "no per-draw keyword scan / `classify_pbr` fallback" |
| The old per-draw `Material::classify_pbr` is **deleted** | no `fn classify_pbr` anywhere; only `classify_pbr_keyword` (`crates/core/src/ecs/components/material.rs:898`), called from `Material::resolve_pbr` (`:1165`) |
| `metalness` / `roughness` are plain resolved `f32` | `material.rs:598-599`, filled by `resolve_pbr` from a `f32::NAN` sentinel |
| Ordering: `resolve_pbr()` **before** `classify_glass_into_material` | `material_translate.rs:590` then `:591` — forced-glass roughness wins over the keyword default |
| `EmissiveSource` discriminator (#1280) | `Lighting` set at `dedicated_shader.rs:381` (BSLightingShaderProperty), `Effect` at `:523` (BSEffectShaderProperty); both gated on `emissive_contribution_is_authored` (#2591) |

## Corpus measurement (96 123 imported Skyrim meshes)

```
emissive_source: None 89 040 | Effect 4 695 | Lighting 2 388
is_pbr = 0        model_space_normals = 13 656    thin_glass = 0
metalness_override set on 88 910   (NaN 0, out-of-range 0)
roughness_override set on 88 910   (NaN 0, out-of-range 0)
Effect-source materials carrying a non-zero shader_type = 0
```

Four independent confirmations fall out of this:

1. **Skyrim emissive maps to `Lighting`, never `Effect`.** Zero of the 4 695
   `Effect`-sourced materials carry a non-zero `shader_type`, i.e. no
   `BSLightingShaderProperty` ever leaks into the BSEffect diffuse-tint
   conflation. The discriminator stays type-visible, exactly as #1280
   intended.
2. **`is_pbr == 0` across the entire vanilla mesh corpus** — an independent
   re-derivation of SK-D2-01's guard, this time at the `ImportedMaterial`
   boundary rather than from the shader-property name field. The Disney
   lobe is unreachable from two directions.
3. **No fabrication escapes the boundary.** The `f32::NAN` sentinel appears
   in 0 of 88 910 populated override slots, and 0 land outside `[0, 1]`, so
   `resolve_pbr`'s clamp is a no-op on Skyrim — nothing downstream ever sees
   an unresolved or out-of-contract scalar.
4. **`model_space_normals = 13 656` matches SLSF1 bit 12's occurrence count
   exactly** (Dimension 2). The flag is read once, at the boundary, and
   carried — no second decode.

The remaining 7 213 meshes (96 123 − 88 910) are the ones with no
translator-supplied scalars, which is precisely the population
`resolve_pbr`'s keyword classifier exists to fill. That split is the
designed division of labour, not a gap.

---

# Shader-Type Coverage Matrix

`ShaderTypeData` variants × parse / import / render, measured across 85 104
`BSLightingShaderProperty` blocks in `Skyrim - Meshes0/1.bsa`.

| Numeric type | nif.xml name | Occurrences | Variant | Parse | Import | Render |
|---|---|---|---|---|---|---|
| 0 | Default | 47 850 | `None` | ✓ | ✓ | ✓ |
| 1 | Environment Map | 6 726 | `EnvironmentMap` | ✓ | ✓ | ✓ (env scale) |
| 2 | Glow Shader | 1 395 | `None` (no trailing data) | ✓ | ✓ | ✓ (glow via SLSF2 bit 6) |
| 3 | Parallax | 11 | `None` | ✓ | ✓ | ✓ |
| 4 | Face Tint | 3 158 | `None` | ✓ | ✓ (detail TS4 + tint TS7 via `slot_to_role`) | ✓ |
| 5 | Skin Tint | 1 631 | `SkinTint` (Color3) | ✓ | ✓ | ✓ |
| 6 | Hair Tint | 10 817 | `HairTint` (Color3) | ✓ | ✓ | ✓ |
| 7 | Parallax Occ | **0** | `ParallaxOcc` | ✓ | ✓ | untestable — nif.xml itself marks it "Unimplemented" |
| 8 | Multitexture Landscape | 0 | `None` | ✓ | — | — |
| 9 | LOD Landscape | 0 | `None` | ✓ | — | — |
| 10 | Snow | 0 | `None` | ✓ | — | — |
| 11 | MultiLayer Parallax | 662 | `MultiLayerParallax` (5 f32) | ✓ | ✓ | ✓ |
| 12 | Tree Anim | 0 | `None` | ✓ | — | — |
| 13 | LOD Objects | 0 | `None` | ✓ | — | — |
| 14 | Sparkle Snow | 19 | `SparkleSnow` (Vector4) | ✓ | ✓ | ✓ |
| 15 | LOD Objects HD | 0 | `None` | ✓ | — | — |
| 16 | Eye Envmap | 3 251 | `EyeEnvmap` (7 f32) | ✓ | ✓ | ✓ |
| 17 | Cloud | 0 | `None` | ✓ | — | — |
| 18 | LOD Landscape Noise | 9 584 | `None` | ✓ | ✓ | ✓ (but see SK-D1-01 — flat normals) |
| 19 | Multitex Landscape LOD Blend | 0 | `None` | ✓ | — | — |
| 20 | FO4 Dismemberment | 0 (FO4-only) | `None` | ✓ | — | — |

Variant totals reconcile exactly (`None` 61 998 = 47 850 + 1 395 + 11 +
3 158 + 9 584). The FO76 `BSShaderType155` numbering
(`parse_shader_type_data_fo76`) is reachable only on `bsver == 155` and
cannot cross-contaminate this table.

`BSEffectShaderProperty`: 8 116 blocks, all fields parse; `env_map_min_lod`
is **0 on every one** (nif.xml range `0:16`), so that field is unexercised
by vanilla content.

# Cell-Load Regression Status

| Check | Result |
|---|---|
| TES5 cells parse through the unified `esm/cell/` walker | ✓ (`parse_real_skyrim_esm` finds `SolitudeWinkingSkeever`) |
| Compressed records decompress | ✓ — 106 999 compressed records across the 10 installed plugins, 44 153 in `Skyrim.esm` alone |
| DLC partial-CELL merge preserves base landscape | ✓ (`merge_real_dawnguard_partial_cells_preserves_skyrim_landscape`) |
| Repeatable `--master` FormID remap | ✓ — `Dawnguard.esm` MAST = `[Skyrim.esm, Update.esm]`, confirmed by measurement |
| `.STRINGS` per-plugin loader | ✓ — invoked inside the per-plugin loop; all 10 installed plugins are `localized` |
| ESL / light-master decode | ✓ — 3/3 `.esl` flagged, 7/7 `.esm` not; `_ResourcePack.esl` carries 3 masters |
| Deleted (0x20) tombstones | **partial** — placements ✓, base records ✗ (SK-D4-01, 9 records) |
| Whiterun BanneredMare 6-NPC equip chain | ✓ on real `Skyrim.esm` |
| Creature-race skin retention (#3408) | ✓ |
| Helmet FaceGen hide mask (#3409) | ✓ |
| Whiterun entity count + FPS vs ROADMAP Bench-of-record | **not measured** — needs a live engine run, which `/audit-runtime` owns this cycle and this audit is barred from launching |

# Coverage Note — `crates/facegen`

This suite gives `crates/facegen` no other owner, so it was audited here in
full (Dimension 3). All four public surfaces were traced to their consumers
workspace-wide: `EgmFile` / `apply_morphs` are consumed by the
Oblivion/FO3NV runtime-recipe path, `half_to_f32` by a bit-parity test, and
**`EgtFile` / `EgtMorph` / `TriHeader` have no consumer at all**
(SK-D3-02). Skyrim itself is on the pre-baked FaceGen track and needs none
of them; the crate's Skyrim-relevant surface is the 3 158 pre-baked head
NIFs, which parse cleanly and render flat-shaded (SK-D3-01).

# Suggested Next Step

`/audit-publish docs/audits/AUDIT_SKYRIM_2026-08-30.md`
— label every finding `game:skyrim` + `legacy-compat`, plus its domain label
(`nif` for SK-D1-*, `renderer`/`material` for SK-D2-*, `npc`/`facegen` for
SK-D3-*, `esm` for SK-D4-01, `bsa` for SK-D5-01, `lod` for SK-D6-02).
SK-D2-01 and SK-D6-01 are **green guards**, not defects — publish them as
documentation of a verified invariant or not at all.
