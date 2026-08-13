//! Prebaked distant-**terrain** LOD meshes (Skyrim+ / FO4) — the `.btr`
//! macro-meshes the games ship per quad, the terrain counterpart to
//! [`super::object_lod`]'s `.bto` object LOD.
//!
//! These are a **source upgrade** over [`super::terrain_lod`]'s heightmap
//! synthesis: where the synth path decimates the LAND grid and paints a
//! single base ground texture (flat at distance), the `.btr` carries the
//! game's authored distant terrain mesh + per-quad diffuse, so the far
//! horizon reads with real texturing instead of tiled dirt.
//!
//! ## Integration (one banded ring, per-quad source choice)
//!
//! [`super::terrain_lod`] keys its ring by `(level, qx, qy)` and picks the
//! level from [`super::lod_bands`]' per-game ladder (#2371). Each coordinate
//! resolves to **either** a textured `.btr` block (this module) **or** a
//! heightmap-synth block — never both, so no double-draw / z-fight.
//!
//! * **Finest band (level 4).** `.btr` is chosen for **fully-distant**
//!   blocks (`hole_mask == 0` → all 16 cells have landscape and none lie
//!   inside the full-detail ring); boundary blocks fall through to synth,
//!   which can punch per-cell holes a baked mesh cannot. As the player
//!   approaches a `.btr` quad its mask flips non-zero → the block
//!   regenerates into a hole-masked synth boundary block, then the near
//!   cells stream full-detail.
//! * **Coarse bands (8 / 16 / 32).** `.btr` only. The band descent proposes
//!   a coarse quad exclusively where one is baked — a missing `.btr`
//!   subdivides into finer children instead of holing the horizon — and
//!   those quads sit far outside the streaming radius, so their hole mask is
//!   always 0.
//!
//! The synth path stays the universal fallback: Oblivion / FO3 / FNV bake no
//! quadtree at all and keep their single synthesized ring unchanged.
//!
//! ## Format (EXAL §5 / Q2, docs/engine/exal.md)
//!
//! `.btr` is a **renamed NIF** that parses + imports through the existing
//! pipeline (verified: vanilla Skyrim `tamriel.4.*.btr`, BSVER 100 /
//! v20.2.0.7, → 1 mesh). Naming is **level-first**:
//! `meshes\terrain\<world>\<world>.<level>.<x>.<y>.btr` with `(x, y)` the
//! quad's SW-corner cell on that worldspace's own level-aligned grid. Diffuse sibling:
//! `textures\terrain\<world>\<world>.<level>.<x>.<y>.dds`, with the
//! tangent-space normal map at the same stem plus `_n` (wired in #2371 —
//! see [`btr_normal_path`], which also documents why FO4's `_msn`
//! model-space variant is not bound yet).
//!
//! ## Placement convention (verified against real data — NOT like `.bto`)
//!
//! Unlike object `.bto` (whose sub-meshes are world-absolute), a `.btr` is
//! authored as a **normalized quad-local mesh**: every `.btr`, at any level,
//! has a constant local footprint `X ∈ [0, 4096]`, `Z ∈ [-4096, 0]` (one
//! cell) at the origin with identity transform — only the heights differ.
//! Placement scales the horizontal footprint by the LOD `level` (cells per
//! quad edge) and offsets to the quad's SW world corner; **heights are
//! absolute world heights and are not scaled**:
//!   * `world_x = qx·CELL + local_x · level`
//!   * `world_z = local_z · level − qy·CELL`   (`CELL` = [`EXTERIOR_CELL_UNITS`])
//!   * `world_y = local_y` (height, unscaled)
//!
//! Normals/tangents are corrected for the anisotropic XZ scale
//! (normal ∝ `(nx/level, ny, nz/level)`; tangent ∝ `(tx·level, ty, tz·level)`).

use byroredux_core::ecs::components::RenderLayer;
use byroredux_core::ecs::{
    GlobalTransform, MeshHandle, TextureHandle, Transform, World, WorldBound,
};
use byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
use byroredux_core::math::Vec3;
use byroredux_nif::import::MaterialTextureSet;
use byroredux_renderer::{Vertex, VulkanContext};

use crate::asset_provider::{resolve_texture, TextureProvider};
use crate::components::{IsLodTerrain, MaterialTextureHandles};
use crate::streaming::LodBlock;

