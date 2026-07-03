//! Distant **object** LOD (Oblivion / FO3 / FNV) — the per-cell
//! `DistantLOD\<World>_<x>_<y>.lod` placement scheme.
//!
//! This is the older-game counterpart to [`super::object_lod`] (the
//! Skyrim+/FO4 baked-`.bto` scheme). The two are structurally different
//! LOD producers (EXAL §5, docs/engine/exal.md) and deliberately do NOT
//! share a code path:
//!
//! - **Skyrim LE/SE, FO4** ([`super::object_lod`]): one baked macro-mesh
//!   per quad, selected by filename.
//! - **Oblivion / FO3 / FNV** (this module): per-cell placement lists that
//!   instance individual `_far.nif` low-poly meshes — one draw per entry,
//!   no atlas, no combined mesh.
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
//! Validation across the corpus: the SoA layout consumes 9888/9889 files
//! exactly (the lone outlier is `toddland`, the CS tutorial world, whose
//! LOD data is degenerate); rotations are all within ±2π rad; scales are
//! all positive. Positions confine to the single cell named by the file,
//! so the files are **per-cell**.
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
use byroredux_renderer::{Vertex, VulkanContext};

use crate::asset_provider::{resolve_texture, TextureProvider};
use crate::components::IsLodTerrain;

use super::euler::euler_zup_to_quat_yup_refr;
use super::exterior::ExteriorWorldContext;

