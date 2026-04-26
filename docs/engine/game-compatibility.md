# Game Compatibility

ByroRedux targets the entire Bethesda Gamebryo / Creation engine lineage.
This doc tracks what works for each game, what's deferred, and what the
real measured numbers are.

The headline result: **every supported game parses its full mesh archive
at 100%**. The aggregate is 177,286 NIFs across 7 games with zero failures.

## Parse rate matrix

Latest run of `cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored`:

| Game              | Archive            | NIFs in archive | NIFs parsed       | Rate    |
|-------------------|--------------------|-----------------|-------------------|---------|
| Oblivion          | BSA v103           | 8,032           | **8,032**         | **100.00%** |
| Fallout 3         | BSA v104           | 10,989          | **10,989**        | **100.00%** |
| Fallout New Vegas | BSA v104           | 14,881          | **14,881**        | **100.00%** |
| Skyrim SE         | BSA v105 (LZ4)     | 18,862          | **18,862**        | **100.00%** |
| Fallout 4         | BA2 BTDX v1/v7/v8  | 34,995          | **34,995**        | **100.00%** |
| Fallout 76        | BA2 BTDX v1 GNRL   | 58,469          | **58,469**        | **100.00%** |
| Starfield         | BA2 BTDX v2 GNRL   | 31,058          | **31,058**        | **100.00%** |
| **Total**         |                    | **177,286**     | **177,286**       | **100.00%** |

Numbers are from the `parse_real_nifs.rs` integration test suite running
against unmodded retail game data. The full sweep across all 7 games
takes ~20 seconds release-mode on a Ryzen 9 7950X.

## Per-game support detail

### Tier 1: Working end-to-end

These games load cells, render them with full RT lighting, and have NIF
parsers at 100% on the full mesh archive.

#### Fallout: New Vegas

- **NIF parser**: 14,881 / 14,881 (100%)
- **Archive**: BSA v104 ✓ (zlib compression)
- **ESM parser**: ~23 record types via cell parser, plus the full M24
  records pass (items, NPCs, factions, leveled lists, globals)
