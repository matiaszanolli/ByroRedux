# Game Compatibility

ByroRedux targets the entire Bethesda Gamebryo / Creation engine lineage.
This doc tracks what works for each game, what's deferred, and what the
real measured numbers are.

> **Authoritative source**: the [ROADMAP.md compatibility
> matrix](../../ROADMAP.md#compatibility-matrix) is refreshed every
> `/session-close` and is the canonical ground truth for parse rates and
> per-cell status. This doc reconciles to it; where the two disagree,
> ROADMAP wins. Last reconciled 2026-07-11 (#1900 / NIF-D3-02).

The headline result: **every supported game parses its full mesh archive
*recoverably* at 100%** — every file links end-to-end (counting `NiUnknown`
placeholders and truncated trailers as recoverable). The stricter *clean*
rate (no `NiUnknown`, no truncation) is 100% on FO3 / FNV / Skyrim SE / FO4 /
FO76, 99.93% on Oblivion (6 pre-Gamebryo NetImmerse marker files with no
global type table), and 99.64% aggregate on Starfield (the MeshesPatch
terrain-overlay truncation tail, #746/#747) — see #1900 / NIF-D3-02.

## Parse-rate matrix

Clean = no `NiUnknown` placeholders + no truncation. Recoverable = file
parses end-to-end. Numbers from the
`cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored parse_rate`
sweep against unmodded retail data; refreshed 2026-07-11 (#1900 / NIF-D3-02
— the 2026-04-26/27 figures had gone stale-low by 2-4 points on every row
but Fallout 3 / FNV / Skyrim SE).

| Game              | Archive            | NIF clean rate            | Recoverable | Notes |
|-------------------|--------------------|---------------------------|-------------|-------|
| Oblivion          | BSA v103           | **99.93%** (8 026 / 8 032) | 100%        | `#687` recovered 83 truncations (NiGeomMorpherController + NiControllerSequence Phase). The corrupt-by-design debug marker (#698) is closed and no longer a hard failure. 6 v3.3.0.13 NetImmerse-era marker files (`meshes/marker_*.nif`) truncate; tracked in git log. |
| Fallout 3         | BSA v104           | **100%** (10 989)         | 100%        | — |
| Fallout New Vegas | BSA v104           | **100%** (14 881)         | 100%        | Reference title — most engine features shipped against FNV first. |
| Skyrim SE         | BSA v105 (LZ4)     | **100%** (18 862)         | 100%        | — |
| Fallout 4         | BA2 BTDX v1/v7/v8  | **100%** (34 995 + 124 871 MeshesExtra) | 100% | FaceGen truncation tail resolved (#1457, 2026-06-14). |
| Fallout 76        | BA2 BTDX v1 GNRL   | **100%** (58 469)         | 100%        | — |
| Starfield         | BA2 BTDX v2/v3 LZ4 | **99.64%** aggregate      | 100%        | Per-archive (all 5): Meshes01 100% (31 058), Meshes02 100% (7 552), MeshesPatch 98.91% (29 849), LODMeshes 100% (19 535), FaceMeshes 100% (1 282). Truncation tail in MeshesPatch is residual drift (#746/#747). |

The full multi-game sweep runs the seven `Game` variants in
[`crates/nif/tests/common/mod.rs`](../../crates/nif/tests/common/mod.rs)
(`Oblivion`, `Fallout3`, `FalloutNV`, `SkyrimSE`, `Fallout4`, `Fallout76`,
`Starfield`); the Starfield run walks all five vanilla mesh archives. Tests
skip cleanly when a game's data dir is unset (per-game `BYROREDUX_*_DATA`
env vars, with a development-machine Steam fallback path).

A per-block-type histogram gate
([`crates/nif/tests/per_block_baselines.rs`](../../crates/nif/tests/per_block_baselines.rs),
opt-in via `--ignored`) compares `parsed` vs `NiUnknown` counts against
checked-in TSV baselines for all seven games and fails on any unknown
growth or parsed shrinkage. `BYROREDUX_REGEN_BASELINES=1` regenerates after
intentional changes. A cross-game translation-completeness harness
([`crates/nif/tests/translation_completeness.rs`](../../crates/nif/tests/translation_completeness.rs),
#1277 Task 8) tracks how much of each parsed scene survives the NIF→ECS
translation boundary (see [NIFAL](nifal.md)).

## Per-game support detail

The Tier structure below reflects how far each game gets *end-to-end*
(parse → archive → cell load → render), not just parse rate. As of
Session 42, every Tier-1/2 game renders at least one cell; Oblivion's
gap is its exterior worldspace wiring, not its parser or archive.

### Tier 1: Working end-to-end (interior + exterior, RT lighting)

#### Fallout: New Vegas

- **NIF parser**: 14,881 / 14,881 (100% clean)
- **Archive**: BSA v104 ✓ (zlib compression)
- **ESM parser**: cell walker + the full M24 records pass (items, NPCs,
  factions, leveled lists, globals, game settings); M47.0/M47.1 event +
  condition runtime; M24.2 QUST stage/objective + PERK decoders are
  landing (Session 42).
- **Cell loading**: interior cells (Prospector Saloon — **3507 entities
  @ 71.4 FPS / 14.00 ms / fence=11.65 ms** on RTX 4070 Ti, R6a-stale-13
  bench `4e2ebe8c` 2026-05-28; entity count grew ~37% vs the prior 2564
  record because FNV architecture ships no `bhk` collision, so each piece
  spawns a synthesized static-trimesh collider and an RT BLAS — the
  collider-cost regression is tracked as R6a-stale-13-collider-cost) +
  exterior 7×7 grid (radius 3) via M32 landscape (LAND heightmap +
  LTEX/TXST splatting) + M34 directional sun.
- **Lighting**: XCLL ambient + directional, multi-light SSBO with point
  lights from LIGH records, RT shadow rays per light, candle/chandelier
  flicker (Phase 17).
- **Coordinate system**: Z-up→Y-up with CW rotation handling.
- **Status**: the canonical "demo path."

```bash
cargo run -- --esm FalloutNV.esm \
             --cell GSProspectorSaloonInterior \
             --bsa "Fallout - Meshes.bsa" \
             --textures-bsa "Fallout - Textures.bsa" \
             --textures-bsa "Fallout - Textures2.bsa"
```

#### Fallout 3

- **NIF parser**: 10,989 / 10,989 (100% clean)
- **Archive**: BSA v104 ✓ (same reader as FNV)
- **ESM parser**: same record set as FNV (FO3 and FNV share the engine).
  Oblivion-shared CLAS/RACE arms were extended for FO3-era layouts (#967/#968).
- **Cell loading**: Megaton Player House interior carries 929 REFRs
  on-disk (validated via `parse_real_fo3_megaton_cell_baseline`). Exterior
  `wasteland` worldspace is on the auto-pick list (#444):
  `--esm Fallout3.esm --grid 0,0 --bsa 'Fallout - Meshes.bsa'
  --textures-bsa 'Fallout - Textures.bsa'`. Like FNV, FO3 architecture
  ships no `bhk` collision and synthesizes static-trimesh colliders.
- **Status**: identical pipeline to FNV.

#### Skyrim SE

- **NIF parser**: 18,862 / 18,862 (100% clean)
- **Archive**: BSA v105 ✓ (LZ4 frame compression)
- **NIF support**: BSTriShape (packed vertex format),
  BSLightingShaderProperty (8 shader-type variants; `BSLightingShaderProperty::parse`
  itself was split into three BSVER-keyed per-variant parsers —
  `parse_skyrim` / `parse_fo4` / `parse_fo76_plus` — via #1279),
  BSEffectShaderProperty, NiAVObject
  conditional layout fixes; #638 added the SSE 12-byte VF_SKINNED skin
  payload decode for M29 GPU skinning. SSE-reconstructed BSTriShape routes
  through Y-up tangent synthesis (#1204) and carries a `BsTriShapeKind`
  discriminator (#1206/#1207).
- **ESM parser**: 92-byte XCLL sub-records parse cleanly (validated against
  `Skyrim.esm`); TES5 Localized-flag + lstring placeholder handling for
  `FULL` / `DESC` (#348). The records-side parser is largely game-agnostic
  and reuses the FNV implementation.
- **Cell loading**: WhiterunBanneredMare renders end-to-end —
  **3211 entities @ 329.8 FPS / 3.03 ms / 1296 draws / fence=1.01 ms**
  (R6a-stale-13 bench `4e2ebe8c` 2026-05-28; **+14.6% FPS** vs the prior
  287.8 record at identical entity count — Whiterun is the *control* bench
  proving the steady-state hot path did not regress over the 125-commit
  window). Skyrim ships real `bhk` collision, so it did **not** grow the
  synthesized-collider count that slowed FNV/FO4. The cell loads 246 unique
  textures across `Skyrim - Textures0..8.bsa` — the repro must list all
  nine archives explicitly, since the asset-provider's numeric-sibling
  auto-load gates on a non-digit suffix.
- **Status**: parser + archive + cell loader all live; BGSM material
  resolver + per-shader-variant texture routing are mature in the
  FO4-shared pipeline.

```bash
cargo run -- --bsa "Skyrim - Meshes0.bsa" \
             --mesh "meshes\clutter\ingredients\sweetroll01.nif" \
             --textures-bsa "Skyrim - Textures3.bsa"
```

#### Fallout 4

- **NIF parser**: 34,995 / 34,995 + 124,871 / 124,871 MeshesExtra (100%
  clean, 100% recoverable, #1457) — across both BA2 v1 (original release)
  and v7/v8 (Next Gen update) archives
- **Archive**: BA2 BTDX v1/v7/v8 GNRL + DX10 ✓ — verified across the
  vanilla archive set, see [Archives](archives.md)
- **NIF support**: BSTriShape FO4 packed vertex format with
  VF_FULL_PRECISION bit + half-float vertices, FO4 shader flags (u32 pair),
  BSLightingShaderProperty FO4 trailing fields (subsurface, rimlight,
  backlight, fresnel, wetness), FO4 shader-type extras, BSSubIndexTriShape,
  BSClothExtraData, `BSConnectPoint::` family.
- **Cell loading**: renders end-to-end — MedTekResearch01 interior
  **15546 entities @ 90.7 FPS / 11.02 ms / 8304 draws / brd=2.63 ms /
  fence=4.73 ms** (R6a-stale-13 bench `4e2ebe8c` 2026-05-28; entities grew
  ~42% vs the prior 10 913 record due to the same synthesized-collider
  growth as FNV, but `build_render_data` improved 7.81 → 2.63 ms so the
  frame is now GPU-bound, not CPU-bound). The ESM parser handles `SCOL`,
  `MOVS`, `PKIN`, `TXST` (FO4's prefab-architecture building blocks; #584/
  #585/#589) plus `MSWP` material swaps (#590/#971), all gated on FO4+
  GameKind (#1277 Task 3). `asset_provider` auto-detects BSA vs BA2 from
  file magic.
- **PreCombined Mesh pipeline** (#1188 / #1220 / #1221 / #1222): the CELL
  walker parses `XCRI` (precombined-mesh hash list) + `XPRI` (absorbed-REFR
  form-IDs) on both interior and exterior cells;
  `byroredux::cell_loader::precombined::spawn_precombined_meshes` walks the
  hash list. Vanilla FO4 currently takes the fallback path — the `_oc.nif`
  precombine geometry lives in a `Fallout4 - Geometry.csg` companion blob
  not yet parsed, so the loader renders each absorbed architecture REFR
  individually (`bUseCombinedObjects=0` semantics) rather than producing a
  void floor.
- **BGSM / BGEM materials**: external material files are now **parsed** by
  the dedicated [`crates/bgsm`](../../crates/bgsm) crate (BGSM base + BGEM
  effect + cycle-aware template resolver, #1148). BGSM
  spec-glossiness → metallic-roughness translation, PBR / translucency /
  model-space-normal flags, and standalone texture slots all flow through
  to `ImportedMesh` and the canonical `Material` (FO4-D6-003 / #1076 /
  #1077). The `Material.material_path` string surfaces in `mesh.info` over
  the debug CLI.
- **Status**: parser + archive + cell loader + BGSM/BGEM materials all
  live; full quest/dialog/perk ESM (QUST / DIAL / INFO / PERK) is the
  M24.2 surface, landing incrementally.

### Tier 2: Walkable interior bring-up

#### Starfield

- **NIF parser**: 31,058 / 31,058 on Meshes01; **99.64% aggregate clean**
  (100% recoverable) across all five mesh archives — MeshesPatch (98.91%)
  is the sole sub-100% archive, residual drift tracked under #746/#747
- **Mesh archive**: BA2 BTDX v2 GNRL ✓ (32-byte header, +8-byte extension)
- **Texture archive**: BA2 BTDX v3 DX10 ✓ — verified against the 30
  vanilla Starfield texture archives (see [Archives](archives.md)). The v3
  header has a 12-byte extension (vs 8 for v2) carrying a
  `compression_method` field: 0 = zlib, 3 = LZ4 block. v2 DX10 also exists
  in vanilla. Both GNRL and DX10 extraction are fully supported for v2 and
  v3.
- **Material database**: vanilla Starfield ships all material data inside a
  single binary component database (`materials\materialsbeta.cdb` packaged
  in `Starfield - Materials.ba2`), parsed by the dedicated
  [`crates/sfmaterial`](../../crates/sfmaterial) crate (`byroredux-sfmaterial`,
  #762). The CDB consumer loads the database and a `.mat` path arm flips
  `is_pbr=true` → `MAT_FLAG_PBR_BSDF` (#1289). Loose `.mat` JSON files (CK /
  mod output) are a future Stage A.
- **NIF support**: same FO76+ shader flag arrays + stopcond as FO76, with
  BSVER ≥ 168 (retail; the historical FO76-vs-Starfield cutoff was 170).
  `BSGeometry` extraction iterates every LOD slot (#1209), resolves
  external `.mesh` files via the `geometries\<hash>.mesh` canonical path
  (#1292), and routes empty tangents through Y-up Mikkelsen synthesis
  (#1232/#1293).
- **Cell loading**: **walkable Cydonia interior** (Session 42 bring-up,
  #1289 / #1291 / #1292 / #1294 / #1295). Empirical measurement via the
  `sf_smoke` walker + `sf_parse_check` bridge showed the existing parser
  already captured ~99.9% of vanilla `Starfield.esm` records (11 985
  interior + 18 424 exterior cells / 3 287 923 REFRs / 41 620 STAT-family
  base objects in ~4 s), collapsing the original 7–11-session Starfield
  roadmap to 3–4. The bring-up arc:
  - #1291 — split Starfield off its own XCLL canonical-size set `[28, 108]`
    (was bucketed with the FNV-era `[28, 40]`; silenced 11 985 warns). The
    108-byte body is the Skyrim+ 92-byte layout plus a 16-byte tail.
  - #1292 — `BSGeometry` external `.mesh` resolution took cell entities from
    75 to 93 547 (a ~1 247× spawn-rate jump).
  - #1294 — static-trimesh-fallback gate moved from `final_layer` to
    `base_layer`: SF NIFs are sub-decomposed per-LOD per-material and trip
    the small-static (<50-unit) Clutter escalation, which silently lost
    colliders; the fix took the synthesized collider count from 0 to
    91 698, so the player is `grounded=true` from frame 0.
  - #1295 — spawn-degradation diagnostic.
  - First-render audit at
    [`docs/audits/SF_FIRST_RENDER_2026-05-28.md`](../audits/SF_FIRST_RENDER_2026-05-28.md).
- **Skin headroom**: #1284 bumped the `SkinSlotPool` ceiling (32 768 →
  49 152 → 196 608 bones) to cover Cydonia's skinned-mesh density.
- **Status**: meshes + textures + CDB materials + cell load all live;
  follow-ups filed (#1290 SF-D6 roadmap re-ordering, #1293 16-byte SF XCLL
  tail decode).

```bash
cargo run -- --esm Starfield.esm \
             --cell citycydoniamainlevel \
             --bsa "Starfield - Meshes01.ba2" \
             --textures-bsa "Starfield - Textures01.ba2" \
             --materials-ba2 "Starfield - Materials.ba2"
```

### Tier 3: Parser + archive complete, cell loader pending

#### Oblivion

- **NIF parser**: 8,026 / 8,032 (99.93% clean, 100% recoverable) —
  recoverable rate bumped from the longstanding gap once the M26+ per-block
  recovery path made the failures legible; the residual 6 files are the
  pre-Gamebryo NetImmerse marker placeholders below, not a hard failure
- **Archive**: BSA v103 ✓ (147 629 / 147 629 vanilla files extract cleanly
  across all 17 Oblivion BSAs; the old "v103 decompression broken" framing
  was a stale premise, closed via #699)
- **NIF support**: all 15 Oblivion-specific block types from N23.3 plus the
  M26+ header parser fixes for v10.0.1.0 and v10.0.1.2 NetImmerse files.
  See [NIF Parser — Header parser](nif-parser.md#header-parser).
- **Pre-Gamebryo NetImmerse** (v3.3.0.13): the 6 `meshes/marker_*.nif`
  debug placeholders inline each block's type name and have no global type
  table; the parser returns an empty scene for them. They're filtered by
  the marker-name filter at render time anyway.
- **Cell loading**: interior cells render (Anvil Heinrich Oaken Halls). The
  cell walker handles Oblivion XCLL (the `[28, 32, 36]` canonical-size set).
  **Exterior** is blocked on TES4 worldspace + LAND wiring — the same shape
  FO3's exterior was before it landed, *not* an archive or parser gap. This
  is covered by the M40 world-streaming track; no separate tracker.
- **Status**: parser + archive complete, interior renders, exterior gated
  on worldspace wiring.

#### Fallout 76

- **NIF parser**: 58,469 / 58,469 (100% clean, 100% recoverable)
- **Archive**: BA2 BTDX v1 GNRL + DX10 ✓
- **NIF support**: BSVER 155+ shader stopcond — non-empty Name = BGSM file
  path, rest of the block absent. CRC32-hashed shader flag arrays
  (`Num SF1` / `SF1[]` since BSVER ≥ 132, `Num SF2` / `SF2[]` since
  BSVER ≥ 152). `BSShaderType155` enum. `BSSPLuminanceParams`,
  `BSSPTranslucencyParams`, `BSTextureArray`. Post-FO4 BSEffectShader
  quintet now captured (#1205); inherited NiVertexColorProperty gated so it
  doesn't clobber BGSM-derived flags (#1208).
- **Header parser fix** (M26+): BSVER > 130 inserts an `Unknown Int u32`
  after Author and **drops** Process Script.
- **Cell loading**: not yet started (no FO76 ESM stub).
- **Status**: parser + archive complete, no cell loader.

## NIF Abstraction Layer (NIFAL)

Parsing a NIF correctly is necessary but not sufficient — a parsed block
must survive *translation* into the engine's canonical ECS/material/scene
types to actually render. The [NIFAL](nifal.md) tier (the canonical
translation boundary) tracks per-category translation completeness. As of
Session 42 the material, geometry/transform, skinning, and light slices are
converged; the particle slice authors emitter base params + birth rate +
grow/fade size; node passthrough is triaged; and a collision audit
confirmed all 13 parsed `bhk*Shape` variants now translate (#1277 epic;
`BhkMultiSphereShape` + `BhkConvexListShape` were the last two leaks). The
material translate boundary is
`byroredux/src/material_translate.rs::translate_material`, where per-game
shader properties resolve to a single PBR `Material` with plain `f32`
scalars — the governing rule is: never branch the shader/renderer per
game, translate at the parser→`Material` boundary instead.

## Achievements

### N23 — NIF parser overhaul (10/10 milestones)

| | | |
|---|---|---|
| N23.1 | Trait hierarchy + FNV audit | DONE |
| N23.2 | BSLightingShaderProperty completeness | DONE |
| N23.3 | Oblivion block types | DONE |
| N23.4 | FO3/FNV validation | DONE |
| N23.5 | Skinning blocks | DONE |
| N23.6 | Havok collision (full parse) | DONE |
| N23.7 | Fallout 4 support | DONE |
| N23.8 | Particle systems | DONE |
| N23.9 | Fallout 76 / Starfield shader stopcond + CRC32 arrays | DONE |
| N23.10 | Test infrastructure + per-block parse recovery | DONE |

### Format readers

| | |
|---|---|
| BSA v103 (Oblivion) | DONE — M26+ per-block recovery + header fixes; v103 extract verified across all 17 Oblivion BSAs (#699) |
| BSA v104 (FO3 / FNV / Skyrim LE) | DONE — M11 |
| BSA v105 (Skyrim SE) | DONE — M18 (LZ4 frame) |
| BA2 BTDX v1 GNRL (FO4 original / FO76) | DONE — M26 |
| BA2 BTDX v2 GNRL+DX10 (Starfield, FO4 patches) | DONE — M26 |
| BA2 BTDX v3 DX10 (Starfield textures, zlib / LZ4 block) | DONE — Session 7 |
| BA2 BTDX v7 DX10 (FO4 Next Gen textures) | DONE — M26 |
| BA2 BTDX v8 GNRL (FO4 Next Gen meshes) | DONE — M26 |
| Starfield CDB (`materialsbeta.cdb`) | DONE — `byroredux-sfmaterial` (#762) |
| BGSM / BGEM (FO4 / FO76 / Skyrim material files) | DONE — `byroredux-bgsm` (base + effect + template resolver, #1148) |

### ESM record parser

| | |
|---|---|
| Cell + WRLD + REFR walker | DONE — M16 / M19 |
| MODL-bearing base records (~24 types) | DONE — M19 |
| Items (WEAP, ARMO, AMMO, MISC, KEYM, ALCH, INGR, BOOK, NOTE) | DONE — M24 Phase 1 |
| Containers + leveled lists (CONT, LVLI, LVLN, LVLC) | DONE — M24 Phase 1 |
| Actors (NPC_, CREA, RACE, CLAS, FACT) | DONE — M24 Phase 1 |
| Globals + game settings (GLOB, GMST) | DONE — M24 Phase 1 |
| Scripts (SCPT pre-Papyrus bytecode + VMAD) | DONE |
| FO4 prefab architecture (SCOL, MOVS, PKIN, TXST, MSWP) | DONE — gated on FO4+ GameKind |
| Conditions (CTDA) + ConditionFunction catalog | DONE — M47.1 |
| QUST (stage + objective), PERK (Quest/Ability/EntryPoint) | In progress — M24.2 Phase 1 |
| DIAL / INFO / MGEF / SPEL / ENCH / AVIF | Deferred — M24.2 surface |

## Known gaps and follow-ups

### Cell loaders for Oblivion exterior / FO76

The cell walker lives in
[`crates/plugin/src/esm/cell/`](../../crates/plugin/src/esm/cell)
(`walkers.rs` carries the CELL/REFR walk; `wrld.rs` the exterior WRLD
walk; `helpers.rs` / `support.rs` the per-feature decoders). It handles
FNV / FO3 / Skyrim SE / FO4 / Starfield today; Oblivion interior renders
and Oblivion exterior is gated on the TES4 worldspace + LAND wiring (M40).
FO76 has no cell loader yet (no ESM stub). The records-side parser
(`records/`) is game-agnostic — it reads by sub-record code, migrated to a
sequential `SubReader` cursor across all 169 field-read sites (R2 Phase B).

The XCLL canonical-size sets are pinned in `cell/walkers.rs`:
Oblivion `[28, 32, 36]`, Fallout-era `[28, 40]`, Skyrim `[28, 92]`,
Starfield `[28, 108]`, with an `xcll_size_sanity_warn` helper that warns at
WARN level on a non-canonical size.

### FO4 PreCombined Mesh CSG companion

FO4's `_oc.nif` precombine geometry lives in a `Fallout4 - Geometry.csg`
companion blob that isn't parsed yet, so the loader takes the
render-the-absorbed-REFRs fallback path. See the post-mortem at
[`docs/audits/POST_MORTEM_2026-05-19_PRECOMBINED.md`](../audits/POST_MORTEM_2026-05-19_PRECOMBINED.md).

### NIF v3.3.0.13 inline-block-name support

The 6 `meshes/marker_*.nif` files in Oblivion are pre-Gamebryo NetImmerse
v3.3.0.13. They inline each block's type name as a sized string instead of
using a global type table; we return an empty scene for them (debug
placeholders, filtered at render time). A non-marker v3.x NIF would need a
sequential block-with-inline-name walker.

### Long-tail parser drift

The residual clean-rate gaps on Oblivion / FO4 / FO76 / Starfield are
truncation drift on edge-case blocks, surfaced by the per-block baseline
gate and tracked in git log (#687/#688/#697/#698 closed; #746/#747 for the
Starfield tail). All four games stay at 100% recoverable. One Starfield NIF
(`meshes\marker_radius.nif`) requests a 318 MB single-buffer allocation
exceeding the 256 MB per-allocation cap and is intentionally rejected
(one file out of 320 483 in the mesh corpus).

## How to add a new game

If a new Bethesda title ships:

1. **Identify** the NIF version and `BSStreamHeader` BSVER. The version is
   at offset 39 of the file; the BSVER is the first u32 after the basic
   header.
2. **Add** a variant to `NifVariant` in
   [`crates/nif/src/version.rs`](../../crates/nif/src/version.rs) and update
   `NifVariant::detect()` for the new `(user_version, user_version_2)`
   ranges (the existing tuple ladder fans Oblivion → Starfield).
3. **Add** any new feature flags it needs on `NifVariant`. Existing flags
   cover the major splits — `uses_fo4_shader_flags()`,
   `has_dedicated_shader_refs()`, etc. Prefer named helpers over raw BSVER
   compares (#1277 Task 5 migrated the variant-aligned raw compares to
   helpers); the typed `ShaderFlags` variant view (#1277 Task 6) is the
   pattern for new shader-flag families.
4. **Identify** the archive format. For a new BA2 BTDX version, add it to
   [`Ba2Archive::open()`](../../crates/bsa/src/ba2.rs); if the header layout
   differs, follow the v2/v3 8/12-byte extension pattern.
5. **Run** `parse_real_nifs.rs` against a sample BSA / BA2 with a new `Game`
   enum entry in
   [`crates/nif/tests/common/mod.rs`](../../crates/nif/tests/common/mod.rs).
   Anything below 95% clean is usually a few extra fields in some block for
   the new BSVER — patch the relevant `blocks/*.rs` parser. Then check the
   per-block baseline TSV and the translation-completeness harness so the
   parsed blocks actually translate (see [NIFAL](nifal.md)).
6. **For cells**, add the game's XCLL canonical-size set to `cell/walkers.rs`
   and wire its `GameKind` arm; validate against one interior cell. The
   Starfield bring-up (Session 42) followed exactly this path — the parser
   already covered most records, and the work was XCLL sizing + mesh-path
   resolution + collider gating, not new block parsers.

## Reference materials

- [`docs/legacy/nif.xml`](../legacy/nif.xml) — niftools' authoritative NIF
  format spec; every parser cross-references it
- [Gamebryo 2.3 Architecture](../legacy/gamebryo-2.3-architecture.md)
- [API Deep Dive](../legacy/api-deep-dive.md) — `NiObject` / `NiAVObject` /
  `NiStream` class hierarchy

## Related docs

- [NIF Parser](nif-parser.md) — block coverage, version handling, robustness
- [NIFAL](nifal.md) — the NIF→ECS/material translation boundary + per-category completeness
- [Archives](archives.md) — BSA + BA2 reader catalog
- [ESM Records](esm-records.md) — record category catalog
- [Testing](testing.md) — how to run the per-game integration sweeps
- [ROADMAP](../../ROADMAP.md) — authoritative compat matrix + full milestone history