/// Object-LOD ring radius in **cells** (Chebyshev) for the placement
/// scheme. Cells within this distance of the player — and entirely beyond
/// the full-detail ring — load their `.lod`. Mirrors
/// [`super::object_lod::OBJECT_LOD_RADIUS_CELLS`]; the placement scheme is
/// many small draws per cell, so the same conservative 16-cell band.
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
/// (`lod_radius`, Chebyshev) of the player **and** entirely beyond
/// `max_full_cell_radius`, so distant objects never overlap a resident full
/// model. Mirrors the desired-set logic in
/// [`super::object_lod::stream_object_lod_blocks`], but per-cell (the
/// placement files are one-per-cell, not one-per-quad).
///
/// `max_full_cell_radius` **must** be the caller's cell-streaming
/// `radius_unload`, not `radius_load` — see the identical note on
/// [`stream_object_lod_blocks`] (#1866 / LC0703-01): full cells can still be
/// resident up through `radius_load + 1` under the load/unload hysteresis
/// band, so gating on `radius_load` let this ring load one cell early and
/// z-fight a still-resident full model.
pub(crate) fn placement_lod_cells_in_radius(
    player: (i32, i32),
    max_full_cell_radius: i32,
    lod_radius: i32,
) -> Vec<(i32, i32)> {
    let mut cells = Vec::new();
    for dj in -lod_radius..=lod_radius {
        for di in -lod_radius..=lod_radius {
            let cheb = di.abs().max(dj.abs());
            if cheb > max_full_cell_radius && cheb <= lod_radius {
                cells.push((player.0 + di, player.1 + dj));
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
/// placement scheme (Oblivion / FO3 / FNV). Mirrors
/// [`super::object_lod::stream_object_lod_blocks`]: cells entering the ring
/// load their `.lod`, cells leaving unload. A cell loads only when it is
/// **entirely outside** `max_full_cell_radius`, so the distant `_far.nif`
/// never overlaps a resident full model.
///
/// `max_full_cell_radius` **must** be the caller's `radius_unload` — see
/// [`placement_lod_cells_in_radius`] (#1866 / LC0703-01).
///
/// No-op for Skyrim+/FO4 — those ship the baked `.bto` scheme
/// ([`super::object_lod`]), not `DistantLOD\*.lod`.
pub(crate) fn stream_placement_lod_blocks(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    wctx: &ExteriorWorldContext,
    player_grid: (i32, i32),
    max_full_cell_radius: i32,
    blocks: &mut HashMap<(i32, i32), PlacementLodBlock>,
) {
    // Oblivion + Fallout 3 / New Vegas (the latter two collapse to one
    // `GameKind`). These ship the `DistantLOD\*.lod` placement scheme.
    if !matches!(
        wctx.record_index.game,
        GameKind::Oblivion | GameKind::Fallout3NV
    ) {
        return;
    }

    let desired: std::collections::HashSet<(i32, i32)> = placement_lod_cells_in_radius(
        player_grid,
        max_full_cell_radius,
        PLACEMENT_LOD_RADIUS_CELLS,
    )
    .into_iter()
    .collect();

    let mut spawned = 0usize;
    let mut unloaded = 0usize;

    // Unload cells that left the ring (skip empty sentinels — nothing to free).
    blocks.retain(|coord, blk| {
        if desired.contains(coord) {
            true
        } else {
            if !blk.entities.is_empty() {
                unload_placement_lod_block(world, ctx, blk);
                unloaded += 1;
            }
            false
        }
    });

    // Load entering cells.
    for &(cx, cy) in &desired {
        if blocks.contains_key(&(cx, cy)) {
            continue; // already loaded (or a known-missing sentinel)
        }
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

    if spawned + unloaded > 0 {
        log::info!(
            "Placement-LOD ring @cell ({},{}): +{} cells loaded, -{} unloaded ({} tracked)",
            player_grid.0,
            player_grid.1,
            spawned,
            unloaded,
            blocks.len(),
        );
    }
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
        let imported = byroredux_nif::import::import_nif_scene(&scene, &mut pool);
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
            let verts: Vec<Vertex> = (0..mesh.positions.len())
                .map(|i| {
                    let color3 = mesh
                        .colors
                        .get(i)
                        .map(|c| [c[0], c[1], c[2]])
                        .unwrap_or([1.0, 1.0, 1.0]);
                    let normal = mesh.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
                    let uv = mesh.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
                    let mut v = Vertex::new(mesh.positions[i], color3, normal, uv);
                    if let Some(t) = mesh.tangents.get(i) {
                        v.tangent = *t;
                    }
                    v
                })
                .collect();

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
            let tex_str = mesh.texture_path.and_then(|fs| pool.resolve(fs).map(str::to_owned));
            let raw = resolve_texture(ctx, tex_provider, tex_str.as_deref());
            let texture = if raw == ctx.texture_registry.fallback() {
                0
            } else {
                texture_handles.push(raw);
                raw
            };

            let mut lmin = Vec3::splat(f32::INFINITY);
            let mut lmax = Vec3::splat(f32::NEG_INFINITY);
            for p in &mesh.positions {
                let v = Vec3::from_array(*p);
                lmin = lmin.min(v);
                lmax = lmax.max(v);
            }
            let local_centre = (lmin + lmax) * 0.5;
            let local_radius = (lmax - local_centre).length();

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
            });
        }

        // Instance each sub-mesh at every placement (placement ∘ local TRS).
        for placement in &group.placements {
            let (p_pos, p_rot, p_scale) = placement_world_transform(placement);
            for sub in &subs {
                let scale = p_scale * sub.local_scale;
                let rot = p_rot * sub.local_rot;
                let pos = p_pos + p_rot * (sub.local_pos * p_scale);
                let bound =
                    WorldBound::new(pos + rot * (sub.local_centre * scale), sub.local_radius * scale);

                let entity = world.spawn();
                world.insert(entity, Transform::new(pos, rot, scale));
                world.insert(entity, GlobalTransform::new(pos, rot, scale));
                world.insert(entity, MeshHandle(sub.handle));
                if sub.texture != 0 {
                    world.insert(entity, TextureHandle(sub.texture));
                }
                world.insert(entity, bound);
                world.insert(entity, RenderLayer::Architecture);
                // No BLAS, lean static draw, kept out of the TLAS (shared with
                // terrain / object LOD). Cells load only outside the
                // full-detail ring, so no resident full model conflicts.
                world.insert(entity, IsLodTerrain);
                entities.push(entity);
            }
        }
    }

    if entities.is_empty() {
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
    for &h in &block.mesh_handles {
        ctx.mesh_registry.drop_mesh(h);
    }
    for &t in &block.texture_handles {
        ctx.texture_registry.drop_texture(&ctx.device, t);
    }
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
        // full_radius=1, lod_radius=2: only the ring of Chebyshev distance 2.
        let cells = placement_lod_cells_in_radius((0, 0), 1, 2);
        // Distance-2 ring around origin = the 5×5 border = 16 cells.
        assert_eq!(cells.len(), 16);
        // No cell inside the full-detail ring (cheb <= 1).
        assert!(cells.iter().all(|(x, y)| x.abs().max(y.abs()) == 2));
        // The player's own cell is never in the LOD set.
        assert!(!cells.contains(&(0, 0)));
    }

    /// #1866 / LC0703-01 — a cell at exactly the streaming hysteresis
    /// boundary (`radius_load + 1 == radius_unload`) must NOT be desired
    /// when gated on `radius_unload`, even though gating on `radius_load`
    /// (the pre-fix behaviour) would have included it. That one-cell band
    /// is exactly where a full REFR can still be resident (full cells only
    /// unload past `radius_unload`), so loading `.lod` there would z-fight
    /// a still-resident full model.
    #[test]
    fn ring_excludes_hysteresis_band_when_gated_on_radius_unload() {
        let radius_load = 1;
        let radius_unload = radius_load + 1; // streaming.rs's hysteresis rule
        let lod_radius = 4;

        // Sanity: radius_load gating reproduces the pre-fix bug.
        let buggy = placement_lod_cells_in_radius((0, 0), radius_load, lod_radius);
        assert!(buggy.contains(&(2, 0)), "distance-2 cell == radius_unload");

        // Fixed: radius_unload gating excludes the hysteresis-band cell.
        let fixed = placement_lod_cells_in_radius((0, 0), radius_unload, lod_radius);
        assert!(
            !fixed.contains(&(2, 0)),
            "a cell at exactly radius_load+1 can still hold a resident full \
             REFR under the load/unload hysteresis band — LOD must not load there"
        );
        // A cell safely beyond the hysteresis band still loads.
        assert!(fixed.contains(&(3, 0)));
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
}
