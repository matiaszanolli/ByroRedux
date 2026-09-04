//! Distant **object** LOD (Oblivion only) — the per-cell
//! `DistantLOD\<World>_<x>_<y>.lod` placement scheme.
//!
//! This is the older-game counterpart to [`super::object_lod`] (the
//! Skyrim+/FO4 baked-`.bto` scheme). The two are structurally different
//! LOD producers (EXAL §5, docs/engine/exal.md) and deliberately do NOT
//! share a code path:
//!
//! - **Skyrim LE/SE, FO4** ([`super::object_lod`]): one baked macro-mesh
//!   per quad, selected by filename.
//! - **Oblivion** (this module): per-cell placement lists that instance
//!   individual `_far.nif` low-poly meshes — one draw per entry, no atlas,
//!   no combined mesh.
//! - **FO3/FNV**: ship **neither** scheme. FO3-D4-01 (#2086) found zero
//!   `distantlod\*.lod` files in any vanilla FO3/FNV archive; this module is
//!   gated to `GameKind::Oblivion` only. Landmark-object LOD on those two
//!   titles instead folds into the `meshes\landscape\lod\<worldspace>\`
//!   terrain-LOD block tree ([`super::terrain_lod`]), not decoded here.
//!
//! ## File format (verified 2026-06-23 against all 9889 vanilla Oblivion
//! `.lod` files in `Oblivion - Meshes.bsa`)
//!
//! A `.lod` is a **structure-of-arrays per base-object group** — NOT
//! array-of-structs (the per-entry interleaving is split into parallel
//! position / rotation / scale blocks):
//!
//! ```text
//! u32  num_groups
//! per group:
//!   u32  base_form_id          (the STAT/etc. base record this LODs)
//!   u32  count                 (number of placements of that base)
//!   count × Vec3<f32>  position  (Bethesda Z-up world units)
//!   count × Vec3<f32>  rotation  (Euler radians, Z-up; zero in vanilla)
//!   count × f32        scale     (PERCENT — divide by 100 → multiplier)
//! ```
//!
//! Validation across the corpus (re-measured 2026-08-30): the SoA layout
//! consumes **9889/9889 files exactly** — no outlier, `toddland` included;
//! zero trailing bytes, zero overrun, zero errors. Rotations are all within
//! ±2π rad except a single one (a rotation-range note, not a consume
//! failure); scales are all positive. Positions confine to the single cell
//! named by the file, so the files are **per-cell**.
//!
//! Each placement spawns one imported `_far.nif` as an
//! [`IsLodTerrain`](crate::components::IsLodTerrain) entity (no BLAS, lean
//! static draw) — reusing the proven import path
//! [`super::object_lod`] uses for `.bto`. The base record's model is
//! resolved through `record_index.statics` (the same table the REFR spawn
//! path reads); the `_far.nif` is that model with `.nif` → `_far.nif`.

use std::collections::HashMap;
use std::io;

use byroredux_core::ecs::components::RenderLayer;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{
    GlobalTransform, MeshHandle, TextureHandle, Transform, World, WorldBound,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm::reader::GameKind;
use byroredux_renderer::VulkanContext;

use crate::asset_provider::{resolve_texture, TextureProvider};
use crate::components::IsLodTerrain;

use super::euler::euler_zup_to_quat_yup_refr;
use super::exterior::ExteriorWorldContext;
use super::lod_support::{
    release_lod_gpu_resources, sort_lod_coords_nearest, LodReconcileInput, LodWorkBudget,
};

/// Object-LOD ring radius in **cells** (Chebyshev) for the placement
/// scheme (Oblivion `.lod`, FO3/FNV legacy blocks) — the flat, single-ring
/// object-LOD reader for games with no baked quadtree. Cells within this
/// distance of the player — and entirely beyond the full-detail ring —
/// load their `.lod`. This no longer mirrors a same-shaped constant on the
/// Skyrim/FO4 side: the flat object-LOD ring there was replaced by the
/// quadtree band ladder in [`super::lod_bands::LodBandLadder`], which
/// streams multiple discrete levels (4/8/16 for Skyrim) rather than one
/// flat ring, so the two schemes no longer mirror each other. The
/// placement scheme correctly stays a flat 16-cell ring — no baked
/// quadtree exists for Oblivion/FO3/FNV.
pub(crate) const PLACEMENT_LOD_RADIUS_CELLS: i32 = 16;

/// One distant-object placement decoded from a `.lod` file. Values are in
/// the **source** convention (Bethesda Z-up world units, Euler radians,
/// scale already converted from the file's percent to a multiplier);
/// [`placement_world_transform`] converts to the engine's Y-up frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placement {
    /// World position, Bethesda Z-up units.
    pub position: [f32; 3],
    /// Euler rotation, radians, Z-up (zero in vanilla content).
    pub rotation: [f32; 3],
    /// Scale **multiplier** (the file stores percent; this is already
    /// divided by 100, so vanilla `100.97` → `1.0097`).
    pub scale: f32,
}

