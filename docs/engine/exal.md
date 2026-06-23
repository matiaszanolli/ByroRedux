# EXAL — Exterior Abstraction Layer

**EXAL** (Exterior Abstraction Layer; pronounced "EX-al") is the canonical
translation tier for **outdoors rendering** — the worldspace environment that
sits behind the open world: terrain, sky, sun, weather, water, and distant LOD.
It is the sibling of [`nifal.md`](nifal.md): where NIFAL translates per-game
**NIF geometry/material** data into one canonical representation, EXAL translates
per-game **ESM environment** data (WRLD / CLMT / WTHR / LAND / WATR / CELL
lighting) into one canonical representation the renderer consumes identically for
every game.

"Abstraction" is the brand; the mechanism is **canonical translation** (per-game
`Imported*` → one resolved, game-agnostic representation). The verbs stay
`translate` / `canonical` / `resolve`; **EXAL** names the layer as a whole.

**Status**: PROPOSED (design, 2026-06-02). Implementation rolls out per §7.

**Goal**: every supported engine version (Oblivion / FO3 / FNV / Skyrim LE/SE /
FO4 / FO76 / Starfield) translates its native, per-game exterior data into **one
canonical, fully-resolved representation** through a single explicit `translate()`
boundary per category. The renderer (sky pass, terrain pass, water pass, the sun
directional light, the LOD ring) consumes the canonical representation
**identically for every game** — no per-game branches downstream, no `Option`
"resolve-it-later" fallbacks, no render-time heuristics.

