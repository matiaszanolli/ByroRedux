# Asset Pipeline

This doc walks the **happy path** for loading a Bethesda cell into ECS
entities and onto the GPU. Every other engine doc covers a slice of this
pipeline; this one is the integration story end-to-end.

```
ESM file ──┐
           ├──► EsmCellIndex (cells, statics, items, NPCs, ...) ──┐
BSA / BA2 ─┤                                                      │
           │                                                      ▼
           └──► Asset bytes (NIF, DDS, BGSM/BGEM) ──► NifScene ──► ImportedMesh
                                                                       │
                                          merge_bgsm_into_mesh ────────┤
                                                                       │
                                          translate_material (NIFAL) ──┤
                                                                       ▼
                                                                  ECS entities
                                                                       │
                                                                       ▼
                                                                   Vulkan upload
                                                                   (mesh + texture + BLAS)
```

The cell loader at [`byroredux/src/cell_loader/`](../../byroredux/src/cell_loader/)
orchestrates the whole thing — Session 36 split the original 2 992-line
flat `cell_loader.rs` into a submodule directory (see
[`cell_loader.rs`](../../byroredux/src/cell_loader.rs) for the module
table and the shared flag-packers). The asset providers at
[`byroredux/src/asset_provider.rs`](../../byroredux/src/asset_provider.rs)
hold the open BSA / BA2 archives and expose a small `extract()` /
`extract_mesh()` API plus a separate `MaterialProvider` for BGSM/BGEM
resolution.

> **Note on freshness.** This doc was last fully reconciled
> **2026-05-28** (Session 42 closeout). Function and path references are
> verified against the tree as of that date; line numbers are
> approximate and drift between refactors.

## Step by step: loading the Prospector Saloon

```bash
cargo run -- --esm FalloutNV.esm --cell GSProspectorSaloonInterior \
             --bsa "Fallout - Meshes.bsa" \
             --textures-bsa "Fallout - Textures.bsa"
```

### 1. Open the archives (CLI parse)

[`build_texture_provider()`](../../byroredux/src/asset_provider.rs) walks
the CLI args and opens every `--bsa` (mesh) / `--textures-bsa` (texture)
path. Each path is opened by
[`Archive::open()`](../../byroredux/src/asset_provider.rs), which
**auto-detects BSA vs BA2 from the 4-byte file magic** (`BTDX` → BA2,
otherwise BSA) — there is no separate `--ba2` flag; a `.ba2` passed to
`--bsa` / `--textures-bsa` opens correctly. Mesh archives go into
`mesh_archives`, texture archives into `texture_archives`; the matching
`extract_mesh()` / `extract()` scan their archive list in declaration
order, so multiple archives layer naturally.

BGSM/BGEM material archives are opened separately by
[`build_material_provider()`](../../byroredux/src/asset_provider.rs) from
repeated `--materials-ba2 <path>` flags (see step 9.5). The main binary
also auto-appends a `--materials-ba2` for each of the selected game
entry's `default_materials_bsas` (e.g. `Fallout4 - Materials.ba2` /
`Starfield - Materials.ba2`) when launched against a registered game.

#### Numeric-suffix sibling auto-load

[`open_with_numeric_siblings()`](../../byroredux/src/asset_provider.rs)
runs after each explicit open: when the path ends in an unsuffixed
`.bsa` / `.ba2` (no digit immediately before the extension), the loader
scans the same directory for `<stem>2.*` … `<stem>9.*` and opens each
that exists. The FNV demo above triggers this for
`Fallout - Textures.bsa` → automatically pulls
`Fallout - Textures2.bsa`, where Bethesda parked roughly half of the
vanilla wall / floor / trim textures (the v104 archive size budget
forced the split). Without that second archive, ~263 entities in Doc
Mitchell's house resolve to the magenta-checker fallback — which,
composited with the (correctly loaded) tangent-space normal map,
produced the "chrome posterized walls" diagnosis chased through R1 /
#783 / #784 and finally nailed via `byro-dbg`'s `tex.missing` command
(see the Session 27 entry in [HISTORY.md](../../HISTORY.md)).

The rule fires only on unsuffixed paths so it stays inert for Skyrim's
already-numeric `Skyrim - Meshes0.bsa` / `Skyrim - Meshes1.bsa` style —
those expect the user to list each archive explicitly — and is harmless
when the sibling simply doesn't exist. The same helper applies to
`--bsa` mesh archives, `--textures-bsa` texture archives, and BA2
material archives.

