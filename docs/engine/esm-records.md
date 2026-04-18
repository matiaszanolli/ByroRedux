# ESM / ESP Records

The `byroredux-plugin` crate parses Bethesda's plugin file format —
ESM (master), ESP (plugin), ESL (light master), ESH (header). Plugins
are organised as nested groups (`GRUP`) of records (`STAT`, `WEAP`,
`NPC_`, `CELL`, ...), each with a 24-byte header and a sequence of
sub-records carrying the actual data.

There are two related parser layers:

- **Cell parser** ([`esm/cell.rs`](../../crates/plugin/src/esm/cell.rs))
  — extracts cells, placed references (REFR/ACHR), exterior worldspaces,
  and base records that have a `MODL` sub-record (everything renderable).
  This is what the cell loader consumes.
- **Records parser** ([`esm/records/`](../../crates/plugin/src/esm/records/))
  — structured parsers for the record types game systems need beyond
  rendering: items, containers, leveled lists, NPCs, races, classes,
  factions, globals, and game settings.

The two layers run as one pass over the file via `parse_esm()`. Existing
callers that only need cells continue to use `parse_esm_cells()` (which is
now a thin shim over the underlying walker).

Source: [`crates/plugin/src/esm/`](../../crates/plugin/src/esm/)

## At a glance

| | |
|---|---|
| Reader file size | 386 lines (`reader.rs`) |
| Cell parser | 724 lines (`cell.rs`) |
| Records parser | 6 files in `records/` (~1900 lines) |
| Record categories supported | 10 (items, containers, leveled lists, NPCs, races, classes, factions, globals, game settings, cells) |
| Specific record types | 35+ |
| Tests (unit) | 105 |
| Real FNV.esm record count | 13,684 (release-mode parse: 0.19 s) |
| Exterior grid load | Single ESM walk (#374), was three |

## Module map

```
crates/plugin/src/esm/
├── mod.rs            Public re-exports
├── reader.rs         Low-level binary reader (records, sub-records, groups)
├── cell.rs           CELL/REFR/STAT/WRLD walker, lighting, exterior grids
└── records/
    ├── mod.rs        EsmIndex aggregator + parse_esm() entry point
    ├── common.rs     Sub-record helpers (find_sub, read_zstring, ...)
    ├── items.rs      WEAP, ARMO, AMMO, MISC, KEYM, ALCH, INGR, BOOK, NOTE
    ├── container.rs  CONT, LVLI, LVLN
    ├── actor.rs      NPC_, RACE, CLAS, FACT
    └── global.rs     GLOB, GMST
```

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
              6=cell temporary children, 8=persistent, 9=visible distant)
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
in `read_sub_records()`.

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
    pub fn read_sub_records(&mut self, header: &RecordHeader) -> Result<Vec<SubRecord>>;
    pub fn skip_record(&mut self, header: &RecordHeader);
    pub fn skip_group(&mut self, header: &GroupHeader);
    pub fn read_file_header(&mut self) -> Result<FileHeader>;
}
```

`read_sub_records` is the workhorse: it pulls the record body (with
optional zlib decompression), then walks it splitting into a
`Vec<SubRecord>`. Each higher-level parser walks that vector once,
matching on the 4-char `sub_type` codes it cares about.

The reader is **format-agnostic** — it doesn't know about CELL or WEAP. The
record-walking logic lives in `cell.rs` and `records/`.

## Cell parser

[`esm/cell.rs`](../../crates/plugin/src/esm/cell.rs)

`parse_esm_cells(data)` walks the GRUP tree and builds an `EsmCellIndex`
containing:

```rust
pub struct EsmCellIndex {
    pub cells: HashMap<String, CellData>,                          // interior, by editor ID
    pub exterior_cells: HashMap<String, HashMap<(i32, i32), CellData>>, // by worldspace, then grid coords
    pub statics: HashMap<u32, StaticObject>,                       // base records with MODL
}

pub struct CellData {
    pub form_id: u32,
    pub editor_id: String,
    pub references: Vec<PlacedRef>,
    pub is_interior: bool,
    pub grid: Option<(i32, i32)>,
    pub lighting: Option<CellLighting>,
}

pub struct PlacedRef {
    pub base_form_id: u32,
    pub position: [f32; 3],     // Z-up Bethesda units
    pub rotation: [f32; 3],     // Euler radians
    pub scale: f32,
}