- **Cell loading**: interior cells (Prospector Saloon: 809 entities,
  784 draws — historical M22 bench cited 85 FPS but predates M29 GPU
  skinning + M31.5 + M36 + M37 + M37.5; re-bench pending per #456) +
  exterior 3×3 / 7×7 grid (WastelandNV via M32 Phase 1+2 landscape
  with LTEX/TXST splatting + M34 Phase 1 directional sun)
- **Lighting**: XCLL ambient + directional, multi-light SSBO with point
  lights from LIGH records, RT shadow rays per light
- **Coordinate system**: Z-up→Y-up with CW rotation handling
- **Status**: this is the canonical "demo path" — most engine features
  shipped against FNV first

```bash
cargo run -- --esm FalloutNV.esm \
             --cell GSProspectorSaloonInterior \
             --bsa "Fallout - Meshes.bsa" \
             --textures-bsa "Fallout - Textures.bsa" \
             --textures-bsa "Fallout - Textures2.bsa"
```

#### Fallout 3

- **NIF parser**: 10,989 / 10,989 (100%)
- **Archive**: BSA v104 ✓ (same reader as FNV)
- **ESM parser**: same record set as FNV (FO3 and FNV share the engine)
- **Cell loading**: Megaton Player House interior carries 929 REFRs
  on-disk (validated 2026-04-19 via
  `parse_real_fo3_megaton_cell_baseline`). Post-NIF-expansion entity
  count from the N23.4 demo was ~1609 / 199 textures / 42 FPS;
  that figure predates M31 / M36 / M37 / M37.5 and needs a fresh
  GPU bench — tracked as #456.
- **Exterior**: `wasteland` worldspace now on the auto-pick list
  (#444); `--esm Fallout3.esm --grid 0,0 --bsa 'Fallout - Meshes.bsa'
  --textures-bsa 'Fallout - Textures.bsa'` is the supported entry
  point. End-to-end GPU bench tracked as #457.
- **Status**: identical pipeline to FNV

### Tier 2: Working for assets, no cell loading yet

These games' NIFs all parse, archive extraction works, but the cell
loader path hasn't been validated (the ESM parser works but no end-to-end
demo has been wired up).

#### Skyrim SE

- **NIF parser**: 18,862 / 18,862 (100%)
- **Archive**: BSA v105 ✓ (LZ4 frame compression)
- **NIF support**: BSTriShape (packed vertex format),
  BSLightingShaderProperty (8 shader-type variants),
  BSEffectShaderProperty, NiAVObject conditional layout fixes — all
  from M18; #638 added the SSE 12-byte VF_SKINNED skin payload decode
  for M29 GPU skinning support
- **ESM parser**: 92-byte XCLL sub-records parse cleanly (validated
  against `Skyrim.esm` — see `parse_real_skyrim_esm`); TES5 Localized
  flag + lstring placeholder handling for `FULL` / `DESC` fields
  (#348). The records-side parser (items, NPCs, factions, leveled
  lists) is largely game-agnostic and reuses the FNV implementation.
- **Cell loading**: WhiterunBanneredMare and similar interior cells
  load end-to-end (~2400 meshes, 2700 draws observed). BGSM material
  resolver + per-shader-variant texture routing are still maturing
  in the FO4-shared pipeline — the rainbow-hearth-flame artifact in
  Whiterun (a `BSEffectShader` mis-routed slot) is the canonical
  open issue at the time of writing.
- **Loose-mesh entry point**:

```bash
cargo run -- --bsa "Skyrim - Meshes0.bsa" \
             --mesh "meshes\clutter\ingredients\sweetroll01.nif" \
             --textures-bsa "Skyrim - Textures3.bsa"
```

- **Status**: parser + archive + cell loader all live; shader-side
  material routing for SSE-specific variants is the active surface.

#### Oblivion

- **NIF parser**: 8,032 / 8,032 (100%) — bumped from 99.13% in M26+
- **Archive**: BSA v103 ✓ (was the longest-deferred archive format;
  finally validated during M26+ when the per-block recovery path made the
  Oblivion failures legible)
- **NIF support**: All 15 Oblivion-specific block types from N23.3 plus
  the M26+ header parser fixes for v10.0.1.0 and v10.0.1.2 NetImmerse
  files. See [NIF Parser — Header parser](nif-parser.md#header-parser)
  for the three subtle fixes.
- **Pre-Gamebryo NetImmerse** (v3.3.0.13): the 6 `meshes/marker_*.nif`
  debug placeholders inline each block's type name and don't have a
  global type table; the parser returns an empty scene for them. They're
  filtered out by the M17 marker name filter at render time anyway.
- **Cell loading**: ESM parser is stubbed (`legacy/tes4.rs`)
- **Status**: parser side complete, cell loader needs the Oblivion ESM parser

#### Fallout 4

- **NIF parser**: 34,995 / 34,995 (100%) — across both BA2 v1 (original
  release) and v8 (Next Gen update) archives
- **Archive**: BA2 BTDX v1/v7/v8 GNRL + DX10 ✓ — 53 vanilla archives verified, see [Archives](archives.md)
- **NIF support**: BSTriShape FO4 packed vertex format with VF_FULL_PRECISION
  bit + half-float vertices, FO4 shader flags (u32 pair), BSLightingShaderProperty
  FO4 trailing fields (subsurface, rimlight, backlight, fresnel, wetness),
  FO4 shader-type extras (SSR bools, skin tint alpha), BSSubIndexTriShape,
  BSClothExtraData, BSConnectPoint:: family — all from N23.7
- **Header parser fix** (M26+): `BSStreamHeader` reads `Max Filepath` for
  BSVER ≥ 103, which fixed FO4 parsing entirely
- **Cell loading**: partial — ESM parser now handles `SCOL`, `MOVS`, `PKIN`,
  and `TXST` record types (the building blocks of FO4's prefab architecture).
  `asset_provider` auto-detects BSA vs BA2 from file magic. Full architecture
  placements render; full ESM (QUST / DIAL / PERK / INFO / etc.) is still
  deferred.
- **BGSM / BGEM references**: `BSLightingShaderProperty.net.name` flows
  through `ImportedMesh` → `Material.material_path` and surfaces in
  `mesh.info` over the debug CLI. The external BGSM / BGEM files themselves
  are not yet parsed (next milestone in the FO4 track).
- **Status**: parser + archive complete, cell loader renders architecture,
  material file parsing pending.

#### Fallout 76

- **NIF parser**: 58,469 / 58,469 (100%)
- **Archive**: BA2 BTDX v1 GNRL + DX10 ✓
- **NIF support**: BSVER 155 (FO76) shader stopcond — non-empty Name = BGSM
  file path, rest of the block is absent. CRC32-hashed shader flag arrays
  (`Num SF1` / `SF1[]` since BSVER ≥ 132, `Num SF2` / `SF2[]` since BSVER ≥ 152).
  `BSShaderType155` enum with type 4 = skin tint Color4, type 5 = hair
  tint Color3. `BSSPLuminanceParams`, `BSSPTranslucencyParams`,
  `BSTextureArray` lists. All from N23.9.
- **Header parser fix** (M26+): BSVER > 130 inserts an `Unknown Int u32`
  after Author and **drops** Process Script — wrong threshold corrupted
  every FO76 NIF before the fix
- **Cell loading**: not yet started (no ESM parser stub)
- **Status**: parser side complete, no cell loader

### Tier 3: Working for meshes and textures

#### Starfield

- **NIF parser**: 31,058 / 31,058 (100%)
- **Mesh archive**: BA2 BTDX v2 GNRL ✓ (32-byte header with 8-byte extension, zlib)
- **Texture archive**: BA2 BTDX v3 DX10 ✓ — 22 archives, ~128K textures,
  LZ4 block compression. The v3 header has a 12-byte extension (vs. 8
  for v2) containing a `compression_method` field. DX10 base record and
  chunk layouts are identical to FO4 v1/v7.
- **NIF support**: same FO76+ shader flag arrays + stopcond as FO76,
  with BSVER ≥ 170. Inherits all of N23.9.
- **Cell loading**: not yet started; Starfield's ESM format also has a
  different version code that hasn't been mapped
- **Status**: meshes parse perfectly; textures deferred; no cell loader

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
| BSA v103 (Oblivion) | DONE — M26+ per-block recovery + header fixes pushed to 100% |
| BSA v104 (FO3 / FNV / Skyrim LE) | DONE — M11 |
| BSA v105 (Skyrim SE) | DONE — M18 (LZ4 frame) |
| BA2 BTDX v1 GNRL (FO4 original / FO76) | DONE — M26 |
| BA2 BTDX v2 GNRL (Starfield meshes) | DONE — M26 |
| BA2 BTDX v3 GNRL (Starfield meshes patches) | DONE — M26 |
| BA2 BTDX v3 DX10 (Starfield textures, LZ4) | DONE — session 7 |
| BA2 BTDX v7 DX10 (FO4 textures) | DONE — M26 |
| BA2 BTDX v8 GNRL (FO4 Next Gen meshes) | DONE — M26 |

### ESM record parser

| | |
|---|---|
| Cell + WRLD + REFR walker | DONE — M16 / M19 |
| MODL-bearing base records (~24 types) | DONE — M19 |
| Items (WEAP, ARMO, AMMO, MISC, KEYM, ALCH, INGR, BOOK, NOTE) | DONE — M24 Phase 1 |
| Containers + leveled lists (CONT, LVLI, LVLN) | DONE — M24 Phase 1 |
| Actors (NPC_, RACE, CLAS, FACT) | DONE — M24 Phase 1 |
| Globals + game settings (GLOB, GMST) | DONE — M24 Phase 1 |
| QUST / DIAL / INFO / PERK / MGEF / SPEL / ENCH | Deferred — M24 Phase 2 |

## Known gaps and follow-ups

### Cell loaders for Oblivion / FO76 / Starfield

The cell parser in [`crates/plugin/src/esm/cell.rs`](../../crates/plugin/src/esm/cell.rs)
handles FNV / FO3 / Skyrim SE / FO4 today. Adding the rest is a
per-game effort: write a small XCLL variant table, validate against
one interior cell, and fix any per-game sub-record codes that differ.
The records-side parser (`records/`) is already game-agnostic since
it reads by sub-record code rather than fixed offsets.

FO4 cell loading covers `SCOL` / `MOVS` / `PKIN` / `TXST` (#584 / #585
/ #589) plus `MSWP` material swaps (#590); the FO4 prefab-architecture
records render today, while quest / dialog / perk records are still
deferred (M24 Phase 2).

### ~~Starfield BA2 v3 DX10 textures~~ — RESOLVED (session 7)

The v3 DX10 texture gap is now closed. See
[Archives — Resolved gaps](archives.md#resolved-gaps-session-7).

### NIF v3.3.0.13 inline-block-name support

The 6 `meshes/marker_*.nif` files in Oblivion are pre-Gamebryo NetImmerse
v3.3.0.13. They inline each block's type name as a sized string instead
of using a global type table. We currently return an empty scene for
them — they're debug placeholders that get filtered at render time
anyway. If a non-marker v3.x NIF ever shows up we'd need to add a
sequential block-with-inline-name walker.

## How to add a new game

If a new Bethesda title ships, the steps to add it would be:

1. **Identify** the NIF version and `BSStreamHeader` BSVER. Open one of
   the game's NIFs in a hex editor; the version is at offset 39 of the
   file and the BSVER is the first u32 after the basic header.
2. **Add** a new variant to `NifVariant` in
   [`crates/nif/src/version.rs`](../../crates/nif/src/version.rs) and
   update `NifVariant::detect()` for the new (uv, uv2) ranges.
3. **Add** any new feature flags it needs on `NifVariant`. The existing
   flags cover the major splits — look at `uses_fo76_shader_flags()`,
   `has_dedicated_shader_refs()`, etc.
4. **Identify** the archive format. If it's BA2 with a new BTDX version,
   add the version to the [`Ba2Archive::open()`](../../crates/bsa/src/ba2.rs)
   header check. If the layout differs, follow the existing v2/v3 8-byte
   extension pattern.
5. **Run** `parse_real_nifs.rs` against a sample BSA / BA2 with a new
   `Game` enum entry in `tests/common/mod.rs`. Watch the rate. Anything
   below 95% is a parser bug — usually a few extra fields in some block
   type for the new BSVER. Patch the relevant `blocks/*.rs` parser; the
   existing per-game variants give you the pattern.
6. **For cells**, implement an XCLL variant for the new game's lighting
   layout if it differs from FNV's.

The pattern is well-trodden at this point — N23.6 through N23.9 each
followed it.

## Reference materials

- [`docs/legacy/nif.xml`](../legacy/nif.xml) — niftools' authoritative NIF
  format spec; every parser cross-references it
- [Gamebryo 2.3 Architecture](../legacy/gamebryo-2.3-architecture.md)
- [API Deep Dive](../legacy/api-deep-dive.md) — `NiObject` / `NiAVObject` /
  `NiStream` class hierarchy

## Related docs

- [NIF Parser](nif-parser.md) — block coverage, version handling, robustness
- [Archives](archives.md) — BSA + BA2 reader catalog
- [ESM Records](esm-records.md) — record category catalog
- [Testing](testing.md) — how to run the per-game integration sweeps
- [ROADMAP](../../ROADMAP.md) — full milestone history with achievement details