If anything looks like it's missing texture detail — banded specular,
checker grid, "chrome posterized" plaster — run `tex.missing` from
`byro-dbg` *before* opening shader files. The full triage recipe lives
in [debug-cli.md](debug-cli.md) under the fragment-shader bypass / viz
bits.

Both BSA and BA2 archives are now first-class in the runtime
`TextureProvider` via the internal
[`Archive`](../../byroredux/src/asset_provider.rs) enum (`Bsa` / `Ba2`);
the separate `MeshArchive` test helper at
[`crates/nif/tests/common/mod.rs`](../../crates/nif/tests/common/mod.rs)
predates that unification and is now test-only infrastructure for
`nif_stats` and the per-game integration sweeps.

### 2. Parse the ESM into an `EsmCellIndex`

[`load_cell_with_masters()`](../../byroredux/src/cell_loader/load.rs)
reads the ESM file(s) into memory and calls
[`byroredux_plugin::esm::cell::parse_esm_cells()`](../../crates/plugin/src/esm/cell/mod.rs)
(or `parse_esm_cells_with_load_order()` for the repeatable `--master`
DLC case). The result is an `EsmCellIndex` containing:

- `cells: HashMap<String, CellData>` keyed by lowercase editor ID
- `exterior_cells: HashMap<String, HashMap<(i32, i32), CellData>>`
- `statics: HashMap<u32, StaticObject>` — every base record with a `MODL`
  sub-record, keyed by form ID

Each `CellData` carries its `Vec<PlacedRef>` plus FO4 extras (the
`absorbed_refs` set for precombined-mesh REFRs, texture-set / material-
swap overlays). For richer record extraction (items, NPCs, factions)
the same code path can call
[`byroredux_plugin::esm::records::parse_esm()`](../../crates/plugin/src/esm/records/mod.rs)
and get an `EsmIndex` with those categories on top. See
[ESM Records](esm-records.md) for the full record catalog.

### 3. Find the cell

Look up `cells[lowercase(editor_id)]`. If missing, error out and list
available cell editor IDs to help the user find the typo.

### 4. Walk placed references

The inner REFR walk lives in
[`cell_loader/references.rs`](../../byroredux/src/cell_loader/references.rs).
Each `CellData` has a `Vec<PlacedRef>` — REFR / ACHR records that placed
a base form into the cell. The loader walks these in order, resolves
each `base_form_id` against the `statics` map, and dispatches the result
into [`cell_loader/spawn.rs`](../../byroredux/src/cell_loader/spawn.rs).

For each ref it resolves the base record, skipping refs that point at a
form we don't recognise. FO4 REFRs whose geometry was absorbed into a
`meshes\precombined\<cell>_<hash>_oc.nif` are skipped here and brought
in by the precombined-spawn pass instead (or, when precombined-spawn
fails, re-rendered from the original REFR list — #1188; see
[the FO4 PreCombined gap note](../../CLAUDE.md)). A typical interior
cell has ~95% hit rate against `statics`; misses are base records we
don't yet parse (logged at debug level).

### 5. Coordinate conversion

REFR records carry positions and Euler rotations in **Bethesda's Z-up
clockwise** convention. The loader converts each into a Y-up
`(Vec3, Quat, f32)` triplet — the position axis swap is
`(X, Z, -Y)` and the rotation goes through
[`euler_zup_to_quat_yup_refr()`](../../byroredux/src/cell_loader/euler.rs)
(REFR variant; there is also a plain `euler_zup_to_quat_yup` used by
non-REFR call sites). The conversion is the documented CW→CCW transform
in [Coordinate System](coordinate-system.md); the `--rotation-mode`
diagnostic switch (`set_refr_rotation_mode_diag`) lets you flip between
candidate orderings at runtime when debugging a tilted placement.

### 6. Light-only refs (LIGH with no mesh)

Some LIGH records have no `MODL` — they're pure point lights with no
billboard. The cell loader detects this (`stat.model_path.is_empty()`
and `stat.light_data.is_some()`) and spawns a light-only entity with a
[`LightSource`](../../crates/core/src/ecs/components/light.rs) carrying
`radius` (via `light_radius_or_default`), `color`, `flags`, and the
authored `falloff_exponent`. Lights with both a model and light data
spawn the model entities first and then attach the light to each.

### 7. Effect-mesh filter