This is the same doctrine NIFAL formalises (`feedback_format_translation.md`:
"never per-game branches in the shader; translate at the parser→Material
boundary"; `format_abstraction.md`: the GameVariant pattern), now applied to the
outdoors pipeline.

---

## 1. The three-tier model

```
                 parse                       translate()                   consume
  ESM records ───────────▶  Imported*  ───────────────────▶  Canonical  ─────────────▶  renderer
  (WRLD/CLMT/WTHR/         (raw, per-game          (one site per         (resolved,       (sky / terrain /
   LAND/WATR/CELL)          record structs:         category: folds       game-agnostic    water passes,
                            WorldspaceRecord,        in every per-game     resources +      sun light,
                            ClimateRecord,           quirk)                components)      LOD ring)
                            WeatherRecord, …)
```

| Tier | What it is | Where it lives | Rule |
|---|---|---|---|
| **Raw / `Imported*`** | A faithful, per-game decode of the ESM wire format. May carry `Option`s, per-game quirk fields (Oblivion has no DNAM; Skyrim adds DALC). **This tier is allowed to be messy** — it mirrors the file. | `crates/plugin/src/esm/` (`WorldspaceRecord`, `ClimateRecord`, `WeatherRecord`, WATR/LAND/XCLL parses) | Decode only; never the engine's source of truth. |
| **`translate()` boundary** | The single function per category that resolves a raw `Imported*` into the canonical tier. Folds in every per-game quirk so the output is one convention. | One module: `byroredux/src/env_translate.rs` (mirrors `material_translate.rs`; created step 1). | Exactly **one** site per category. No duplicate construction sites. |
| **Canonical** | The resolved, game-agnostic resource/component the renderer consumes. No `Option` "resolve-later" fields; every per-game decision already made. | ECS resources/components in `byroredux/src/components.rs` + `crates/core`. | The single source of truth. |

### The canonical-type rule (inherited from NIFAL)

> **Where an ECS resource/component already serves the game-agnostic,
> renderer-facing role, that resource IS the canonical type.** Introduce a *new*
> canonical type only where none exists.

The renderer already reads canonical exterior resources today — `CellLightingRes`,
`SkyParamsRes`, `WeatherDataRes`, `CloudSimState`, `WeatherTransitionRes`,
`WaterMaterial`/`WaterFlow`/`WaterKind`. EXAL does **not** fabricate a parallel
`CanonicalSky` struct that these copy from. It reaches the canonical tier by
(a) making one `translate()` site the sole producer of each resource, and
(b) removing residual per-game leaks (scattered `if game == Oblivion`, hardcoded
fallbacks, and the render-time heuristics noted in §2).

The one genuine **gap** where no canonical type yet exists is **distant object
LOD** (§5); that slice introduces a new canonical type, per the rule.

---

## 2. Per-category leak inventory (2026-06-02)

How close each exterior category is to the canonical contract today.

### Terrain (LAND) — **mostly canonical at the GPU layer; translation in one place already**

LAND parsing is format-stable across all games (33×33 vertex grid, 128-unit
spacing, VNML/VCOL bytes, ATXT/VTXT splat). `cell_loader/terrain.rs`
(`spawn_terrain_mesh`, `build_cell_splat_layers`) is effectively a single
translate site producing the GPU mesh + the terrain-tile splat slot — no per-game
branching. **Status: clean, no leaks.** EXAL's only terrain work is (a) bringing
the splat-layer resolution under the named boundary for consistency, and (b)
distant terrain LOD, covered in §5.

### Sky / clouds — **canonical resources exist; translation scattered + has a hardcoded fallback**

The renderer reads `SkyParamsRes` (+ `CloudSimState`) in `composite.frag`'s
`compute_sky` — no per-game branches in the shader. **Good.** But the *producer*
is split across two functions in `scene/world_setup.rs`:
- `apply_worldspace_weather` builds `SkyParamsRes` from a WTHR record.
- `insert_procedural_fallback_resources` builds a **hardcoded warm Mojave desert
  sky** when the worldspace has no climate/weather.

Leak: the fallback is a render-affecting heuristic living outside any boundary,
and the WTHR→Sky mapping is interleaved with texture-acquisition bookkeeping
(`resolve_cloud_layer`, sun-sprite `load_dds`, `sky_textures_to_release`). EXAL
consolidates the WTHR→`SkyParamsRes` decode into one `translate_sky` and keeps the
fallback as an explicit, documented canonical default (not an inline magic block).

### Sun / directional light — **two producers; one hardcoded seed that the other overwrites**

- `apply_worldspace_weather` seeds `CellLightingRes.directional_dir` with a
  **hardcoded** `[-0.4, 0.8, -0.45]` (`world_setup.rs:235`).
- `compute_sun_arc` (`systems/weather.rs`) derives the real per-frame sun
  direction + intensity from the climate's `tod_hours`, overwriting the seed.

Leak: the hardcoded seed is dead for one frame and misleading; the sun model is a
genuine canonical computation but lives in the per-frame system, not at the
translate boundary, and the per-worldspace **latitude tilt** is still a fixed
`z = 0.15` constant (`#802`, with `#1019` deferred). EXAL makes the climate's
`tod_hours` + a per-worldspace tilt the canonical sun inputs, resolved once.

### Weather / TOD — **canonical; the cleanest of the dynamic categories**

`WeatherDataRes` holds the full WTHR NAM0 table + `tod_hours` + Skyrim-only
`skyrim_dalc_per_tod` (`Option`, correctly `None` on FNV/FO3/Oblivion) +
`wind_speed`. `weather_system` interpolates it per frame; `WeatherTransitionRes`
cross-fades. The `Option<DalcCubeYup>` is **not** a leak — it is the canonical
encoding of "this game has no DALC ambient cube", consumed uniformly. **Status:
canonical.** EXAL only moves the WTHR→`WeatherDataRes` decode under the boundary.

### Water (WATR) — **carved out into [WATAL](watal.md) (its own double-ended layer)**

> **Moved (2026-06-19):** water graduated from an EXAL sub-category to its own
> first-class **double-ended** layer — [`watal.md`](watal.md) — because water feeds
> the **physics solver** (buoyancy/flow/swim/drown) as well as the renderer, which
> EXAL (render-only) does not model. EXAL retains the `default_water_for_worldspace`
> height-leak below as the shared §4 GameVariant row; WATAL owns the WATR→material
> translate, the canonical-type promotion, and the physics arm.

`WaterMaterial`/`WaterKind`/`WaterFlow` (`crates/core/src/ecs/components/water.rs`)
are a fully-resolved canonical component; `resolve_water_material` (water.rs) is a
single translate site for WATR→`WaterMaterial`. **Good.** The leak is the
**default water height** decision in `default_water_for_worldspace`
(`exterior.rs:88`): an inline `if game == Oblivion { Z=0 } else { DNAM }`. This is
exactly the kind of per-game branch EXAL routes through the GameVariant table (§4).

### LOD distance rendering — **terrain LOD exists; object/tree LOD absent; no canonical model**

`terrain_lod.rs` synthesizes distant terrain from the LAND heightmap directly
(4×4-cell blocks, 12-block Chebyshev radius, stride-8 sampling, single base
texture, no BLAS). This is game-agnostic and works, and as of step 6 (below) it
is now the **universal fallback** behind the games' prebaked assets:
- Skyrim+/FO4 prebaked **`.btr` terrain meshes** + per-quad diffuse are consumed
  (step 6, 2026-06-19) as a per-block source upgrade inside the synth ring, so
  distant terrain is textured rather than flat there; Oblivion/FO3/FNV
  `Landscape\LOD\*.nif` + `_lod` textures are still un-consumed (synth-only);
- distant **object LOD** is consumed for Skyrim+/FO4 (baked per-quad `.bto`
  macro-meshes + object atlas, step 6) **and** for Oblivion/FO3/FNV via the
  `DistantLOD\*.lod` → `_far.nif` placement scheme (`PlacementLodProvider`,
  `cell_loader/placement_lod.rs`, #1726 — format reverse-engineered from the
  vanilla Oblivion corpus, see §Q3 below);
- there is still **no single canonical LOD model** — `terrain_lod` /
  `terrain_lod_btr` / `object_lod` are the per-game providers, fused to the
  streaming ring, with `IsLodTerrain` as the shared renderer-facing marker
  (the trait/`WorldLodRes` was judged forced ceremony — see step 6 note).

(Note: FO4 **precombined/previs** geometry is a *near-field* active-cell
optimization, not distant LOD — it is already handled by the M49 path and is
out of scope here.)

This is the largest gap and the user-flagged must-have — see §5.

---

## 3. The single boundary (proposed)

A top-level module `byroredux/src/env_translate.rs` (created in step 1, mirroring
`material_translate.rs`) hosts the per-category translate functions. As built
(steps 1 + 3), every function is **pure** — the caller pre-resolves the
`VulkanContext`-coupled texture handles into a `SkyTextures` and hands them in,
exactly as `material_translate` takes a pre-resolved `ResolvedPaths`:

```rust
// Pure: raw per-game record(s) (+ pre-resolved handles) → canonical value.

pub(crate) fn default_water_for_worldspace(            // step 1 — the GameVariant decision
    wrld: Option<&WorldspaceRecord>, game: GameKind,
) -> (Option<f32>, Option<u32>);                       // (height, type_form)

pub(crate) fn resolve_water_material(                  // step 1 — WATR → WaterMaterial
    waters: &HashMap<u32, WatrRecord>, xcwt_form: Option<u32>,
) -> (WaterMaterial, WaterKind, Option<WaterFlow>, Option<String>);

pub(crate) fn translate_exterior_cell_lighting(        // step 3 — WTHR day-slot → CellLightingRes
    wthr: &WeatherRecord, sun_dir: [f32; 3]) -> CellLightingRes;

pub(crate) fn translate_sky(                           // step 3 — WTHR + pre-resolved handles → SkyParamsRes
    wthr: &WeatherRecord, sun_dir: [f32; 3], textures: SkyTextures) -> SkyParamsRes;

pub(crate) fn translate_weather(                       // step 3 — WTHR (+climate) → WeatherDataRes
    wthr: &WeatherRecord, climate: Option<&ClimateRecord>) -> WeatherDataRes;

// Procedural fallback (no climate/weather) — explicit canonical constructors,
// replacing the old inline hardcoded Mojave block in the render-setup path.
pub(crate) fn procedural_fallback_cell_lighting(sun_dir: [f32; 3]) -> CellLightingRes;
pub(crate) fn procedural_fallback_sky(sun_dir: [f32; 3]) -> SkyParamsRes;
pub(crate) fn procedural_fallback_weather() -> WeatherDataRes;
```

The caller (`scene::world_setup::apply_worldspace_weather`) keeps only
orchestration: pre-resolving cloud/sun textures (`resolve_cloud_layer`,
`resolve_sun_sprite`), world insertion, the bindless-handle release lifecycle,
and the WTHR cross-fade-vs-insert decision. A future `translate_sun` (step 4)
will fold the sun-arc model in the same shape; `translate_cell_lighting` for the
*interior* XCLL path is deferred (its decode already lives correctly in the
parser tier).

**Contract for every function above:**
1. **Single site.** Both the bulk `--grid` loader and the streaming bootstrap
   call these — no second construction of `SkyParamsRes` etc. anywhere.
2. **No render-time fallback.** The "no climate/weather" case is handled *here*
   by returning an explicit, documented canonical default (the current Mojave
   block becomes `SkyParamsRes::procedural_default()`), not by a branch in the
   render loop or a separate `insert_procedural_fallback_resources`.
3. **No `Option` resolve-later leaks** on the canonical output beyond the ones
   that *encode a real game distinction* (e.g. `skyrim_dalc_per_tod: None`,
   `default_water_height: None` = "no default water"). Those are documented as
   canonical sentinels, not "fill it in later".

The per-frame `weather_system` stays where it is — it is a *consumer* that samples
the canonical `WeatherDataRes` + `SunModel`, not a translate site.

---

## 4. The GameVariant doctrine for environment

Per-game quirks must route through a **single `GameKind`-keyed decision**, not
scattered `if game == X` checks. `GameKind` (`crates/plugin/src/esm/reader.rs`)
already exists — Oblivion / Fallout3NV / Skyrim / Fallout4 / Fallout76 /
Starfield. EXAL concentrates the exterior quirks into one table consumed by the
translate functions:

| Quirk | Oblivion | FO3/FNV | Skyrim | FO4 | Source field |
|---|---|---|---|---|---|
| Default water height | sea level `Z=0` if NAM2 present (no DNAM) | WRLD DNAM `[1]` (e.g. WastelandNV −2300) | WRLD DNAM (Tamriel −14000) | WRLD DNAM | `WorldspaceRecord::{water_form, default_water_height}` |
| DALC ambient cube | none | none | per-TOD 6-axis cube | (varies) | `WeatherDataRes::skyrim_dalc_per_tod` |
| XCLL extended tail | base | +40-byte FNV tail | +92-byte Skyrim tail | Skyrim-like | `CellLightingRes` extended fields |
| Per-cell water type (XCWT) | — | worldspace NAM2 only | XCWT override | XCWT override | cell `xcwt` |
| Distant terrain source | `Landscape\LOD\*.nif` + `_lod` tex | same | `<World>.<lvl>.<x>.<y>.btr` + per-quad DDS | same (`.btr`) | §5 |
| Distant object source | `DistantLOD\<W>_<x>_<y>.lod` → `_far.nif` | same | baked `.bto` per quad + atlas, VWD-gated | same (`.bto`) | §5 |

The current `default_water_for_worldspace` is the *prototype* of this pattern
(it already branches on `GameKind`); EXAL generalises it so every row above is one
match arm in one place, and the translate functions never re-derive game behaviour
inline.

---

## 5. LOD distance rendering (the must-have)

LOD is the category with the biggest gap and gets a dedicated canonical model. The
abstraction: the renderer consumes a uniform **LOD ring** description; per-game
providers supply the geometry/textures behind it.

### 5.1 Canonical model (new types — no existing ECS role)

```rust
/// Distance bands, shared by terrain + objects + trees. The renderer
/// selects a band from camera distance; providers fill each band.
pub enum LodBand { Full, Near, Mid, Far }   // Full = streamed full-detail cell

/// One distant-object LOD instance the renderer draws without BLAS /
/// without gameplay components. Produced by the per-game LOD provider.
pub struct LodInstance {
    pub mesh: MeshHandle,
    pub transform: GlobalTransform,
    pub band: LodBand,
    pub material: u32,        // canonical material index (atlas-aware)
}

/// Per-worldspace LOD resource the streaming system maintains.
pub struct WorldLodRes {
    pub terrain_blocks: Vec<TerrainLodBlock>,   // generalises terrain_lod.rs
    pub object_instances: Vec<LodInstance>,     // NEW
}
```

### 5.2 The `LodProvider` trait (GameVariant for LOD)

The research (Q2/Q3) established that the engine lineage uses **two structurally
different distant-LOD schemes**, so the trait abstracts the *runtime authority* of
each:

```rust
trait LodProvider {
    /// Distant terrain for a quad/block. Prebaked providers load the
    /// game's LOD mesh + per-quad textures; the heightmap fallback
    /// synthesizes from LAND (today's path).
    fn terrain_quad(&self, quad: QuadCoord, level: LodLevel) -> Option<TerrainLodBlock>;
    /// Distant static objects for a quad/cell at a given band.
    fn object_lod(&self, quad: QuadCoord, band: LodBand) -> Vec<LodInstance>;
}
```

Per-game impls (the runtime source per the Q3 finding):

- **HeightmapLodProvider** (all games, universal fallback) — the current
  `terrain_lod.rs` synthesis, refactored behind the trait. Keeps distant terrain
  working everywhere on day one and backstops missing prebaked assets. Supplies
  **no** object LOD.
- **CombinedLodProvider** (Skyrim LE/SE, FO4) — the **baked, atlas** scheme.
  - Terrain: `Meshes\Terrain\<World>\<World>.<level>.<x>.<y>.btr` (a renamed NIF;
    `level ∈ {4,8,16,32}` = cells-per-quad, `x`/`y` = SW-corner cell); textured
    from `Textures\Terrain\<World>\<World>.<level>.<x>.<y>.dds` + `_n.dds`. Quad
    sizing comes from the 16-byte binary `LODSettings\<World>.lod`
    (SW cell X/Y, stride, level-min 4 / level-max 32).
  - Objects: baked per-quad `.bto` macro-meshes (also renamed NIFs) + the shared
    `Textures\Terrain\<World>\Objects\<World>.Objects.dds` atlas, **selected by
    filename** (level + quad X/Y). **STAT `MNAM` is generation-time only and is
    NOT read at runtime** — so this provider needs no MNAM parse.
  - **VWD culling rule**: the base record's *Visible-When-Distant* / "Has Distant
    LOD" flag is the one runtime signal the real engine reads — to **cull the full
    model** once its quad's `.bto` is active (otherwise the full mesh and the LOD
    mesh both draw → z-fighting). EXAL must parse this record-header flag and have
    the streaming ring suppress the full REFR beyond the full-detail radius.
