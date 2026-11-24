# Plugin Loading

How plugins (ESM, ESP, ESL, ESH, and Redux-native manifests) are
discovered, parsed, conflict-resolved, and exposed to the ECS. The system
lives in [`crates/plugin/`](../../crates/plugin/).

---

> **Status (#5609): the manifest → `DataStore` → `DependencyResolver`
> pipeline this doc describes has no production caller. The live path is
> `parse_esm_with_load_order` + `FormIdRemap`/`GlobalSlot`
> (`byroredux/src/cell_loader/load_order.rs`); this document describes
> the designed Redux-native layer, kept as the surface a future loader
> grows into (ROADMAP's Owed list tracks it).**


## The live legacy load order: activation and the fold

What the engine does today with a legacy plugin set (`--master …` `--esm`), the Phase 1 slice of the
[Legacy Mod Compatibility PRD](../prd/legacy-mod-compatibility.md) (#6670) built on the M46.0 path:

1. **Identity (LMC-ID-001, -009).** A legacy plugin is known by `byroredux_core::form_id::plugin_key`: its basename,
   case-folded, `\` and `/` equivalent. The load-order key (`plugin_basename_lc`), a `MAST` entry, a script's
   `GetFormFromFile` / `IsDLCInstalled` file name and `PluginId::from_filename` all go through it, so `Skyrim.esm`,
   `skyrim.esm` and `Data\Skyrim.esm` are one plugin. (Before the fold a script's `"Skyrim.esm"` hashed apart from the
   load order's `skyrim.esm`, so `GetFormFromFile` returned `None` and no New Vegas DLC read as installed.) The fold is
   Rust's full Unicode `to_lowercase`. The load-order key used to be ASCII-only, so a plugin named with a non-ASCII
   capital changed identity, and ids saved against it no longer resolve. The fold is also not any filesystem's case
   rule: final sigma and `İ` fold differently from NTFS's per-character table. Which host rule to match is an open
   LMC-ID-009 decision (#7249).
2. **Activation (LMC-ID-003, -011)**, once at boot, before anything reads argv
   (`byroredux/src/plugin_activation.rs` over `crates/plugin/src/activation.rs`). The profile is the `--master` plugins
   in argv order, then the `--esm`. Each plugin's `TES4` record alone is read for its masters. A master the profile
   lists after one of its dependents is hoisted to just ahead of the first dependent that needs it; nothing else moves,
   so profile order decides among unrelated plugins. A plugin whose master is not in the profile is disabled with every
   plugin that transitively depends on it, and activation continues; so are a duplicate identity, a master cycle and an
   unreadable header. The activated `--master` list replaces the given one in the effective args, so the cell loader,
   the exterior context, the HUD probes, `--list-cells` and the plugin set a save records all see one order. Every move
   and every disabled plugin (with the chain back to the missing master) is logged as a warning and kept for
   `plugins.activation`. The `--esm` must activate and stay last, or boot stops with the report: a missing master of
   its own names the `--master` to add.
3. **The fold (LMC-REC-002, -005, -007, -009; LMC-ID-008).** `parse_record_indexes_in_load_order` walks the plugins in parallel and folds
   them in load order with `EsmIndex::merge_from`, the last carrier of a record winning it whole (the `legacy` policy;
   `merge`, LMC-REC-003, combines records with lists, item 6). Before each plugin's merge, `FoldProvenance::note_plugin`
   (`crates/plugin/src/esm/records/provenance.rs`) notes, from the record headers the walk traced, every form that
   plugin carries that an earlier one already did (an override, or a `DELE` deletion), and every form it defines in an
   ID space it does not own (an injected record). Only those forms are stored; the rest cost one lookup each. The
   provenance rides on the `LoadOrder` (it replaced #5428's SCPT-only last-writer table, `record_winner`), and the log
   line `load-order fold: N plugins, … forms overridden, … injected, in … s (… s of it provenance)` reports it.
   A deletion (the record header's `0x20` flag, LMC-REC-004) removes the form from every map that holds it, the cell
   index's included (cells, modelled base objects, pack-ins, texture sets, material swaps), and a deleted reference
   leaves its cell; `load_order_deletion_tests.rs` measures this on FNV, Skyrim SE and Fallout 4 + UFO4P against a raw
   record walk.
   Every plugin is read as the profile's game, the base master's (`ProfileGame`, LMC-REC-009): its TES4 light and
   medium bits mean what that game defines (xEdit tests them against the game it loads, not the file's own header), so
   an ESL-flagged plugin with an older header is light in a Skyrim SE profile; a plugin whose header bands as another
   game is read as the profile's anyway and logged. Those bits decide only the plugin's slot, and so its Form ID
   decoding (LMC-ID-008): no record count is imposed, but a light or medium plugin's own records whose object ids its
   space cannot hold (above `0xFFF` or `0xFFFF`) are masked into it and may collide, and the fold logs how many.
4. **The conflict report (LMC-REC-007).** `plugins.conflicts` lists the overridden forms with wins, losses and
   injections per plugin; `plugins.conflicts <formid>` one form's carriers in load order, its winner and the policy;
   and `plugins.conflicts <formid> fields` reads each carrier's record back from its file
   (`crates/plugin/src/esm/raw_record.rs`, which loads only the top-level groups the record type can sit in) and names
   the sub-records the winner changed, added or dropped against it. Each override is judged against the record it
   replaced (LMC-REC-006): it changes it, is identical to it (an ITM), or reverts an earlier plugin's change back to the
   definer's record. The winner is the carrier that owns the record: under `merge`, an override identical in every
   field the merge reads to the record it would replace takes no ownership (LMC-REC-006), so the chain marks it
   `[no-op]` and names the carrier before it; when `fields` finds bytes differing in such an override, the line says
   they are in no field the merge reads. A repeated sub-record is compared as a multiset,
   so one entry added to a list is one addition. The comparison is of bytes, so when two files index a shared master
   differently a form-ID field can differ in bytes only, and the line says so. `--plugin-report [formid] [fields]`
   prints the same (with the activation report) without a window: `byroredux --game fnv --master … --plugin-report`.

5. **Paired archives (LMC-AST-002).** After activation, each activated plugin brings the archives the game pairs with
   it by name, found in the plugin's own directory, whatever its extension (`.esm` and `.esl` too, an owner ruling on
   #6670). The rules are per game, from the base master's header (`crates/plugin/src/archive_pairing.rs`): Oblivion
   `<stem>` then nothing or a space, `-` or `_`; Fallout 3 / New Vegas `<stem>.bsa` or `<stem> - <anything>.bsa`;
   Skyrim `<stem>.bsa`; Fallout 4 `- Main`, `- Textures`, `- Voices_en` `.ba2`; Starfield `- Main`, `- Textures`,
   `- Localization`, `- Voices_en` `.ba2`. A paired archive is a mod bundle (`DeadMoney - Main.bsa` carries meshes,
   textures and sounds), so it is written into the effective args for the mesh, texture and sound providers, the script
   provider where the game archives its compiled scripts, and the material provider where its materials are `.bgsm`
   files. The last-wins providers get them after the listed archives in load order; the first-wins script provider
   gets them ahead, reversed, so the same archive wins everywhere: the listed (official) archives first, then each
   plugin's, the later plugin's winning, and loose files over all of them. An archive the args already name keeps its
   place and its flags. What each plugin brought is logged and shown by `plugins.activation` and `--plugin-report`:
   `DeadMoney.esm pairs DeadMoney - Main.bsa, DeadMoney - Sounds.bsa`.

6. **Records with lists merge (LMC-REC-003).** Under the session's record policy, `--merge-policy merge` (the default,
   per the PRD's decision Q2) or `legacy`, the fold combines independent overrides of a record with lists (the levelled
   lists `LVLI`, `LVLN`, `LVLC`, `LVSP`, form lists `FLST`, outfits `OTFT`, containers `CONT`; actors `NPC_` and `CREA`:
   inventory, factions, spells, perks, keywords, actor values; races `RACE`: spells, keywords, race relations; carried
   items `ALCH`, `AMMO`, `ARMO`, `BOOK`, `MISC`, `WEAP` and the rest: keywords; the keyword lists of `ACTI`, `FLOR`,
   `FURN` and `CONT` bases, which `EsmIndex::placed_object_keywords` keeps apart from their records and which merge on
   their own, reported as `keywords merged from: …`) instead of keeping the last record whole
   (`crates/plugin/src/esm/records/list_merge.rs`; each type declares its lists and scalar fields to the
   `list_record!` macro, which destructures the record whole so a new field cannot go unmerged). Two
   overrides are independent when neither plugin declares the other as a master, directly or through its masters; a
   plugin that masters every plugin whose changes the list holds wins it whole, as under `legacy`. Otherwise the
   override is diffed against its base (the latest earlier carrier among its own masters) per entry form, the entries of
   a form compared as a multiset of the rest of each entry (a levelled entry's level and count, a container entry's
   count, a faction's or perk's rank, an actor value's value): an added, removed or changed form is applied to the
   merged list, the later plugin winning a form or a field (a list's chance none, a container's name or weight, an
   actor's level) that two changed, and the earlier operation recorded as overruled. A form list's null entries hold a
   place and are never an operation. A list whose order is its meaning is one field: an actor's AI packages and a
   consumable's effects. An item's kind is compared as its record authored it, and the merge re-runs the Fallout
   loose-mod classification after writing merged items back. `ListMerge::note_plugin` runs beside `FoldProvenance::note_plugin`, before each
   plugin's `merge_from`, and writes the merged records back after it; the fold's log line counts the records merged and
   the operations overruled. `plugins.conflicts <formid>` shows a merged record's contributors and, per list entry form
   (`inventory 00000A02: added by mergea.esp`) and field, who decided it and whom they overruled; its carrier chain ends
   `(merged)`. Every such base has a keyword-list entry, empty when it authors none, so an override that drops every
   keyword clears them under either policy. Locations (`LCTN`) and quests (`QUST`) are still last-wins: Fallout 4 and
   Starfield write overrides of them as partial forms (record flag `0x4000`), which carry only the sub-records they
   change, and the fold does not overlay a partial form on the record it overrides yet (it replaces it whole, under
   either policy).

   The merge's ownership follows LMC-REC-006: an override identical to the record it would replace is a no-op that
   takes no ownership (it neither wins whole nor becomes a contributor), and an independent override identical to its
   own base changes nothing; the report names both (`no-op:`, `changes nothing:`), on merged records and on those the
   merge left as `legacy` keeps them.

7. **Mod roots (LMC-ID-012, LMC-AST-010, LMC-ID-011).** A mod root is one mod's directory as a mod manager deploys it
   (`byroredux/src/mod_roots.rs`). The profile names its roots with `--mod-root <dir>`, repeated, lowest priority
   first; the base master's directory, the game's `Data`, is the implicit unmanaged root and sorts first. Before
   activation, each root lists the plugins and archives at its top level, and a `--master` or `--esm` given as a bare
   file name that names no file resolves through the roots, the highest-priority root first and the unmanaged root
   last (the resolved path replaces it in the effective args). A root's identity is its directory name, case-folded;
   two roots sharing one are reported (LMC-ID-010). After activation, each root learns which of the profile's plugins
   it ships; a root whose every such plugin was disabled is disabled with them and its loose files leave the session,
   while a root that keeps one keeps its assets with a warning. When the profile names any root, activation writes the
   enabled roots into the effective args as `--loose-root` flags in order, and every asset provider's loose tier is
   their loose files in that order (`asset_provider::loose_dirs`), each root's index cached and walked once, the later
   root winning a path and the whole tier above every archive. Without `--mod-root` the loose tier is the `--esm`
   directory, as before. `plugins.activation` and `--plugin-report` list each root with its plugins (and how many are
   in the profile), archives and loose files, and how many of its files take a path from an earlier root.
8. **The profile fingerprint (LMC-ID-007, LMC-SAV-001).** The fold hashes each plugin's bytes (xxHash64) on the thread
   that walks it, and `install_form_resolver` builds the session's `ProfileRecord` from the load order, the mod roots,
   the extension manifests and grants and the record policy (`byroredux/src/profile_fingerprint.rs`). A save stores it
   and a load reports how the save's differs; `plugins.activation` and `--plugin-report` print the fingerprint
   ([save-load-roundtrip.md](save-load-roundtrip.md), the profile section).
9. **Sub-records no parser decodes (LMC-REC-010).** A parser reads the sub-records it knows and ignores the rest, so a mod's
   edit to a field with no decoder used to leave no trace. `EsmReader` now gathers them as it reads (`esm/opaque.rs`): a
   record a parser reads keeps the sub-records `crates/plugin/data/decoded_subrecords.toml` does not list as decoded for its
   type and game, and a record or group nobody reads (`skip_record`, `skip_group`: a record type with no parser) is kept
   whole. The plugin's parse returns them in `EsmIndex::opaque_subrecords` and the fold takes them out per plugin into
   `LoadOrder::opaque()` (`OpaqueStore`), stamped with the plugin that carried them, every carrier of a form kept in load
   order, because a loser's field is one a mod touched too. The load order's first plugin, the game's own master, is
   not held: its records are on disk (the official/user boundary proper is LMC-REC-008). `plugins.conflicts <formid>`
   prints `not decoded: <plugin>: TAG (n bytes), …` for each carrier and the summary the totals per plugin;
   `--plugin-report` prints the same. A mod plugin parses about 20% slower with collection on, and holds about a quarter
   of its file as opaque bytes (measured on the Unofficial Fallout 4 Patch: 14.6 MB, 177,513 sub-records, 3.5 MB).

   **The decoded-sub-record table.** No parser records what it reads, so the table is measured: `esm/decode_probe.rs`
   picks real records so every sub-record tag shows several different payloads, rebuilds a small plugin of them, parses it,
   removes one tag from every record of the type, parses again and compares the two indexes with `structural_digest`
   (the order-independent digest the load-order tests use). A tag whose removal changes the index, or fails the parse, is
   decoded. A tag judged on the payloads sampled can only err toward "not decoded", which keeps bytes the parsers also read.
   `cargo run --release -p byroredux-plugin --example inspect -- decoded-subrecords > crates/plugin/data/decoded_subrecords.toml`
   rewrites the file from the vanilla plugins in `decode_probe::TABLE_SOURCES` (about three minutes); the ignored tests in
   `crates/plugin/tests/decoded_table_real_data.rs` re-derive each section and fail when the parsers and the file disagree,
   in the direction that loses data first. A game or record type with no entry decodes nothing, so everything of it is kept.

10. **Scaling (LMC-PERF-001, -002).** The fold costs per record, not per pair of plugins: `load_order_scale_tests.rs` (opt-in, `--ignored`)
   folds synthetic profiles of up to 4,000 light plugins, wide and as a master chain, and fails when time per record triples
   (measured 0.6 to 0.9 µs per record across the range). A plugin finds its masters through one name-to-position map built per
   fold (`load_positions`, which also rejects a repeated name), and the masters' transitive closure is a bitset row per plugin
   (`MasterClosure`). A door transition does not fold again: `parse_record_indexes_shared` serves the resident index while
   the plugin set and file fingerprints are unchanged (#6123, #6591).
   Activation as a whole (LMC-PERF-001) is timed by the same file: a profile of light plugins that each bring a paired
   archive goes through master hoisting and archive pairing, the fold, and the texture, mesh, sound and script pools, warm
   and with its files evicted from the page cache. Pairing finds a plugin's archives through one `ArchiveDirectory` per
   directory (the names sorted by folded key, searched by the plugin's stem), not by testing every archive against every
   plugin, which made a 2,000-plugin profile's pairing quadratic (0.345 s, now 0.043 s). Measured on a Ryzen 9 7950X with
   an NVMe drive: 1,000 plugins activate in 0.35 s with the page cache warm and 0.33 s cold.

Not built yet in Phase 1: the mod profile beyond argv (discovery and the import that writes roots, LMC-ID-006; manifest
identities for roots), the official/user boundary
(LMC-REC-008), partial forms overlaid on the record they override and then the `merge` policy for locations and quests
(LMC-REC-003), lifting the regular-plugin ceiling (LMC-ID-004: a global FormID's top byte is its plugin's slot, so
regular slots end at `0xFB`) and the virtual legacy index that follows from it (LMC-ID-005).

## Overview

```
plugin.toml / FalloutNV.esm
         │
         ▼
  PluginManifest         ←── TOML (Redux-native)
  ESM binary parser      ←── binary (legacy Bethesda)
         │
         ▼
  DataStore.add_plugin() ── stages per-FormIdPair candidates
         │
         ▼
  DependencyResolver     ── computes winner per conflict
         │
         ▼
  DataStore.resolve()    ── produces ResolvedRecord map
         │
         ▼
  EsmCellIndex           ── cells / statics / texture-sets / worldspaces
  FormIdPool (Resource)  ── FormIdPair ↔ runtime FormId interning
```

---

## Plugin Manifests (Redux-native)

[`crates/plugin/src/manifest.rs`](../../crates/plugin/src/manifest.rs)

Redux-native plugins declare their identity and dependencies in a
`plugin.toml` file. The manifest assigns a stable UUID that replaces
the load-order-dependent slot index of the legacy format.

**TOML schema:**

```toml
[plugin]
uuid = "550e8400-e29b-41d4-a716-446655440000"   # UUID v4 — stable plugin identity
name = "MyMod"
version = "1.0.0"                               # SemVer

[[dependencies]]                                # Optional; repeat per dependency
uuid = "12345678-1234-1234-1234-123456789abc"
name = "BaseMaster.esm"                         # Informational only
```

```rust
pub struct PluginManifest {
    pub id: PluginId,
    pub name: String,
    pub version: semver::Version,
    pub dependencies: Vec<PluginId>,
}
```

Parsed by `PluginManifest::from_toml(src: &str)`. All validation is
structural (TOML parse errors); semantic validation (dependency graph
cycles) happens downstream in `DependencyResolver`.

---

## Form ID System

[`crates/core/src/form_id.rs`](../../crates/core/src/form_id.rs)

The Form ID design is the key architectural difference from Bethesda's
legacy format.

### Three layers

| Layer | Type | Stability | Purpose |
|---|---|---|---|
| **Stable identity** | `FormIdPair { plugin: PluginId, local: LocalFormId }` | Persistent across sessions and load-order changes | Serialisation, saves, manifests |
| **Runtime handle** | `FormId(u64)` | Session-local only | O(1) ECS lookups; interned via `FormIdPool` |
| **Legacy** | raw `u32` (`0xPP_LLLLLL`) | Load-order-dependent | Exists only in ESM binary; converted at parse time |

### PluginId

```rust
pub struct PluginId(pub u128);   // 128-bit UUID stored as u128

// For legacy ESM files: UUID v5 derived from the plugin key (deterministic)
PluginId::from_filename("FalloutNV.esm")
// → UUID v5(PLUGIN_NAMESPACE, plugin_key("FalloutNV.esm")) = UUID v5(PLUGIN_NAMESPACE, b"falloutnv.esm")
// → always the same hash regardless of load-order position, case or path (LMC-ID-001)

// For Redux-native plugins: declared in plugin.toml
PluginId::from_uuid(uuid)
```

The UUID v5 derivation means legacy plugins get a stable cross-session
identity without any manifest file — the filename alone is enough.

### Legacy Form IDs decoded at parse time

| Format | Layout | Slot size | Max forms |
|---|---|---|---|
| **Standard ESM/ESP** | `0xPP_LLLLLL` | 8-bit slot | 16 M per plugin |
| **ESL (Light Master)** | `0xFE_III_FFF` | 12-bit sub-slot (bits 12–23) | 4 096 per ESL |
| **ESH (Medium Master, Starfield)** | `0xFD_SS_FFFF` | 8-bit sub-slot (bits 16–23) | 65 536 per ESH |
| **Save-generated** | `0xFF_*` | — | ephemeral, never interned |

`FormIdRemap` + `GlobalSlot` (`crates/plugin/src/esm/reader.rs`, #1554)
resolve all three layouts to global slots at parse time:
`GlobalSlot::{Regular, Light, Medium}` (the ESH `0xFD` layout landed in
#4639) with `FormIdScheme::{Positional, Starfield}` choosing how a plugin's
master ordinals are read (Starfield's per-kind ordinals, #6118).

### FormIdPool (ECS Resource)

```rust
pub struct FormIdPool {
    to_runtime: HashMap<FormIdPair, FormId>,
    to_pair: Vec<FormIdPair>,
    /// Lazily rebuilt `local → entity` index behind `entity_with_local` (#6338).
    entity_index: Mutex<EntityIndex>,
}
```

Interning is idempotent — `pool.intern(pair)` always returns the same
`FormId` for the same `FormIdPair`. The pool is registered as an ECS
`Resource` and looked up during cell loading by `FormIdComponent`.

---

## DataStore

[`crates/plugin/src/datastore.rs`](../../crates/plugin/src/datastore.rs)

Accumulates records from all loaded plugins and produces a conflict-free
`ResolvedRecord` map.

```rust
pub struct DataStore {
    candidates: HashMap<FormIdPair, Vec<(PluginId, Record)>>,
    records:    HashMap<FormIdPair, ResolvedRecord>,
    plugins:    Vec<PluginManifest>,
    pub conflicts: Vec<Conflict>,
}

pub struct ResolvedRecord {
    pub record: Record,
    pub source: PluginId,            // winner
    pub overridden_by: Vec<PluginId>, // losers, in override order
}
```

**Lifecycle:**

1. `add_plugin(manifest, records)` — stages each record as a candidate keyed by `FormIdPair`.
2. `resolve_conflicts()` — for every `FormIdPair` with more than one candidate, calls `DependencyResolver::resolve_winner` and stores the result.
3. `get(pair)` — returns `Option<&ResolvedRecord>` from the resolved map.

---

## DependencyResolver & Conflict Resolution

[`crates/plugin/src/resolver.rs`](../../crates/plugin/src/resolver.rs)

When two plugins provide the same `FormIdPair`, the resolver picks a winner.

```rust
pub enum ConflictResolution {
    DepthResolved { winner: PluginId },  // dependency DAG determined winner
    TieBreak      { winner: PluginId },  // UUID lexicographic order (deterministic but arbitrary)
    UserResolved  { winner: PluginId },  // future: explicit user choice
}
```

**Winner selection algorithm** (`resolve_winner(&[PluginId])`):**

1. For each candidate, compute its full transitive dependency set (BFS over the adjacency graph).
2. Count how many other candidates it transitively depends on (overlap).
3. If exactly one candidate reaches the highest overlap **and it's > 0**, it wins (`DepthResolved`) — an intentional override.
4. Otherwise (no candidate depends on any other, OR more than one candidate is tied at the max — a diamond: two plugins that both depend on the same base but not on each other, #4082) → `TieBreak` by `min(PluginId)` among the tied candidates — deterministic and order-independent of plugin registration order, but arbitrary as far as intent goes, hence flagged for review.

**Example:**

```
Plugin A defines WEAP 0x001.
Plugin B defines WEAP 0x001.
B's manifest lists A as a dependency.

→ B's transitive deps = {A}, overlap = 1
→ A's transitive deps = {}, overlap = 0
→ Winner = B  (DepthResolved)
→ Conflict logged; A tracked in overridden_by
```

**Cycle handling:** BFS uses a visited set so cycles in the dependency
graph terminate without error — cycles are silently absorbed. A future
pass should detect and report them.

---

## ESM Parser

[`crates/plugin/src/esm/`](../../crates/plugin/src/esm/)

### Entry points

| Function | Purpose |
|---|---|
| `parse_esm(data: &[u8])` | Parse full ESM, return `EsmIndex` (typed record maps + cell index) |
| `parse_esm_with_load_order(data, remap)` | Same, with optional `FormIdRemap` for multi-plugin stacks |
| `parse_esm_cells(data)` | Deprecated wrapper — calls `parse_esm_with_load_order` internally |

`parse_esm` is a fused single-pass design (#527): one traversal of the
top-level GRUP tree extracts both the typed record maps and the full cell
index. Pre-fix required two passes.

### Record types currently decoded

**Structured decode** (sub-records fully parsed into typed Rust structs):

| Category | Types |
|---|---|
| Items | WEAP, ARMO, AMMO, MISC, KEYM, ALCH, INGR, BOOK, NOTE |
| Containers & leveled lists | CONT, LVLI, LVLN, LVLC |
| Actors | NPC_, CREA, RACE, CLAS, FACT |
| World data | WTHR, CLMT, GLOB, GMST, REGN |
| Magic & scripting | SPEL, ENCH, MGEF, PERK, SCPT, VMAD |
| AI & quests | PACK, QUST, DIAL, INFO |
| Misc | ACTI, ARMA, AVIF, BPTD, COBJ, EXPL, EYES, FLST, FURN, HAIR, HDPT, IDLE, IMGS, LGTM, MESG, MSWP, MOVS, NAVM, NOTE, OTFT, PKIN, PROJ, SCOL, SLGM, SNDR, TERM, TREE, WATR |
| Cells & worldspaces | CELL, WRLD, LAND, LTEX, TXST |

**Passthrough-raw** (model path extracted, rest skipped): STAT, MSTT,
FURN, DOOR, LIGH, FLOR, IDLM, BNDS, ADDN, TACT.

**Audio**: SOUN, MUSC, MUST and ASPC are parsed (`records/dispatch_misc_stub.rs` —
SOUN into a typed `SounRecord` with FNAM sound-path decode; MUSC per game —
FO3/FNV's file + level, Skyrim's flags + `TNAM` track list; Skyrim's `MUST`
into `EsmIndex::music_tracks` with file, loop points and palette members, #3816).
**Not yet parsed**: SNCT, SOPM, REVB, AECH.

### EsmCellIndex

The structured output of cell parsing:

```rust
pub struct EsmCellIndex {
    pub cells:                       HashMap<String, CellData>,                        // EditorID → interior cell
    pub exterior_cells:              HashMap<String, HashMap<(i32, i32), CellData>>,   // worldspace → grid → cell
    pub worldspace_persistent_cells: HashMap<String, CellData>,                        // worldspace → its persistent CELL
    pub statics:                     HashMap<u32, StaticObject>,                       // FormID → base form
    pub landscape_textures:          HashMap<u32, String>,                             // LTEX → texture path
    pub landscape_texture_sets:      HashMap<u32, TextureSet>,                         // LTEX → its TXST bundle
    pub landscape_texture_txsts:     HashMap<u32, u32>,                                // LTEX → TNAM TXST FormID; re-joined after every merge (#6036)
    pub landscape_grasses:           HashMap<u32, Vec<u32>>,                          // LTEX → authored GRAS FormIDs (GNAM array, authored order — #4642)
    pub landscape_texture_names:     HashMap<u32, String>,                            // LTEX → EDID; the ground-cover affinity key (#4899)
    pub worldspaces:                 HashMap<String, WorldspaceRecord>,
    pub worldspace_climates:         HashMap<String, u32>,                             // worldspace → CLMT FormID
    pub texture_sets:                HashMap<u32, TextureSet>,                         // TXST → 8-slot bundle
    pub scols:                       HashMap<u32, ScolRecord>,                         // FO4+ combined static
    pub packins:                     HashMap<u32, PkinRecord>,                         // FO4+ pack-in
    pub movables:                    HashMap<u32, MovableStaticRecord>,
    pub material_swaps:              HashMap<u32, MaterialSwapRecord>,
    pub layered_material_swaps:      HashMap<u32, LayeredMaterialSwapRecord>,          // Starfield LMSW (#5757)
    pub material_path_forms:         HashMap<u32, String>,                             // Starfield MTPT path
    pub landscape_texture_havok:     HashMap<u32, LandscapeTextureHavok>,              // LTEX → HNAM/MNAM Havok data (#6048)
    pub material_type_havok:         HashMap<u32, u32>,                                // MATT → Havok material id, BSCRC32 of MNAM (Skyrim, #6048)
}
```

`worldspace_persistent_cells` is the one most easily missed and the least
cosmetic: a worldspace's persistent CELL is where quest actors such as
Skyrim's Hadvar and Ralof are placed. It is keyed by worldspace rather than
by grid because it has no XCLC grid of its own, which is why it cannot live
in `exterior_cells`.

```rust
```

Each `CellData` holds:
- `references: Vec<PlacedRef>` — all REFR/ACHR placed in the cell
- `lighting: Option<CellLighting>` — from the XCLL sub-record
- `landscape: Option<LandscapeData>` — LAND heightmap + splat (exterior only)
- `water_height: Option<f32>`
- Extended fields for Skyrim+ (image space, water type, acoustic space,
  music, climate, location, lighting template, regions, FO4+ precombined
  mesh XCRI list, navmeshes)

---

## Legacy Bridge (removed)

The *crates/plugin/src/legacy/* module (*LegacyFormId* / *LegacyLoadOrder*)
was scaffolding for converting load-order-dependent Bethesda Form IDs into
stable `FormIdPair` values. It never had a production consumer and was
deleted under #4384.

Load-order FormID resolution shipped under a different design: `FormIdRemap`
+ `GlobalSlot` (`crates/plugin/src/esm/reader.rs`, #1554), built per plugin
from its own TES4 header and handed to `parse_esm_with_load_order`. That is
what `cell_loader/load_order.rs` calls today.

The deleted module once held the only ESH (`0xFD`, Starfield medium master)
decoder; `GlobalSlot::Medium` and `FormIdScheme::Starfield` now model it in
the live path (#4639, #6118), so there is nothing to re-add. The old decoder
is recoverable from git history before #4384's commit.

---

## See Also

- [ESM Records](esm-records.md) — what each record type contains
- [Asset Pipeline](asset-pipeline.md) — how the `EsmCellIndex` drives cell loading
- [ECS](ecs.md) — how `FormIdComponent` attaches a `FormId` to spawned entities
- [`crates/plugin/src/`](../../crates/plugin/src/) — full source