pub struct StaticObject {
    pub form_id: u32,
    pub editor_id: String,
    pub model_path: String,
    pub light_data: Option<LightData>,    // populated for LIGH records
}
```

The walker:

1. Reads top-level GRUPs by label
2. For `b"CELL"` GRUPs: walks interior cell blocks/sub-blocks, extracts
   `CELL` records (including `XCLL` lighting), and matches each cell with
   its **cell children** GRUP (group types 6/8/9) where the placed
   references live
3. For `b"WRLD"` GRUPs: extracts the worldspace name from the WRLD record,
   walks its **world children** GRUP (group_type=1), then nested exterior
   block / sub-block GRUPs (4/5), then per-cell children groups
4. For any GRUP whose label is a record type with a `MODL` sub-record
   (`STAT`, `MSTT`, `FURN`, `DOOR`, `ACTI`, `CONT`, `LIGH`, `MISC`, ...
   24 types), extracts the editor ID + model path into the `statics`
   map. This is what placed REFR records resolve against at cell-load
   time.

The `LIGH` parser also extracts the `DATA` sub-record's radius / color /
flags into a `LightData` struct so the cell loader can spawn point lights
even for LIGH records that have no mesh model.

### XCLL interior lighting

`XCLL` carries the cell's interior lighting in a 32-byte sub-record:

```
0x00  4 bytes  ambient RGBA
0x04  4 bytes  directional RGBA
0x08  4 bytes  fog color near RGBA
0x0C  f32      fog near
0x10  f32      fog far
0x14  i32      directional rotation X (degrees)
0x18  i32      directional rotation Y (degrees)
0x1C  f32      directional fade
```

The cell parser extracts ambient + directional color + rotation into a
`CellLighting` struct that the cell loader spawns as a directional light
component on a sentinel entity. See [Cell Lighting](lighting-from-cells.md)
for the full pipeline from XCLL bytes to RT-shadowed multi-light rendering.

## Records parser (M24)

[`esm/records/`](../../crates/plugin/src/esm/records/)

`parse_esm(data)` extends the cell parser with structured extraction for
record types game systems need beyond rendering. It runs the existing
cell walker first (so the cell pipeline is unchanged) and then walks the
file a second time, dispatching each top-level GRUP to a per-category
parser:

```rust
pub fn parse_esm(data: &[u8]) -> Result<EsmIndex>;

#[derive(Debug, Default)]
pub struct EsmIndex {
    pub cells: EsmCellIndex,
    pub items: HashMap<u32, ItemRecord>,
    pub containers: HashMap<u32, ContainerRecord>,
    pub leveled_items: HashMap<u32, LeveledList>,
    pub leveled_npcs: HashMap<u32, LeveledList>,
    pub npcs: HashMap<u32, NpcRecord>,
    pub races: HashMap<u32, RaceRecord>,
    pub classes: HashMap<u32, ClassRecord>,
    pub factions: HashMap<u32, FactionRecord>,
    pub globals: HashMap<u32, GlobalRecord>,
    pub game_settings: HashMap<u32, GameSetting>,
}
```

Existing `parse_esm_cells()` callers continue to work — they get just
`.cells`. Two passes over a 100 MB ESM run in well under a second on a
release build, and keeping the cell pipeline untouched preserves the
renderer behaviour we already trust.

### Items (`items.rs`)

`ItemRecord` covers every item-bearing record type via an `ItemKind` enum
that carries the type-specific stats. Common name / model / value / weight
fields live on the parent struct in `CommonItemFields`:

```rust
pub struct ItemRecord {
    pub form_id: u32,
    pub common: CommonItemFields, // editor_id, full_name, model_path, value, weight
    pub kind: ItemKind,
}

