# ESM / ESP Records

The `byroredux-plugin` crate parses Bethesda's plugin file format —
ESM (master), ESP (plugin), ESL (light master), ESH (header). Plugins
are organised as nested groups (`GRUP`) of records (`STAT`, `WEAP`,
`NPC_`, `CELL`, ...), each with a 24-byte header and a sequence of
sub-records carrying the actual data.

A single GRUP-tree walker fills one `EsmIndex` that aggregates two
families of data:

- **Cell tier** ([`esm/cell/`](../../crates/plugin/src/esm/cell/))
  — extracts interior + exterior cells, placed references (REFR/ACHR),
  worldspaces, landscape (LAND), and every base record that carries a
  `MODL` sub-record (everything renderable, into `cells.statics`).
  This is what the cell loader consumes.
- **Typed record tier** ([`esm/records/`](../../crates/plugin/src/esm/records/))
  — structured parsers for the ~90 record categories game systems
  need beyond rendering: items, containers, leveled lists, NPCs,
  creatures, races, classes, factions, globals, game settings,
  weather/climate, scripts, quests, dialogue, perks, spells, magic
  effects, enchantments, actor values, FormID lists, and a long tail
  of stub records so cross-references stop dangling.

Both tiers fill in **one pass** over the file. Up through Session 12
the records tier ran as a *second* walk over the cell pass; #527
(FNV-ESM-2) fused them into a single dispatch loop so a 70 MB ESM only
goes through the header decoder + sub-record splitter once. The
full-index entry points are `parse_esm` and `parse_esm_with_load_order`,
both of which return a fully-populated `EsmIndex` (callers that only
want cells read `index.cells`). The cell-only `parse_esm_cells` /
`parse_esm_cells_with_load_order` shims remain public (in
[`cell/mod.rs`](../../crates/plugin/src/esm/cell/mod.rs)) and are still
used by the cell-tier integration tests.

Source: [`crates/plugin/src/esm/`](../../crates/plugin/src/esm/)