Some "meshes" are pure FX overlays the engine doesn't raster
(`fxlightrays`, `fxlight`, `fxfog`). The loader filters them by name
substring after the marker pass. Unlike the original behaviour (which
just dropped them), the current code **promotes them to lights**: when
the ref carries `stat.light_data`, it spawns a `LightSource` at the ref
position before `continue`-ing past the mesh. These are the light-volume
hints the legacy renderer used; in our RT multi-light pipeline the
actual light data drives the lighting instead of the billboard. Editor
markers (`marker*`, `xmarker`, `doormarker`, `northmarker`, …) are
filtered just above this by filename prefix.

### 8. NIF extraction + import cache

The expensive parse+import is memoised in the process-lifetime
[`NifImportRegistry`](../../byroredux/src/cell_loader/nif_import_registry.rs)
resource (#381), not a per-cell `HashMap`. A unique mesh path is parsed
and imported **once for the whole process**; subsequent placements (in
this cell or any later cell) reuse the `CachedNifImport` entry. The
registry tracks per-cell touch keys so `unload_cell` can release entries
that no streamed-in cell still references. On a registry miss the loader
calls [`extract_mesh()`](../../byroredux/src/asset_provider.rs), whose
[`normalize_mesh_path()`](../../byroredux/src/asset_provider.rs) prepends
the `meshes\` root segment when the authored `MODL` omits it (RACE /
NPC_ / ARMO records author relative to `meshes\`; the BSA stores the full
prefix).

### 9. NIF parse + import

`byroredux_nif::parse_nif(&nif_data)` returns a `NifScene`, which is
handed to
[`byroredux_nif::import::import_nif_with_collision_and_resolver()`](../../crates/nif/src/import/mod.rs)
(the `_and_resolver` form supplies a `MeshResolver` so Starfield
`BSGeometry` blocks can resolve their external `.mesh` payloads — the
plain `import_nif_with_collision()` passes `None`). The result is
`(Vec<ImportedMesh>, Vec<ImportedCollision>)`.

`ImportedMesh` is the post-coordinate-converted, SVD-repaired,
strip-to-triangle-flattened, single-material chunk of a NIF subtree. It
is a large struct (defined in
[`crates/nif/src/import/types.rs`](../../crates/nif/src/import/types.rs))
— the renderer-facing core, abbreviated:

```rust
pub struct ImportedMesh {
    pub positions: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,        // RGBA — alpha lane preserved (#618)
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 4]>,      // [Tx, Ty, Tz, bitangent_sign]
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],           // quaternion (Y-up after coord conversion)
    pub scale: f32,
    pub texture_path: Option<FixedString>,   // interned StringPool handle (#609)
    pub material_path: Option<FixedString>,  // .bgsm / .bgem (FO4+)
    pub name: Option<Arc<str>>,
    pub has_alpha: bool,
    pub src_blend_mode: u8,
    pub dst_blend_mode: u8,
    pub alpha_test: bool,
    pub alpha_threshold: f32,
    pub alpha_test_func: u8,
    pub two_sided: bool,
    pub is_decal: bool,
    pub normal_map: Option<FixedString>,
    pub glow_map: Option<FixedString>,
    pub detail_map: Option<FixedString>,
    pub gloss_map: Option<FixedString>,
    pub dark_map: Option<FixedString>,
    pub parallax_map: Option<FixedString>,
    pub env_map: Option<FixedString>,
    pub env_mask: Option<FixedString>,
    pub specular_map: Option<FixedString>,   // BGSM/BGEM v>2 standalone slot
    pub lighting_map: Option<FixedString>,   // BGSM/BGEM v>2 LUT
    // PBR / translucency scalars, emissive, glossiness, z-state,
    // bounds, skin, effect-shader data, BGSM flag bools … (see types.rs)
    pub metalness_override: Option<f32>,
    pub roughness_override: Option<f32>,
    pub is_pbr: bool,
    pub from_bgsm: bool,
    pub model_space_normals: bool,
    // ...
}
```

Note that the path slots are interned `FixedString` handles into the
engine-wide `StringPool` (#609), not owned `String`s — resolve with
`pool.resolve(handle)`. The NIF→ECS importer (split into per-block
extractors under
[`crates/nif/src/import/mesh/`](../../crates/nif/src/import/mesh/)
and a hierarchical/flat walk under
[`crates/nif/src/import/walk/`](../../crates/nif/src/import/walk/))
handles the geometry extraction. See
[NIF Parser — NIF→ECS import](nif-parser.md#nifecs-import) for the
per-step breakdown.

### 9.5. Merge BGSM/BGEM material (FO4 / Starfield)

When `ImportedMesh.material_path` points at a `.bgsm` / `.bgem`
(`.mat` for Starfield), the loader calls
[`merge_bgsm_into_mesh()`](../../byroredux/src/asset_provider.rs)
against the `MaterialProvider`. NIF fields take precedence — only empty
slots are filled from the resolved material:

- **BGSM** template chains are walked child-first (the first non-empty
  value for each field wins), resolving inherited `root_material_path`
  references; spec-glossiness is translated to metallic-roughness and
  `metalness_override` / `roughness_override` seeded.
- **BGEM** has no inheritance (the format carries no parent path), so
  the single parsed file is read; `bgem_glass` becomes the authoritative
  glass signal.
- **Starfield `.mat`** flips `mesh.is_pbr = true` so downstream packing
  routes the surface through the Disney BSDF path. The actual material
  data lives in the binary Component Database
  (`materials\materialsbeta.cdb` inside `Starfield - Materials.ba2`),
  loaded once per provider via `load_starfield_cdb()`; the CDB-presence
  gate (`has_starfield_cdb()`) keeps stray modded `.mat` paths on a
  non-Starfield archive set off the PBR path. (#1289 Phase 1 — authored
  CDB-value extraction is Phase 2.)

`merge_bgsm_into_mesh` returns `true` when any field was filled and is a
no-op (returns `false`) when the path can't resolve in any opened
materials archive. Failed paths are cached so a missing BGSM is only
warned about once. The `bgem_cache` and `failed_paths` maps both have a
capacity ceiling; when it is reached the oldest 50 % of entries are evicted
(half-eviction, `797424e4`) rather than flushing the entire map — the
prior full-flush strategy caused a cold-restart thundering-herd on the
next cell load.

### 10. Spawn collision entities

For each `ImportedCollision`, the loader composes the REFR transform
with the NIF-internal collision transform and spawns an entity carrying
the [`CollisionShape`](../../crates/core/src/ecs/components/collision.rs) and
`RigidBodyData` components (the shape clone is wrapped in
`catch_unwind` so a parry3d panic on a nested composite shape skips that
one shape instead of killing the cell load). These feed Rapier3D via
[`byroredux_physics`](../../crates/physics/): the
`physics_sync_system` builds Rapier bodies/colliders from these
components and the **M28.5 kinematic character controller** (closed
2026-05-22) collides against the static architecture — the player walks
on floors, slides along walls, autosteps, and falls under gravity. When
a NIF authored no `bhk*` collision, a trimesh fallback is synthesised
from the render geometry (gated on `base_layer`, #1294). Havok
constraints / ragdolls remain parse-only.

### 11. Mesh GPU upload

For each `ImportedMesh`, the loader builds a `Vec<Vertex>` (renderer-side
type with position, color, normal, UV, tangent, bone data, splat) and
uploads it via the mesh registry. The mesh registry caches by content,
so identical buffers don't allocate twice on the GPU. A BLAS build is
queued onto a one-time command buffer and waited on with a
HOST→AS_BUILD memory barrier before the next frame's TLAS build (see
[Renderer — RT acceleration structures](renderer.md#rt-acceleration-structures)).
Because the parse+import is cached in `NifImportRegistry`, the GPU
upload + BLAS build are likewise reused across placements of the same
mesh.

### 12. Texture lookup and upload

Each populated path slot resolves through
[`resolve_texture()`](../../byroredux/src/asset_provider.rs) (or
`resolve_texture_with_clamp()` for decals / non-default clamp modes) into
the texture registry. Cache hit → reuse the descriptor set; cache miss →
extract DDS bytes from the provider, decode the header, upload the pixel
data, transition to `SHADER_READ_ONLY_OPTIMAL`, build a descriptor set,
return the handle. Missing textures fall back to the debug fallback
texture; the normal / dark / greyscale-LUT slots only attach their
`*MapHandle` component when the path resolves to a real handle (so the
shader's "handle != fallback" gate stays meaningful).

`extract()` runs the path through
[`normalize_texture_path()`](../../byroredux/src/asset_provider.rs) first:
it strips a leading `data\` segment (FO4 FaceGen `BSShaderTextureSet`
paths author this form, F1.1 / 2026-05-26) and prepends `textures\` when
the authored WTHR / CLMT / LTEX path is relative to the textures root
(#468).

For BA2 DX10 textures (FO4 / FO76 / Starfield), `extract()` returns the
**reconstructed** DDS bytes — the BA2 reader synthesises the 148-byte
DDS + DX10 extended header from the record metadata
([`build_dds_header()`](../../crates/bsa/src/ba2.rs), pinned by the
`build_dds_header_is_148_bytes` test) before handing them off. Same
downstream path as a BSA-extracted DDS. See
[Archives — DDS reconstruction](archives.md#dds-header-reconstruction).

### 13. Canonical material translation (NIFAL)

This is the **single boundary** that turns the raw, per-game
`ImportedMesh` (BGSM/BGEM already merged) into the engine's canonical
[`Material`](../../crates/core/src/ecs/components/material.rs) ECS
component:
[`material_translate::translate_material()`](../../byroredux/src/material_translate.rs).
It is the **material slice of NIFAL** (the NIF Abstraction Layer, the
cross-game canonical translation tier — see [nifal.md](nifal.md)). Every
consumer downstream of `Material` reads game-agnostic, fully-resolved
data; the per-game quirks resolve here exactly once.

`translate_material` does:

- copy all material scalars / colours / flags across;
- pack `effect_shader_flags` as the union of the BSEffectShader SLSF
  bits ([`cell_loader::pack_effect_shader_flags`](../../byroredux/src/cell_loader.rs)),
  the BGSM v>2 PBR / translucency / model-space-normals bits
  ([`cell_loader::pack_bgsm_material_flags`](../../byroredux/src/cell_loader.rs)),
  and any caller-supplied `extra_material_flags` (the cell loader passes
  the REFR-overlay model-space-normals bit; loose-NIF loads pass `0`);
- resolve PBR once via `Material::resolve_pbr()` so legacy inline-shader
  content lands with explicit `(metalness, roughness)` scalars (NaN
  sentinel → keyword classifier), just like BGSM content;
- classify glass once, alpha-aware, via
  [`helpers::classify_glass_into_material`](../../byroredux/src/helpers.rs),
  after the PBR resolve so the forced glass roughness wins.

Before this boundary existed the `Material` literal was hand-built at
two sites — the cell-loader spawn path and the loose-NIF
[`scene/nif_loader.rs`](../../byroredux/src/scene/nif_loader.rs) — ~110
near-identical lines each, a translation leak waiting to diverge. Both
sites now call `translate_material`. The caller passes a
`ResolvedPaths` struct holding the per-slot texture/material paths it
already resolved (REFR XATO/XTNM/XTXR overlays applied).

### 14. Compose the final transform

```rust
// Composed: parent_rot * (parent_scale * child_pos) + parent_pos
let final_pos   = ref_rot * (ref_scale * nif_pos) + ref_pos;
let final_rot   = ref_rot * nif_quat;
let final_scale = ref_scale * mesh.scale;
```

The REFR placement is the parent transform; the NIF mesh's internal
transform (often non-identity for sub-meshes within a multi-shape NIF)
is the child. The entity gets a **local-space `Transform`** (the NIF-
local `nif_pos` / `nif_quat` / `mesh.scale`, for hierarchy propagation)
plus a **world-space `GlobalTransform`** seeded from the composed
`final_*` for first-tick consumers (#544). Meshes are parented to a
shared per-REFR `placement_root` entity so embedded-animation subtree
walks and Name-keyed clip channels resolve.

### 15. Spawn the renderable entity

The spawn site (in
[`cell_loader/spawn.rs`](../../byroredux/src/cell_loader/spawn.rs))
inserts:

```rust
let entity = world.spawn();
world.insert(entity, Transform::new(nif_pos, nif_quat, mesh.scale));
world.insert(entity, GlobalTransform::new(final_pos, final_rot, final_scale));
world.insert(entity, LocalBound::new(center, radius));   // #1213
world.insert(entity, WorldBound::ZERO);                  // re-seeded by bounds pass
if mesh.flags != 0 { world.insert(entity, SceneFlags::from_nif(mesh.flags)); } // #1235
world.insert(entity, Parent(placement_root));
if let Some(sym) = name_sym { world.insert(entity, Name(sym)); }
world.insert(entity, MeshHandle(mesh_handle));
world.insert(entity, TextureHandle(tex_handle));
world.insert(entity, material);                          // from translate_material
if texture_path_is_fx_mesh(tp) { world.insert(entity, IsFxMesh); }   // #1136
// + NormalMapHandle / DarkMapHandle / GreyscaleLutHandle when those paths resolve
```

Alpha-blend / two-sided / decal state now rides on the `Material`
component (resolved in step 13) rather than separate marker components.
The spawned entity is a fully renderable thing in the ECS: the next
`build_render_data` pass picks it up; the next `draw_frame` pushes its
model matrix and draws indexed.

## End-to-end timing reference

> The numbers below are an order-of-magnitude reference last sanity-
> checked mid-2026 on the dev box (RTX 4070 Ti, Ryzen 7950X, release
> build). Per-frame FPS for specific cells is tracked authoritatively
> by the `--bench-frames` / `--bench-hold` harness and the R6a re-bench
> rows in [ROADMAP.md](../../ROADMAP.md) — treat those, not this table,
> as ground truth for any current FPS claim.

| Stage | Order of magnitude |
|---|---|
| ESM parse (full file) | tens of ms |
| Cell lookup + REFR walk | sub-ms |
| Per-unique-mesh NIF parse + import (cached in `NifImportRegistry`) | the dominant CPU cost |
| Per-unique-mesh GPU upload + BLAS build | staging buffers + transitions |
| Texture extraction + decode (per unique texture) | small fraction of total |
| Per-frame draw afterwards (RT shadows + GI) | single-digit-to-low-double-digit ms |

The cell load is single-threaded on the spawn side; the parse + import
is memoised across placements and across cells via `NifImportRegistry`,
and M40 world streaming (closed 2026-05-24) runs the *next* cell's parse
on a background pre-parse worker so a cell boundary doesn't stall the
frame. Load wall time is dominated by per-unique-mesh GPU uploads; it
improves as more uploads batch onto fewer command buffers.

## Loose-NIF demo path

For `cargo run -- path/to/mesh.nif`, the pipeline is the same minus the
ESM step. [`scene/nif_loader.rs`](../../byroredux/src/scene/nif_loader.rs)
(`load_nif_from_args` → `load_nif_bytes`) parses the NIF directly, runs
the same import + `merge_bgsm_into_mesh` + `translate_material` + upload +
spawn flow, and skips REFR resolution (using the fallback texture for
any path that doesn't resolve). It calls the same `translate_material`
boundary as the cell path, so material handling can't diverge between
the two entry points.

## Tests

The asset pipeline doesn't have one giant integration test — each layer
is tested in isolation, then exercised together by the manual cell-load
smoke run:

- **ESM parsing** → `crates/plugin/src/esm/` (~376 `#[test]`s across the
  cell, records, and reader modules, including per-game regression sweeps)