/// Archive-relative path of the distant-terrain `.btr` for a worldspace
/// quad: `meshes\terrain\<world>\<world>.<level>.<x>.<y>.btr`.
///
/// **Level-first** naming with the quad's SW-corner cell `(qx, qy)` (the
/// ordering EXAL Q2 corrected — it is NOT `<x>.<y>.<level>`). Sibling of
/// [`super::object_lod::bto_archive_path`], which uses the same scheme under
/// the `\objects\` subfolder for object LOD.
pub(crate) fn btr_archive_path(worldspace_key: &str, level: i32, qx: i32, qy: i32) -> String {
    let w = worldspace_key.to_ascii_lowercase();
    format!("meshes\\terrain\\{w}\\{w}.{level}.{qx}.{qy}.btr")
}

/// Per-quad diffuse texture path for a distant-terrain `.btr`:
/// `textures\terrain\<world>\<world>.<level>.<x>.<y>.dds`.
pub(crate) fn btr_diffuse_path(worldspace_key: &str, level: i32, qx: i32, qy: i32) -> String {
    let w = worldspace_key.to_ascii_lowercase();
    format!("textures\\terrain\\{w}\\{w}.{level}.{qx}.{qy}.dds")
}

/// Per-quad **tangent-space** normal map for a distant-terrain `.btr`: the
/// diffuse path's stem plus the `_n` suffix
/// (`textures\terrain\<world>\<world>.<level>.<x>.<y>_n.dds`).
///
/// Skyrim ships one of these for every `.btr` at every level — measured on
/// `Skyrim - Textures7.bsa` (2026-08-12): 4608 / 1152 / 288 / 72 DDS at
/// levels 4 / 8 / 16 / 32, exactly twice the 2304 / 576 / 144 / 36 `.btr`
/// count, i.e. one diffuse + one `_n` each.
///
/// **FO4 is deliberately not covered here.** Its terrain LOD normals are
/// `_msn` (`commonwealth.32.32.0_msn.dds`) — *model*-space, not tangent
/// space. Binding those into the tangent-space normal slot would shade
/// worse than no normal map at all. Routing them correctly needs the
/// `MAT_FLAG_MODEL_SPACE_NORMALS` bit, which reaches the shader only through
/// a `Material` component, and LOD entities carry none — that is #2444
/// (MAT-D3-02). Until it lands, FO4 distant terrain keeps the mesh's own
/// per-vertex normals.
pub(crate) fn btr_normal_path(worldspace_key: &str, level: i32, qx: i32, qy: i32) -> String {
    let w = worldspace_key.to_ascii_lowercase();
    format!("textures\\terrain\\{w}\\{w}.{level}.{qx}.{qy}_n.dds")
}

/// Map a `.btr`'s quad-local Y-up vertex into world space for the level-`level`
/// quad whose SW-corner cell is `(qx, qy)`. The mesh is normalized to a unit
/// 4096-BU square, so the horizontal footprint scales by `level` and offsets
/// to the quad's SW world corner; the height passes through unscaled. Pure +
/// unit-tested because the convention is non-obvious (verified against vanilla
/// Skyrim Tamriel `.btr`; module docs).
fn btr_local_to_world(local: [f32; 3], level: i32, qx: i32, qy: i32) -> [f32; 3] {
    let lvl = level as f32;
    let ox = qx as f32 * EXTERIOR_CELL_UNITS;
    let oz = qy as f32 * EXTERIOR_CELL_UNITS;
    [ox + local[0] * lvl, local[1], local[2] * lvl - oz]
}