/// All placements of one base object within a `.lod` cell.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlacementGroup {
    /// The base record FormID these placements instance (resolved to a
    /// model via `record_index.statics`).
    pub base_form_id: u32,
    pub placements: Vec<Placement>,
}

fn u32_at(b: &[u8], o: usize) -> io::Result<u32> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "truncated .lod"))
}

fn f32_at(b: &[u8], o: usize) -> io::Result<f32> {
    Ok(f32::from_bits(u32_at(b, o)?))
}

/// Parse a `DistantLOD\*.lod` placement file. See the module docs for the
/// byte layout. Returns the groups in file order. Errors (rather than
/// panics) on any out-of-bounds read, so a malformed / degenerate file
/// (e.g. `toddland`) is skipped by the caller rather than crashing.
///
/// The `scale` field is converted from the file's percent to a multiplier
/// (`/100`) here, so callers get an engine-ready value.
pub(crate) fn parse_placement_lod(bytes: &[u8]) -> io::Result<Vec<PlacementGroup>> {
    let num_groups = u32_at(bytes, 0)?;
    // #3518 — bound the header count against the file's own smallest legal
    // encoding (8 B/group: `base_form_id` + `count`, even with zero
    // placements) before allocating, same guard doctrine as
    // `checked_entry_count` (BSA/BA2 entry counts, #586) and
    // `allocate_vec`/`allocate_vec_sized` (NIF, #2523). Without this, a
    // hostile/corrupt `0xFFFFFFFF` header word requests ~137 GB in one
    // `Vec::with_capacity`, which aborts the process (`handle_alloc_error`)
    // instead of returning the `Err` this function's own doc promises.
    let max_groups = bytes.len().saturating_sub(4) / 8;
    if num_groups as usize > max_groups {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "group count {num_groups} exceeds what {} remaining bytes could encode \
                 ({max_groups} groups at 8 B minimum each)",
                bytes.len().saturating_sub(4),
            ),
        ));
    }
    let mut off = 4usize;
    let mut groups = Vec::with_capacity(num_groups as usize);
    for _ in 0..num_groups {
        let base_form_id = u32_at(bytes, off)?;
        let count = u32_at(bytes, off + 4)? as usize;
        off += 8;
        // SoA blocks: positions, then rotations, then scales.
        let pos_base = off;
        let rot_base = pos_base + count * 12;
        let scale_base = rot_base + count * 12;
        let end = scale_base + count * 4;
        if end > bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("group of {count} overruns .lod ({end} > {})", bytes.len()),
            ));
        }
        let mut placements = Vec::with_capacity(count);
        for i in 0..count {
            let position = [
                f32_at(bytes, pos_base + i * 12)?,
                f32_at(bytes, pos_base + i * 12 + 4)?,
                f32_at(bytes, pos_base + i * 12 + 8)?,
            ];
            let rotation = [
                f32_at(bytes, rot_base + i * 12)?,
                f32_at(bytes, rot_base + i * 12 + 4)?,
                f32_at(bytes, rot_base + i * 12 + 8)?,
            ];
            let scale = f32_at(bytes, scale_base + i * 4)? / 100.0;
            placements.push(Placement {
                position,
                rotation,
                scale,
            });
        }
        off = end;
        groups.push(PlacementGroup {
            base_form_id,
            placements,
        });
    }
    Ok(groups)
}

/// Convert a [`Placement`] (Bethesda Z-up) to the engine's Y-up spawn
/// transform `(position, rotation, scale)`. Routed through the SAME coord
/// SoT the REFR spawn path uses ([`euler_zup_to_quat_yup_refr`] +
/// `coord::zup_to_yup_pos`) so distant objects land in exactly the frame
/// their full-detail REFRs would — never an independent inline swap.
pub(crate) fn placement_world_transform(p: &Placement) -> (Vec3, Quat, f32) {
    let pos = Vec3::from_array(byroredux_core::math::coord::zup_to_yup_pos(p.position));
    let rot = euler_zup_to_quat_yup_refr(p.rotation[0], p.rotation[1], p.rotation[2]);
    (pos, rot, p.scale)
}

/// Archive-relative path of the per-cell placement file:
/// `distantlod\<world>_<cx>_<cy>.lod`. The worldspace folder/stem is the
/// EDID lowercased (the cell loader's `worldspace_key` is already
/// lowercase); backslash separators match the BSA path convention.
/// Verified against real entries (e.g. `distantlod\tamriel_-34_-10.lod`,
/// `distantlod\anvilworld_-45_-7.lod`).
pub(crate) fn placement_lod_archive_path(worldspace_key: &str, cx: i32, cy: i32) -> String {
    let w = worldspace_key.to_ascii_lowercase();
    format!("distantlod\\{w}_{cx}_{cy}.lod")
}

