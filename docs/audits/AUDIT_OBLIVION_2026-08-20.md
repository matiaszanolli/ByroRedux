# Oblivion (TES4) Compatibility Audit — 2026-08-20

**Scope**: all 7 dimensions of `/audit-oblivion` — NIF v20.0.0.4 retail body
(v20.0.0.5 minority) + the v10.x NetImmerse tail, BSA v103, the live ESM path,
the Oblivion render / shader path, NIFAL/WATAL canonical material translation,
real-data validation, and the exterior blocker chain. Run as part of the
`comprehensive` audit-suite sweep of 2026-08-20 (335 commits since 2026-08-16).
**No sub-agents, no `cargo`, no engine launch** — every claim below was verified
by static read of the tree at `bb0b92f2` plus direct byte-level decode of
`/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/` (a standalone Python
BSA v103 lister + TES4 record walker written for this audit, in
`/tmp/audit/oblivion/`).

The delta is overwhelmingly session-70 WATAL water work, so this audit weighted
dimensions 3 / 5 / 7 toward the Oblivion water path and treated dimensions 1 / 2
/ 4 as regression-guard sweeps.

---

## Executive Summary

**Oblivion NIF parsing is now 100% clean — 8,032 / 8,032, zero truncations, zero
hard failures.** The checked-in baseline
`crates/nif/tests/data/block_coverage_baselines/oblivion_truncations.tsv` was
regenerated against real data by `17cb417d` (Fix #3082) and now reads
`truncating=0 parsed=8032`. The last truncation (`meshes\marker_map.nif`) was
recovered in this delta. Four documentation files still carry the stale 99.93% /
6-truncation figure.

The two largest Oblivion defects the 2026-08-16 sweep reported are **both fixed**
in this delta: the BSXFlags bit-5 file-level drop (`#3036`, which was deleting 70
real meshes across 5,112 placements in 1,041 Oblivion cells) and the
Oblivion-named regression test that asserted the wrong semantics (`#3102`).

What this sweep found instead is in the **water layer**, where session 70 landed
~1,900 lines and where Oblivion's WATR record diverges from every other title:

> **Oblivion's `WATR.TNAM` is a *diffuse* texture, and the canonical water
> translation binds it into `WaterMaterial::normal_map_index` — the shader's
> tangent-space normal map.** 15 of 23 vanilla WATR records author a TNAM, and
> those records are referenced by **163 vanilla cells**. `OblivionLava06.dds`
> decodes to an average RGB of (178, 55, 26); through the shader's
> `normalize(rgb * 2 - 1)` that is a surface normal pointing *into* the plane.

Alongside it, `WATR.MNAM` — which literally spells `lava\0` on the two damaging
lava planes — is parsed by no arm at all, so the engine's only non-heuristic
"this surface is lava" signal on Oblivion is discarded at the parser boundary.

Per-dimension status (every dimension enumerated, including clean ones):

| Dim | Area | New findings |
|-----|------|--------------|
| 1 | NIF version handling — v20.0.0.4/.5 + v10.x NetImmerse tail | **0** |
| 2 | BSA v103 archive | **0** |
| 3 | ESM record coverage (live path) | **1** (MEDIUM) |
| 4 | Rendering path for Oblivion shaders | **0** |
| 5 | NIFAL / WATAL canonical translation for Oblivion | **2** (1 HIGH, 1 MEDIUM) |
| 6 | Real-data validation | **1** (LOW) |
| 7 | Exterior blocker chain & game-specific quirks | **1** (LOW) |

Totals: **5 findings — 0 CRITICAL, 1 HIGH, 2 MEDIUM, 2 LOW.**

---

## Dimension Findings

### Dimension 1 — NIF Version Handling (v20.0.0.4/.5 + the v10.x NetImmerse tail) · **0 new findings**

Every guard the skill nominates was re-read at HEAD and holds. Nothing in the
delta touched the v10.x bands.

- `user_version` threshold — `crates/nif/src/header.rs:114`,
  `if version >= NifVersion::V10_0_1_8`. ✓
- BSStreamHeader dual-band (`#170`) — `crates/nif/src/header.rs:137-143`
  reproduces the documented band exactly. ✓
- v10.x band constants present in `crates/nif/src/version.rs`: `V10_0_1_2` (:71),
  `V10_0_1_8` (:77), `V10_1_0_0` (:79), `V10_1_0_114` (:113), `V10_2_0_0` (:116),
  `V20_0_0_4` (:130), `V20_0_0_5` (:132). ✓
- `#1509` morph gate — `crates/nif/src/blocks/controller/morph.rs:107-110`,
  `V10_2_0_0 <= version <= V20_0_0_5 && bsver >= MORPH_LEGACY_CUTOFF` (10, the
  `#2423`-normalised spelling of `> 9`), with the complementary
  `< MORPH_LEGACY_CUTOFF` half at `:219`. ✓
- `NiTexturingProperty` reads a raw `u32` count with no leading bool —
  `crates/nif/src/blocks/properties.rs:211`. ✓
- `havok_motion_type` (`#1652`) still maps the full nif.xml enum —
  `crates/nif/src/import/collision/mod.rs:222-231` (1–5|8 → Dynamic, 6 →
  Keyframed, 7 → Static, 9 → CharacterKinematic). ✓
- The `#1506`/`#1507`/`#1508` stride-drift family shows **zero** regression: the
  checked-in truncation baseline is now empty (Dim 6).
- Delta review: `crates/nif/src/import/walk/mod.rs:882-928` widened
  `extract_emitter_rate` to follow `NiBlendFloatInterpolator` sub-interpolators
  (`#2548`, FO3-driven). `NiBlendFloatInterpolator` is reachable on Oblivion too,
  so this is a strict improvement on this title, not a regression.
  `crates/nif/src/import/material/slot_role.rs` (+499) is `BSShaderTextureSet`-only
  and structurally unreachable from Oblivion's `NiTexturingProperty` path.

### Dimension 2 — BSA v103 Archive · **0 findings**

Regression guard `#699` intact. Verified statically and against real archives
with an independent Python v103 reader (no engine code in the loop):

- `crates/bsa/src/archive/open.rs:40` rejects anything outside {103, 104, 105}. ✓
- `open.rs:100` — `let folder_record_size: usize = if version == BSA_V_SKYRIM_SE { 24 } else { 16 };`
  (v103 **and** v104 are 16 bytes). ✓
- `open.rs:75` — `embed_file_names` gates on `version >= BSA_V_FO3_SKYRIM`, so
  the "Xbox archive" bit several vanilla v103 archives set is correctly ignored. ✓
- Independent read of `Oblivion - Misc.bsa` (ver 103, flags `0x703`, 115 files),
  `Oblivion - Textures - Compressed.bsa` (ver 103, 18,040 files) and per-file
  zlib extraction of four `textures\` entries all succeeded, confirming the
  header / folder-record / name-block / compressed-payload layout the Rust
  reader implements.

### Dimension 3 — ESM Record Coverage (live path) · **1 new finding (MEDIUM)**

Oblivion's ESM surface is in good shape; the Oblivion-specific decode branches
are real-data-derived and unchanged in the delta (`records/items.rs:190-200`
WEAP DATA 30 B, `:382-389` ARMO DATA 14 B + 4-byte BMDT, `:499-505` AMMO DATA
18 B, the 16-byte ACBS arm `#1650`, MGEF-by-code, CLMT WLST, XCLL).

Real-data checks run this sweep against vanilla `Oblivion.esm`:

- **WRLD**: 84 records; sub-record census is `EDID 84 / DATA 84 (all 1 byte) /
  NAM0 84 / NAM9 84 / OFST 84 / FULL 66 / NAM2 54 / MNAM 54 / SNAM 39 / WNAM 30 /
  CNAM 27 / ICON 5`. **Zero `DNAM`, zero `NAM3`, zero `NAM4`, zero `PNAM`** —
  exactly what `crates/plugin/src/esm/cell/wrld.rs:131-160` and
  `byroredux/src/env_translate.rs:153-155,187` claim. ✓
- **WATR**: 23 records. `DATA` lengths are 102×17, 86×2, 62×1, 42×2, 2×1 —
  matching `/audit-esm`'s independent census.
- ACRE (Oblivion-only placed-creature) is walked
  (`crates/plugin/src/esm/cell/walkers.rs:641-643`) and `CREA` bases resolve
  through the unified `ObjectIndex::actor` accessor (`records/index.rs:436`,
  `#2567`). ✓

#### OBL-2026-08-20-D3-01: `WATR.MNAM` is Oblivion's authored "this is lava" signal and no parser arm reads it

- **Severity**: MEDIUM
- **Dimension**: ESM record coverage (Dim 3) / WATAL parse boundary
- **Location**: `crates/plugin/src/esm/records/misc/water.rs:1282-1380`
  (the `parse_watr` sub-record `match`; arms exist for `ANAM`, `FNAM`, `TNAM`,
  `NNAM`, `NAM1`–`NAM5`, `DATA`, `DNAM`, `GNAM` — none for `MNAM` or `SNAM`)
- **Status**: NEW
- **Description**: TES4 `WATR` carries an `MNAM` zstring — the Construction
  Set's *Material* field, the Havok surface-material name. On vanilla
  `Oblivion.esm` it is the literal ASCII `lava\0` on exactly two records, and an
  empty `\0` or absent on the other 21. It is the only **authored,
  non-heuristic** discriminator between water and lava anywhere in Oblivion's
  data, and the engine currently has none: `WaterKind`
  (`crates/core/src/ecs/components/water.rs:48-66`) has only
  `Calm`/`River`/`Rapids`/`Waterfall`, and the classifier
  (`byroredux/src/env_translate.rs:912-947`) is a pure EditorID keyword match on
  `"rapid"`/`"waterfall"`/`"falls"`/`"river"`/`"stream"` — none of which any of
  Oblivion's 23 EditorIDs contains. **Every Oblivion lava plane therefore
  becomes `WaterKind::Calm`**: refractive, buoyant, swimmable, blue-water-shaded.
- **Evidence**: byte-level decode of the WATR GRUP in `Oblivion.esm`:
  ```
  OblivionCitadelLavaPlane  FNAM=01  MNAM=6c 61 76 61 00 ("lava")  DATA[100]=5000
  OblivionLavaTest01        FNAM=01  MNAM=6c 61 76 61 00 ("lava")  DATA[100]=50
  CamoranLava               FNAM=01  MNAM=absent                   DATA(2)=65535
  CamoranLava02             FNAM=01  MNAM=absent                   DATA(42) tail=50
  OblivionOil01             FNAM=01  MNAM=00                       DATA(62) tail=0
  (the remaining 18 records: FNAM=02 or 00, MNAM=00 or absent, tail=0)
  ```
  The two `MNAM="lava"` records are precisely the two with a non-zero damage
  value in the full-length `DATA` layout. `SNAM` (the surface sound, present on
  17 of 23) is dropped by the same `match`.
- **Impact**: 63 vanilla `Oblivion.esm` cells reference a damage-flagged WATR
  (45 × `OblivionLavaTest01`, 15 × `OblivionCitadelLavaPlane`, 2 ×
  `OblivionOil01`, 1 × `CamoranLava02`) — the entire Deadlands / Oblivion-realm
  content set plus `MS13OblivionCave*`, `DAPeryiteCave01` and the Kvatch towers.
  All of them present as ordinary calm water. Once
  `ESM-2026-08-20-D5-06`'s damage fix lands, the damage will apply but the
  *surface* will still be classified, shaded, and simulated as water.
- **Related**: `ESM-2026-08-20-D5-06` (the damage/`FNAM` half of the same
  record — file the fixes together; `MNAM` and the trailing damage `u16` come
  from the same 20-line `match`). `docs/engine/watal.md` §4 lists neither field.
  No GitHub issue matches (`/tmp/audit/issues.json`).
- **Suggested Fix**: Add an `b"MNAM" => out.material_name = read_zstring(&sub.data)`
  arm and surface it on `WatrRecord`. Decide the canonical consumer separately —
  the minimum useful step is to let `env_translate`'s classifier read it so a
  future `WaterKind::Lava` (or a `WaterMaterial` hazard flag) has an authored
  input rather than another EditorID keyword list. Do **not** invent a lava
  `WaterKind` from the EditorID string; that is the guessing this field exists
  to prevent.

### Dimension 4 — Rendering Path for Oblivion Shaders · **0 new findings**

Guards re-verified at HEAD; none of the delta's shader work reaches Oblivion's
legacy property tree.

- `#1239` Oblivion `NiPSysEmitter` version gate — documented and in place at
  `crates/nif/src/blocks/particle.rs:81-89` (the pre-fix `bsver() >= 34` gate is
  named in the comment as the thing that excluded Oblivion). ✓
- Disney/PBR gate stays 0 on Oblivion — `MAT_FLAG_PBR_BSDF` is `1 << 5`
  (`crates/renderer/src/shader_constants_data.rs:384`) mirrored as `32u` in the
  generated `crates/renderer/shaders/include/shader_constants.glsl:149`, and
  `crates/nif/src/import/material/legacy_is_pbr_tests.rs` still pins `!is_pbr`
  for legacy Oblivion-shaped material trees. ✓
- `#337` `NiStencilProperty` state capture —
  `crates/nif/src/import/material/stencil_state_capture_tests.rs` present. ✓
- `emissive_source` legacy arm —
  `crates/nif/src/import/material/emissive_source_tests.rs` present. ✓
- Delta review of `crates/nif/src/import/material/legacy_properties.rs`: the two
  new `#2320` `legacy_shader_type` assignments and the new `is_water_shader`
  flag are inside `BSShaderPPLightingProperty` / `BSShaderNoLightingProperty` /
  `WaterShaderProperty` arms — all FO3-and-later block types, unreachable from
  Oblivion's `NiTexturingProperty`/`NiMaterialProperty` tree.

### Dimension 5 — NIFAL / WATAL Canonical Translation for Oblivion · **2 new findings (1 HIGH, 1 MEDIUM)**

#### OBL-2026-08-20-D5-01: Oblivion's `WATR.TNAM` is a diffuse texture and the canonical water translation binds it as the shader's tangent-space normal map

- **Severity**: HIGH
- **Dimension**: WATAL canonical translation (Dim 5)
- **Location**: `byroredux/src/env_translate.rs:1030-1035` (the single
  translate site); consumed at `byroredux/src/cell_loader/water.rs:442-465`;
  decoded as a tangent-space normal at
  `crates/renderer/shaders/water.frag:309-310`; parsed at
  `crates/plugin/src/esm/records/misc/water.rs:1312-1313`
- **Status**: NEW
- **Description**: `parse_watr` writes both `TNAM` and `NNAM` into the same
  `WatrRecord::texture_path` field, whose own docstring
  (`water.rs:65-70`) enumerates only *"FO3 / FNV ship this in `NNAM` … Skyrim+
  ships it in `TNAM`"* — **Oblivion is not in the contract at all**. The
  translate boundary then does:
  ```rust
  // TNAM is the diffuse / noise texture — used as the
  // bindless normal map for the shader. Empty path =
  // procedural fallback.
  if !rec.texture_path.is_empty() {
      normal_path = Some(rec.texture_path.clone());
  }
  ```
  and the cell loader assigns the resolved handle to
  `WaterMaterial::normal_map_index` (`byroredux/src/cell_loader/water.rs:463-465`), which the
  shader samples as a strict tangent-space normal:
  ```glsl
  vec3 n = texture(textures[nonuniformEXT(normalMapIndex)], uv).xyz;
  n = normalize(n * 2.0 - 1.0);
  ```
  On Oblivion, `TNAM` is the Construction Set's **Texture** field — the water
  surface's *colour* art. Feeding albedo through `rgb * 2 - 1` does not produce
  a normal; it produces an arbitrary, usually downward-facing vector.
  `noise_map_indices` then inherit the same handle for all three wave layers
  (`byroredux/src/cell_loader/water.rs:466-472`), so every layer samples it.
- **Evidence**: extracted and header/block-decoded straight out of
  `Oblivion - Textures - Compressed.bsa`:
  ```
  textures\water\oblivionlava06.dds   512×512 DXT1  mean RGB (178, 55, 26)
  textures\water\dungeonwater01.dds   512×512 DXT1  mean RGB ( 27, 30, 23)
  ```
  A tangent-space normal map has a mean near (128, 128, 255). (178, 55, 26)
  maps to `normalize((0.396, −0.569, −0.796))` ≈ `(0.38, −0.55, −0.75)` — the
  Z component is **negative**, i.e. the surface normal points into the plane.
  `textures\water\` contains exactly these two files and **no `_n` sibling**,
  confirming Oblivion ships no separate water normal map.

  The authored `TNAM` values themselves are conclusive — they are reused
  architecture / landscape / dungeon albedo:
  ```
  SEBrellachWater              Architecture\city\Dementia\Sewage01.dds
  SEPinnacleRockWater          Landscape\Dementia\DementiaMold01.dds
  SERuinDungeonWaterNoSwim     Dungeons\RuinsDungeons\RRubblePileA01.dds
  SErootDungeonWaterDeepNasty  Dungeons\Rootcaves\Rooms\RootRoomCeiling02.dds
  XPBlood                      Dungeons\Misc\BloodPool02.dds
  Blood / CamoranLava02        Landscape\Oblivion\TerrainHDOblivionLava01.dds
  DungeonWater01 / SewerWater /
    DungeonWaterBrightFog01    Water\DungeonWater01.dds
  OblivionLavaTest01           Water\OblivionLava06.dds
  CamoranLava                  OblivionGate\Lava01.dds
  OblivionOil01                Water\OblivionOil01.dds
  MS31Water                    Water\water00.dds
  ```
  15 of 23 vanilla WATR records author a non-empty `TNAM`.
- **Impact**: **163 vanilla `Oblivion.esm` cells** reference a TNAM-bearing
  WATR — 74 × `DungeonWater01`, 45 × `OblivionLavaTest01`, 15 × `SewerWater`,
  12 × `SErootDungeonWaterShallow`, plus the Shivering Isles set, the blood
  pools and the Camoran lava. Every one gets an inverted, high-frequency,
  colour-derived normal field on all three wave layers: broken Fresnel, broken
  reflection-ray direction, broken specular. Bounded away from CRITICAL because
  the *default* waters (`DefaultWater`, `DefaultWaterNight`,
  `DefaultUnderwater`, `SwampWater`, `SEDefault*`, `OblivionCitadelLavaPlane`)
  all ship an **empty** `TNAM`, so the Tamriel open world correctly falls
  through to the shader's procedural path
  (`water.frag:235`, `normalMapIndex == 0xFFFFFFFF`). The bug is interiors,
  dungeons, sewers, SI and lava.
- **Related**: `ESM-2026-08-20-D5-06` and `OBL-2026-08-20-D3-01` are the other
  two Oblivion-specific `WATR` defects — same record, same 20-line `match`, one
  fix commit. `LC-D2-01` (mesh-bound `blend_normals`) is Skyrim-only and does
  **not** apply here (see *Candidates Investigated and Disproved* #2). No
  GitHub issue matches.
- **Suggested Fix**: Give `WatrRecord` a second field so the diffuse and the
  noise/normal roles stop sharing one string, and gate the Oblivion `TNAM` arm
  into the diffuse role. Until a water-diffuse consumer exists, the correct
  canonical value for Oblivion's `normal_map_index` is the `u32::MAX` procedural
  sentinel — i.e. dropping the TNAM is strictly better than binding it, and is a
  one-line change at `env_translate.rs:1033`. Pin it with a real-data-shaped
  test asserting a `GameKind::Oblivion` WATR carrying
  `Water\OblivionLava06.dds` leaves `normal_map_index` at the sentinel.

#### OBL-2026-08-20-D5-02: `docs/engine/watal.md`'s per-game matrix states three Oblivion rows as SENTINEL that real data shows are AUTHORED

- **Severity**: MEDIUM
- **Dimension**: WATAL design contract (Dim 5)
- **Location**: `docs/engine/watal.md:475-490` (the "GameVariant doctrine for
  water" table, Oblivion column)
- **Status**: NEW
- **Description**: `watal.md` §3 defines the contract that governs the whole
  layer: *"**SENTINEL** = explicit canonical game-default … **never** a
  render-time guess"*, and §4's matrix is the authoritative per-game statement of
  which fields each game authors. Three of its Oblivion rows are wrong against
  vanilla `Oblivion.esm`, and each wrong row is the stated justification for a
  live defect:

  | Row (watal.md) | Doc says (Oblivion) | Real `Oblivion.esm` |
  |---|---|---|
  | `legacy water damage` (`:479`) | SENTINEL | **AUTHORED** — `FNAM` bit 0x01 on 5 records; damage `5000 / 65535 / 50 / 50` |
  | `diffuse/normal texture` (`:483`) | SENTINEL `u32::MAX` → procedural | **AUTHORED** — non-empty `TNAM` on 15 of 23 |
  | `fog_near`/`fog_far` (`:480`) | SENTINEL 80/600 (short DATA) | **AUTHORED** on the 17 full-length (102 B) records; `decode_data_oblivion` reads them at DATA[36]/[40] |

  The same `diffuse/normal texture` row also attributes `NNAM` to FO3/FNV and
  `TNAM` to Skyrim — `TNAM` is Oblivion's field, and the row has no Oblivion
  entry for it at all.
- **Evidence**: the WATR census in `OBL-2026-08-20-D3-01` and the TNAM listing
  in `OBL-2026-08-20-D5-01` above. For the fog row, compare
  `crates/plugin/src/esm/records/misc/water.rs:497-502` (`decode_data_oblivion`
  reads `fog_near` at offset 36 and `fog_far` at 40) against the doc's claim —
  the row predates the Oblivion offset fix `/audit-esm` verifies as landed.
- **Impact**: MEDIUM rather than LOW because this is not a stale prose
  paragraph — it is the design document's *contract table*, and two of its three
  wrong rows encode exactly the false premise ("Oblivion authors nothing here")
  that produced `ESM-2026-08-20-D5-06` (all Oblivion lava harmless) and
  `OBL-2026-08-20-D5-01` (diffuse bound as a normal map). A reader checking
  whether the Oblivion water path is complete is told, by the authority, that it
  already is. `watal.md` is also the single most-changed file in this delta (63
  touches), so the rows were live-edited around without being re-checked.
- **Related**: `ESM-2026-08-20-D5-06`, `OBL-2026-08-20-D5-01`,
  `OBL-2026-08-20-D3-01`. Distinct from `OBL-2026-08-20-D7-01`, which is
  parse-rate doc rot in a different set of files.
- **Suggested Fix**: Correct the three rows and add the two missing ones
  (`MNAM` material, `SNAM` sound). Then adopt the convention `/audit-esm` uses
  elsewhere: cite the measured vanilla record count next to each AUTHORED cell,
  so the table can be re-checked against data instead of re-asserted.

### Dimension 6 — Real-Data Validation · **1 new finding (LOW)**

**Oblivion NIF parsing is 100% clean.** The checked-in baseline
`crates/nif/tests/data/block_coverage_baselines/oblivion_truncations.tsv` now
reads:

```
# Oblivion sizeless-truncation baseline	truncating=0	parsed=8032
parsed	8032
```

regenerated against real data by `17cb417d` (Fix `#3082`). The last truncated
file, `meshes\marker_map.nif`, was recovered earlier in this delta; the
corrupt-by-design `marker_radius.nif` family is gone from the list entirely.
Per the suite briefing this audit did **not** re-run `nif_stats` (no `cargo`),
so the figure above is the checked-in baseline, not a fresh sweep — but the
baseline is generated from the same code path and was refreshed one day before
HEAD.

The `#3082` fix also added a `parsed >= baseline_parsed` assertion
(`crates/nif/tests/block_coverage_baselines.rs:193-209`), closing the
"hard-fail regression with an unchanged truncating set" hole.

#### OBL-2026-08-20-D6-01: `#3082` closed with only half its fix — the truncation gate is still one-directional

- **Severity**: LOW
- **Dimension**: Real-data validation (Dim 6)
- **Location**: `crates/nif/tests/block_coverage_baselines.rs:177-192`
- **Status**: NEW (residual of the CLOSED `#3082`, whose title names both halves)
- **Description**: `#3082` is titled *"the Oblivion truncation gate is
  one-directional **and** its `parsed=` count is never read back"*. `17cb417d`
  fixed the second clause — `parsed` now has its own baseline line and a
  `parsed >= baseline_parsed` assertion. The first clause did not land: the gate
  still computes only `new_truncations = live \ baseline` and panics when that
  set is non-empty. It never computes `baseline \ live`, so a file that *stops*
  truncating leaves a phantom baseline entry no test run will ever surface. Its
  sibling gate `per_block_baselines.rs` in the same directory does check
  shrinkage, so the two baselines still disagree on whether improvement is worth
  pinning.
- **Evidence**: `block_coverage_baselines.rs:177-192` —
  ```rust
  let new_truncations: Vec<&String> = truncating
      .keys()
      .filter(|p| !baseline.contains(*p))
      .collect();
  if !new_truncations.is_empty() { … panic!(…) }
  ```
  No mirror comparison follows; the next statement is the `parsed` assertion
  added by `17cb417d`.
- **Impact**: Currently inert — the baseline is empty, so `baseline \ live` is
  necessarily empty too. It re-arms the moment any Oblivion truncation is
  baselined and later fixed, which is the exact mechanism that produced the
  five-cycle-old stale row `OBL-2026-08-20-D7-01` still reports. Only ever
  hides *good* news, hence LOW.
- **Related**: `#3082` (CLOSED — reopen or file a follow-up rather than
  regressing the closed half); `#2564`; `OBL-2026-08-20-D7-01`.
- **Suggested Fix**: Add the mirror check — collect `baseline \ live` and fail
  with `"regenerate: N file(s) no longer truncate"`, matching the shrinkage
  semantics `per_block_baselines.rs` already uses. ~8 lines, next to the
  assertion `17cb417d` already added.

### Dimension 7 — Exterior Blocker Chain & Game-Specific Quirks · **1 new finding (LOW)**

- The dead framings were **not** regenerated: BSA v103 decompression works
  (Dim 2), and TES4 worldspace + LAND wiring is implemented and game-agnostic
  since `#1556`.
- **`_far.nif` / distant-terrain LOD re-verified against real data.** The new
  `#3100` legacy-LOD texture translation
  (`byroredux/src/env_translate.rs:99-127`, `fmt_oblivion_lod_coord` at `:79-85`)
  emits `textures\landscapelod\generated\{form_id & 0xFFFFFF}.{ox}.{oy}.32.dds`
  with `"00"` for a zero coordinate. Independent listing of
  `Oblivion - Textures - Compressed.bsa` finds exactly 200 files under
  `textures\landscapelod\generated\`, spanning **17 worldspace ids** with
  Tamriel at `60` (= `0x3C`) carrying 36 quads, and coordinate tokens `-96`,
  `-64`, `-32`, `00`, `32`, `64` — a byte-exact match for the generated names,
  including the `_fn.dds` normal sibling. The `qx.div_euclid(32) * 32` quad
  origin is correct for negative cell coordinates. ✓
- Pre-Gamebryo inline-type fallback still logs at `debug`
  (`crates/nif/src/lib.rs:369`, `:380`), with `warn` reserved for an actual
  inline-type read failure (`:404`, `:417`). No spam risk on full-archive
  sweeps. ✓
- Animation blocks that parse but can't play: unchanged cross-game cell-loader
  gap (`#261`), not re-filed.

#### OBL-2026-08-20-D7-01: the stale Oblivion parse rate is now wrong in 8 places across 4 files, and `#2564` under-scopes it

- **Severity**: LOW
- **Dimension**: Exterior blocker chain / doc accuracy (Dim 7)
- **Location**: `ROADMAP.md:413`, `:566`, `:1009`, `:1295`;
  `docs/engine/nif-parser.md:9`, `:667`;
  `docs/engine/game-compatibility.md:17`, `:32`, `:258`;
  `docs/engine/architecture.md:319`
- **Status**: Existing: `#2564` — carried forward, but the issue's scope is
  wrong in two ways (see Description). Not re-filed as a separate defect.
- **Description**: Every one of the sites above still states Oblivion at
  **99.93% (8,026 / 8,032)** with **6 residual NetImmerse truncations**. The
  live baseline is **8,032 / 8,032, zero truncations** (Dim 6).
  `ROADMAP.md:1009` additionally still says *"recoverable rate at 100% across
  all seven games except Oblivion's single hard-fail (#698, closed)"* — there is
  no hard fail, and the parenthetical already contradicts its own sentence.
  `#2564` frames the drift as "stale by 5" (6 baselined vs 1 live); at HEAD it is
  **stale by 6**, and its scope is `ROADMAP.md` alone, whereas the figure is
  duplicated into three `docs/engine/` files that `#2564` does not name.
- **Evidence**: `crates/nif/tests/data/block_coverage_baselines/oblivion_truncations.tsv`
  reads `truncating=0 parsed=8032` at HEAD (regenerated by `17cb417d`).
  `git log --oneline -- <that tsv>` shows the previous state was the 6-marker
  baseline `795896b7` (`#1611`).
- **Impact**: The Oblivion row is the compat matrix's only non-100% clean entry
  outside Starfield, and `ROADMAP.md` declares itself the live source of truth.
  Three `docs/engine/` copies mean a single-file fix will leave the rot in place
  — which is how it survived five sweeps.
- **Related**: `#2564`; `OBL-2026-08-20-D6-01` (the gate asymmetry that made a
  *fixed* truncation invisible in the first place).
- **Suggested Fix**: Update all ten sites to 100% (8,032 / 8,032), drop the
  "6 residual NetImmerse marker files" and "single hard-fail" clauses, and note
  in `#2564` that the figure lives in four files, not one. Landing
  `OBL-2026-08-20-D6-01` first would make the next such drift self-reporting.

---

## Sibling-Audit Findings — Oblivion Blast Radius (measured, not re-filed)

The suite briefing assigned three already-filed cross-audit findings for
Oblivion blast-radius measurement. All three verified at HEAD; **none re-filed**.

### 1. `ESM-2026-08-20-D5-06` — Oblivion `WATR` damage + `FNAM` never reach canonical

**Confirmed, and Oblivion is the only title it breaks.** The premise holds
exactly as filed: `crates/plugin/src/esm/records/misc/water.rs:1293-1302` excludes
`GameKind::Oblivion` from the `FNAM` arm, and `:1332` guards the 2-byte-`DATA`
damage arm with `!matches!(game, GameKind::Oblivion)`, so `water_flags`,
`legacy_flags` and `legacy_damage` are all permanently `None` on this title.
`byroredux/src/cell_loader/water.rs:492-502` (and its LOD twin at `:825-835`)
requires flag bit 0x01 **and** `Some(legacy_damage)` before it will set
`damage_per_second`, so it is unconditionally `0.0`.

**Blast radius this audit measured (new — the sibling report quantified the
records, not the placements):**

The trailing damage `u16` is the **last 2 bytes of `DATA` at every authored
length**, not only the 102-byte layout — verified across all 23 records
(`len ∈ {2, 42, 62, 86, 102}`; every non-damaging record's tail is `0`, every
`FNAM` bit-0 record's is not):

```
OblivionCitadelLavaPlane  FNAM=01  DATA len=102  damage=5000
CamoranLava               FNAM=01  DATA len=  2  damage=65535
CamoranLava02             FNAM=01  DATA len= 42  damage=50
OblivionLavaTest01        FNAM=01  DATA len=102  damage=50
OblivionOil01             FNAM=01  DATA len= 62  damage=0
```

Cells in `Oblivion.esm` whose `XCWT` (or worldspace `NAM2`) points at a
damage-flagged WATR:

```
 45  OblivionLavaTest01     (OblivionRDCaves*, OblivionRD00*, MS13OblivionCave*,
                             DAPeryiteCave01, OblivionMqKvatchSmallTower02, …)
 15  OblivionCitadelLavaPlane (OblivionRD002Citadel*, OblivionRDCitadel05, …)
  2  OblivionOil01
  1  CamoranLava02
 ───
 63  total (61 with a non-zero damage value)
```

Note the `CamoranLava` 2-byte-`DATA` shape: because the Oblivion carve-out sends
it to `decode_data_oblivion` instead of the damage arm, a 2-byte payload also
produces an all-default `WaterParams` — so that record loses both its damage
*and* its visual payload. The sibling's suggested fix (read the tail `u16`,
accept the 2-byte Oblivion stub, drop `Oblivion` from the `FNAM` exclusion list)
is correct; recommend implementing the tail read as *last-2-bytes* rather than
*offset 100* so the 42/62/86-byte variants are covered too.

### 2. `ESM-2026-08-20-D5-04` — rain / displacement simulator misaligned by one field on the Oblivion arm

**Confirmed.** `decode_data_oblivion`
(`crates/plugin/src/esm/records/misc/water.rs:468-545`) reads
`rain_start_size` from DATA[96] (`:529-531`) and `displacement[0]` from DATA[76] (`:524`). The TES4
`DATA` runs are Rain Simulator at 60/64/68/72/**76** (force, velocity, falloff,
dampener, **starting size**) and Displacement Simulator at 80/84/88/92/**96** —
so the two "starting size" fields are exchanged. The other eight offsets in both
runs are correct. The sibling's suggested Oblivion correction
(`displacement ← [96, 88, 92]`, `rain_start_size ← 76`) is right.
**Oblivion blast radius: all 17 full-length (102-byte) WATR records**, i.e. every
record except the five short variants, which have no simulator tail at all.
Bounded — both fields are small positive floats and affect ripple *shape*, not
surface integrity. Not re-filed.

### 3. `LC-D2-01` — mesh-water `blend_normals` gated on undefined bit 16

**Confirmed as filed, and the Oblivion blast radius is zero.** The gate lives in
`water_material_from_mesh` and reads `BSWaterShaderProperty.water_shader_flags`;
nif.xml declares `WaterShaderPropertyFlags` with `versions="#SKY_AND_LATER#"`, so
the block type cannot appear in an Oblivion NIF, and the 82-type vanilla Oblivion
block histogram contains no `BS*ShaderProperty` at all. Oblivion cell water takes
the `env_translate` path exclusively. Recorded here so a future sweep does not
re-derive an Oblivion angle on it. Not re-filed.

---

## Blocker Chain — "an Oblivion exterior cell renders"

Interiors already render end-to-end (Anvil Heinrich Oaken Halls; the checked-in
runtime baseline `.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv`
is the cleanest path in the corpus — zero fallback textures, zero parse fails).
TES4 worldspace + LAND wiring is implemented and game-agnostic (`#1556`); Tamriel
`(0,0)` radius 1 last measured 6,043 entities / 2,355 draws (2026-08-12). The
remaining chain is short and includes **no** archive or wiring work:

1. **On-device exterior render bench** on the current build (tracked by `#2377` /
   `#2368`) — the same shape FO3 was pre-bench.
2. Whatever placement / LOD gaps the bench surfaces. The `#3100` legacy LOD
   texture naming is verified byte-exact against the real archive (Dim 7), so
   distant terrain should texture rather than fall back.
3. *(Not a blocker for the exterior bench — the open-world default waters have an
   empty `TNAM` and are unaffected.)* `OBL-2026-08-20-D5-01` and
   `ESM-2026-08-20-D5-06` should land before any **interior / Deadlands** bench
   is taken as a baseline: 163 interior cells render with an inverted water
   normal and 63 render harmless lava.

The 2026-08-16 chain's step 2 ("fix the BSXFlags bit-5 drop before the bench") is
**done** — `#3036` is closed and the file-level drop is gone from both sites.

---

## Regression Guard List — verified still holding this sweep

| Guard | Where | Status |
|---|---|---|
| v10.x stride-drift family `#1506`/`#1507`/`#1508` | truncation baseline is now **empty** (0 files) | ✓ |
| `#1509` `NiGeomMorpherController` `bsver >= 10` gate | `blocks/controller/morph.rs:107-110` + `:219` | ✓ |
| `NiTexturingProperty` raw `u32` count, no bool gate | `blocks/properties.rs:211` | ✓ |
| BSStreamHeader dual-band `#170` | `header.rs:137-143` | ✓ |
| `user_version` threshold `V10_0_1_8` | `header.rs:114` | ✓ |
| v10.x band constants | `version.rs:71,77,79,113,116,130,132` | ✓ |
| BSA v103 open + extract `#699` | `bsa/src/archive/open.rs:40,75,100` + independent real-archive read | ✓ |
| `#1652` `havok_motion_type` full enum | `import/collision/mod.rs:222-231` | ✓ |
| Disney/PBR gate stays 0 on Oblivion | `shader_constants_data.rs:384` ↔ `shader_constants.glsl:149`; `legacy_is_pbr_tests.rs` | ✓ |
| `#1239` Oblivion `NiPSysEmitter` gate | `blocks/particle.rs:81-89` | ✓ |
| `#337` `NiStencilProperty` state capture | `import/material/stencil_state_capture_tests.rs` | ✓ |
| Oblivion legacy `emissive_source` arm | `import/material/emissive_source_tests.rs` | ✓ |
| Pre-Gamebryo inline-type fallback logs at `debug` | `crates/nif/src/lib.rs:369,380` (`warn` only at `:404`,`:417`) | ✓ |
| Oblivion 16-byte ACBS `#1650`, WEAP 30 B / ARMO 14 B + 4 B BMDT / AMMO 18 B | `esm/records/items.rs:190,382,499`; `actor/mod.rs` | ✓ |
| ACRE placed-creature walk `#396` + unified actor lookup `#2567` | `esm/cell/walkers.rs:641-643`; `esm/records/index.rs:436` | ✓ |
| Oblivion WRLD ships no `DNAM`/`NAM3`/`NAM4`/`PNAM` (`#1305`) | re-measured: 84/84 records, all absent | ✓ |
| **`#3036` BSXFlags bit-5 file-level drop removed** | `cell_loader/references/import.rs:68-75`, `partial.rs:116-120` — no `& 0x20` drop remains | ✓ (fixed this delta) |
| **`#3102` Oblivion bit-5 test corrected** | `cell_loader/finish_partial_tests.rs:254-298` now asserts geometry is *kept* | ✓ (fixed this delta) |
| `#3100` Oblivion legacy LOD texture naming | `env_translate.rs:79-127` ↔ 200 real files in `Oblivion - Textures - Compressed.bsa` | ✓ |

---

## Candidates Investigated and Disproved

Recorded so future sweeps do not re-derive them.

1. **"The Oblivion legacy LOD texture path (`#3100`) guesses its naming
   scheme."** It does not. Listing `Oblivion - Textures - Compressed.bsa`
   independently of engine code yields 200 files under
   `textures\landscapelod\generated\` across 17 worldspace ids, and every
   component of the generated string — decimal low-24-bit FormID, `"00"` for a
   zero coordinate, the `32` quad size, the `_fn` normal suffix — matches
   byte-for-byte. Tamriel (`60` = `0x3C`) carries 36 quads spanning `-96`..`64`;
   `div_euclid` handles the negative half correctly.

2. **"`LC-D2-01` (mesh-water `blend_normals` bit 16) has an Oblivion blast
   radius."** It has none. `BSWaterShaderProperty` is `#SKY_AND_LATER#` in
   nif.xml and appears nowhere in the 82-type vanilla Oblivion block histogram.
   Oblivion cell water is `env_translate`-only.

3. **"Oblivion's `WaterKind` classification is wrong because none of its 23
   WATR records classifies as `River`."** Checked and *not* a defect: no Oblivion
   EditorID contains `river`/`stream`/`falls`/`rapid`, and Oblivion authors no
   `NAM0` linear velocity or `NAM5` flow texture — Bethesda genuinely applied
   the same `DefaultWater` to the Niben and to every lake. `Calm` is the correct
   canonical answer for all 23. (The lava records are a separate problem —
   `OBL-2026-08-20-D3-01` — and the fix there is `MNAM`, not a keyword.)

4. **"The `#3082` fix regressed something."** No — it strictly added a `parsed`
   assertion and moved the count out of a `#`-comment into a parsed line. What it
   *didn't* do is the other half of its own title (`OBL-2026-08-20-D6-01`).

5. **"Oblivion still has 6 truncating NIFs."** Stale. Baseline is empty at HEAD.
   Reported only as documentation rot (`OBL-2026-08-20-D7-01`), never as a live
   parse defect — per the suite briefing.

6. **"`WATR.GNAM` (day/night/underwater related waters) is dropped."** It is
   parsed (`water.rs:1363`). 18 of 23 Oblivion records author it.

---

## Scratch Artifacts

`/tmp/audit/oblivion/` — `bsa.py` (standalone BSA v103 lister), `extract.py`
(zlib per-file extractor), `watr.py` / `watr2.py` (TES4 WATR sub-record dump +
tail-damage hypothesis test), `cellwatr.py` (CELL/WRLD → WATR reference census).
All are read-only probes against the vanilla install; none was added to the repo.

Not covered, and why:
- **No fresh `nif_stats` / `recovery_trace` sweep** (Dim 6) and **no test
  execution** anywhere — the suite briefing forbids `cargo` while 25 agents
  contend on the target lock. Dim 6's parse figures come from the checked-in
  baseline regenerated one commit before HEAD; Dim 3's Oblivion parity tests
  (`clas_oblivion_knight_against_vanilla`,
  `race_oblivion_data_and_subs_against_vanilla`) were not re-run.
- **No on-device render verification** — no engine launch permitted. The
  inverted-normal impact in `OBL-2026-08-20-D5-01` is derived from the DDS
  contents and the shader's decode expression, not observed on screen.

---

TALLY: CRITICAL=0 HIGH=1 MEDIUM=2 LOW=2