> **Reconciled 2026-05-28 (post-Session 42).** The pre-Session-12
> layout (`cell.rs` monolith + a flat 10-file `records/`) was split
> into `cell/` and a themed `records/` moduledir across Sessions 34/35
> (the >2000-LOC refactor sweep) and the category set grew from 18 to
> ~90 across the FO3/FNV/Oblivion/FO4/Skyrim audit waves (#519–#1277).
> Historical narrative below is date-stamped where it freezes at a
> particular session.

## At a glance

| | |
|---|---|
| Top-level modules | `reader.rs`, `sub_reader.rs`, `strings_table.rs`, `cell/`, `records/`, `test_paths.rs` (test only) |
| Cell tier | `cell/` — `mod.rs` + `helpers.rs` / `support.rs` / `walkers.rs` / `wrld.rs` + `tests/` |
| Records tier | `records/` — per-category files + `index.rs` (the `EsmIndex` struct) + `grup_walker.rs` (the fused dispatcher's shared helpers) + `misc/` themed-stub split |
| Walker passes | **One** fused pass (`parse_esm_with_load_order`, #527) — was two pre-Session 12 |
| Record categories on `EsmIndex::categories()` | ~90 (full list is the single source of truth in [`index.rs`](../../crates/plugin/src/esm/records/index.rs)) |
| Plugin-crate `#[test]` fns | ~454 (unit + ignored integration) |
| Real FNV.esm M24 record total | 13,684 (release-mode parse: ~0.19 s) |
| Real FO3.esm CREA / LVLC / SCPT | 533 / 60 / ~1500 (observed; floors in `records/tests.rs`) |
| Exterior grid load | Single ESM walk (#374), was three |
| Multi-plugin FormID remap | `parse_esm_with_load_order` (#445) — single-plugin is identity |
| Multi-plugin merge | `EsmIndex::merge_from` (#561) — later-plugin-wins, per-worldspace exterior merge |
| FO4-only record gate | SCOL / PKIN / MOVS / MSWP skipped (with a one-shot warn) unless `game` is FO4+ (#1277 task 3) |

## Module map

```
crates/plugin/src/esm/
├── mod.rs            Public re-exports
├── reader.rs         Low-level reader + RecordHeader/GroupHeader/SubRecord
│                     + FileHeader + GameKind + FormIdRemap load-order rewrite
├── sub_reader.rs     SubReader cursor — bounds-checked typed decode over one sub-record
├── strings_table.rs  .STRINGS / .DLSTRINGS / .ILSTRINGS companion-file loader (localized ESMs, #989)
├── test_paths.rs     (cfg(test)) on-disk game-data path helpers (#1058)
├── cell/             Cell tier — walker split (Session 34, stage B)
│   ├── mod.rs        EsmCellIndex + CellData / PlacedRef / StaticObject / CellLighting / … structs
│   ├── helpers.rs    Small shared sub-record helpers
│   ├── support.rs    parse_modl_group / parse_ltex_group / parse_txst_group /
│   │                 parse_scol_group / parse_pkin_group / parse_movs_group / parse_mswp_group
│   ├── walkers.rs    parse_cell_group — interior cell blocks/sub-blocks + cell-children REFRs
│   ├── wrld.rs       parse_wrld_group — worldspace decode + exterior block/sub-block grid
│   └── tests/        Per-topic regression siblings (light / addn_stat / refr / cell /
│                     cell_for_refr / txst / merge / movs / wrld / integration)
└── records/          Typed record tier
    ├── mod.rs        parse_esm{,_with_load_order} — the fused single-pass dispatch loop
    ├── index.rs      EsmIndex struct + categories() / total() / category_breakdown() /
    │                 merge_from() / base_record_script() (#1118 / TD9-003 split)
    ├── grup_walker.rs  extract_records / extract_records_with_modl / extract_dial_with_info
    ├── common.rs     Sub-record helpers (find_sub, read_zstring, CommonNamedFields,
    │                 read_lstring_or_zstring, LocalizedPluginGuard, StringsTableGuard)
    ├── condition.rs  CTDA condition parser (ComparisonOp / RunOn / ConditionValue / Condition) — M47.1
    ├── items.rs      WEAP, ARMO, AMMO, MISC, KEYM, ALCH, INGR, BOOK, NOTE
    ├── container.rs  CONT, LVLI/LVLN/LVLC (parse_leveled_list)
    ├── list_record.rs FLST FormID lists
    ├── actor.rs      NPC_, CREA (shared parse_npc), RACE, CLAS, FACT
    ├── global.rs     GLOB, GMST
    ├── climate.rs    CLMT + TNAM sunrise/sunset hours
    ├── weather.rs    WTHR sky-color tables, cloud layers, fog (FNV + Skyrim schemas)
    ├── script.rs     SCPT pre-Papyrus bytecode (SCHR / SCDA / SCTX / SLSD+SCVR / SCRV+SCRO)
    ├── outfit.rs     OTFT (Skyrim+ default-equipped armor list)
    ├── tree.rs       TREE (SpeedTree .spt base records / Skyrim BSTreeNode)
    ├── scol.rs       SCOL static-collection body (ONAM + DATA[])
    ├── pkin.rs       PKIN pack-ins (FO4) + FLTR workshop filter
    ├── movs.rs       MOVS movable statics (FO4) — sound / destruction / script flags
    ├── mswp.rs       MSWP material swaps (FO4)
    └── misc/         Themed stub-parser split (was a single records/misc.rs, #fa309d8d)
        ├── water.rs       WATR (+ NNAM noise texture, FO3/FNV)
        ├── character.rs   HDPT / EYES / HAIR
        ├── world.rs       NAVI / NAVM / REGN / ECZN / LGTM / IMGS / ACTI / TERM
        ├── ai.rs          PACK / QUST / DIAL / INFO / MESG / IDLE / CSTY
        ├── magic.rs       PERK / SPEL / MGEF / ENCH
        ├── effects.rs     AVIF / PROJ / EFSH / IMOD / EXPL / IPCT / IPDS / REPU
        └── equipment.rs   ARMA / BPTD / COBJ + parse_minimal_esm_record long-tail
```

(`records/misc.rs` itself is now just a module-doc + `pub use` facade
over the `misc/` submodules.)

## File format primer

```
ESM file
├── TES4 record       — file header (HEDR + MAST master file list + ONAM)
└── top-level GRUPs   — one per record type, in a fixed order
    ├── GRUP "GMST"
    ├── GRUP "GLOB"
    ├── GRUP "CLAS"
    ├── ...
    └── GRUP "WRLD"
        └── WRLD record
            └── GRUP "WRLD children" (group_type=1)
                ├── exterior block (group_type=4)
                │   └── exterior sub-block (group_type=5)
                │       └── CELL record
                │           └── GRUP "cell children" (group_type=6/8/9)
                │               └── REFR / ACHR / NAVM / ...
                └── ...
```

Each record header is 24 bytes:

```
0x00  4 bytes  record type ("CELL", "STAT", "WEAP", ...)
0x04  u32      data size (sub-records combined)
0x08  u32      flags (compressed, master, persistent, ...)
0x0C  u32      form ID
0x10  u16      vc info  ── version control
0x12  u16      vc unknown
0x14  u16      version
0x16  u16      unknown
```

Each group header is 24 bytes:

```
0x00  4 bytes  "GRUP"
0x04  u32      total size (including this header)
0x08  4 bytes  label (record type for top groups, parent form ID for children, ...)
0x0C  u32      group type (0=top, 1=world children, 2=interior block,
              3=interior sub-block, 4=exterior block, 5=exterior sub-block,
              6=cell temporary children, 7=topic (DIAL) children,
              8=persistent, 9=visible distant)
0x10  8 bytes  vc + version + unknown
```

A record's body is a sequence of sub-records. Each sub-record is:

```
0x00  4 bytes  sub-record type ("EDID", "FULL", "MODL", "DATA", ...)
0x04  u16      data size
0x06  ...      data bytes
```

If a record's `flags & 0x00040000` is set, the record body is zlib-compressed
with a u32 uncompressed-size prefix. The reader decompresses transparently
in `read_sub_records()` (regression-tested in `#990`).

## Reader layer

[`reader.rs`](../../crates/plugin/src/esm/reader.rs) is the low-level
binary reader. It exposes:

```rust
pub struct EsmReader<'a> { ... }

impl<'a> EsmReader<'a> {
    pub fn new(data: &'a [u8]) -> Self;
    pub fn position(&self) -> usize;
    pub fn remaining(&self) -> usize;
    pub fn skip(&mut self, n: usize);
    pub fn peek_type(&self) -> Option<[u8; 4]>;
    pub fn is_group(&self) -> bool;
    pub fn read_record_header(&mut self) -> Result<RecordHeader>;
    pub fn read_group_header(&mut self) -> Result<GroupHeader>;
    pub fn group_content_end(&self, header: &GroupHeader) -> usize;
    pub fn read_sub_records(&mut self, header: &RecordHeader) -> Result<Vec<SubRecord>>;
    pub fn skip_record(&mut self, header: &RecordHeader);
    pub fn skip_group(&mut self, header: &GroupHeader);
    pub fn read_file_header(&mut self) -> Result<FileHeader>;
    pub fn set_form_id_remap(&mut self, remap: FormIdRemap);
    pub fn variant(&self) -> EsmVariant;
}
```

`read_sub_records` is the workhorse: it pulls the record body (with
optional zlib decompression), then walks it splitting into a
`Vec<SubRecord>`. Each higher-level parser walks that vector once,
matching on the 4-char `sub_type` codes it cares about. For decoding
*within* a single sub-record, parsers use the
[`SubReader`](../../crates/plugin/src/esm/sub_reader.rs) cursor — a
bounds-checked typed reader that replaced the ad-hoc `read_*_at(buf,
offset)` helpers (R2 stage D, 169 call-sites migrated in #6d889d70).

The reader is **format-agnostic** — it doesn't know about CELL or WEAP.
The record-walking logic lives in `cell/` and `records/`.

### Game-variant discrimination

`read_file_header` returns a `FileHeader` carrying `hedr_version: f32`
(the TES4 HEDR `Version`), `localized` (the TES4 `Localized` flag),
`record_count`, and the `master_files` list. `GameKind::from_header`
maps the HEDR version onto one of:

```rust
pub enum GameKind {
    Oblivion,    // HEDR 1.0
    Fallout3NV,  // FO3 (0.85) + FNV (1.34) — shared DATA/DNAM layouts; default
    Skyrim,      // LE + SE (1.7)
    Fallout4,    // 0.95
    Fallout76,   // 68.0
    Starfield,   // 0.96
}
```

ARMO / WEAP / AMMO `DATA`/`DNAM` layouts, the WTHR `NAM0` schema, the
CLMT `WLST` entry size, and several other sub-record shapes diverge
across these bands, so `game` is threaded into the parsers that care
(`parse_weap`, `parse_armo`, `parse_wthr`, `parse_clmt`, `parse_race`,
`parse_clas`, `parse_arma`, `parse_npc`). The HEDR band edges were
pinned against every installed master on 2026-04-19 (#439) — prior
bands inverted the FO3↔FO4 classification for every vanilla master.

### Localized strings

Skyrim+ masters set the TES4 `Localized` flag and store FULL/DESC text
as a 4-byte index into a companion `.STRINGS` / `.DLSTRINGS` /
`.ILSTRINGS` table rather than inline. The parser pushes the flag into
a thread-local via `LocalizedPluginGuard` (RAII, restored on
drop/unwind) so every record parser's text decoder routes through
`read_lstring_or_zstring`. The actual string table is loaded by
[`strings_table.rs`](../../crates/plugin/src/esm/strings_table.rs)
(#989); an unresolved index renders as a `<lstring 0xNNNNNNNN>`
placeholder.

## Cell tier

[`esm/cell/`](../../crates/plugin/src/esm/cell/)

The cell walker (`parse_cell_group` / `parse_wrld_group`, invoked from
the fused loop in `records/mod.rs`) builds an `EsmCellIndex`:

```rust
pub struct EsmCellIndex {
    pub cells: HashMap<String, CellData>,                          // interior, by lowercased editor ID
    pub exterior_cells: HashMap<String, HashMap<(i32, i32), CellData>>, // by worldspace, then grid coords
    pub statics: HashMap<u32, StaticObject>,                       // base records with MODL
    pub landscape_textures: HashMap<u32, String>,                  // LTEX form ID → diffuse path
    pub worldspaces: HashMap<String, WorldspaceRecord>,            // full WRLD decode (#965)
    pub worldspace_climates: HashMap<String, u32>,                 // WRLD.CNAM → CLMT form ID
    pub texture_sets: HashMap<u32, TextureSet>,                    // 8-slot TXST
    pub scols: HashMap<u32, ScolRecord>,                           // FO4 static collections
    pub packins: HashMap<u32, PkinRecord>,                         // FO4 pack-ins
    pub movables: HashMap<u32, MovableStaticRecord>,               // FO4 movable statics
    pub material_swaps: HashMap<u32, MaterialSwapRecord>,          // FO4 MSWP
}
```

`CellData` has grown well past the four fields it carried in Session 12.
The current shape (see [`cell/mod.rs`](../../crates/plugin/src/esm/cell/mod.rs)
for the authoritative struct + per-field docs) covers, among others:

```rust
pub struct CellData {
    pub form_id: u32,
    pub editor_id: String,
    pub display_name: Option<String>,          // FULL / lstring (#624)
    pub references: Vec<PlacedRef>,
    pub is_interior: bool,
    pub grid: Option<(i32, i32)>,
    pub lighting: Option<CellLighting>,        // XCLL
    pub landscape: Option<LandscapeData>,      // LAND (exterior)
    pub water_height: Option<f32>,             // XCLW (#397)
    pub image_space_form: Option<u32>,         // XCIM (Skyrim+)
    pub water_type_form: Option<u32>,          // XCWT
    pub acoustic_space_form: Option<u32>,      // XCAS
    pub music_type_form: Option<u32>,          // XCMO (Skyrim+)
    pub music_type_enum: Option<u8>,           // XCMT (pre-Skyrim, #693)
    pub climate_override: Option<u32>,         // XCCM (Skyrim+, #693)
    pub location_form: Option<u32>,            // XLCN
    pub regions: Vec<u32>,                     // XCLR
    pub lighting_template_form: Option<u32>,   // LTMP → LGTM fallback (#566)
    pub ownership: Option<CellOwnership>,      // XOWN/XRNK/XGLB (#692)
    pub regional_color_override: Option<[u8; 3]>, // RCLR (Oblivion, #970)
    pub precombined_mesh_hashes: Vec<u32>,     // XCRI (FO4 PreCombined, #1188)
    pub absorbed_refs: HashSet<u32>,           // XCRI/XPRI absorbed REFRs (#1188/#1220)
    pub navmeshes: Vec<NavmRecord>,            // per-cell NAVM (#1272)
}
```

`PlacedRef` likewise carries the placement identity plus a large set of
FO4-era REFR overrides (`XESP` enable-parent gating, `XTEL` teleport,
`XPRM` primitive bounds, `XLKR` linked refs, `XRMR`/`XPOD` room+portal
culling, `XRDS` light-radius override, `XATO`/`XTNM`/`XTXR` texture
overrides, `XEMI` emissive light, `XMSP` material-swap, per-ref
ownership) — see the struct docs.

The walker:

1. Dispatches top-level GRUPs by label from the fused loop.
2. For `b"CELL"` GRUPs (`parse_cell_group`): walks interior cell
   blocks/sub-blocks, extracts `CELL` records (including `XCLL`
   lighting, water plane, ownership, the Skyrim+ extended sub-records),
   and matches each cell with its **cell children** GRUP (group types
   6/8/9) where the placed references and nested NAVMs live.
3. For `b"WRLD"` GRUPs (`parse_wrld_group`): decodes the WRLD record
   (parent / bounds / flags / water / music / map — #965), walks its
   **world children** GRUP (group_type=1), then nested exterior
   block / sub-block GRUPs (4/5), then per-cell children groups.
4. For any GRUP whose label is a record type with a `MODL` sub-record
   (`STAT`, `MSTT`, `FURN`, `DOOR`, `LIGH`, `FLOR`, `IDLM`, `BNDS`,
   `ADDN`, `TACT`, …) `parse_modl_group` extracts the editor ID +
   model path + `record_type` + `has_script` flag into the `statics`
   map. This is what placed REFR records resolve against at cell-load
   time. Labels that *also* carry a typed record (WEAP/ARMO/NPC_/
   CREA/ACTI/TERM/CLOT/…) go through `extract_records_with_modl`, which
   fills `statics` *and* the typed map from the same `subs` slice in
   one walk.

`StaticObject` now records the source four-CC `record_type` (drives
`RenderLayer` classification at cell-load), a `LightData` payload for
`LIGH` records (radius / color / flags, so the cell loader can spawn
point lights for mesh-less lights), an `AddonData` payload for `ADDN`
records (#370), and a `has_script` flag.

### XCLL interior lighting

`XCLL` carries the cell's interior lighting. The on-disk size is
version-dependent — FO3/FNV/Oblivion ship a 36/40-byte record, Skyrim+
ships a 92-byte record with directional-ambient cube + specular +
fog-far fields. A size sanity gate (#1277 task 4) rejects payloads that
don't match a known per-game canonical size before decode, and Starfield
has its own size set (`[28, 108]`, #1291). The core fields:

```
0x00  4 bytes  ambient RGBA
0x04  4 bytes  directional RGBA
0x08  4 bytes  fog color near RGBA
0x0C  f32      fog near
0x10  f32      fog far
0x14  i32      directional rotation X
0x18  i32      directional rotation Y
0x1C  f32      directional fade
```

The Skyrim+ tail (bytes 32–91) adds fog clip / power / far-color / max,
light-fade begin/end, a 6-face directional-ambient cube (`[+X,-X,+Y,
-Y,+Z,-Z]`, #367), specular color/alpha, and Fresnel power — all
surfaced as `Option` fields on `CellLighting`. The cell loader spawns
ambient + directional color + rotation as a directional light on a
sentinel entity, falling back to the `LTMP → LGTM` template chain when
the cell ships no XCLL (#566). See [Cell Lighting](lighting-from-cells.md)
for the full pipeline from XCLL bytes to RT-shadowed multi-light
rendering.

## Records tier

[`esm/records/`](../../crates/plugin/src/esm/records/)

`parse_esm_with_load_order(data, remap)` is the one walker. It reads the
TES4 header, derives `GameKind`, installs the localized-strings guard,
then loops over top-level GRUPs dispatching by 4-char label. Each arm
either:

- calls a cell-tier helper (`CELL` / `WRLD` / `LTEX` / `TXST`, plus the
  FO4-gated `SCOL` / `PKIN` / `MOVS` / `MSWP`),
- `extract_records(reader, end, expected, cb)` — a typed-only walk for
  records with no `cells.statics` consumer,
- `extract_records_with_modl(reader, end, expected, &mut statics, cb)` —
  a dual-target walk that fills both `cells.statics` and the typed map
  from one `subs` slice (the #527 fusion), or
- `extract_dial_with_info(reader, end, &mut dialogues)` — the special
  DIAL→INFO walker that descends the Topic-Children sub-GRUP
  (group_type 7) the generic walker would drop (#631).

```rust
pub fn parse_esm(data: &[u8]) -> Result<EsmIndex>;
pub fn parse_esm_with_load_order(
    data: &[u8],
    remap: Option<FormIdRemap>,
) -> Result<EsmIndex>;
```

### `EsmIndex`

[`index.rs`](../../crates/plugin/src/esm/records/index.rs) holds the
aggregate struct and is the authoritative list of parsed categories —
it grew from 18 maps in Session 12 to ~90 today. The struct opens with
`game: GameKind` and `cells: EsmCellIndex`, then a flat set of
`HashMap<u32, …Record>` maps. Major families:

- **Items & containers** — `items` (WEAP/ARMO/AMMO/MISC/KEYM/ALCH/
  INGR/BOOK/NOTE → one `ItemRecord`), `containers`, `leveled_items`
  (LVLI), `leveled_npcs` (LVLN), `leveled_creatures` (LVLC, #448),
  `recipes` (COBJ).
- **Actors** — `npcs` (NPC_), `creatures` (CREA, #442 — shares
  `parse_npc`), `races`, `classes`, `factions`, `outfits` (OTFT,
  Skyrim+, #896), `body_parts` (BPTD), `armor_addons` (ARMA),
  `combat_styles` (CSTY), `idle_animations` (IDLE), `packages` (PACK).
- **Values & lists** — `globals` (GLOB), `game_settings` (GMST),
  `actor_values` (AVIF, #519), `form_lists` (FLST, #630).
- **World / environment** — `weathers` (WTHR), `climates` (CLMT),
  `waters` (WATR), `regions` (REGN), `encounter_zones` (ECZN),
  `lighting_templates` (LGTM), `image_spaces` (IMGS, #624), `navi_info`
  (NAVI), `navmeshes` (NAVM, drained from cell children, #1272),
  `trees` (TREE, SpeedTree Phase 1.1).
- **FaceGen** — `head_parts` (HDPT), `eyes` (EYES), `hair` (HAIR).
- **Scripts & gameplay** — `scripts` (SCPT, #443), `quests` (QUST,
  M24.2 Phase 1a), `dialogues` (DIAL+INFO, #631), `messages` (MESG),
  `perks` (PERK, M24.2 Phase 1b), `spells` (SPEL), `enchantments`
  (ENCH, #629), `magic_effects` (MGEF), `magic_effects_by_code`
  (Oblivion 4-char EFID → MGEF FormID secondary index, #969),
  `activators` (ACTI), `terminals` (TERM), `projectiles` (PROJ),
  `effect_shaders` (EFSH), `item_mods` (IMOD), `explosions` (EXPL),
  `impacts` (IPCT), `impact_data_sets` (IPDS), `reputations` (REPU).
- **Oblivion-unique base records** (#966) — `birthsigns` (BSGN),
  `clothing` (CLOT), `apparatuses` (APPA), `sigil_stones` (SGST),
  `soul_gems` (SLGM).
- **Long-tail minimal stubs** (#810) — 31 record types
  (audio: ALOC/ANIO/ASPC/CAMS/CPTH/DOBJ/MICN/MSET/MUSC/SOUN/VTYP;
  visual/world: AMEF/DEBR/GRAS/IMAD/LSCR/LSCT/PWAT/RGDL; FNV hardcore:
  DEHY/HUNG/RADS/SLPD; Caravan/Casino: CCRD/CDCK/CHAL/CHIP/CMNY/CSNO;
  recipe residuals: RCCT/RCPE) parsed via `parse_minimal_esm_record`
  (EDID + optional FULL) so cross-references resolve at lookup time
  even though no consumer drives a full per-record parser yet.

`EsmIndex` carries four cross-cutting helpers:

- `categories() -> &'static [(&str, fn(&EsmIndex) -> usize)]` — the
  single source of truth for the per-category breakdown. `total()` sums
  it and `category_breakdown()` formats the end-of-parse log line, so
  adding a category is a one-edit operation (#634). Note: `cells.statics`
  overlaps the typed maps (a WEAP fills both `items` *and* `statics`),
  so `total()` is a sum-of-bucket-fills, not a unique-record count.
- `base_record_script(base_form_id) -> Option<u32>` — resolves the
  pre-Papyrus SCRI attached-script FormID for a base record by walking
  the maps that capture `script_form_id` (activators, containers,
  terminals, items, NPCs, creatures), nil-ing the `0` "no script"
  sentinel (M47.0 Phase 3 / #1273).
- `merge_from(other)` — later-plugin-wins merge for multi-plugin load
  orders (masters first, main ESM last), with per-worldspace exterior
  merge so a DLC adding a worldspace doesn't stomp the base game (#561).

### Items (`items.rs`)

`ItemRecord` covers every item-bearing record type via an `ItemKind`
enum that carries the type-specific stats. Common name / model / value /
weight / script fields live on the parent struct in `CommonItemFields`:

```rust
pub struct ItemRecord {
    pub form_id: u32,
    pub common: CommonItemFields, // editor_id, full_name, model_path, value, weight, script_form_id, ...
    pub kind: ItemKind,
}

pub enum ItemKind {
    Misc,
    Book { teaches_skill, skill_bonus, flags },
    Note { note_type, topic_form },
    Ingredient { magic_effects },
    Aid { magic_effects, addiction_chance },
    Key,
    Ammo { damage, dt_mult, spread, casing_form, clip_rounds },
    Armor { biped_flags, dt, dr, health, slot_mask },
    Weapon { ammo_form, damage, clip_size, anim_type, ap_cost,
             skill_form, min_spread, spread, crit_mult, reload_anim },
}
```

(Exact field set is in `items.rs`.) Field selection is intentionally
minimal — just what gameplay systems need. The parsers walk sub-records
by 4-char code and ignore anything they don't recognise, so growing a
variant is a local edit.

### Containers and leveled lists (`container.rs`)

```rust
pub struct ContainerRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub model_path: String,
    pub weight: f32,
    pub open_sound: u32,
    pub close_sound: u32,
    pub script_form_id: u32,
    pub contents: Vec<InventoryEntry>,
    // + DATA flags byte (FNV 5-byte payload, #376)
}

pub struct LeveledList {
    pub form_id: u32,
    pub editor_id: String,
    pub chance_none: u8,        // 0–100, "list rolls nothing" probability
    pub flags: u8,              // bit 0 = calc from all, bit 1 = calc per item
    pub entries: Vec<LeveledEntry>,
}
```

`LVLI` (items), `LVLN` (NPCs), and `LVLC` (creatures, #448) all share
`parse_leveled_list` / `LeveledList` because their sub-record layout is
byte-identical — they only differ in which type of base record the
entries reference.

### Actors (`actor.rs`)

NPCs, creatures, races, classes, factions. `NpcRecord` (used for both
NPC_ and CREA via `parse_npc`):

```rust
pub struct NpcRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub model_path: String,
    pub race_form_id: u32,        // RNAM
    pub class_form_id: u32,       // CNAM
    pub voice_form_id: u32,
    pub factions: Vec<FactionMembership>,   // SNAM
    pub inventory: Vec<NpcInventoryEntry>,  // CNTO
    pub default_outfit: Option<u32>,        // DOFT → OTFT (Skyrim+, #896)
    pub ai_packages: Vec<u32>,              // PKID
    pub death_item_form_id: u32,
    pub level: i16,
    pub disposition_base: i16,    // ACBS offset 20 — i16, not u8 (#377)
    pub acbs_flags: u32,
    pub has_script: bool,         // VMAD presence (Skyrim+, #369)
    pub script_form_id: u32,      // SCRI → SCPT (pre-Skyrim, #416)
    // + TPLT template inheritance + FaceGen / face-morph fields (M41.0)
}
```

`RaceRecord`, `ClassRecord` (both extended with Oblivion DATA shapes,
#967/#968), `FactionRecord` (+ `FactionRelation` from XNAM, ranks from
MNAM) are also defined here. See the struct docs for the full field set.

### Globals and game settings (`global.rs`)

Both record types boil down to `(editor_id, value)` pairs. The value
type is determined by an `FNAM` byte for `GLOB` and by the editor_id
prefix convention for `GMST` (`i…` int, `f…` float, `s…` string, `b…`
bool/short):

```rust
pub enum SettingValue {
    Int(i32),
    Float(f32),
    String(String),
    Short(i16),
}

pub struct GlobalRecord { pub form_id: u32, pub editor_id: String, pub value: SettingValue }
pub struct GameSetting  { pub form_id: u32, pub editor_id: String, pub value: SettingValue }
```

### Conditions (`condition.rs`) — M47.1

The shared `CTDA` parser (`parse_ctda`) decodes Bethesda's universal
predicate sub-record into a `Condition` (`ComparisonOp`, `RunOn`,
`ConditionValue`, function index + params). It backs perk entry-point
gating, AI-package conditions, and the OR-precedence condition
evaluator (M47.1 Phases 1–3, #ea9d0cfa). See the
[Condition System](../legacy/papyrus-api-reference.md) reference for the
function catalogue.

## Real FNV.esm — measured

Running `parse_esm()` on a real `FalloutNV.esm` (April 2026 patch
revision) in release mode, M24 record-tier counts (the floors that gate
`parse_real_fnv_esm_record_counts` in
[`records/tests.rs`](../../crates/plugin/src/esm/records/tests.rs)):

| Category | Count |
|---|---:|
| Items (WEAP/ARMO/AMMO/MISC/KEYM/ALCH/INGR/BOOK/NOTE) | 2,643 |
| Containers (CONT) | 2,478 |
| Leveled item lists (LVLI) | 2,738 |
| Leveled NPC lists (LVLN) | 365 |
| NPCs (NPC_) | 3,816 |
| Races (RACE) | 22 |
| Classes (CLAS) | 74 |
| Factions (FACT) | 682 |
| Globals (GLOB) | 218 |
| Game settings (GMST) | 648 |
| **M24 records total** | **13,684** |

Supplementary-record counts also validated against the same parse:
WATR=78, NAVI=1, REGN=276, ECZN=17, LGTM=31, HDPT=61, EYES=12, HAIR=67.
(NAVM lives nested under CELL children on FO3/FNV, drained into
`navmeshes` post-walk per #1272.) Plus the cell side (~1,100 interior
cells, ~30k exterior cells, ~17k base objects). The categories beyond
this M24 set (perks, spells, scripts, the long tail) push the *grand*
`EsmIndex::total()` well above 13,684, but the 13,684 figure is the
stable, test-floored M24-category total.

## Tests

The plugin crate carries ~454 `#[test]` functions (unit + ignored
integration) across `src/` and `tests/`:

- **Unit** — manifest TOML parsing; `RecordType` 4-char codes + ECS
  spawn integration; DataStore + dependency resolver; legacy ESM/ESP/ESL
  slot bridge; the cell walker (STAT extraction, REFR position/scale,
  XCLL/XCLW/ownership, NAVM drain, group walking) under
  [`cell/tests/`](../../crates/plugin/src/esm/cell/tests/) (split into
  per-topic siblings in Session 35 + #9c1f7234); and the record parsers
  (item field extraction, CONT inventory, leveled entries, NPC
  race/class/factions/inventory/AI packages, FACT relations + ranks,
  GLOB/GMST typed values, CTDA conditions, the SCOL/PKIN/MOVS/MSWP
  group walkers, `EsmIndex::total/categories/base_record_script`).
- **Ignored integration** (need on-disk game data, paths resolved via
  `test_paths.rs`):
  - `parse_real_fnv_esm` — FNV cell + static counts, Saloon refs
  - `parse_real_fnv_esm_record_counts` — FNV record-category floors +
    Varmint Rifle / NCR faction spot-check
  - `parse_real_fnv_dial_infos_populated` — DIAL→INFO walker
  - `parse_real_fo3_esm_scpt_count_and_scri_resolves` — FO3 SCPT
    (>500, observed ~1500+) with SCRV/SCRO cross-refs
  - `parse_real_fo3_esm_crea_and_lvlc_counts` — FO3 CREA (533) / LVLC (60)
  - `parse_real_fo3_megaton_cell_baseline` — Megaton interior REFR count
  - `parse_real_oblivion_esm_walker_survives` — Oblivion clean-walk
  - `parse_real_skyrim_esm` — Skyrim cell + localized-name spot-check
  - `parse_real_fo4_esm_surfaces_scol_placements` /
    `..._surfaces_pkin_contents` — FO4 SCOL/PKIN expansion

Run them:

```bash
cargo test -p byroredux-plugin                          # unit tests, no game data
cargo test -p byroredux-plugin --release -- --ignored   # integration tests (need installed masters)
```

## FO4 architecture records

To render FO4 architectural cells, the parser composes prefab buildings
from four FO4-introduced record types. As of #1277 (task 3) these GRUPs
are **gated on `GameKind` being FO4+** — encountering them on a
non-FO4 master skips the whole group and logs a one-shot warning, so a
cross-game plugin stack can't silently consume SCOL/PKIN/MOVS/MSWP forms
that REFRs would then mis-resolve against.

- **`SCOL`** ([`scol.rs`](../../crates/plugin/src/esm/records/scol.rs))
  — static collections: a list of `(STAT reference, transforms[])`
  tuples. One SCOL expands into many placements at cell-load. The body
  parser (#405) walks every `ONAM` (STAT ref) and collects all following
  `DATA` blocks as that part's transform list; the FULL display name is
  preserved (#816) and VMAD presence is plumbed through to
  `StaticObject.has_script` (#1178).
- **`MOVS`** ([`movs.rs`](../../crates/plugin/src/esm/records/movs.rs))
  — movable statics: STATs that respond to havok impulses. The typed
  parser (#588) captures sound / destruction / script flags.
- **`PKIN`** ([`pkin.rs`](../../crates/plugin/src/esm/records/pkin.rs))
  — pack-ins: pre-assembled reusable room modules. The parser also reads
  the FLTR workshop build-mode filter (#815).
- **`MSWP`** ([`mswp.rs`](../../crates/plugin/src/esm/records/mswp.rs))
  — material swaps: per-REFR BGSM/BGEM substitution tables, resolved by
  the cell loader against `cells.material_swaps` when a REFR carries
  `XMSP` (#971).
- **`TXST`** texture sets are parsed by the cell tier into the 8-slot
  `TextureSet` (TX00 diffuse … TX07 specular, plus FO4 `MNAM` → BGSM
  path, plus `DODT` decal data + `DNAM` flags, #813/#814). Referenced by
  `BSLightingShaderProperty`, by LAND splatting, and by REFR XATO/XTNM
  overrides.

These all live on `EsmCellIndex` (`scols` / `movables` / `packins` /
`material_swaps` / `texture_sets`) and are surfaced through
`categories()` so a regression that empties any of them fails CI (#817).
The cell loader expands `SCOL` / `PKIN` inline when walking placements.

### FO4 PreCombined Meshes

FO4 cells bake architecture REFRs into precombined `_oc.nif` meshes.
The cell parser reads the cell's `XCRI` / `XPRI` sub-records into
`precombined_mesh_hashes` + `absorbed_refs` (#1188 interior, #1220
exterior). The cell loader skips absorbed REFRs during normal placement
(their geometry is in the precombined mesh) and, when precombined spawn
fails, falls back to rendering the absorbed REFRs individually. See the
FO4 PreCombined Mesh feedback note
(`~/.claude/projects/-mnt-data-src-gamebyro-redux/memory/feedback_fo4_precombined.md`)
for the CSG companion gap.

## Historical narrative

### Session 11 fixes (2026-04)

A batch of correctness + performance fixes landed on top of the FO4
architecture work:

- **`XCLW` water plane height** (#397) — surfaced on `CellData` so water
  rendering picks up the correct plane height instead of z=0.
- **`XESP` ref gating** (#349) — default-disabled REFRs (quest-spawn
  markers, ruined walls) no longer render until a quest enables them.
- **Skyrim CELL extended sub-records** (#356) — the FNV-first walker now
  recognises Skyrim's extra CELL sub-records instead of warning.
- **`VMAD` script attachments** (#369) — `has_script` surfaced on REFR /
  base records so the eventual script runtime can find Papyrus-attached
  references without re-scanning.
- **Variant-aware group end** (#391) — game variant threaded through the
  walker so per-game GRUP end-marker differences no longer mis-align.
- **Single-pass exterior cell load** (#374) — three ESM walks collapsed
  to one; 3×3 grid load time dropped proportionally.
- **`LIGH` DATA color byte order** (#389/#700/#702) — colour bytes are
  `BGRA`, matching the CK.
- **Legacy ESM parser stub cleanup** (#390) — dead per-game stubs
  (`tes3.rs` / `tes4.rs`) deleted; the `EsmIndex` aggregator is the only
  entry point.

### Session 12 audit sweep (2026-04-20)

The second expansion drove the parser from 10 to 18 indexed categories,
mostly FO3-shaped records the FNV dev pass had skipped. Filed from
`AUDIT_FO3_2026-04-19.md` / `AUDIT_FNV_2026-04-20.md`: **CREA** (#442,
533 in FO3), **LVLC** (#448, 60), **SCPT** (#443, ~1500 — full
structural parse, runtime out of scope), the **CLMT TNAM** per-worldspace
sunrise/sunset clock (#463), the nine **#458 stub records** (WATR / NAVI /
NAVM / REGN / ECZN / LGTM / HDPT / EYES / HAIR), the **HEDR version
bands** (#439, fixing the inverted FO3↔FO4 classification), and
**`FormIdRemap`** (#445, multi-plugin top-byte rewrite).

### Since Session 12 (2026-04 → 2026-05, Sessions 13–42)

The deferred-phase list at the bottom of the Session-12 doc has largely
been worked off:

- **Single-pass fusion** (#527) — the records second-walk was fused into
  the cell walker; the full-index entry points now drive one pass, while
  the cell-only `parse_esm_cells` shim was kept for the cell-tier tests.
- **Long-tail dispatch** — FLST (#630), ENCH (#629), AVIF (#519), PROJ /
  EFSH / IMOD / ARMA / BPTD (#808), REPU / EXPL / CSTY / IDLE / IPCT /
  IPDS / COBJ (#809), and 31 minimal stubs (#810) all moved out of the
  catch-all skip. ACTI / TERM gained typed maps (#521). IMGS (#624).
- **Quest / perk / condition runtime groundwork** — QUST stage +
  objective decoder (M24.2 Phase 1a), PERK Quest/Ability/EntryPoint
  decoder (M24.2 Phase 1b), DIAL→INFO walker (#631), MESG / SPEL / MGEF,
  and the CTDA condition parser + OR-precedence evaluator (M47.1). The
  `base_record_script` lookup helper landed for M47.0 Phase 3.
- **Skyrim** — full WTHR parser (#539), `parse_wthr` / `parse_clmt`
  gated on `GameKind` (#539/#540), OTFT outfits + Skyrim equip pipeline
  scaffolding (#896), localized `.STRINGS` loader (#989).
- **Oblivion** — CLAS/RACE DATA shapes (#967/#968), WRLD parent/bounds/map
  decode (#965), CELL RCLR (#970), the BSGN/CLOT/APPA/SGST/SLGM
  base-record family + the 4-char-EFID → MGEF secondary index (#966/#969).
- **FO4** — MOVS/MSWP typed parsers, PreCombined Mesh XCRI/XPRI (#1188/
  #1220), the FO4+ GameKind gate on SCOL/PKIN/MOVS/MSWP (#1277).
- **Multi-plugin** — `EsmIndex::merge_from` later-plugin-wins merge for
  DLC interiors (M46.0 / #561).
- **NPC spawn** — M41.0 face/anim predicates, FaceGen recipe parse, and
  TPLT inventory inheritance for FNV Lvl* template NPCs.
- **Refactors** — `cell.rs` → `cell/` (Session 34, stage B); the flat
  `records/misc.rs` → themed `misc/` submodules; `records/mod.rs` split
  into `index.rs` + `grup_walker.rs` (#1118); the `SubReader` cursor
  migration (R2 stage D); `test_paths.rs` extraction (#1058).

## Phase 2 — still deferred

Some surfaces remain parsed-as-stub or unparsed until their consuming
runtime arrives:

- **SCPT bytecode runtime** — extraction only; the ECS-native scripting
  track (M30.2) consumes the retained blob + cross-refs later.
- **Deep PACK / QUST / PERK / SPEL / MGEF decoding** — the records
  parse to a usable stub/decoder today, but full condition-tree
  execution waits on the M47 ECS runtime.
- **VMAD decoding** — only the `has_script` presence flag is surfaced;
  Skyrim+ script names + property bindings decode under M48.
- **Multi-plugin CLI** — `parse_esm_with_load_order` + `merge_from`
  exist and the `--master` repeatable flag wires DLC interiors (#561),
  but a general mod load-order stack lands with the scripting runtime.
- **Long-tail minimal records (#810)** — the 31 stub types store EDID +
  optional FULL; per-type decoding lands when a consumer needs it.

See [ROADMAP.md](../../ROADMAP.md) for the full deferred list and the
M24 entry for the original Phase 1 scope.

## Related docs

- [Cell Lighting](lighting-from-cells.md) — XCLL/LGTM extraction and the
  RT multi-light pipeline that consumes it
- [Asset Pipeline](asset-pipeline.md) — how cell loading composes ESM
  records, BSA/BA2 archives, NIF parsing, and ECS spawning
- [Game Loop](game-loop.md) — where in the startup flow the cell loader runs
- [Papyrus API Reference](../legacy/papyrus-api-reference.md) — the
  quest / dialogue / condition surface the QUST/DIAL/CTDA parsers feed