/// Derive the `_far.nif` low-poly mesh archive path for a base record's
/// `model_path`. The distant variant is the model with its `.nif`
/// extension replaced by `_far.nif` (verified: 130 `*_far.nif` entries in
/// `Oblivion - Meshes.bsa`, named `<stem>_far.nif`). The result carries a
/// `meshes\` prefix (the form `extract_mesh` expects), added when the
/// stored `model_path` is folder-relative — mirroring the REFR spawn
/// path. Returns `None` for a record with no `.nif` model.
pub(crate) fn far_nif_path(model_path: &str) -> Option<String> {
    let lower = model_path.to_ascii_lowercase();
    let stem = lower.strip_suffix(".nif")?;
    let far = format!("{stem}_far.nif");
    if far.starts_with("meshes\\") || far.starts_with("meshes/") {
        Some(far)
    } else {
        Some(format!("meshes\\{far}"))
    }
}

/// Archive path of a base record's **full** model, used as the distant
/// mesh when no `_far.nif` exists (the common case — only ~130 Oblivion
/// objects ship a dedicated far mesh). Adds the `meshes\` prefix when the
/// stored `model_path` is folder-relative, mirroring the REFR spawn path.
pub(crate) fn full_model_path(model_path: &str) -> String {
    let lower = model_path.to_ascii_lowercase();
    if lower.starts_with("meshes\\") || lower.starts_with("meshes/") {
        lower
    } else {
        format!("meshes\\{lower}")
    }
}

/// Cells whose `.lod` should be resident this frame: within the LOD ring
/// (`lod_radius`, Chebyshev) of the player and not actually resident at full
/// detail, so distant objects never overlap a resident full model while LOD
/// still fills unpopulated hysteresis cells. Mirrors the desired-set logic in
/// [`super::object_lod::stream_object_lod_blocks`], but per-cell (the
/// placement files are one-per-cell, not one-per-quad).
pub(crate) fn placement_lod_cells_in_radius(
    player: (i32, i32),
    resident_full_cells: &std::collections::HashSet<(i32, i32)>,
    lod_radius: i32,
) -> Vec<(i32, i32)> {
    let mut cells = Vec::new();
    for dj in -lod_radius..=lod_radius {
        for di in -lod_radius..=lod_radius {
            let cheb = di.abs().max(dj.abs());
            let coord = (player.0 + di, player.1 + dj);
            if cheb <= lod_radius && !resident_full_cells.contains(&coord) {
                cells.push(coord);
            }
        }
    }
    cells
}

/// One streamed placement-LOD **cell**: every `_far.nif` sub-mesh of every
/// placement in the cell's `.lod`, spawned as [`IsLodTerrain`] entities.
/// Tracked so a cell leaving the ring frees all of its meshes, textures,
/// and entities (mirrors [`super::object_lod::ObjectLodBlock`], but a cell
/// is many base objects × many placements).
pub(crate) struct PlacementLodBlock {
    pub(crate) entities: Vec<EntityId>,
    /// Unique global-SSBO mesh ranges (one per uploaded `_far.nif`
    /// sub-mesh; shared across that group's placements). Dropped on unload.
    pub(crate) mesh_handles: Vec<u32>,
    /// Per-sub-mesh diffuse `TextureHandle`s acquired via `resolve_texture`
    /// (one refcount bump each). Released once each on unload — `despawn`
    /// has no GPU side effects, so without this the refcount never reaches
    /// 0 (#1537, sibling of the object-LOD / terrain-LOD leak). Never `0`.
    pub(crate) texture_handles: Vec<u32>,
}

impl PlacementLodBlock {
    /// Sentinel for a cell with no `.lod` (or a degenerate one). Inserted so
    /// the streaming reconcile doesn't re-extract a missing entry every
    /// cell-boundary crossing.
    fn empty() -> Self {
        Self {
            entities: Vec::new(),
            mesh_handles: Vec::new(),
            texture_handles: Vec::new(),
        }
    }
}

/// Stream the distant **object** LOD ring around the player for the
/// placement scheme (Oblivion only — see FO3-D4-01 below). Mirrors
/// [`super::object_lod::stream_object_lod_blocks`]: cells entering the ring
/// load their `.lod`, cells leaving unload. A cell loads only while the same
/// coordinate has no actually-resident full-detail representation.
///
/// No-op for everything but Oblivion. The `DistantLOD\*.lod` scheme this
/// module implements was reverse-engineered and validated against all 9889
/// real Oblivion `.lod` files; a direct probe of every FO3/FNV vanilla
/// archive (base game + all DLC) found **zero** `distantlod\` entries
/// anywhere — Bethesda didn't ship this scheme for the Fallout titles.
/// FO3's `Fallout - Meshes.bsa` carries 2 `_far.nif` files total (one-off
/// landmark assets, not a systematic scheme) and FNV's carries **zero** —
/// #3321 corrected the attribution; the "2" was never FNV's. Gating this
/// module to FO3/FNV as well (as it did before FO3-D4-01) was harmless —
/// `spawn_placement_lod_cell` just returned `None` on every call and the
/// ring silently inserted empty sentinels — but wasted a per-cell archive
/// lookup for no result.
///
/// **This does not mean FO3/FNV have no distant-object LOD.** They ship a
/// systematic per-quad combined-mesh family under
/// `meshes\landscape\lod\<world>\blocks\`, consumed by
/// [`super::object_lod`] since #3321 — the same module that handles
/// Skyrim+/FO4's baked `.bto`. Only the `DistantLOD\*.lod` *placement-list*
/// scheme this module implements is Oblivion-exclusive.
/// Whether `game` ships the `DistantLOD\*.lod` placement scheme this module
/// implements. Oblivion only — see FO3-D4-01 (#2086): FO3/FNV ship zero
/// `distantlod\*.lod` files in any vanilla archive, despite
/// `GameKind::Fallout3NV` collapsing both titles into one enum variant
/// elsewhere in the parser.
pub(crate) fn placement_lod_supported(game: GameKind) -> bool {
    game == GameKind::Oblivion
}

