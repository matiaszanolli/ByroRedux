# Asset Pipeline

This doc walks the **happy path** for loading a Bethesda cell into ECS
entities and onto the GPU. Every other engine doc covers a slice of this
pipeline; this one is the integration story end-to-end.

```
ESM file ──┐
           ├──► EsmIndex (cells, statics, items, NPCs, ...) ──┐
BSA / BA2 ─┤                                                  │
           │                                                  ▼
           └──► Asset bytes (NIF, DDS) ──► NifScene ──► ImportedMesh ──► ECS entities
                                                                            │
                                                                            ▼
                                                                        Vulkan upload
                                                                        (mesh + texture + BLAS)
```

The cell loader at [`byroredux/src/cell_loader.rs`](../../byroredux/src/cell_loader.rs)
orchestrates the whole thing. The `TextureProvider` at
[`byroredux/src/asset_provider.rs`](../../byroredux/src/asset_provider.rs)
holds the open BSA / BA2 archives and exposes a tiny `extract()` /
`extract_mesh()` API.

## Step by step: loading the Prospector Saloon

```bash
cargo run -- --esm FalloutNV.esm --cell GSProspectorSaloonInterior \
             --bsa "Fallout - Meshes.bsa" \
             --textures-bsa "Fallout - Textures.bsa" \
             --textures-bsa "Fallout - Textures2.bsa"
```

### 1. Open the archives (CLI parse)

[`build_texture_provider()`](../../byroredux/src/asset_provider.rs) walks
the CLI args and opens every `--bsa`/`--textures-bsa`/`--ba2` path with
the appropriate reader. Mesh archives go into `mesh_archives`, texture
archives into `texture_archives`. Both are scanned in declaration order at
extract time, so multiple texture archives can be layered (the FNV demo
above uses both `Textures.bsa` and `Textures2.bsa`).

The current `TextureProvider` only knows about `BsaArchive`; the unified
`MeshArchive` enum used by tests and `nif_stats` lives in
[`crates/nif/tests/common/mod.rs`](../../crates/nif/tests/common/mod.rs)
and could be lifted into the asset provider when we wire BA2 into the
runtime cell loader.

### 2. Parse the ESM into an `EsmIndex`

[`load_cell()`](../../byroredux/src/cell_loader.rs) reads the ESM file
into memory and calls
[`byroredux_plugin::esm::cell::parse_esm_cells()`](../../crates/plugin/src/esm/cell.rs).
The result is an `EsmCellIndex` containing:

- `cells: HashMap<String, CellData>` keyed by lowercase editor ID
- `exterior_cells: HashMap<String, HashMap<(i32, i32), CellData>>`
- `statics: HashMap<u32, StaticObject>` — every base record with a `MODL`
  sub-record, keyed by form ID

For richer record extraction (items, NPCs, factions) the same code path
can call [`byroredux_plugin::esm::records::parse_esm()`](../../crates/plugin/src/esm/records/mod.rs)
instead and get an `EsmIndex` with those categories on top. The cell
loader currently only needs the cell index. See [ESM Records](esm-records.md)
for the full record catalog.

### 3. Find the cell

Look up `cells[lowercase(editor_id)]`. If missing, error out and print the
first 20 available cell editor IDs to help the user find the typo.

### 4. Walk placed references

Each `CellData` has a `Vec<PlacedRef>` — REFR / ACHR records that placed
a base form into the cell. The loader walks these in order, resolves each
`base_form_id` against the `statics` map, and processes the result.

For each ref:

```rust
let stat = match index.statics.get(&placed_ref.base_form_id) {
    Some(s) => s,
    None => continue,  // ref points at something we don't recognize — skip
};
```

A typical interior cell has ~95% hit rate against `statics`. Misses are
typically references to base records we don't yet parse (TERM, NAVM,
LIGH-without-mesh edge cases). They're logged at debug level.

### 5. Coordinate conversion

REFR records carry positions and Euler rotations in **Bethesda's Z-up
clockwise** convention. The loader converts each into a Y-up
`(Vec3, Quat, f32)` triplet via:

```rust
let ref_pos = Vec3::new(
    placed_ref.position[0],     // X stays X
    placed_ref.position[2],     // Z becomes Y (up)
    -placed_ref.position[1],    // -Y becomes Z (forward)
);
let ref_rot = euler_zup_to_quat_yup(rx, ry, rz);
let ref_scale = placed_ref.scale;
```

`euler_zup_to_quat_yup` is the documented CW→CCW conversion in
[Coordinate System](coordinate-system.md). It composes
`Ry(-rz) * Rz(ry) * Rx(-rx)` to handle both the axis swap and the
rotation handedness in one shot.