- **PlacementLodProvider** (Oblivion, FO3, FNV) — the **per-object placement**
  scheme.
  - Terrain: `Meshes\Landscape\LOD\*.nif` + `_lod` diffuse/normal textures.
  - Objects: per-cell `DistantLOD\<World>_<x>_<y>.lod` placement lists that
    instance individual `_far.nif` low-poly meshes (one draw per entry). This is
    genuinely per-object (no atlas, no combined mesh) and is a separate code path
    from the `.bto` scheme — do not try to unify them.

The renderer's draw path reads `WorldLodRes` only — it never knows which provider
filled it. This is the §1 contract applied to LOD: one consumer, per-game
producers. `.btr`/`.bto`/`_far.nif` all parse with the **existing NIF parser**
(just register the extensions); the only verify-against-real-data unknown is the
exact `BSTriShape`/`BSSubIndexTriShape` block flavour inside per-game `.bto`
(Q2, MEDIUM confidence).

### 5.3 Why a trait here but a `GameKind` table in §4

The §4 quirks are *scalar decisions* (one height, one flag) — a match arm is the
right weight. LOD is a *behaviour with state* (asset lookup, atlas management,
fallback chains) — a trait object is the right weight. Both are "per-game impl,
not scattered version checks"; the doctrine is the same, the mechanism scales to
the complexity.