pub(crate) fn stream_placement_lod_blocks(
    world: &mut World,
    ctx: &mut VulkanContext,
    input: &LodReconcileInput<'_>,
    blocks: &mut HashMap<(i32, i32), PlacementLodBlock>,
    budget: &mut LodWorkBudget,
) -> bool {
    let tex_provider = input.tex_provider;
    let wctx = input.wctx;
    let player_grid = input.player_grid;
    if !placement_lod_supported(wctx.record_index.game) {
        return true;
    }

    let mut desired = placement_lod_cells_in_radius(
        player_grid,
        input.resident_full_cells,
        PLACEMENT_LOD_RADIUS_CELLS,
    );
    sort_lod_coords_nearest(&mut desired, |(cx, cy)| {
        (cx - player_grid.0).abs().max((cy - player_grid.1).abs())
    });
    let desired_set: std::collections::HashSet<_> = desired.iter().copied().collect();

    let mut spawned = 0usize;
    let mut unloaded = 0usize;

    // Unload cells that left the ring (skip empty sentinels — nothing to free).
    blocks.retain(|coord, blk| {
        if desired_set.contains(coord) {
            true
        } else {
            if !blk.entities.is_empty() {
                unload_placement_lod_block(world, ctx, blk);
                unloaded += 1;
            }
            false
        }
    });

    let candidates: Vec<_> = desired
        .into_iter()
        .filter(|coord| !blocks.contains_key(coord))
        .collect();
    let candidate_count = candidates.len();
    let mut attempted = 0usize;

    // Load entering cells.
    for (cx, cy) in candidates {
        if !budget.try_take() {
            break;
        }
        attempted += 1;
        match spawn_placement_lod_cell(world, ctx, tex_provider, wctx, cx, cy) {
            Some(blk) => {
                if !blk.entities.is_empty() {
                    spawned += 1;
                }
                blocks.insert((cx, cy), blk);
            }
            None => {
                blocks.insert((cx, cy), PlacementLodBlock::empty());
            }
        }
    }

    let complete = attempted == candidate_count;
    if complete && spawned + unloaded > 0 {
        log::info!(
            "Placement-LOD ring @cell ({},{}): +{} cells loaded, -{} unloaded ({} tracked)",
            player_grid.0,
            player_grid.1,
            spawned,
            unloaded,
            blocks.len(),
        );
    }

    complete
}

/// One uploaded `_far.nif` sub-mesh, reused across every placement of its
/// base-object group (instanced — geometry uploaded once, drawn at many
/// transforms).
struct FarSubMesh {
    handle: u32,
    /// `_far.nif`-local transform (already Y-up via import).
    local_pos: Vec3,
    local_rot: Quat,
    local_scale: f32,
    /// Local AABB centre + radius (for the per-placement world bound).
    local_centre: Vec3,
    local_radius: f32,
    /// Resolved diffuse texture (`0` = fallback / untextured).
    texture: u32,
    /// Canonical material, translated once per sub-mesh and cloned onto each
    /// placement instance. #2444 (MAT-D3-02) sibling — this LOD path had the
    /// same "spawns a draw with no `Material`" shape as the three the audit
    /// named, so its draws also fell into `render/static_meshes.rs`'s
    /// hardcoded-literal arm. Unlike terrain and object LOD it *does* have a
    /// source record — `_far.nif` sub-meshes carry a real `ImportedMaterial`
    /// — so it routes through the full `translate_material` boundary rather
    /// than the texture-path-only helper.
    material: byroredux_core::ecs::components::Material,
}