### 6. Light-only refs (LIGH with no mesh)

Some LIGH records have no `MODL` — they're pure point lights with no
billboard. The cell loader detects this (`stat.model_path.is_empty()` and
`stat.light_data.is_some()`) and spawns a light-only entity:

```rust
let entity = world.spawn();
world.insert(entity, Transform::new(ref_pos, ref_rot, ref_scale));
world.insert(entity, GlobalTransform::new(ref_pos, ref_rot, ref_scale));
world.insert(entity, LightSource {
    radius: ld.radius,
    color: ld.color,
    flags: ld.flags,
});
```

Lights with both a model and light data spawn the model entities first
and then attach the light component to each.

### 7. Effect mesh filter

Some "meshes" are pure FX overlays the engine doesn't really render
(`fxlightrays`, `fxlight`, `fxfog`). The loader filters them by name
substring after the M17 marker pass:

```rust
if model_lower.contains("fxlightrays")
    || model_lower.contains("fxlight")
    || model_lower.contains("fxfog")
{
    continue;
}
```

These are light-volume hints that the legacy renderer used; in our RT
multi-light pipeline they're handled by the actual light data instead.

### 8. NIF extraction (cached)

The loader keeps a per-cell `mesh_cache: HashMap<String, Vec<u8>>` so the
same NIF only extracts from the BSA once per cell, even if it's placed
hundreds of times:

```rust
let nif_data = match mesh_cache.get(&model_path) {
    Some(data) => data.clone(),
    None => match tex_provider.extract_mesh(&model_path) {
        Some(d) => { mesh_cache.insert(model_path.clone(), d.clone()); d }
        None => continue,
    },
};
```

A typical FNV interior has ~30% mesh reuse — Prospector Saloon places ~800
references over ~200 unique meshes.

### 9. NIF parse + import

`byroredux_nif::parse_nif(&nif_data)` returns a `NifScene`, which we hand
to `byroredux_nif::import::import_nif_with_collision(&scene)`. The result
is `(Vec<ImportedMesh>, Vec<ImportedCollision>)`:

```rust
pub struct ImportedMesh {
    pub name: Option<String>,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],     // quaternion (Y-up after coord conversion)
    pub scale: f32,
    pub texture_path: Option<String>,
    pub has_alpha: bool,
    pub two_sided: bool,
    pub is_decal: bool,
    // ...
}
```