### 5.4 What the runtime LOD does *not* need

The Q3 research clarified that runtime distant rendering is **asset-driven**, not
NIF-hint-driven:
- It is **not** driven by NIF LOD nodes. The parked `bs_value_node` LOD-distance
  override (NIFAL §"Nodes") and `NiLODNode`-style switches are authoring/in-cell
  detail hints, *not* the open-world distant-LOD authority. EXAL §5 does **not**
  unblock them; they stay parked for a separate in-cell LOD-switch feature.
- It is **not** driven by STAT `MNAM`. We only ever parse `MNAM` if we later build
  our *own* LOD baker (out of scope) — the shipped `.bto`/`.lod` assets already
  encode the result.

What runtime LOD **does** need that we don't parse yet (new, small parser work):
the **VWD / "Has Distant LOD" record-header flag** (§5.2), and the WRLD `NAM3`/
`NAM4` LOD-water fields + `OFST` cell-offset table currently skipped in
`wrld.rs`. These feed the LOD ring, not the full-detail scene.

---

## 6. What stays out of scope

- **Shader passes.** Like NIFAL, no EXAL slice touches the Vulkan render-pass /
  pipeline. `compute_sky`, the terrain pass, `water.frag`, and the sun light
  already consume canonical inputs; EXAL changes only what *produces* those inputs.