/// Resolve + import + spawn one cell's `.lod`. Returns `None` when the cell
/// has no `.lod`, the file is degenerate, or nothing resolved.
fn spawn_placement_lod_cell(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    wctx: &ExteriorWorldContext,
    cx: i32,
    cy: i32,
) -> Option<PlacementLodBlock> {
    let lod_path = placement_lod_archive_path(&wctx.worldspace_key, cx, cy);
    let bytes = tex_provider.extract_mesh(&lod_path)?;
    let groups = match parse_placement_lod(&bytes) {
        Ok(g) => g,
        Err(e) => {
            log::warn!("Placement-LOD '{lod_path}' parse failed: {e}");
            return None;
        }
    };
    ctx.allocator.as_ref()?;

    let mut entities = Vec::new();
    let mut mesh_handles = Vec::new();
    let mut texture_handles = Vec::new();

    for group in &groups {
        // base FormID → STAT model (the same statics table the REFR spawn
        // path reads).
        let Some(stat) = wctx.record_index.cells.statics.get(&group.base_form_id) else {
            continue;
        };
        if stat.model_path.is_empty() {
            continue;
        }
        // Prefer the dedicated `_far.nif`; fall back to the FULL model when
        // none is shipped. Vanilla Oblivion ships a `_far.nif` for only ~130
        // landmark objects (castle walls, towers, bridges) — every other
        // VWD object in a `.lod` renders its full mesh at distance, which is
        // what the real engine does. Skipping them (the pre-fix behaviour)
        // left almost every cell with no distant geometry.
        let (mesh_path, far_bytes) = match far_nif_path(&stat.model_path)
            .and_then(|p| tex_provider.extract_mesh(&p).map(|b| (p, b)))
        {
            Some((p, b)) => (p, b),
            None => {
                let full = full_model_path(&stat.model_path);
                match tex_provider.extract_mesh(&full) {
                    Some(b) => (full, b),
                    None => continue, // neither far nor full mesh resolvable
                }
            }
        };
        let far = mesh_path;
        let scene = match byroredux_nif::parse_nif(&far_bytes) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Placement-LOD far '{far}' parse failed: {e}");
                continue;
            }
        };
        let mut pool = byroredux_core::string::StringPool::new();
        // #2362 / SF2D2-05 — thread the already-in-scope `tex_provider`
        // through so external-geometry `BSGeometry` LOD slots resolve
        // instead of silently importing to zero meshes. See the matching
        // note on `object_lod.rs::spawn_object_lod_quad`.
        let imported = byroredux_nif::import::import_nif_scene_with_resolver(
            &scene,
            &mut pool,
            Some(tex_provider),
        );
        if imported.meshes.is_empty() {
            continue;
        }

        // Upload each sub-mesh ONCE, reuse the handle across this group's
        // placements (instancing — no per-placement geometry re-upload).
        let mut subs: Vec<FarSubMesh> = Vec::new();
        for mesh in &imported.meshes {
            if mesh.positions.is_empty() || mesh.indices.is_empty() {
                continue;
            }
            let verts = super::lod_support::imported_mesh_to_vertices(mesh);

            let handle = match ctx
                .mesh_registry
                .upload_scene_mesh_global_only(&verts, &mesh.indices)
            {
                Ok(h) => h,
                Err(e) => {
                    log::warn!("Placement-LOD '{far}' mesh upload failed: {e}");
                    continue;
                }
            };
            mesh_handles.push(handle);

            // Diffuse texture from the `_far.nif`'s own shader texture set.
            // #2444 sibling — the whole owned texture set (not just the
            // diffuse) is resolved here because it is also the input to the
            // canonical material translation below.
            let owned_textures = mesh
                .material
                .textures
                .map_ref(|sym| sym.and_then(|s| pool.resolve(s)).map(str::to_owned));
            let material = crate::material_translate::translate_material(
                &mesh.material,
                mesh.name.as_deref(),
                crate::material_translate::ResolvedPaths {
                    textures: owned_textures.clone(),
                    material_path: mesh
                        .material
                        .material_path
                        .and_then(|s| pool.resolve(s))
                        .map(str::to_owned),
                },
                0,
            );
            let tex_str = owned_textures.base_color.clone();
            let raw = resolve_texture(ctx, tex_provider, tex_str.as_deref());
            let texture = if raw == ctx.texture_registry.fallback() {
                0
            } else {
                texture_handles.push(raw);
                raw
            };

            let (local_centre, local_radius) =
                super::lod_support::local_aabb_center_radius(&mesh.positions);

            subs.push(FarSubMesh {
                handle,
                local_pos: Vec3::from_array(mesh.translation),
                local_rot: Quat::from_xyzw(
                    mesh.rotation[0],
                    mesh.rotation[1],
                    mesh.rotation[2],
                    mesh.rotation[3],
                ),
                local_scale: mesh.scale,
                local_centre,
                local_radius,
                texture,
                material,
            });
        }

        // Instance each sub-mesh at every placement (placement ∘ local TRS).
        for placement in &group.placements {
            let (p_pos, p_rot, p_scale) = placement_world_transform(placement);
            for sub in &subs {
                let (pos, rot, scale) = GlobalTransform::compose_trs(
                    p_pos,
                    p_rot,
                    p_scale,
                    sub.local_pos,
                    sub.local_rot,
                    sub.local_scale,
                );
                let bound = WorldBound::new(
                    pos + rot * (sub.local_centre * scale),
                    sub.local_radius * scale,
                );

                let entity = world.spawn();
                world.insert(entity, Transform::new(pos, rot, scale));
                world.insert(entity, GlobalTransform::new(pos, rot, scale));
                world.insert(entity, MeshHandle(sub.handle));
                if sub.texture != 0 {
                    world.insert(entity, TextureHandle(sub.texture));
                }
                world.insert(entity, bound);
                // #2444 (MAT-D3-02) sibling — canonical `Material` per
                // placement instance. Cloned from the once-translated
                // sub-mesh material rather than re-translated per placement:
                // the boundary ran once, at import.
                world.insert(entity, sub.material.clone());
                world.insert(entity, RenderLayer::Architecture);
                // No BLAS, lean static draw, kept out of the TLAS (shared with
                // terrain / object LOD). Cells load only where that exact
                // coordinate has no resident full-detail representation.
                world.insert(entity, IsLodTerrain);
                entities.push(entity);
            }
        }
    }

    if entities.is_empty() {
        release_lod_gpu_resources(ctx, &mesh_handles, &texture_handles);
        return None;
    }
    Some(PlacementLodBlock {
        entities,
        mesh_handles,
        texture_handles,
    })
}