- **BSA / BA2 extraction** → `crates/bsa/` (~69 tests, ~22 gated behind
  `#[ignore]` as on-disk integration sweeps)
- **NIF parsing + import** → `crates/nif/` (~738 `#[test]`s in-tree, plus
  ~21 `#[ignore]`d per-game integration sweeps walking the vanilla NIF
  corpus end-to-end)
- **BGSM/BGEM merge + path normalisation** →
  [`asset_provider.rs`](../../byroredux/src/asset_provider.rs)'s own
  `#[cfg(test)]` module (`normalize_material_path_*`,
  `build_material_provider_*`)
- **Material translation** → exercised through the cell-loader and
  loose-NIF spawn paths
- **Cell loading** → the per-topic test siblings under
  [`cell_loader/`](../../byroredux/src/cell_loader/)
  (`pkin_expansion_tests`, `scol_expansion_tests`,
  `refr_texture_overlay_tests`, `nif_light_spawn_gate_tests`,
  `lgtm_fallback_tests`, `terrain_splat_tests`, …) plus the manual
  `cargo run -- --esm … --cell …` smoke test

(Workspace-wide: ~2 635 tests pass as of 2026-05-28.) The closest thing
to a true end-to-end check is running a cell demo with `--bench-hold`
and driving `byro-dbg` (`tex.missing`, `entities <Component>`, `stats`)
against the loaded scene.

## Related docs

- [NIF Parser](nif-parser.md) — how raw NIF bytes become `ImportedMesh`
- [NIFAL](nifal.md) — the cross-game canonical translation tier (material + particle slices)
- [Archives](archives.md) — how raw archive bytes become `Vec<u8>` (incl. DDS reconstruction)
- [ESM Records](esm-records.md) — how raw ESM bytes become `EsmCellIndex`
- [Vulkan Renderer](renderer.md) — what the GPU does with the uploaded mesh + texture
- [Coordinate System](coordinate-system.md) — Z-up→Y-up, CW rotation, transform composition
- [Cell Lighting](lighting-from-cells.md) — XCLL data → multi-light SSBO
- [Game Loop](game-loop.md) — when cell loading runs in the startup sequence