Each `ImportedMesh` is the post-coordinate-converted, SVD-repaired,
strip-to-triangle-flattened, single-material chunk of a NIF subtree. The
NIF→ECS importer in [`crates/nif/src/import/`](../../crates/nif/src/import/)
handles all of that. See [NIF Parser — NIF→ECS import](nif-parser.md#nifecs-import)
for the per-step breakdown.

### 10. Spawn collision entities

For each `ImportedCollision`, the loader composes the REFR transform with
the NIF-internal collision transform and spawns an entity carrying the
collision shape and rigid body components. The shapes come from the
N23.6 Havok parser and feed Rapier3D via `byroredux-physics` (M28
Phase 1) — dynamic bodies fall under gravity, static floors block
them. Constraints / ragdolls remain parse-only pending M29's full
ragdoll integration.

### 11. Mesh GPU upload

For each `ImportedMesh`, the loader builds a `Vec<Vertex>` (renderer-side
type with position, color, normal, UV) and uploads it via the mesh
registry:

```rust
let mesh_handle = ctx.mesh_registry.upload(
    &ctx.device,
    alloc,
    &ctx.graphics_queue,
    ctx.command_pool,
    &vertices,
    &mesh.indices,
    ctx.device_caps.ray_query_supported,
    None,  // no skin data yet
)?;
ctx.build_blas_for_mesh(mesh_handle, num_verts, num_indices);
```

The mesh registry caches by content, so identical buffers don't allocate
twice on the GPU. The BLAS build is queued onto a one-time command buffer
and waited on with a HOST→AS_BUILD memory barrier before the next frame's
TLAS build (see [Renderer — RT acceleration structures](renderer.md#rt-acceleration-structures)).

### 12. Texture lookup and upload

The mesh's `texture_path` (e.g. `"textures\clutter\food\beerbottle.dds"`)
goes through the texture registry. Cache hit → reuse the descriptor set;
cache miss → extract DDS bytes from `tex_provider`, decode the header,
upload the pixel data, transition to `SHADER_READ_ONLY_OPTIMAL`, build a
descriptor set, return the handle. Missing textures fall back to a
debug fallback texture.

```rust
let tex_handle = match &mesh.texture_path {
    Some(tex_path) => {
        if let Some(cached) = ctx.texture_registry.get_by_path(tex_path) {
            cached
        } else if let Some(dds_bytes) = tex_provider.extract(tex_path) {
            ctx.texture_registry.load_dds(...).unwrap_or_else(|_| {
                ctx.texture_registry.fallback()
            })
        } else {
            ctx.texture_registry.fallback()
        }
    }
    None => ctx.texture_registry.fallback(),
};
```

For BA2 DX10 textures (FO4), `tex_provider.extract()` returns the
**reconstructed** DDS bytes — the BA2 reader synthesizes the 148-byte
DDS+DX10 header from the record metadata before handing them off. Same
downstream path as a BSA-extracted DDS. See [Archives — DDS reconstruction](archives.md#dds-header-reconstruction).

### 13. Compose the final transform

```rust
// Composed: parent_rot * (parent_scale * child_pos) + parent_pos
let final_pos   = ref_rot * (ref_scale * nif_pos) + ref_pos;
let final_rot   = ref_rot * nif_quat;
let final_scale = ref_scale * mesh.scale;
```

The REFR placement is the parent transform; the NIF mesh's internal
transform (often non-identity for sub-meshes within a multi-shape NIF) is
the child. The composed transform is what goes on the spawned entity's
`Transform` and `GlobalTransform` components.

### 14. Spawn the renderable entity

```rust
let entity = world.spawn();
world.insert(entity, Transform::new(final_pos, final_rot, final_scale));
world.insert(entity, GlobalTransform::new(final_pos, final_rot, final_scale));
world.insert(entity, MeshHandle(mesh_handle));
world.insert(entity, TextureHandle(tex_handle));
if mesh.has_alpha   { world.insert(entity, AlphaBlend); }
if mesh.two_sided   { world.insert(entity, TwoSided); }
if mesh.is_decal    { world.insert(entity, Decal); }
if let Some(ld)     = light_data { world.insert(entity, LightSource { ... }); }
```

The spawned entity is now a fully renderable thing in the ECS. The next
`build_render_data` pass picks it up; the next `draw_frame` pushes its
model matrix as a push constant and draws indexed.

## End-to-end timing reference

For the FNV Prospector Saloon (release build, RTX 4070 Ti):

| Stage | Cost |
|---|---|
| ESM parse (full file) | ~80 ms |
| Cell lookup + REFR walk (~800 refs) | <1 ms |
| Per-mesh NIF parse (200 unique × ~10 ms) | ~2 s aggregated |
| Per-mesh GPU upload (200 unique) | ~1.5 s aggregated (staging buffers + transitions) |
| Texture extraction + decode (~150 unique) | ~0.5 s |
| Total cell load wall time | ~5 s |
| Per-frame draw afterwards (789 entities, RT shadows) | ~12 ms (≈85 FPS) |

The cell load is single-threaded and dominated by per-mesh GPU uploads.
M27 (parallel system dispatch) and M28 (physics) won't change this; load
times will get noticeably better when we batch more uploads onto fewer
command buffers.

## Loose-NIF demo path

For `cargo run -- path/to/mesh.nif`, the pipeline is the same minus the
ESM step: the loader parses the NIF directly, runs the same import +
upload + spawn flow, and skips the REFR resolution and the texture BSA
lookup (using the fallback texture for any missing files). It's the same
~14 lines of code.

## Tests

The asset pipeline doesn't have a single integration test — each layer is
tested in isolation:

- ESM parsing → `crates/plugin/src/esm/` (64 unit tests + 2 integration)
- BSA / BA2 extraction → `crates/bsa/` (8 unit + 7 ignored integration)
- NIF parsing → `crates/nif/src/blocks/*` (118 unit + 8 ignored
  per-game integration sweeps walking 177k NIFs end-to-end)
- NIF→ECS import → `crates/nif/src/import/`
- Mesh / texture upload → `crates/renderer/`
- Cell loading → exercised by the manual `cargo run -- --esm ... --cell ...`
  smoke test

The closest thing to an end-to-end test is running the Prospector Saloon
demo and watching the entity count + FPS in the title bar.

## Related docs

- [NIF Parser](nif-parser.md) — how raw NIF bytes become `ImportedMesh`
- [Archives](archives.md) — how raw archive bytes become `Vec<u8>`
- [ESM Records](esm-records.md) — how raw ESM bytes become `EsmCellIndex`
- [Vulkan Renderer](renderer.md) — what the GPU does with the uploaded mesh + texture
- [Coordinate System](coordinate-system.md) — Z-up→Y-up, CW rotation, transform composition
- [Cell Lighting](lighting-from-cells.md) — XCLL data → multi-light SSBO
- [Game Loop](game-loop.md) — when cell loading runs in the startup sequence