- **The per-frame `weather_system`.** It is a canonical *consumer* and stays a
  system; EXAL does not fold it into the boundary.
- **New gameplay** (swim currents, weather-driven AI). The components exist
  (`WaterFlow`); wiring them to gameplay is separate work.

---

## 7. Rollout order

Each step ships independently behind `cargo test`; none touches the Vulkan
render-pass / pipeline.

1. ~~**Boundary skeleton**~~ — **done (2026-06-02).** Created top-level
   `byroredux/src/env_translate.rs` (mirrors `material_translate.rs`); moved
   `default_water_for_worldspace` (from `cell_loader/exterior.rs`) and
   `resolve_water_material` (from `cell_loader/water.rs`) under it verbatim, with
   their tests. Both call sites now go through the boundary. Behaviour-preserving:
   all 6 moved tests green, `cargo check -p byroredux` clean.
2. ~~**GameVariant table (§4)**~~ — **satisfied by step 1 (2026-06-02).** Audit of
   the translate/scene layer (`scene/`, `systems/weather.rs`, `cell_loader/`)
   found the water-default decision is the **only** per-game *environment* branch
   there — every other §4 quirk lives correctly in the parser/`Imported*` tier
   (XCLL tail, DALC, XCWT decode) or as a canonical `Option` sentinel. With one
   entry, a generic `GameKind`-keyed table would be speculative ceremony; the
   `GameKind` match inside `env_translate::default_water_for_worldspace` **is** the
   table. New rows get added there as future quirks surface (the function's doc
   notes this).