/// Free one placement-LOD cell: drop each sub-mesh's global-SSBO range,
/// release each resolved texture's refcount, and despawn every entity
/// (mirrors [`super::object_lod::unload_object_lod_block`]).
pub(crate) fn unload_placement_lod_block(
    world: &mut World,
    ctx: &mut VulkanContext,
    block: &PlacementLodBlock,
) {
    release_lod_gpu_resources(ctx, &block.mesh_handles, &block.texture_handles);
    for &e in &block.entities {
        world.despawn(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real bytes of `distantlod\anvilcastlecourtyardworld_-46_-10.lod`
    /// (extracted 2026-06-23 from `Oblivion - Meshes.bsa`) — the smallest
    /// non-degenerate file: 1 group, 1 placement. The unambiguous ground
    /// truth that pins the SoA field order (pos / rot=0 / scale=percent).
    const ANVIL_COURTYARD: [u8; 40] = [
        0x01, 0x00, 0x00, 0x00, // num_groups = 1
        0x2c, 0x2f, 0x02, 0x00, // base_form_id = 0x00022f2c
        0x01, 0x00, 0x00, 0x00, // count = 1
        0x00, 0x27, 0x37, 0xc8, // pos.x = -187548.0
        0x00, 0x02, 0x19, 0xc7, // pos.y = -39170.0
        0x14, 0x3e, 0x17, 0x44, // pos.z = 604.97
        0x00, 0x00, 0x00, 0x00, // rot.x = 0
        0x00, 0x00, 0x00, 0x00, // rot.y = 0
        0x00, 0x00, 0x00, 0x00, // rot.z = 0
        0xa4, 0xf0, 0xc9, 0x42, // scale = 100.97 (%)
    ];

    #[test]
    fn parses_real_single_placement_file() {
        let groups = parse_placement_lod(&ANVIL_COURTYARD).expect("parses");
        assert_eq!(groups.len(), 1);
        let g = &groups[0];
        assert_eq!(g.base_form_id, 0x0002_2f2c);
        assert_eq!(g.placements.len(), 1);
        let p = g.placements[0];
        assert_eq!(p.position, [-187548.0, -39170.0, 604.97]);
        assert_eq!(p.rotation, [0.0, 0.0, 0.0]);
        // 100.97% → multiplier 1.0097.
        assert!((p.scale - 1.0097).abs() < 1e-4, "scale={}", p.scale);
    }

    /// A hand-built two-group file matching the STRUCTURE decoded from real
    /// `tamriel_0_0.lod` (group0: 1 placement; group1: 2 placements). Pins
    /// the structure-of-arrays grouping — the bug-prone part: a naive
    /// array-of-structs reader misreads count>1 groups (rot/scale columns
    /// land on the wrong entry).
    #[test]
    fn parses_soa_multi_group_multi_placement() {
        let mut b = Vec::new();
        let push_u32 = |b: &mut Vec<u8>, v: u32| b.extend_from_slice(&v.to_le_bytes());
        let push_f32 = |b: &mut Vec<u8>, v: f32| b.extend_from_slice(&v.to_le_bytes());

        push_u32(&mut b, 2); // num_groups
                             // group 0: form 0x10, 1 placement
        push_u32(&mut b, 0x10);
        push_u32(&mut b, 1);
        push_f32(&mut b, 100.0); // pos
        push_f32(&mut b, 200.0);
        push_f32(&mut b, 300.0);
        push_f32(&mut b, 0.0); // rot
        push_f32(&mut b, 0.0);
        push_f32(&mut b, 0.0);
        push_f32(&mut b, 150.0); // scale %
                                 // group 1: form 0x20, 2 placements (SoA blocks)
        push_u32(&mut b, 0x20);
        push_u32(&mut b, 2);
        // positions (2 × Vec3)
        push_f32(&mut b, 1.0);
        push_f32(&mut b, 2.0);
        push_f32(&mut b, 3.0);
        push_f32(&mut b, 4.0);
        push_f32(&mut b, 5.0);
        push_f32(&mut b, 6.0);
        // rotations (2 × Vec3)
        push_f32(&mut b, 0.1);
        push_f32(&mut b, 0.2);
        push_f32(&mut b, 0.3);
        push_f32(&mut b, 0.4);
        push_f32(&mut b, 0.5);
        push_f32(&mut b, 0.6);
        // scales (2 × f32, %)
        push_f32(&mut b, 100.0);
        push_f32(&mut b, 250.0);

        let groups = parse_placement_lod(&b).expect("parses");
        assert_eq!(groups.len(), 2);

        assert_eq!(groups[0].base_form_id, 0x10);
        assert_eq!(groups[0].placements.len(), 1);
        assert_eq!(groups[0].placements[0].position, [100.0, 200.0, 300.0]);
        assert!((groups[0].placements[0].scale - 1.5).abs() < 1e-6);

        let g1 = &groups[1];
        assert_eq!(g1.base_form_id, 0x20);
        assert_eq!(g1.placements.len(), 2);
        // Critical: the SoA split must pair entry 0's pos with entry 0's
        // rot/scale, and entry 1's with entry 1's.
        assert_eq!(g1.placements[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(g1.placements[0].rotation, [0.1, 0.2, 0.3]);
        assert!((g1.placements[0].scale - 1.0).abs() < 1e-6);
        assert_eq!(g1.placements[1].position, [4.0, 5.0, 6.0]);
        assert_eq!(g1.placements[1].rotation, [0.4, 0.5, 0.6]);
        assert!((g1.placements[1].scale - 2.5).abs() < 1e-6);
    }

    /// A truncated / degenerate file (count claims more entries than the
    /// buffer holds — the `toddland` failure mode) must error, not panic,
    /// so the streaming loop skips it.
    #[test]
    fn truncated_file_errors_rather_than_panics() {
        let mut b = Vec::new();
        b.extend_from_slice(&1u32.to_le_bytes()); // 1 group
        b.extend_from_slice(&0x10u32.to_le_bytes()); // formid
        b.extend_from_slice(&100u32.to_le_bytes()); // count=100 but no data
        assert!(parse_placement_lod(&b).is_err());
        // Empty buffer also errors cleanly.
        assert!(parse_placement_lod(&[]).is_err());
    }

    /// #3518 — a hostile/corrupt `0xFFFFFFFF` group count must return `Err`
    /// (the documented recovery path) rather than reach
    /// `Vec::with_capacity`, which would request ~137 GB and abort the
    /// process instead of unwinding to the caller's "skip this file" logic.
    #[test]
    fn u32_max_group_count_errors_instead_of_aborting() {
        let mut b = Vec::new();
        b.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_placement_lod(&b).is_err());

        // Same header word backed by a few real bytes still can't possibly
        // encode u32::MAX groups (8 B minimum each) — still an error, not
        // an allocation sized off the untrusted count.
        let mut b2 = Vec::new();
        b2.extend_from_slice(&u32::MAX.to_le_bytes());
        b2.extend_from_slice(&[0u8; 64]);
        assert!(parse_placement_lod(&b2).is_err());
    }

    /// The bound is exact, not overzealous: a header claiming exactly the
    /// number of groups the buffer can encode (here, zero-placement groups
    /// at the 8 B floor) must still parse successfully.
    #[test]
    fn group_count_at_the_exact_capacity_boundary_still_parses() {
        let mut b = Vec::new();
        let push_u32 = |b: &mut Vec<u8>, v: u32| b.extend_from_slice(&v.to_le_bytes());
        push_u32(&mut b, 3); // num_groups — exactly 3 zero-placement groups fit
        for form in [0x10u32, 0x20, 0x30] {
            push_u32(&mut b, form);
            push_u32(&mut b, 0); // count = 0 → no SoA blocks follow
        }
        let groups = parse_placement_lod(&b).expect("exact-capacity header must parse");
        assert_eq!(groups.len(), 3);
        assert!(groups.iter().all(|g| g.placements.is_empty()));
    }

    /// FO3-D4-01 (#2086): the `DistantLOD\*.lod` placement scheme is
    /// Oblivion-only. `GameKind::Fallout3NV` (which covers BOTH FO3 and
    /// FNV) — as well as Skyrim/FO4/76/Starfield — must all be excluded, or
    /// `stream_placement_lod_blocks` wastes a per-cell archive lookup that
    /// can never resolve (vanilla FO3/FNV archives ship zero `distantlod\`
    /// entries).
    #[test]
    fn placement_lod_supported_is_oblivion_only() {
        assert!(placement_lod_supported(GameKind::Oblivion));
        assert!(!placement_lod_supported(GameKind::Fallout3NV));
        assert!(!placement_lod_supported(GameKind::Skyrim));
        assert!(!placement_lod_supported(GameKind::Fallout4));
        assert!(!placement_lod_supported(GameKind::Fallout76));
        assert!(!placement_lod_supported(GameKind::Starfield));
    }

    #[test]
    fn archive_path_matches_vanilla_filenames() {
        assert_eq!(
            placement_lod_archive_path("Tamriel", -34, -10),
            "distantlod\\tamriel_-34_-10.lod"
        );
        assert_eq!(
            placement_lod_archive_path("anvilworld", -45, -7),
            "distantlod\\anvilworld_-45_-7.lod"
        );
        assert_eq!(
            placement_lod_archive_path("Tamriel", 0, 0),
            "distantlod\\tamriel_0_0.lod"
        );
    }

    #[test]
    fn far_nif_derivation() {
        // Folder-relative model → meshes\ prefix + _far suffix.
        assert_eq!(
            far_nif_path("architecture\\kvatch\\kvatchcastletower01.nif").as_deref(),
            Some("meshes\\architecture\\kvatch\\kvatchcastletower01_far.nif")
        );
        // Already meshes-prefixed model keeps a single prefix.
        assert_eq!(
            far_nif_path("meshes\\clutter\\barrel01.nif").as_deref(),
            Some("meshes\\clutter\\barrel01_far.nif")
        );
        // Case-insensitive extension.
        assert_eq!(
            far_nif_path("Clutter\\Rock01.NIF").as_deref(),
            Some("meshes\\clutter\\rock01_far.nif")
        );
        // Non-.nif model (e.g. light-only record) → None.
        assert_eq!(far_nif_path(""), None);
        assert_eq!(far_nif_path("textures\\foo.dds"), None);
    }

    #[test]
    fn full_model_derivation() {
        // Folder-relative → meshes\ prefix, lowercased.
        assert_eq!(
            full_model_path("Architecture\\Anvil\\AnvilHouse01.nif"),
            "meshes\\architecture\\anvil\\anvilhouse01.nif"
        );
        // Already prefixed → single prefix.
        assert_eq!(
            full_model_path("meshes\\clutter\\barrel01.nif"),
            "meshes\\clutter\\barrel01.nif"
        );
    }

    #[test]
    fn ring_excludes_full_detail_and_caps_at_lod_radius() {
        let resident: std::collections::HashSet<_> = (-1..=1)
            .flat_map(|x| (-1..=1).map(move |y| (x, y)))
            .collect();
        let cells = placement_lod_cells_in_radius((0, 0), &resident, 2);
        // Distance-2 ring around origin = the 5×5 border = 16 cells.
        assert_eq!(cells.len(), 16);
        // No cell inside the full-detail ring (cheb <= 1).
        assert!(cells.iter().all(|(x, y)| x.abs().max(y.abs()) == 2));
        // The player's own cell is never in the LOD set.
        assert!(!cells.contains(&(0, 0)));
    }

    #[test]
    fn ring_fills_never_loaded_hysteresis_cells_but_excludes_residents() {
        let lod_radius = 4;
        let resident = std::collections::HashSet::from([(2, 0)]);

        let cells = placement_lod_cells_in_radius((0, 0), &resident, lod_radius);
        assert!(
            !cells.contains(&(2, 0)),
            "an actually-resident hysteresis cell remains full detail"
        );
        assert!(
            cells.contains(&(-2, 0)),
            "an equally distant cell that was never loaded belongs to LOD"
        );
    }

    #[test]
    fn world_transform_applies_zup_to_yup() {
        // Z-up (x, y, z) → engine Y-up (x, z, -y) per the coord SoT.
        let p = Placement {
            position: [10.0, 20.0, 30.0],
            rotation: [0.0, 0.0, 0.0],
            scale: 1.5,
        };
        let (pos, _rot, scale) = placement_world_transform(&p);
        let expect = byroredux_core::math::coord::zup_to_yup_pos([10.0, 20.0, 30.0]);
        assert_eq!(pos, Vec3::from_array(expect));
        assert_eq!(scale, 1.5);
    }

    /// #2362 / SF2D2-05 — `spawn_placement_lod_cell` must thread the
    /// already-in-scope `tex_provider` through as a `MeshResolver`. See
    /// the matching pin on `object_lod.rs::spawn_object_lod_quad_threads_
    /// the_mesh_resolver`.
    #[test]
    fn spawn_placement_lod_cell_threads_the_mesh_resolver() {
        // Whitespace-insensitive so a reformat doesn't spuriously break this.
        let normalized: String = include_str!("placement_lod.rs")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        assert!(
            normalized
                .contains("import_nif_scene_with_resolver(&scene,&mutpool,Some(tex_provider),)"),
            "spawn_placement_lod_cell must pass tex_provider as the \
             MeshResolver, not call the no-resolver import_nif_scene overload",
        );
    }
}