pub enum ItemKind {
    Misc,
    Book { teaches_skill: u32, skill_bonus: u8, flags: u8 },
    Note { note_type: u8, topic_form: u32 },
    Ingredient { magic_effects: Vec<u32> },
    Aid { magic_effects: Vec<u32>, addiction_chance: f32 },
    Key,
    Ammo { damage, dt_mult, spread, casing_form, clip_rounds },
    Armor { biped_flags, dt, dr, health, slot_mask },
    Weapon { ammo_form, damage, clip_size, anim_type, ap_cost,
             skill_form, min_spread, spread, crit_mult, reload_anim },
}
```

Per-record parsers for **WEAP, ARMO, AMMO, MISC, KEYM, ALCH, INGR, BOOK,
NOTE** all dispatch through the records aggregator. Field selection is
intentionally minimal — just what gameplay systems need. Adding more
fields later is trivial; the parsers walk sub-records by 4-char code and
ignore anything they don't recognize.

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
}

pub struct InventoryEntry {
    pub item_form_id: u32,
    pub count: i32,
}

pub struct LeveledList {
    pub form_id: u32,
    pub editor_id: String,
    pub chance_none: u8,        // 0–100, "list rolls nothing" probability
    pub flags: u8,              // bit 0 = calc from all, bit 1 = calc per item
    pub entries: Vec<LeveledEntry>,
}

pub struct LeveledEntry {
    pub level: u16,
    pub form_id: u32,
    pub count: u16,
}
```

`LVLI` (leveled items) and `LVLN` (leveled NPCs) share the same `LeveledList`
type because they have the same sub-record layout — they only differ in
which type of base record they reference.

### Actors (`actor.rs`)

NPCs, races, classes, factions:

```rust
pub struct NpcRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub model_path: String,
    pub race_form_id: u32,
    pub class_form_id: u32,
    pub voice_form_id: u32,
    pub factions: Vec<FactionMembership>,   // SNAM sub-records
    pub inventory: Vec<NpcInventoryEntry>,  // CNTO sub-records
    pub ai_packages: Vec<u32>,              // PKID sub-records
    pub death_item_form_id: u32,
    pub level: i16,
    pub disposition_base: u8,
    pub acbs_flags: u32,
}

pub struct RaceRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    pub skill_bonuses: Vec<(u32, i8)>,   // (AVIF form_id, bonus)
    pub body_models: Vec<String>,
}

pub struct ClassRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    pub attribute_weights: [u8; 7],     // S/P/E/C/I/A/L
    pub tag_skills: Vec<u32>,
}

pub struct FactionRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub flags: u32,
    pub relations: Vec<FactionRelation>,    // XNAM
    pub ranks: Vec<String>,                 // MNAM (per-rank male label)
}

pub struct FactionRelation {
    pub other_faction: u32,
    pub modifier: i32,
    pub combat_reaction: u8,    // 0=neutral, 1=enemy, 2=ally, 3=friend
}
```

### Globals and game settings (`global.rs`)

Both record types boil down to `(editor_id, value)` pairs. The value type
is determined by an `FNAM` byte for `GLOB` and by the editor_id prefix
convention for `GMST` (`i…` int, `f…` float, `s…` string, `b…` bool/short):

```rust
pub enum SettingValue {
    Int(i32),
    Float(f32),
    String(String),
    Short(i16),
}

pub struct GlobalRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub value: SettingValue,
}

pub struct GameSetting {
    pub form_id: u32,
    pub editor_id: String,
    pub value: SettingValue,
}
```

## Real FNV.esm — measured

Running `parse_esm()` on a real `FalloutNV.esm` (April 2026 patch
revision) in release mode:

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
| **Records total** | **13,684** |

Plus the cell side (~1100 interior cells, ~30k exterior cells, ~17k base
objects). Total parse time: **0.19 s** for the full ~190 MB file.

## Tests

- **64 unit tests** in the plugin crate covering:
  - Manifest TOML parsing
  - `RecordType` 4-char codes and ECS spawn integration
  - DataStore + dependency resolver (DAG, conflict resolution, deterministic tiebreak)
  - Legacy ESM/ESP/ESL slot bridge
  - ESM cell parser (STAT extraction, REFR position/scale, group walker)
  - **M24 record parsers** — WEAP/ARMO/MISC field extraction, CONT inventory, LVLI leveled entries, NPC race/class/factions/inventory/AI packages, FACT relations + ranks, GLOB/GMST typed values, `extract_records` group walking
- **2 ignored integration tests** running against real `FalloutNV.esm`:
  - `parse_real_fnv_esm` — cell + static counts, Saloon refs
  - `parse_real_fnv_esm_record_counts` — record category counts with
    realistic floors, plus a Varmint Rifle / NCR faction spot-check

Run them:

```bash
cargo test -p byroredux-plugin                                                # 64 tests, no game data
cargo test -p byroredux-plugin --release -- --ignored                         # both integration tests
```

## FO4 architecture records (session 10)

