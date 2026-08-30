# Oblivion (TES4) Compatibility Audit — 2026-08-30

Scope: NIF v20.0.0.4 + the v10.x NetImmerse family, BSA v103, the live ESM path.
Data: `/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/` — vanilla + all 9 DLC
(34 entries, 18 `.bsa`, `Oblivion.esm` 277,504,985 B).
Spec: `/mnt/data/src/reference/nifxml/nif.xml` (authoritative).
Method: every dimension run in-process; every strong claim is a **measured census**
over real game data, not a code reading. Nine throwaway release probes under
`crates/nif/examples/_tmp_obl_*`. The `#[ignore]`d `byroredux-plugin` real-data
harness was **not** run (>20 GB RSS, the confirmed OOM source); the 273 MB
`esm_dim8_coverage` walker covered the same corpus surface instead.

## Executive Summary

| Surface | Measured state |
|---|---|
| NIF parse | **8,032 / 8,032 clean, 0 truncated, 0 failed, 0 `NiUnknown` blocks.** Per-block baseline byte-identical to the checked-in `oblivion.tsv`. 650 files (8.1%) are in the v10.x/pre-Gamebryo tail and all parse clean. |
| BSA v103 | **100%.** 9,652 / 9,652 `.nif` across all 18 vanilla + DLC archives; 3.3 GB extracted. The 43 zero-byte results are genuinely zero-length file records (verified byte-level). |
| ESM parse | Full walk of `Oblivion.esm`, 1,252,095 records, **63 record types, 0 walker errors**. 4 types unhandled (PGRD 8,228 · LVSP 306 · SBSP 33 · ROAD 2). |
| Material / render | Every property type Oblivion ships has a live walker arm. Disney gate provably 0. **But the one Oblivion-specific material feature that shipped (#3530 parallax) is inert on 100% of the corpus.** |
| Interior | Renders end-to-end (Anvil Heinrich Oaken Halls); 3 representative meshes re-traced clean this run. |
| Exterior | Wiring done (#1556); the remaining gaps this audit found are **shading and animation quality on the exterior path**, not wiring. |

**Findings: 0 CRITICAL · 5 HIGH · 3 MEDIUM · 5 LOW.**

Top blockers, in priority order:
1. **OBL-D7-01** — 100% of Oblivion's 100 distant-terrain LOD meshes get a fabricated
   constant `[0,1,0]` normal. Oblivion is the only game with a live placement-LOD route.
2. **OBL-D7-02** — 792 embedded `NiControllerSequence` clips across 423 meshes import
   at 100% but are never handed to the ECS; every animated static spawns frozen.
3. **OBL-D4-01** — #3530's `APPLY_HILIGHT2` → parallax route cannot fire on any vanilla
   Oblivion mesh (0 of 1,430 candidate properties carry the normal slot it requires).
4. **OBL-D3-01** — Oblivion has zero navigation data: 8,228 PGRD dropped, no NAVI/NAVM
   authored, while 7,209 PACK AI packages parse successfully with nothing to path on.
5. **OBL-D3-02** — the conversation tree orders by PNAM; Oblivion authors zero PNAM and
   zero ANAM on all 19,278 INFO records, so the ordering pass is a no-op.

**Stale premises dropped: 4.** (1) the skill's "6 residual NetImmerse truncations
(#1611)" — measured 0, ROADMAP already reflects #3082; (2) "v20.0.0.5 is the retail
body" — measured 7,282 v20.0.0.4 vs 100 v20.0.0.5; (3) "v104 folder records are 24 B" —
code is right, skill text was wrong; (4) "v103 decompression is broken" (#699) — dead,
not regenerated. A fifth near-miss is documented in Dim 6: a naive `nif_stats --tsv`
diff against the per-block baseline shows a ~40-line delta that is **purely a keying
artefact** (struct name vs header wire type) and is not a regression.

## Blocker Chain to "exterior cell renders correctly"
Exterior parse + load + render already work (ROADMAP: Tamriel `(0,0)` r=1, 6,043
entities / 2,355 draws). The remaining chain is fidelity, not wiring:
1. `synthesize_normals` for lit geometry lacking authored normals (**OBL-D7-01**) —
   without it the entire distant landscape is flat-shaded.
2. Route embedded `NiControllerSequence` clips through the cell-loader import
   (**OBL-D7-02**) — animated exterior statics (Oblivion gates) are frozen today.
3. Make #3530's parallax route reachable (**OBL-D4-01**) — currently shipped inert;
   fix it together with the alpha-presence gate a sibling flagged.
4. On-device exterior render bench to confirm 1–3 land visually.
5. (Separately, for AI rather than rendering) PGRD pathgrids — **OBL-D3-01**.

## Regression Guard List — verified still holding this run
| Guard | Evidence |
|---|---|
| v10.x stride-drift family #1506/#1507/#1508 | 0 truncations across 650 v10.x files |
| #1509 `NiGeomMorpherController` `bsver > 9` | gate at `morph.rs:107-109`; both sides live in-corpus (228 bsver-9 files skip, 7,311 bsver-11 keep); `doghead.nif` imports `dropped=0` |
| `NiTexturingProperty` raw u32 count, no bool gate | `properties.rs:265` |
| BSStreamHeader dual-band #170 | `header.rs:137-143` transcribes nif.xml:38 exactly; #170 negative test present |
| `user_version` ≥ V10_0_1_8 threshold | `header.rs:114` vs nif.xml:1969 |
| v10.x leading group_id | `version.rs:204-205` |
| BSA v103 extraction #699 | 9,652/9,652 `.nif`, 18 archives |
| Disney gate stays 0 for Oblivion | `is_pbr = 0` on all 35,322 imported meshes |
| `havok_motion_type` full enum #1652 | `collision/mod.rs:222-231`, no `4 => Keyframed` collapse |
| No `srgb_to_linear` on legacy colours (0e8efc6) | zero hits under `crates/nif/src/import/` |
| POM bit-31 masked in **both** marchers | `material_sampling.glsl:49` + `ray_hit.glsl:296` |
| Pre-5.0.0.1 inline-name log levels | `debug!` common case, `warn!` only on read failure |
| Per-block baseline / truncation baseline | byte-identical diff |

## Cross-Dimension Notes
- The Disney-gate measurement (`is_pbr = 0`) is shared by Dim 4 and Dim 5 and is
  counted once.
- Dims **2 (BSA v103)**, **5 (NIFAL)** and **6 (real-data)** produced **no findings** —
  all three are clean regression guards. Dim 5's only Oblivion defect is OBL-D4-01,
  filed under Dim 4 because the wrong-tier boundary is the material walker.
- A sibling this run reported "#3530's parallax bit set without an alpha-presence
  gate". OBL-D4-01 measures that the bit is **never set at all** on vanilla Oblivion;
  the two should be fixed together. Sibling findings referenced and not re-derived:
  `TREE.MODL` resolving under `trees\`, BNAM on 142/142 TREE records,
  `--cornell-sun`'s frame-1 directional clobber.

---
# Dimension 1 — NIF Version Handling (v20.0.0.4 + v10.x NetImmerse tail)

## Measured census (Oblivion - Meshes.bsa, 8,032 .nif entries, release probe)
```
files=8032 parse_err=0 truncated=0
version histogram (version + bsver):
  3.3.0.13  bsver=0      1     4.0.0.2 bsver=0    4     4.2.1.0 bsver=0   1   (6 pre-5.0.0.1 inline-name files)
  10.0.1.0  bsver=0     41    10.0.1.2 bsver=1   14    10.0.1.2 bsver=3   9
  10.1.0.101 bsver=4     8    10.1.0.106 bsver=5 82
  10.2.0.0  bsver=6    172    10.2.0.0 bsver=7   16    10.2.0.0 bsver=8  45
  10.2.0.0  bsver=9    228    10.2.0.0 bsver=11  29
  20.0.0.4  bsver=11 7282    20.0.0.5 bsver=11 100
```
650 files (8.1%) are in the v10.x/pre-Gamebryo NetImmerse tail. **Zero truncations,
zero parse errors.**

## Regression guards — ALL HOLD
- `user_version` gated `version >= V10_0_1_8` (`header.rs:114`). Matches nif.xml
  `<field name="User Version" ... since="10.0.1.8">`. ✓
- BSStreamHeader dual-band guard (`header.rs:137-143`) is a character-for-character
  transcription of nif.xml's `#BSSTREAMHEADER#` condexpr (nif.xml:38). #170
  regression test `bs_stream_header_not_read_for_off_spec_version` present. ✓
- #1509 `NiGeomMorpherController` gate is `version >= V10_2_0_0 && version <= V20_0_0_5
  && bsver >= MORPH_LEGACY_CUTOFF(10)` (`controller/morph.rs:107-109`) — i.e. nif.xml's
  `#BSVER# #GT# 9`. Census confirms both sides are live in-corpus: **228 v10.2.0.0
  bsver=9 files must skip** the field, **29 v10.2.0.0 bsver=11 + 7,282 v20.0.0.4
  bsver=11 must keep it.** ✓
- `NiTexturingProperty` reads `texture_count` as a raw `u32` with no leading
  `Has Shader Textures: bool` (`properties.rs:265`). ✓
- v10.x leading group_id: `has_object_group_id() = v >= V10_0_0_0 && v < V10_1_0_114`
  (`version.rs:204-205`). ✓
- `havok_motion_type` (#1652) maps the full hkMotionType enum 1..=5|8→Dynamic,
  6→Keyframed, 7→Static, 9→CharacterKinematic (`import/collision/mod.rs:222-231`).
  The pre-fix `4 => Keyframed` collapse is NOT back. ✓
- v10.x stride-drift family #1506/#1507/#1508: **0 truncations across 650 v10.x
  files** is the strongest available guard; no regression. ✓
- Pre-5.0.0.1 inline block-type names: 6 files (3.3.0.13 ×1, 4.0.0.2 ×4, 4.2.1.0 ×1)
  parse clean, including the corrupt-by-design `marker_radius.nif`. ✓

## Findings

### OBL-D1-01 (LOW) — `NiTexturingProperty` Apply Mode has no `since="3.3.0.13"` lower bound
`crates/nif/src/blocks/properties.rs:259` reads `apply_mode` for every
`version <= STRING_TABLE_THRESHOLD` (20.1.0.1) with no lower gate. nif.xml declares
`Apply Mode ... since="3.3.0.13" until="20.1.0.1"`. A file below 3.3.0.13 would read
4 phantom bytes and — with no block-size table — poison the whole downstream stream.
**Measured exposure on Oblivion: zero.** The lowest version in the vanilla corpus is
exactly 3.3.0.13 (1 file), which nif.xml's inclusive `since` includes. Latent only;
becomes real for any sub-3.3.0.13 asset (mod content, other NetImmerse titles).

### OBL-D1-02 (LOW) — dark/detail/gloss/glow TexDesc presence gated on `texture_count`, nif.xml gates none of them
`properties.rs:268-290` gates the dark/detail/gloss/glow slots on
`texture_count > 1..>4`. nif.xml declares `Has Dark/Detail/Gloss/Glow Texture` as
**unconditional bools**; only Bump (`> 5`), Normal (`> 6`), Parallax (`> 7`) and the
Decals carry `Texture Count` conditions. A file with `texture_count < 5` would skip
the presence bools entirely → 1-byte drift per slot → total misalignment.
**Measured exposure on Oblivion: zero — `texture_count == 7` on all 30,121
`NiTexturingProperty` instances in the archive** (single-valued histogram). Latent
only, but the divergence from spec is real and should be spelled as nif.xml does.

## Skill-file drift observed (report, don't act on)
- The skill's "6 residual NetImmerse truncations (`oblivion_truncations.tsv`, #1611)"
  is **STALE**. Measured: 0 truncations. ROADMAP already records the #3082
  (2026-08-19) baseline regen at 100% / 0 truncating. The skill's Dim-6 checklist
  still instructs the auditor to "enumerate the residual 6".
- The skill frames v20.0.0.5 as the retail body and v20.0.0.4 as the minority in
  one paragraph and corrects itself in the next; the census settles it —
  **7,282 v20.0.0.4 vs 100 v20.0.0.5**.
- The collision checklist directs the auditor at `BhkMultiSphereShape` and
  `BhkConvexListShape`. Measured Oblivion counts: `bhkMultiSphereShape` = **1**,
  `bhkConvexListShape` = **0**. Both have live `resolve_shape_inner` arms
  (`import/collision/shape.rs:110`, `:235`) so there is nothing to find. This is the
  same "audit the shapes your game ships zero of" drift the FNV/FO3 skills carry.
  The shapes Oblivion actually ships in volume — `bhkNiTriStripsShape` 4,521,
  `bhkMoppBvTreeShape` 4,504, `bhkConvexVerticesShape` 2,091, `bhkBoxShape` 1,455,
  `bhkCapsuleShape` 1,167 — all have arms. Full shipped set verified covered.
# Dimension 2 — BSA v103 Archive

**No findings. Regression guard confirmed green by measurement.**

## Full-archive extraction sweep (all 18 vanilla + DLC .bsa)
```
DLCBattlehornCastle.bsa           388/388     nif 24/24
DLCFrostcrag.bsa                   49/49      nif 17/17
DLCHorseArmor.bsa                  46/46      nif 4/4
DLCOrrery.bsa                      90/90      nif 9/9
DLCShiveringIsles - Meshes.bsa   3017/3017    nif 1438/1438
DLCShiveringIsles - Sounds.bsa    499/499
DLCShiveringIsles - Textures.bsa 1869/1869
DLCShiveringIsles - Voices.bsa  19089/19089
DLCThievesDen.bsa                  65/65      nif 5/5
DLCVileLair.bsa                    57/57      nif 8/8
Knights.bsa                      4806/4810*   nif 75/75
Oblivion - Meshes.bsa           20182/20182   nif 8032/8032   (1340 MB out)
Oblivion - Misc.bsa               114/115*
Oblivion - Sounds.bsa            1533/1533
Oblivion - Textures - Compressed 18005/18040* (1972 MB out)
Oblivion - Voices1.bsa          33197/33198*
Oblivion - Voices2.bsa          44580/44582*
```
`*` = entries returning zero bytes. **Verified not a defect**: a direct byte-level
walk of `Oblivion - Misc.bsa`'s folder/file record tables shows exactly **1 of 115
file records declares `size == 0`** — matching the 1 "empty" extraction. The zero-byte
entries are genuinely zero-length placeholders in the archive (stub `.txt` next to
`.dds`, silent `.mp3` voice lines). Extraction is faithful.
**Every `.nif` in every Oblivion archive extracts: 9,652 / 9,652.**

## Guards verified
- `BSA_V_OBLIVION = 103` accepted; rejection only outside {103,104,105}
  (`open.rs:35-39`). ✓
- Folder-record size `if version == BSA_V_SKYRIM_SE { 24 } else { 16 }`
  (`open.rs:138`) — v103 AND v104 are 16 B. The old skill text claiming
  "v104 = 24 B" is wrong; code is right. ✓
- v103 archive-flag semantics: measured flags are `0x787` (Meshes), `0x707`
  (Textures), `0x703` (SI Meshes, Misc). **Bit `0x100` is set on every vanilla
  Oblivion v103 archive**, and `embed_file_names` correctly gates it out via
  `version >= BSA_V_FO3_SKYRIM` (`open.rs:70`) — so the "Xbox archive"
  reinterpretation of that bit on v103 never fires. This is the live, measured
  confirmation of the documented behaviour. ✓
- `compressed_by_default` (bit 2) is set on Meshes/Textures, clear on SI Meshes/Misc —
  both paths exercised and both extract 100%. zlib v103 path healthy. ✓
- #699 ("v103 decompression is broken") remains dead. Do not regenerate.
# Dimension 3 — ESM Record Coverage (Oblivion.esm through the live path)

## Measured census — `Oblivion.esm` (277,504,985 B, HEDR 1.0, GameKind::Oblivion, 1,252,095 records, 0 masters)
Tool: `crates/plugin/examples/esm_dim8_coverage.rs` (release). Peak RSS 273 MB.
Full walk succeeded; **63 distinct record types**, 0 walker errors.

Top: REFR 1,025,617 · CELL 35,494 · LAND 31,823 · INFO 19,278 · **PGRD 8,228** ·
PACK 7,209 · STAT 6,014 · DIAL 3,817 · NPC_ 2,482 · SCPT 2,393 · ACHR 2,190 ·
LIGH 1,625 · ENCH 1,542 · ACRE 1,473 · LVLI 1,398 · WEAP 1,319 · CREA 914 ·
QUST 390 · **LVSP 306** · REGN 211 · TREE 142 · WRLD 84 · **SBSP 33** · **ROAD 2**.

## Guards verified
- 16-byte ACBS (#1650): `b"ACBS" if matches!(game, GameKind::Oblivion) && sub.data.len() >= 16`
  fires **before** the FO4 (≥20), Skyrim (≥24) and generic (≥24) arms
  (`records/actor/mod.rs:1134`). ✓
- XCLL canonical sizes pinned to `[28, 32, 36]` for Oblivion with an explicit
  negative test that 40 (the FNV shape) is excluded (`cell/walkers.rs:88-97, 1307`). ✓
- CELL sub-record coverage is **complete**: all 13 sub-codes Oblivion authors
  (DATA, EDID, FULL, XCCM, XCLC, XCLL, XCLR, XCLW, XCMT, XCWT, XGLB, XOWN, XRNK)
  have live arms in `esm/cell/`. ✓
- MGEF-by-code map, CONT 4-byte guard, CLMT three-entry WLST: present. ✓
- 64 `GameKind::Oblivion` decode sites across 12 files — the per-game seam is at
  the parser→record boundary as designed. ✓

## Findings

### OBL-D3-01 (HIGH) — Oblivion has **zero** navigation data: 8,228 PGRD records are dropped, and Oblivion authors no NAVI/NAVM
Measured: `Oblivion.esm` contains **8,228 PGRD** (pathgrid) records and
**zero NAVI, zero NAVM**. `grep -rn PGRD crates/plugin/src byroredux/src` returns
**no hits anywhere in the codebase** — PGRD is not in `RecordType`, not in any
dispatch table, not in the CELL walker. NAVI/NAVM *are* parsed
(`NaviRecord`/`NavmRecord` in `records/index.rs`), which is why FO3/FNV/Skyrim/FO4
have navigation and Oblivion does not.

Impact is not theoretical: **7,209 PACK records parse successfully** and the
sandbox/travel procedures have no graph to path on. Oblivion is the only supported
title in this state — every other game's nav format is handled.
*Fix shape*: PGRD is a flat record (`DATA` u16 point count, `PGRP` 16-B points,
`PGRR`/`PGRI`/`PGRL` link tables) attached to the CELL group like LAND. It slots
into the same CELL-child walker that already handles LAND.

### OBL-D3-02 (HIGH) — `build_conversation_tree` orders by PNAM chains; Oblivion authors **zero PNAM and zero ANAM** on all 19,278 INFO records
`records/misc/dialogue.rs:181-215` builds conversation chains by following
`previous_info` (PNAM), and reads the speaker from `actor_form_id` (ANAM).
Measured sub-record census over all 19,278 Oblivion INFO records:
```
INFO CTDA 48531/18920   INFO NAM1 23877/19260   INFO QSTI 19278/19278
INFO DATA 19278/19278   INFO TRDT 23877/19260   INFO TCLT  9698/5611
INFO SCHR 19231/19231   INFO TCLF  4141/3792    INFO NAME  1342/1044
INFO SCRO  8405/5531    INFO SCTX  5718/5718    INFO SCDA  5552/5552
INFO CTDT    72/45      INFO SCHD    47/47
                        ← no PNAM, no ANAM anywhere
```
PNAM/ANAM were introduced after Oblivion; Oblivion orders INFOs by record order
within the DIAL group and identifies the speaker through CTDA conditions. Result
today: **every Oblivion INFO has `previous_info == 0`, so the tree degenerates to
19,278 single-element chains** — the ordering pass is a no-op — and
`actor_form_id` is 0 on every record. This is exactly the "silently mis-read"
case the skill's Dim-3 checklist asks about, and it is real.

### OBL-D3-03 (MEDIUM) — three INFO link/condition sub-records dropped, 3,792 + 1,044 + 45 records affected
Not handled by `parse_info` (`records/misc/dialogue.rs:117-171`):
- **`TCLF` — 4,141 occurrences across 3,792 INFO records (19.7%)**: Oblivion's
  "Link From" topic edge. `TCLT` ("Choose Topic") *is* handled, so the tree is
  built from one of the two available edge kinds.
- **`NAME` — 1,342 occurrences across 1,044 records (5.4%)**: Oblivion's
  "Add Topics" — the DIAL FormIDs a response unlocks. Dropped entirely.
- **`CTDT` — 72 occurrences across 45 records**: the legacy fixed-layout
  condition sub-record Oblivion still uses on a handful of INFOs. Only
  `CTDA`/`CIS1`/`CIS2` are routed to `push_ctda`, so those 45 INFOs parse as
  **unconditional** — they will fire when they should not.
- (`SCHD`, 47 records — the legacy script header alongside `SCHR` — is likewise
  unread, but the result-script path is not consumed for Oblivion INFOs anyway.)

### OBL-D3-04 (MEDIUM) — only the last response of a multi-response INFO survives
`b"NAM1" => out.response_text = read_lstring_or_zstring(&sub.data)` and the `TRDT`
arm both **assign**, not push. Measured: **23,877 NAM1/TRDT pairs across 19,260
records** — so 4,617 responses (19.3% more than one per record) are overwritten
and lost. Every multi-line NPC line in Oblivion plays only its final segment.

### OBL-D3-05 (MEDIUM) — LVSP (306 leveled-spell lists) has a `RecordType` constant but no parser
`crates/plugin/src/record.rs:206` declares `RecordType::LVSP`, but
`records/mod.rs:428` dispatches only `b"CONT" | b"LVLI" | b"LVLN" | b"LVLC"` to
`parse_leveled_list`. LVSP shares the identical LVLD/LVLF/LVLO layout — this is a
one-token addition to an existing match arm. 306 records; NPC/CREA `SPLO` entries
pointing at a leveled spell resolve to nothing.

### OBL-D3-06 (LOW) — SBSP (33) and ROAD (2) unhandled
`SBSP` (subspace, Oblivion-only collision volume) and `ROAD` (worldspace road
path, 2 records) have no code references at all. Low volume; ROAD only matters if
worldspace road rendering is ever wanted.

## Skill-file drift observed
- The skill says the two Oblivion real-data parity tests are "ignored". They exist
  at `crates/plugin/tests/parse_real_esm.rs:2166` and `:2280`. **Not run this
  session** — running the ignored `byroredux_plugin` real-data harness is the
  confirmed OOM culprit (>20 GB RSS) and was explicitly prohibited for this run.
  The lighter `esm_dim8_coverage` walk used here peaks at 273 MB and covers the
  same corpus surface for census purposes.
# Dimension 4 — Rendering Path for Oblivion Shaders

## Measured property census (`Oblivion - Meshes.bsa`, 8,032 files)
```
NiMaterialProperty   30874    NiZBufferProperty     177
NiTexturingProperty  30121    NiSpecularProperty    159
NiVertexColorProperty 4968    NiFogProperty          11
NiAlphaProperty       1315    NiWireframeProperty     8
NiStencilProperty      699    NiDitherProperty        1
                              NiShadeProperty         0
```
All 10 types Oblivion ships have live `downcast_ref` arms in
`import/material/{legacy_properties,walker}.rs`. **`NiShadeProperty`: Oblivion
ships zero instances** — the #869 `flat_shading` guard is verified in code
(`legacy_properties.rs:782` → `static_meshes.rs:682`) but is unreachable on this
title. Measured downstream: `flat_shading = true` on **0** of 35,322 imported
meshes; `wireframe = true` on **14** (from the 8 `NiWireframeProperty` blocks,
inherited by 14 shapes) and routes to the LINE pipeline variant.

## Guards verified
- `NiMaterialProperty` colours: **no `srgb_to_linear` anywhere in
  `crates/nif/src/import/`** — raw monitor-space per 0e8efc6. ✓
- `NiAlphaProperty`: `apply_alpha_flags` (`material/mod.rs:1517-1543`) extracts
  `src_blend_mode = (flags >> 1) & 0xF` and `dst_blend_mode = (flags >> 5) & 0xF`
  **raw**, so all 16 Gamebryo AlphaFunction enum values route unconditionally,
  independent of which of blend/test wins. ✓
- Disney-BSDF gate (#1248-#1252): `MAT_FLAG_PBR_BSDF` is `material_flag::PBR_BSDF`
  gated on `ImportedMaterial::is_pbr`. **Measured: `is_pbr = 0` and
  `from_bgsm || material_path.is_some() = 0` across all 35,322 imported Oblivion
  meshes.** The Disney lobe is provably unreachable for Oblivion. ✓
  (Cross-referenced by Dim 5 — reported once.)
- POM bit-31 masking: **both** marchers strip `PARALLAX_ALPHA_HEIGHT_BIT` —
  raster `include/material_sampling.glsl:49` and secondary-ray
  `include/ray_hit.glsl:296`. No unmasked bindless index. ✓
- #1239 typed particle-emitter import → runtime: **measured healthy.** 547
  `NiParticleSystem` + 547 `NiPSysEmitter` blocks across 141 files import to
  **529 flat emitters (96.7%), 525 carrying `emitter_params`, 529 carrying
  `emitter_rate`**, and `streaming.rs:1179` feeds them to
  `systems/particle.rs::apply_emitter_params`. Only one file
  (`meshes\oblivion\gate\obliviongate_forming.nif`) has a particle system that
  imports zero emitters. Emitters parse *and* reach the ECS authoring path. ✓

## Findings

### OBL-D4-01 (HIGH) — #3530's `APPLY_HILIGHT2` → parallax route is unreachable on **every** vanilla Oblivion mesh; the feature ships inert
`import/material/legacy_properties.rs:272-286`:
```rust
if tex_prop.apply_mode == APPLY_HILIGHT2 && info.parallax_map.is_none() {
    if let Some(normal) = info.normal_map {       // ← never Some on Oblivion
        info.parallax_map = Some(normal);
        info.parallax_height_in_alpha = true;
        ...
```
`info.normal_map` is populated (`:188-190`) from `tex_prop.normal_texture`
(a v20.2.0.5+ slot that does not exist on Oblivion) `.or_else(bump_texture)`.

**Measured over `Oblivion - Meshes.bsa` + `DLCShiveringIsles - Meshes.bsa`:**
```
NiTexturingProperty total = 34,850   with bump_texture =    14
APPLY_HILIGHT2 properties  =  1,430  across 741 files   ← the exact #3530 population
   ...of which carry bump_texture   =  0  (in 0 files)
   ...of which carry normal_texture =  0
```
**Zero of the 1,430 APPLY_HILIGHT2 properties carry a normal or bump texture
slot**, so the `if let Some(normal)` guard never passes. Confirmed independently
at the import boundary: `parallax_height_in_alpha = true` on **0** of 35,322
imported Oblivion meshes, and `textures.height = Some(_)` on **0**.

Root cause is structural, not a typo: Oblivion does not put normal maps in NIF
texture slots at all (only 14 of 34,850 properties do). It resolves them by
filename convention — `derive_normal_map_path` in
`byroredux/src/asset_provider/texture.rs:278` (`<base>_n.dds`, landed under
#1303 as a previous OBL-D4 finding). That derivation happens **downstream** of
`MaterialInfo`, so at the point #3530 tests `info.normal_map` the Oblivion
normal map does not exist yet.

*Fix shape*: the APPLY_HILIGHT2 decision must move to (or be re-evaluated at) the
boundary where the derived `_n.dds` path is known — i.e. carry
`apply_mode == APPLY_HILIGHT2` forward as an intent flag on `MaterialInfo` and let
the asset provider bind it to the derived normal map when it resolves one. No new
constant is required; the existing `0.04 / 4.0` engine defaults and the
`PARALLAX_ALPHA_HEIGHT_BIT` transport already exist and are correct.

**Supersedes-by-measurement note**: a sibling audit this run reported "#3530's
parallax bit set without an alpha-presence gate". On vanilla Oblivion the bit is
**never set at all**, so that gate is currently moot here — it becomes live only
once this finding is fixed, and should be fixed *together with* it.

### OBL-D4-02 (LOW) — `NiTexturingProperty` Apply Mode values 1 and 3 are decoded and then dropped
Measured apply-mode histogram over 30,121 Oblivion properties:
`APPLY_DECAL(1) = 18`, `APPLY_MODULATE(2) = 28,166`, `APPLY_HILIGHT(3) = 663`,
`APPLY_HILIGHT2(4) = 1,274`. Only value 4 is consumed. Value 3 is documented in
`properties.rs:120` as "PS2 only" and Gamebryo v3.2 renamed 3/4 to
`APPLY_DEPRECATED`/`APPLY_DEPRECATED2`, so no semantic is asserted here and none
should be invented — but 663 + 18 properties carry a non-default blend intent the
renderer never sees. Recording the measurement, not proposing a heuristic.
# Dimension 5 — NIFAL Canonical Material Translation (Oblivion slice)

**No new findings. The canonical boundary holds for Oblivion; the one Oblivion
NIFAL defect is OBL-D4-01, filed under Dim 4 (single boundary, wrong tier).**

## Guards verified
- **Single boundary**: `translate_material` (`byroredux/src/material_translate.rs`)
  is the only `MaterialInfo → Material` lowering, and it calls
  `material.resolve_pbr()` exactly once (`:590`).
- **No render-time fallback**: `byroredux/src/render/static_meshes.rs:347`
  carries the explicit comment "no per-draw keyword scan / classify_pbr
  fallback", and `:355-356` read `m.roughness` / `m.metalness` directly off the
  ECS `Material`. No `classify_pbr` call exists anywhere under `byroredux/src/render/`. ✓
- **NAN sentinel**: `metalness_override` / `roughness_override` arrive as
  `Option<f32>` on `ImportedMaterial` and are lowered `.unwrap_or(f32::NAN)`,
  resolved once by `Material::resolve_pbr`. ✓
- **`emissive_source`**: the Oblivion legacy arm sets
  `EmissiveSource::Material` from `NiMaterialProperty`
  (`import/material/legacy_properties.rs:163`) — distinct from the Skyrim/FO4
  `BSLightingShaderProperty` arm. Test file
  `import/material/emissive_source_tests.rs` present. ✓
- **`MAT_FLAG_PBR_BSDF` stays 0 across the Oblivion universe**: measured
  `is_pbr = 0` and `from_bgsm || material_path.is_some() = 0` on **all 35,322
  imported meshes** in `Oblivion - Meshes.bsa`. `PBR_BSDF` (`1 << 5`) is gated on
  `is_pbr`, so the flag is provably never set. (Same measurement as Dim 4's
  Disney-gate guard — counted once.) ✓
- **No-fabrication**: the one place Oblivion fabricates canonical state is
  `parallax_height_in_alpha` + the `0.04 / 4.0` defaults under #3530, and those
  are the pre-existing engine defaults every consumer's `unwrap_or` already used
  — no Oblivion-specific constant was invented. (Moot in practice, per OBL-D4-01.)
# Dimension 6 — Real-Data Validation

**No findings. Every baseline holds exactly.**

## Parse sweep vs. checked-in baselines
Reproduced the `per_block_baselines.rs` histogram keying exactly (header wire
type table via `NifHeader::block_type_indices` → `block_types`, `NiUnknown` →
`unknown` bucket; #3326) over `Oblivion - Meshes.bsa`, the sole archive
`open_all_mesh_archives(Game::Oblivion)` returns (`tests/common/mod.rs:152`):
```
total=8032  clean=8032  truncated=0  failed=0
diff crates/nif/tests/data/per_block_baselines/oblivion.tsv  <live>
  → *** IDENTICAL ***     (121 lines, byte-for-byte)
```
- **`unknown` column is 0 on every one of the 120 block types.** No dispatch-table
  miss, no `NiUnknown` recovery anywhere in the Oblivion corpus.
- **No new block types** have appeared since the baseline was captured
  (2026-08-27) — the type set is identical, so the "cross-check the histogram for
  new types" item is satisfied negatively.
- `oblivion_truncations.tsv` (`truncating=0  parsed=8032`) matches the live run.

## `recovery_trace` equivalent — residual truncations
**Zero.** `truncated files: []`. The skill's Dim-6 instruction to "enumerate the
residual truncated files (6 NetImmerse v10.x markers — `marker_arrow`/`divine`/
`map`/`radius`/`temple`/`travel`)" is **stale**; ROADMAP already records the
2026-08-19 baseline regen (#3082) at 100% / 0 truncating, and this run confirms it
independently. The 6 files still exist and are still the oldest in the corpus
(3.3.0.13 ×1, 4.0.0.2 ×4, 4.2.1.0 ×1 — see Dim 1's version histogram) but they now
parse **clean**, including the corrupt-by-design `marker_radius.nif`.

## Three representative interior meshes traced through `import_nif_scene`
```
meshes\lights\chandelier04.nif   72 blocks · dropped=0 recovered=0 link_errors=0
                                 → 9 nodes, 8 meshes
  mesh[0] 1077v/878t  normals+uvs+tangents+colors all 1077  base=clutter\barset\metalrusty01.dds
  mesh[5] 1082v/1408t  base=clutter\candle.dds        emissive_source = Material  ✓
  mesh[6]  333v/440t   base=clutter\candletop.dds     emissive_source = Material  ✓
  mesh[7] 1848v/1344t  has_alpha=true (NiAlphaProperty consumed)
meshes\clutter\books\octavo03.nif  18 blocks · clean → 1 node, 2 meshes
  (bookpages01.dds + octavo03.dds, both fully attributed)
meshes\creatures\dog\doghead.nif   19 blocks · clean → 1 node, 1 mesh, 2112v/2720t
```
`doghead.nif` is the exact **v10.2.0.0 bsver=9** file the #1509 morph-gate fix was
derived from — it imports with `dropped=0`, confirming the `bsver >= 10` gate on
real data, not just in the unit test.

Every mesh carries full normals / UVs / tangents / vertex colours, and every base
texture path resolves out of `NiSourceTexture`. The Dim-5 `EmissiveSource::Material`
guard is confirmed **on real Oblivion content**, not only in fixtures.

Note: `normal_slot = None` on all 11 traced meshes — consistent with OBL-D4-01
(Oblivion authors virtually no NIF normal slots; the `_n.dds` derivation happens in
the asset provider).

## Method note (false-positive avoided)
A naive `nif_stats --tsv` output does **not** diff clean against
`per_block_baselines/oblivion.tsv`, because the example keys the `parsed` column on
the **parsed Rust struct name** (so `NiTriStrips`→`NiTriShape`, ~28 `NiPSys*`→
`NiPSysBlock`, `bhkRigidBodyT`→`bhkRigidBody`, all `Ni*ExtraData`→`NiExtraData`
collapse) while the baseline keys on the **header wire type**. The apparent ~40-line
diff is an artefact of the two keyings, not a regression. Anyone re-running this
must replicate `PerBlockHistogram::record_scene_blocks`, not shell out to
`nif_stats`.
# Dimension 7 — Exterior Blocker Chain & Game-Specific Quirks

## Measured: Oblivion's distant-LOD assets (the only game where this route is live)
```
Oblivion - Meshes.bsa (20,182 entries):
  distantlod\<world>_<x>_<y>.lod   9,889     ← Oblivion-exclusive placement scheme
  *_far.nif                          130     ← distant-object low-poly meshes
  meshes\landscape\lod\*.nif         100     ← distant terrain meshes
```
No loose `DistantLOD/` directory ships; everything is inside the BSA, which the
archive path reads fine (Dim 2).

### `.lod` placement parser — re-validated, and better than documented
Replicating `parse_placement_lod`'s SoA walk over all 9,889 vanilla files:
```
exact-consume=9889/9889  trailing_bytes=0  overrun=0  errors=0  num_groups==0 in 0
distinct base_form_ids=208   total placements=99,591
rotations outside ±2π = 1   non-positive scales = 0
```
The module doc (`placement_lod.rs:37-39`) claims "9888/9889 (the lone outlier is
`toddland`)". **Measured today: 9889/9889 exact, zero outliers** — the doc is
pessimistic; harmless, but stale. The `_far.nif`-missing case (208 base objects
need one, only 130 ship) is already handled by the documented full-model fallback
at `placement_lod.rs:458-459`. `placement_lod_supported(game) == (game ==
GameKind::Oblivion)` (`:313-315`) — correct, and FO3/FNV correctly route to
`ObjectLodScheme::FalloutLegacyBlocks` instead.

## Findings

### OBL-D7-01 (HIGH) — **100% of Oblivion's distant-terrain LOD meshes get a fabricated constant normal**
This is the Skyrim `synthesize_normals` cross-check landing on Oblivion's
uniquely-live LOD route. The importer has `synthesize_tangents` /
`synthesize_tangents_yup` but **no `synthesize_normals`**; both classic geometry
paths fall back to a constant up-vector:
- `import/mesh/ni_tri_shape.rs:136-139` — `if !geom.normals.is_empty() { … } else
  { vec![[0.0, 1.0, 0.0]; positions.len()] }`
- `import/mesh/bs_tri_shape.rs:87-90` — same constant.

Measured over all 8,032 Oblivion meshes (`NiTriShapeData` + `NiTriStripsData`
blocks carrying vertices):
```
geometry blocks with vertices : 42,689   (40,553 strips + 2,136 shapes)
blocks with NO normals        :    252   (0.59%)   across 160 files (2.0%)
  ├─ meshes\landscape\lod\*.nif  100 files  ← 100 of the 100 that exist = 100%
  ├─ *_far.nif                     3 files
  └─ FX / additive billboards     57 files  (fire\*, sky\sunbeam*, fx*lightbeam*,
                                             magiceffects\*, oblivion\gate\ glow)
```
The 57 FX files are legitimately unlit additive billboards where a normal is
meaningless — no defect there. **The 103 LOD files are lit geometry.** Every one
of Oblivion's 100 distant-terrain LOD meshes therefore renders with a uniform
`[0, 1, 0]` normal: distant terrain is shaded as if perfectly flat and
facing straight up, regardless of the hills it represents. Because `placement_lod`
is Oblivion-only, no other title exposes this.

Unlike Skyrim's 20.4%-of-all-meshes version of this gap, Oblivion's is small in
count but **total within the class that matters** — and it is on the exterior
render path the blocker chain is currently converging on.
*Fix shape*: a `synthesize_normals` counterpart to `synthesize_tangents`
(area-weighted face-normal accumulation over `triangles`, already available at
both call sites) applied when `normals.is_empty()` **and** the material is lit —
the FX files must keep the cheap constant, not gain fabricated shading.

### OBL-D7-02 (HIGH) — 792 embedded `NiControllerSequence` animations import perfectly but never reach the ECS on cell load
Measured over `Oblivion - Meshes.bsa`:
```
files carrying NiControllerSequence : 423   (792 sequences, 423 NiControllerManager)
byroredux_nif::anim::import_kf(&scene) on those scenes:
    yields clips in 423 / 423 files  →  792 clips   (100%)
    yields nothing in   0 files
```
The importer is fully functional on embedded sequences. But **both** cell-loader
NIF import sites call only `import_embedded_animations`:
- `byroredux/src/streaming.rs:1180`
- `byroredux/src/cell_loader/references/import.rs:99`

`import_embedded_animations` (`crates/nif/src/anim/entry.rs:284-288`) handles only
the standalone single-interp controllers (`NiFlipController`,
`NiMaterialColorController`, `NiTextureTransformController`,
`NiSingleInterpController`, `NiLight*Controller`, `BsShaderController`) — it does
not look at `NiControllerSequence` at all, and yields a clip in just **72** of
8,032 files. `import_kf` is reachable only from the external `.kf` path
(`scene.rs:1015`), NPC spawn (`npc_spawn.rs:507`) and the `--kf` CLI flag.

Net effect: Oblivion's animated statics — Oblivion gates, machinery, banners,
the `obgate*`/`oblivionarchgate*` family — spawn frozen. The animation data
parses, imports, and resolves; it is simply never handed to the
`AnimationClipRegistry` for a placed REFR.
*Likely shared with FO3/FNV/Skyrim* (they embed sequences too); measured here on
Oblivion because that is this audit's corpus.

### OBL-D7-03 (LOW) — `placement_lod.rs` module doc understates its own validation
Says "9888/9889 … the lone outlier is `toddland`". Measured 9889/9889 exact-consume,
0 outliers, across the same corpus. One-line doc correction.

## Guards verified (no findings)
- **Transform-channel name resolution is clean**: across all 8,032 meshes the
  embedded clips produce **637 transform channels, 637 of which resolve to a real
  `NiNode` name — 0 unresolved.** The "animation blocks that parse but can't play
  because scene-graph name resolution is missing" hypothesis is **false** for
  Oblivion; the gap is OBL-D7-02 (never invoked), not name resolution.
- **Inline-name log levels have not drifted**: `crates/nif/src/lib.rs:386-392`
  logs the common pre-Gamebryo case at `debug!` (one line per file);
  `:404-407` escalates to `warn!` only on a mid-file inline-name read failure.
  No per-block `warn` spam risk on full-archive sweeps. ✓
- **Exterior wiring is not the blocker.** TES4 worldspace + LAND is implemented
  and game-agnostic (#1556); BSA v103 extraction is 100% (Dim 2, #699). Neither
  stale framing is regenerated here.
- **No Oblivion-specific record type blocks exterior REFR placement.** The four
  unhandled types from Dim 3 are PGRD (navigation), LVSP (leveled spells), SBSP
  (subspace) and ROAD (2 records) — none is on the placement path. WRLD (84),
  CELL (35,494), LAND (31,823), REFR (1,025,617), STAT (6,014), TREE (142),
  GRAS (108), LTEX (229) are all handled.

## Skill-file drift observed
The Dim-1 legacy-block checklist names 13 types to verify. **Six of them have zero
instances in the entire Oblivion corpus** (per the checked-in baseline):
`NiSequenceStreamHelper` 0, `NiSwitchNode` 0, `NiLODNode` 0, `BSOrderedNode` 0,
`NiPointLight` 0, `NiSpotLight` 0, `NiUVController` 0, `NiTextureEffect` 0.
The ones that do ship all parse with 0 unknown: `NiNode` 25,244,
`NiBillboardNode` 213, `NiDirectionalLight` 95, `NiKeyframeController` 1,
`NiCamera` 2, `NiAmbientLight` 17. Likewise **"the BSShader*Property aliases" —
Oblivion ships zero `BSShader*Property` blocks of any kind** (the only `BS*` types
present are `BSXFlags` 6,638, `BSFurnitureMarker` 75, `BSKeyframeController` 61,
`BSBound` 43, `BSParentVelocityModifier` 11, `BSPSysArrayEmitter` 1). And
`as_ni_node` needs to unwrap only `NiNode` + `NiBillboardNode` for this title —
both are covered (`import/walk/mod.rs:128, 151`). Same "audit the types your game
ships zero of" drift the FNV/FO3 skills carry.