3. ~~**Sky + fallback**~~ — **done (2026-06-02).** Folded `apply_worldspace_weather`'s
   WTHR→Sky/lighting/weather mapping into `translate_exterior_cell_lighting` /
   `translate_sky` / `translate_weather`, and the hardcoded Mojave
   `insert_procedural_fallback_resources` block into the
   `procedural_fallback_*` canonical constructors. `apply_worldspace_weather`
   keeps only orchestration (texture pre-resolve, world insertion, handle
   release, cross-fade decision). Verified: 4 new boundary unit tests + 410 total
   green, `cargo check` clean, **live FNV WastelandNV smoke run** (60 frames @
   105 fps, sane sky/lighting values logged, no in-run errors; the post-completion
   `exit=139` is the pre-existing teardown SIGSEGV #732/LIFE-N1, unrelated).
4. ~~**Sun model**~~ — **done (2026-06-02).** The canonical sun inputs are
   `tod_hours` (already produced by `translate_weather`) + the engine south-tilt,
   now the named `weather::SUN_SOUTH_TILT` constant carrying the Q1 rationale (no
   authored latitude exists; engine-defined). The arbitrary `[-0.4, 0.8, -0.45]`
   bootstrap seed is replaced: `apply_worldspace_weather` now seeds the initial
   resources from `compute_sun_arc(bootstrap_hour, tod_hours)` — the same model
   `weather_system` runs each frame. No separate `translate_sun`/`SunModel`
   resource was added — that would be ceremony (same call as step 2): the sun
   model is `compute_sun_arc` (consumer) + `tod_hours` (canonical input) + the
   tilt constant. 410 tests green.
5. ~~**Weather + cell lighting**~~ — **done as part of step 3 (2026-06-02).** The
   WTHR→`WeatherDataRes` decode is `translate_weather`; the WTHR→`CellLightingRes`
   exterior day-slot decode is `translate_exterior_cell_lighting`. The *interior*
   XCLL→`CellLightingRes` path stays in the parser tier (already canonical); a
   `translate_cell_lighting` boundary for it is deferred until an exterior XCLL
   lighting-override consumer exists.
6. **LOD model (§5)** — distant **object** LOD. **First cut done (2026-06-02):**
   - Q2/Q3 verification closed: extracted real vanilla Skyrim `.btr`/`.bto` and
     confirmed they are NIFs (BSVER 100 / v20.2.0.7) that **parse + yield geometry
     through the existing pipeline** (`.bto` → 5 meshes, `.btr` → 1 mesh). The
     `.bto` is **world-absolute** (mesh `translation` already in engine-aligned
     world coords — verified to match the full-detail / terrain-LOD placement), so
     sub-meshes spawn directly at their import transform, no per-quad offset.
   - `byroredux/src/cell_loader/object_lod.rs`: pure quad addressing
     (`quad_origin`, `bto_archive_path`, unit-tested against the real filenames)
     + `stream_object_lod_blocks` — a streaming ring (mirroring `terrain_lod`)
     that, for Skyrim/FO4 worldspaces, resolves
     `meshes\terrain\<world>\objects\<world>.<level>.<x>.<y>.bto` from the BSA,
     imports it, and spawns each sub-mesh as an `IsLodTerrain` entity (no BLAS,
     lean static draw, shared object atlas). Reuses the existing `IsLodTerrain`
     marker + draw path rather than duplicating one. Quads load only **outside**
     the full-detail ring, so no resident full model conflicts (the runtime half
     of the VWD rule, without needing the flag yet).
   - Wired into both stream sites (bootstrap + per-frame) via a new
     `WorldStreamingState.object_lod_blocks`. **Live-verified on Skyrim Tamriel:**
     `+77 .bto quads loaded`, entities 2552→5501, draws 1190→2866, 30f @ 154 fps,
     no parse/upload errors.

   **Distant terrain `.btr` done (2026-06-19, M35):**
   `byroredux/src/cell_loader/terrain_lod_btr.rs` loads the games' prebaked
   per-quad `.btr` terrain meshes (Skyrim+/FO4) as a *source upgrade* inside
   the existing terrain-LOD ring. Level-4 `.btr` quads align 1:1 with the
   4-cell synth blocks, so `terrain_lod::spawn_lod_block` produces **either** a
   textured `.btr` block **or** a heightmap-synth block per coordinate (never
   both → no double-draw/z-fight); `.btr` is chosen only for fully-distant
   blocks (`hole_mask == 0`), with synth handling boundary holes, missing
   `.btr`, and older games. **Real-data finding (verified, not the doc's
   guess): `.btr` is NOT world-absolute like `.bto`** — every `.btr` at any
   level is a normalized quad-local mesh (constant `X∈[0,4096]`, `Z∈[-4096,0]`,
   identity transform); placement scales the horizontal footprint by the LOD
   `level` (cells/quad) and offsets to the quad's SW world corner, heights
   absolute (`btr_local_to_world`, unit-tested). Live-verified on Skyrim
   Tamriel (grid 2,-4): `+574 LOD blocks spawned (544 prebaked .btr / 30
   synth)`, 0 parse/bake/upload errors, per-quad diffuse resolved from
   `Skyrim - Textures7.bsa`. The LOD-ring log now reports the `.btr`/synth split.

   **Deferred follow-ups** (clearly de-risked now): coarser LOD bands (8/16/32 —
   both terrain + object LOD load level 4 only); the **VWD / "Has Distant LOD"
   record-header flag** to cull full models at the boundary ring (today's
   "outside full-detail only" rule avoids the conflict more conservatively);
   `.btr` per-quad **normal map** (`_n.dds`) — the block carries the mesh's own
   per-vertex normals today, matching the synth path; the `PlacementLodProvider`
   for Oblivion/FO3/FNV (`DistantLOD\*.lod` → `_far.nif`).

   **Object/terrain LOD atlas texturing fixed (2026-06-19, M35):** the
   object-LOD atlas (`<world>.objects.dds`) and the per-quad `.btr` terrain
   diffuse both live in `Skyrim - Textures7.bsa`, and the `.btr`/`.bto` meshes
   in `Skyrim - Meshes1.bsa` — none of which the old numeric-sibling auto-loader
   pulled in, since it bailed on any digit-suffixed archive (`Textures0` ⇒ no
   siblings). Root cause of the "atlas often unresolved → LOD untextured"
   symptom (the path/format were always correct; the atlas is a 2048² 32-bpp
   R8G8B8A8 DDS the parser handles). Fixed by teaching `open_with_numeric_
   siblings` (asset_provider.rs) that a `…0`-suffixed archive is Skyrim's
   zero-based series START → auto-load `…1`..`…9`. The minimal `Meshes0` +
   `Textures0` invocation now textures distant terrain + objects with no
   explicit archive list (live-verified: `+Meshes1` + `Textures1..8`
   auto-opened, 544 `.btr` / 75 `.bto` quads, 0 LOD-atlas missing-texture
   warnings).

   Note: no `LodProvider` trait / `WorldLodRes` was introduced. `terrain_lod` is
   already a clean self-contained provider fused to streaming-ring reconciliation;
   wrapping both it and `object_lod` in a trait would be forced ceremony (the
   steps 2/4 reasoning). The two stream functions ARE the per-game providers; the
   renderer's single `IsLodTerrain` draw path is the single source of truth.

Steps 1–5 are refactors that pay down the scattered-quirk debt; step 6 is the new
rendering capability the user asked for, built on the boundary the earlier steps
establish.

---

## 8. Tooling (proposed)

- `env.dump` debug-server command — print the live `SkyParamsRes` /
  `WeatherDataRes` / `CellLightingRes` / `SunModel` for the current worldspace
  (the runtime analogue of NIFAL's `material_dump.rs`).
- `lod.rings` debug command — visualise active `WorldLodRes` bands + instance
  counts per band (validate the LOD selector without a screenshot).
- A per-game environment golden in the runtime audit baselines
  (`.claude/audit-baselines/runtime/`) — sky/sun/water values per worldspace, so
  the boundary refactors (steps 1–5) are provably behaviour-preserving.

---

## 9. Resolved questions (research pass, 2026-06-02)

### Q1 — sun-path latitude/tilt → **no authored source exists; the model is ours to define**

Verified against the CLMT/WRLD parsers and the Gamebryo reference SDKs:

- **CLMT** (`crates/plugin/src/esm/records/climate.rs`) carries only WLST (weather
  list), FNAM (sun texture), and TNAM = `[sunrise_begin, sunrise_end,
  sunset_begin, sunset_end]` in 10-minute units (bytes 4–5, volatility/moon, are
  read-and-dropped). **No sun angle, latitude, or path data.**
- **WRLD** carries climate/parent/bounds/water/music/map/flags — **no latitude.**
- **Gamebryo engine has no astronomical model.** v2.x CoreLibs (our target games'
  lineage) has *no* sky/sun subsystem at all — the sun is just a directional light
  positioned by Bethesda's proprietary game code we don't have. v3.x added
  `NiEnvironment`, but it positions the sun from a **fixed two-angle (azimuth +
  elevation) authored model** driven by **artist-keyframed time-of-day curves**
  (`NiTimeOfDay`) — grep for `latitude|declination|tilt|orbit|equinox|solar`
  across the v3.2 sky/atmosphere/TOD stack returns **zero hits**.

**Conclusion:** `#1019`'s premise ("read a per-worldspace latitude field") is
**false** — there is nothing to read. The sun-path is engine-defined. Two
defensible designs, both lineage-faithful: (a) the Gamebryo-3.x approach — an
engine azimuth/elevation curve keyed to `tod_hours` (what `compute_sun_arc`
already approximates); or (b) a real solar model with an *engine-chosen*
per-worldspace latitude constant (a deliberate addition beyond the lineage, for
physically-plausible seasonal/latitude variation). Either way the value is **our
constant**, set in the translate boundary — not a parse. The current `z = 0.15`
tilt is a legitimate placeholder for (a); pick the final number deliberately, not
from a (nonexistent) field.

### Q2 — terrain/object LOD container + naming → **confirmed (`.btr`/`.bto` are NIFs)**

(Sources: niftools/nifskope#17, OpenMW MR 4376, dyndolod.info, xEdit `wbLOD.pas`.)

- `.btr` (terrain) and `.bto` (object) are **renamed NIF files** — parse with the
  existing NIF parser after registering the extensions. Internal block flavour:
  Skyrim LE classic `NiTriShape`; SE/FO4 `BSTriShape`/`BSSubIndexTriShape`
  (**MEDIUM** confidence — verify against real `.bto` when implementing).
- Naming is **level-first**: `<World>.<level>.<x>.<y>.btr/.dds` where
  `level ∈ {4,8,16,32}` = cells-per-quad and `x`/`y` = SW-corner cell. (My initial
  `<World>.<x>.<y>.<level>` guess was wrong.) Terrain normal sibling = `_n.dds`.
- `LODSettings\<World>.lod` is a **16-byte binary**: SW cell X, SW cell Y, stride,
  LOD-level-min (4), LOD-level-max (32).
- Object atlas: `Textures\Terrain\<World>\Objects\<World>.Objects.dds`
  (`00`/`01`… suffixes on overflow).
- **Old games are a different scheme** (see §5.2 `PlacementLodProvider`): distant
  terrain `Meshes\Landscape\LOD\*.nif` + `_lod` textures; distant objects per-cell
  `DistantLOD\<World>_<x>_<y>.lod` placement lists → `_far.nif`.

### Q3 — object-LOD runtime authority → **resolved: prebaked assets, not MNAM**

(Sources: Sheson/DynDOLOD author on STEP forum, DynDOLOD Reference, UESP STAT
format, corroborated by OpenMW. The decisive runtime claim is confirmed by two
independent lineages — HIGH confidence.)

- **Skyrim+/FO4:** distant objects come **strictly** from the baked per-quad
  `.bto` macro-meshes, selected **by filename** (level + quad X/Y). STAT `MNAM`
  (4 LOD model paths × 260 B = 1040 B) is **generation-time input to the offline
  baker only — the engine never reads it at runtime.** The one runtime base-record
  signal is the **VWD / "Has Distant LOD" flag**, used to **cull the full model**
  in the LOD ring (prevents full-mesh + LOD-mesh z-fighting). My doc's earlier
  guess ("both per-STAT MNAM and baked `.bto` at different bands") was **wrong** —
  it is strictly the baked `.bto`.
- **Oblivion/FO3/FNV:** the per-object model — runtime reads `DistantLOD\*.lod`
  placement lists and instances individual `_far.nif` meshes. No atlas, no
  combined mesh.

This is why §5.2 splits into `CombinedLodProvider` (Skyrim+/FO4) and
`PlacementLodProvider` (older) rather than a single unified path, and why §5.4
records that neither NIF LOD nodes nor STAT MNAM are needed for runtime LOD.

#### `DistantLOD\<World>_<x>_<y>.lod` binary format → **RESOLVED (#1726)**

Reverse-engineered 2026-06-23 against all **9889** vanilla `.lod` files in
`Oblivion - Meshes.bsa` (`distantlod\`). The layout is a **structure-of-arrays
per base-object group** (the per-entry fields are split into parallel blocks —
a naive array-of-structs reader misreads any `count > 1` group):

```text
u32  num_groups
per group:
  u32  base_form_id            // STAT/etc. base record this LODs
  u32  count                   // number of placements of that base
  count × Vec3<f32>  position  // Bethesda Z-up world units
  count × Vec3<f32>  rotation  // Euler radians, Z-up (zero in vanilla)
  count × f32        scale     // PERCENT — divide by 100 → multiplier
```

Validation: the SoA layout consumes **9888/9889** files exactly (the lone
outlier is `toddland`, the CS tutorial world, whose LOD data is degenerate —
the parser errors on it and the streaming ring skips it); all rotations are
within ±2π rad; all scales positive. Positions confine to the single cell
named by the file, so the files are **per-cell** (the streaming ring is a
per-cell Chebyshev ring, not the `.bto` quad ring). The base record's model is
resolved via `record_index.cells.statics`; the distant mesh is that model with
`.nif` → `_far.nif` (130 such entries in the Oblivion meshes BSA). The spawn
reuses the proven `object_lod` import path (`parse_nif` → `import_nif_scene` →
`upload_scene_mesh_global_only` → `IsLodTerrain`), composing each placement's
world transform with the `_far.nif`-local TRS. **Visual verification is pending
an Oblivion exterior smoke test** (a Vulkan device + on-disk Oblivion data, out
of `cargo test` scope — same as the `.bto` provider).

### Still requires real-data verification (before step 6 implementation)

1. Exact `BSTriShape`/`BSSubIndexTriShape` block layout inside per-game `.bto`
   (Q2, MEDIUM) — dump one real `.bto` from Skyrim.esm's BSA.
2. Exact byte offsets/widths of `LODSettings\<World>.lod` and the MNAM
   260-byte-per-entry trailing field (only if we ever build our own baker).
3. The final sun-tilt value/model for Q1 (an engine decision, not a lookup).
