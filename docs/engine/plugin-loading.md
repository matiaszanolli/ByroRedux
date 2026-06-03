# Plugin Loading

How plugins (ESM, ESP, ESL, ESH, and Redux-native manifests) are
discovered, parsed, conflict-resolved, and exposed to the ECS. The system
lives in [`crates/plugin/`](../../crates/plugin/).

---

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

// For legacy ESM files: UUID v5 derived from filename (deterministic)
PluginId::from_filename("FalloutNV.esm")
// → UUID v5(PLUGIN_NAMESPACE, b"FalloutNV.esm")
// → always the same hash regardless of load-order position

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

`LegacyLoadOrder::resolve(LegacyFormId)` converts any of these to a
`FormIdPair` by looking up the plugin at the given slot index.

### FormIdPool (ECS Resource)

```rust
pub struct FormIdPool {
    to_runtime: HashMap<FormIdPair, FormId>,
    to_pair: Vec<FormIdPair>,
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
3. The candidate with the highest overlap wins (`DepthResolved`).
4. On a tie (no candidate depends on any other) → `TieBreak` by `min(PluginId)` — deterministic but arbitrary.

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

**Not yet parsed**: SOUN, SNCT, SOPM, MUSC, MUST, ASPC, REVB, AECH (audio).

### EsmCellIndex

The structured output of cell parsing:

```rust
pub struct EsmCellIndex {
    pub cells:               HashMap<String, CellData>,                        // EditorID → interior cell
    pub exterior_cells:      HashMap<String, HashMap<(i32, i32), CellData>>,   // worldspace → grid → cell
    pub statics:             HashMap<u32, StaticObject>,                       // FormID → base form
    pub landscape_textures:  HashMap<u32, String>,                             // LTEX → texture path
    pub worldspaces:         HashMap<String, WorldspaceRecord>,
    pub texture_sets:        HashMap<u32, TextureSet>,                         // TXST → 8-slot bundle
    pub scols:               HashMap<u32, ScolRecord>,                         // FO4+ combined static
    pub packins:             HashMap<u32, PkinRecord>,                         // FO4+ pack-in
    pub movables:            HashMap<u32, MovableStaticRecord>,
    pub material_swaps:      HashMap<u32, MaterialSwapRecord>,
}
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

## Legacy Bridge

[`crates/plugin/src/legacy/`](../../crates/plugin/src/legacy/)

Scaffolding for converting the load-order-dependent Bethesda Form IDs
encountered in ESM binaries into stable `FormIdPair` values.

```rust
pub struct LegacyLoadOrder {
    slots:     Vec<Option<PluginId>>,  // 0x00–0xFC
    esl_slots: Vec<Option<PluginId>>,  // 0x000–0xFFF
    esh_slots: Vec<Option<PluginId>>,  // 0x00–0xFF
}

pub fn resolve(&self, legacy: LegacyFormId) -> Option<FormIdPair>
```

Register plugins by their slot before resolving:

```rust
let mut lo = LegacyLoadOrder::new();
lo.register(0x00, PluginId::from_filename("FalloutNV.esm"));
lo.register(0x01, PluginId::from_filename("DeadMoney.esm"));

let pair = lo.resolve(LegacyFormId(0x01_000014));
// → FormIdPair { plugin: PluginId("DeadMoney.esm"), local: LocalFormId(0x000014) }
```

**Status:** The bridge type exists; the call site that passes it into
`parse_esm_with_load_order` for multi-master stacks is in progress.

---

## See Also

- [ESM Records](esm-records.md) — what each record type contains
- [Asset Pipeline](asset-pipeline.md) — how the `EsmCellIndex` drives cell loading
- [ECS](ecs.md) — how `FormIdComponent` attaches a `FormId` to spawned entities
- [`crates/plugin/src/`](../../crates/plugin/src/) — full source