To render FO4 architectural cells, the ESM parser was extended with
four additional record types that compose prefab buildings:

- **`SCOL`** — static collections. A list of `(STAT reference, transforms[])`
  tuples. One SCOL expands into many placements at cell-load time.
  The body parser (#405) walks every `ONAM` (STAT reference) and collects
  all following `DATA` blocks as the transform list for that part, so a
  single SCOL can expand into hundreds of placements in one pass.
- **`MOVS`** — movable statics. STATs that respond to havok impulses;
  record-level we just treat them as STATs plus a "movable" flag.
- **`PKIN`** — pack-ins. Pre-assembled groups of references used as
  reusable room modules (bathroom, kitchen, etc.). Resolve to their
  component references at spawn time.
- **`TXST`** — texture sets. Parsed as an 8-slot texture array (#357):
  diffuse / normal / glow / parallax / env / env mask / multilayer /
  specular (TX00–TX07). Referenced by `BSLightingShaderProperty` and by
  LAND texture splatting. Earlier revisions extracted only TX00 — FO4
  architecture now surfaces the full slot list on `ImportedMesh`.

These land in `EsmIndex` alongside the M24 record categories. The cell
loader expands `SCOL` / `PKIN` inline when walking placements; `TXST`
is keyed by `FormId` and resolved when the material layer references it.

## Session 11 fixes

A batch of correctness + performance fixes landed on top of the FO4
architecture work:

- **`XCLW` water plane height** (#397) — CELL sub-record. Surfaced on the
  cell descriptor so water rendering can pick up the correct plane height
  instead of defaulting to z=0.
- **`XESP` ref gating** (#349) — REFR sub-record. The default-disabled
  flag is honoured at cell load, so ruined walls, stage placeholders, and
  quest-spawn markers that ship "off" in the ESM don't render until a
  quest enables them.
- **Skyrim CELL extended sub-records** (#356) — Skyrim adds several
  sub-records the FNV-first walker didn't recognise; they now parse
  cleanly instead of emitting warnings.
- **`VMAD` script attachments** (#369) — surface `has_script` on REFR /
  base records so the eventual script runtime can see which references
  carry Papyrus attachments without re-scanning the ESM.
- **Variant-aware group end** (#391) — thread the game variant through
  the ESM walker so per-game GRUP end-marker differences (Skyrim vs
  FNV vs FO4) no longer mis-align the walk.
- **Single-pass exterior cell load** (#374) — collapsed three separate
  ESM walks (world, block, sub-block) into one. Exterior 3×3 grid load
  time dropped proportionally.
- **`LIGH` DATA color byte order** (#389) — colour bytes are `BGRA`, not
  `RGB` as the earlier reader assumed. Light colors now match the CK.
- **Legacy ESM parser stub cleanup** (#390) — the per-game legacy stubs
  (`tes3.rs` / `tes4.rs` / etc.) were dead; deleted and the live
  `EsmIndex` aggregator is now the only entry point.

## Phase 2 — deferred

The following record types stay deferred until the runtime systems that
consume them come online:

- **`QUST` / `DIAL` / `INFO`** — quest stages, dialog topics, dialog
  responses. Heavy condition / branching tree complexity; only valuable
  once the quest runtime exists. See the [Quest & Story Manager](../legacy/papyrus-api-reference.md)
  legacy doc for the full surface area.
- **`PERK`** entry points — ~120 types per the Perk Entry Points memory
  doc. Needs the perk evaluator first.
- **`MGEF` / `SPEL` / `ENCH`** — magic effects, spells, enchantments. Need
  the effect runtime.
- **`AVIF`** — actor value definitions. Currently referenced by raw form
  ID; this is just metadata mapping, low value standalone.
- Dynamic weapon `DNAM` fields beyond the basic stats block — many
  version-dependent quirks; we extract enough for current needs.

See [ROADMAP.md](../../ROADMAP.md) for the full deferred list and [the
M24 entry](../../ROADMAP.md#m24-phase-1-full-esmesp-record-parser--done)
for the Phase 1 scope.

## Related docs

- [Cell Lighting](lighting-from-cells.md) — XCLL extraction and the RT
  multi-light pipeline that consumes it
- [Asset Pipeline](asset-pipeline.md) — how cell loading composes ESM
  records, BSA/BA2 archives, NIF parsing, and ECS spawning
- [Game Loop](game-loop.md) — where in the startup flow the cell loader runs