/// Resolve + import + spawn one quad's prebaked `.btr` distant terrain.
///
/// Returns `None` when the quad has no baked `.btr` (the common case — most
/// quads only have synth terrain), the NIF fails to parse, or it yields no
/// geometry; the caller falls back to heightmap synth. On success the `.btr`
/// imports to one (occasionally several) sub-meshes which are **baked into a
/// single world-space mesh** — terrain LOD is a single decimated surface, so
/// one [`MeshHandle`] keeps the [`LodBlock`] tracking (which is `Copy`,
/// single-mesh) unchanged. The block spawns as an [`IsLodTerrain`] entity
/// (no BLAS, lean static draw, never enters the TLAS), exactly like the
/// synth path and object LOD.
///
/// `level` is the quad edge in cells — 4 for the closest band (see
/// [`super::terrain_lod::LOD_BLOCK_CELLS`]), or 8 / 16 / 32 for the coarse
/// bands; the placement math below is level-generic. `(qx, qy)` is the
/// quad's SW-corner cell. `hole_mask` is stored verbatim on the returned
/// block (the caller only invokes this for `hole_mask == 0` blocks, so it is
/// always 0 — the regen trigger that switches a finest-band block to synth
/// when the player nears).
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_btr_block(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    worldspace_key: &str,
    level: i32,
    qx: i32,
    qy: i32,
    hole_mask: u16,
) -> Option<LodBlock> {
    let path = btr_archive_path(worldspace_key, level, qx, qy);
    let bytes = tex_provider.extract_mesh(&path)?;
    let scene = match byroredux_nif::parse_nif(&bytes) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("Distant-terrain '{}' parse failed: {}", path, e);
            return None;
        }
    };
    // Local pool — we consume only geometry + transforms; the diffuse path is
    // deterministic (mirrors `object_lod`'s atlas resolution).
    let mut pool = byroredux_core::string::StringPool::new();
    let imported = byroredux_nif::import::import_nif_scene(&scene, &mut pool);
    if imported.meshes.is_empty() {
        return None;
    }

    ctx.allocator.as_ref()?;

    // Place the normalized quad-local mesh into world space and merge any
    // sub-meshes into one buffer (terrain LOD is a single surface → one mesh
    // handle keeps `LodBlock` `Copy`/single-mesh). `.btr` positions are
    // quad-local with identity transform (see module docs): scale the
    // horizontal footprint by `level`, offset to the quad's SW world corner,
    // leave heights absolute. The mesh's own translation/rotation/scale are
    // identity for `.btr` and deliberately ignored.
    let lvl = level as f32;
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    for mesh in &imported.meshes {
        if mesh.positions.is_empty() || mesh.indices.is_empty() {
            continue;
        }
        let base = vertices.len() as u32;
        for i in 0..mesh.positions.len() {
            let wp = btr_local_to_world(mesh.positions[i], level, qx, qy);
            // Normal under the anisotropic XZ scale (S = diag(level, 1,
            // level)): inverse-transpose = diag(1/level, 1, 1/level).
            let ln = mesh.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
            let wn = Vec3::new(ln[0] / lvl, ln[1], ln[2] / lvl).normalize_or_zero();
            let color = mesh.colors.get(i).copied().unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let uv = mesh.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
            let mut v = Vertex::new_rgba(wp, color, [wn.x, wn.y, wn.z], uv);
            if let Some(tg) = mesh.tangents.get(i) {
                // Tangent is a surface direction → scales like positions (XZ
                // ×level), renormalized; the bitangent sign is preserved.
                let wt = Vec3::new(tg[0] * lvl, tg[1], tg[2] * lvl).normalize_or_zero();
                v.tangent = [wt.x, wt.y, wt.z, tg[3]];
            }
            vertices.push(v);
        }
        for &idx in &mesh.indices {
            indices.push(base + idx);
        }
    }
    if vertices.is_empty() || indices.is_empty() {
        return None;
    }

    // Per-quad diffuse. A missing texture falls back to the same dirt base the
    // synth path uses (still reads as ground), never the magenta checker.
    let diffuse = btr_diffuse_path(worldspace_key, level, qx, qy);
    let tex_handle = resolve_texture(ctx, tex_provider, Some(diffuse.as_str()));
    let tex_handle = if tex_handle == ctx.texture_registry.fallback() {
        resolve_texture(ctx, tex_provider, Some("textures\\landscape\\dirt02.dds"))
    } else {
        tex_handle
    };

    // Per-quad tangent-space normal map (#2371). Absent for FO4 (which bakes
    // `_msn` model-space normals instead — see [`btr_normal_path`]) and for
    // any quad whose sibling is missing; the entity then simply carries no
    // normal map and shades from the mesh's own per-vertex normals, exactly
    // as before. The fallback handle is treated as "absent" rather than bound
    // so distant terrain never samples the checker as a normal map.
    // (`resolve_texture` returns the fallback handle *without* acquiring a
    // reference, so a miss must be discarded by value, never `drop_texture`d
    // — same contract `object_lod` relies on for its atlas.)
    let normal_path = btr_normal_path(worldspace_key, level, qx, qy);
    let resolved_normal = resolve_texture(ctx, tex_provider, Some(normal_path.as_str()));
    let normal_handle = if resolved_normal == ctx.texture_registry.fallback() {
        0
    } else {
        resolved_normal
    };

    // World-space bound over the baked (already world-space) vertices.
    let bound = world_bound(&vertices);

    // Lean global-only upload (no per-mesh buffers / no BLAS) — same path the
    // synth blocks + object LOD use; LOD geometry rasterizes from the global
    // SSBO and never enters the TLAS.
    let mesh_handle = match ctx
        .mesh_registry
        .upload_scene_mesh_global_only(&vertices, &indices)
    {
        Ok(h) => h,
        Err(e) => {
            log::warn!("Distant-terrain '{}' upload failed: {}", path, e);
            // Release the refs the two resolves above acquired, or a failed
            // upload pins their VkImages + bindless slots for the session
            // (the #1537 leak shape).
            if tex_handle != 0 && tex_handle != ctx.texture_registry.fallback() {
                ctx.texture_registry.drop_texture(&ctx.device, tex_handle);
            }
            if normal_handle != 0 {
                ctx.texture_registry
                    .drop_texture(&ctx.device, normal_handle);
            }
            return None;
        }
    };

    let entity = world.spawn();
    // Positions are already baked to world space → identity transform.
    world.insert(entity, Transform::IDENTITY);
    world.insert(entity, GlobalTransform::IDENTITY);
    world.insert(entity, MeshHandle(mesh_handle));
    if tex_handle != 0 {
        world.insert(entity, TextureHandle(tex_handle));
    }
    // #2371 — the authored per-quad normal map. `MaterialTextureHandles` is
    // the same component the static-mesh pass reads for every NIF material,
    // so distant terrain picks up real surface detail through the existing
    // path rather than a LOD-specific one. Only inserted when a real map
    // resolved; every other role stays at 0 ("absent").
    if normal_handle != 0 {
        world.insert(
            entity,
            MaterialTextureHandles {
                textures: MaterialTextureSet {
                    normal: normal_handle,
                    ..Default::default()
                },
                // Terrain LOD normals carry no legacy gloss channel, and the
                // quads have no authored parallax — keep the shader's
                // defaults (see `components::MaterialTextureHandles`).
                normal_has_alpha: false,
                parallax_height_scale: 0.04,
                parallax_max_passes: 4.0,
            },
        );
    }

    world.insert(entity, bound);
    world.insert(entity, RenderLayer::Architecture);
    world.insert(entity, IsLodTerrain);

    Some(LodBlock {
        entity,
        mesh_handle,
        texture_handle: tex_handle,
        normal_texture_handle: normal_handle,
        hole_mask,
    })
}

/// World-space bounding sphere over already-world-space vertices. Falls back
/// to [`WorldBound::ZERO`] for a degenerate (non-finite) set — the caller has
/// already rejected empty meshes.
fn world_bound(vertices: &[Vertex]) -> WorldBound {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for v in vertices {
        let p = Vec3::new(v.position[0], v.position[1], v.position[2]);
        min = min.min(p);
        max = max.max(p);
    }
    if !min.is_finite() {
        return WorldBound::ZERO;
    }
    let center = (min + max) * 0.5;
    let radius = (max - center).length();
    WorldBound::new(center, radius)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell_loader::lod_support::quad_origin;

    #[test]
    fn btr_path_is_level_first_under_world_folder() {
        // Level-first naming with the quad's SW-corner cell (EXAL Q2), under
        // `meshes\terrain\<world>\` (terrain `.btr` sits beside the
        // `\objects\` subfolder that holds object `.bto`).
        assert_eq!(
            btr_archive_path("Tamriel", 4, 88, 8),
            "meshes\\terrain\\tamriel\\tamriel.4.88.8.btr"
        );
        assert_eq!(
            btr_archive_path("tamriel", 4, -8, -16),
            "meshes\\terrain\\tamriel\\tamriel.4.-8.-16.btr"
        );
        assert_eq!(
            btr_archive_path("DLC2SolstheimWorld", 8, 0, 8),
            "meshes\\terrain\\dlc2solstheimworld\\dlc2solstheimworld.8.0.8.btr"
        );
    }

    /// #2586 — real DLC worldspaces do not share Tamriel's zero-aligned grid.
    /// These filenames are present verbatim in Skyrim - Meshes1.bsa.
    #[test]
    fn btr_paths_follow_nonzero_worldspace_origins() {
        let apocrypha = quad_origin(-49, -49, 4, (-50, -50));
        assert_eq!(apocrypha, (-50, -50));
        assert_eq!(
            btr_archive_path("DLC2ApocryphaWorld", 4, apocrypha.0, apocrypha.1),
            "meshes\\terrain\\dlc2apocryphaworld\\dlc2apocryphaworld.4.-50.-50.btr"
        );

        let soul_cairn = quad_origin(-50, -49, 4, (-52, -51));
        assert_eq!(soul_cairn, (-52, -51));
        assert_eq!(
            btr_archive_path("DLC01SoulCairn", 4, soul_cairn.0, soul_cairn.1),
            "meshes\\terrain\\dlc01soulcairn\\dlc01soulcairn.4.-52.-51.btr"
        );
    }

    #[test]
    fn btr_diffuse_is_sibling_of_mesh() {
        assert_eq!(
            btr_diffuse_path("Tamriel", 4, 88, 8),
            "textures\\terrain\\tamriel\\tamriel.4.88.8.dds"
        );
        assert_eq!(
            btr_diffuse_path("tamriel", 16, 0, 0),
            "textures\\terrain\\tamriel\\tamriel.16.0.0.dds"
        );
    }

    /// #2371 — the normal map is the diffuse stem plus `_n`, at every band.
    /// These paths are present verbatim in `Skyrim - Textures7.bsa`
    /// (2026-08-12), which ships one `_n` per `.btr` at all four levels.
    #[test]
    fn btr_normal_is_the_diffuse_stem_plus_n_suffix() {
        assert_eq!(
            btr_normal_path("Tamriel", 4, -44, -56),
            "textures\\terrain\\tamriel\\tamriel.4.-44.-56_n.dds"
        );
        assert_eq!(
            btr_normal_path("tamriel", 16, -16, -80),
            "textures\\terrain\\tamriel\\tamriel.16.-16.-80_n.dds"
        );
        assert_eq!(
            btr_normal_path("Tamriel", 32, 32, -96),
            "textures\\terrain\\tamriel\\tamriel.32.32.-96_n.dds"
        );

        // It is exactly the diffuse path with `_n` before the extension —
        // the two must not drift apart.
        for level in [4, 8, 16, 32] {
            let diffuse = btr_diffuse_path("Tamriel", level, 8, -4);
            let normal = btr_normal_path("Tamriel", level, 8, -4);
            assert_eq!(normal, diffuse.replace(".dds", "_n.dds"));
        }
    }

    #[test]
    fn btr_local_to_world_scales_and_offsets_to_quad() {
        // Quad `tamriel.4.0.-4` (SW corner cell (0,-4), level 4). The probed
        // local footprint is X∈[0,4096], Z∈[-4096,0]. It must place into the
        // 4×4-cell world region cells [0,4)×[-4,0): world X∈[0,16384],
        // world Z∈[0,16384] (Z = −world_y_zup, cells [-4,0) → Z [0,16384]).
        let cell = EXTERIOR_CELL_UNITS; // 4096
                                        // SW local corner (0,0,0) → world (0, _, 16384).
        let sw = btr_local_to_world([0.0, 10.0, 0.0], 4, 0, -4);
        assert_eq!(sw, [0.0, 10.0, 4.0 * cell]);
        // Opposite local corner (4096, _, -4096) → world (16384, _, 0).
        let ne = btr_local_to_world([cell, 20.0, -cell], 4, 0, -4);
        assert_eq!(ne, [4.0 * cell, 20.0, 0.0]);
        // Height passes through unscaled.
        assert_eq!(sw[1], 10.0);
        // Adjacent quad `4.4.-4` (cells [4,8)×[-4,0)) tiles seamlessly: its SW
        // corner sits exactly where quad `4.0.-4`'s east edge ended.
        let adj_sw = btr_local_to_world([0.0, 0.0, 0.0], 4, 4, -4);
        assert_eq!(adj_sw[0], 4.0 * cell); // = ne[0], no gap/overlap
    }

    #[test]
    fn world_bound_spans_vertices() {
        let v =
            |x: f32, y: f32, z: f32| Vertex::new([x, y, z], [1.0; 3], [0.0, 1.0, 0.0], [0.0; 2]);
        let verts = vec![v(0.0, 0.0, 0.0), v(2.0, 0.0, 0.0), v(0.0, 0.0, 2.0)];
        let b = world_bound(&verts);
        assert_eq!(b.center, Vec3::new(1.0, 0.0, 1.0));
        // Half-diagonal of the [0,2]×{0}×[0,2] box from its centre.
        assert!((b.radius - 2f32.sqrt()).abs() < 1e-4);
    }

    #[test]
    fn world_bound_empty_is_zero() {
        assert_eq!(world_bound(&[]).center, WorldBound::ZERO.center);
    }
}
