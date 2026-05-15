//! Cell-load water-plane spawn.
//!
//! Translates a cell's `XCLW` (water height) + `XCWT` (water-type
//! WATR FormID, Skyrim+) into a `WaterPlane` ECS entity backed by a
//! flat quad mesh and a derived [`WaterMaterial`].
//!
//! Coordinate convention matches the rest of the cell loader: the
//! engine renders in Y-up; Bethesda's Z-up `water_height` therefore
//! becomes the Y coordinate of the water plane.
//!
//! Scope (initial cut):
//!
//! - One `WaterPlane` per cell — interior cells get a centred quad
//!   sized to their reference bounds; exterior cells get a single
//!   per-tile quad spanning the cell's 4096×4096 grid square (so
//!   neighbouring tiles tile seamlessly via the world-XZ UV in
//!   `water.frag`).
//! - `WaterKind` heuristic: defaults to `Calm` for every spawn.
//!   Rivers / rapids / waterfalls land in a follow-up pass once we
//!   parse the WATR record's EDID naming convention + cell flow
//!   metadata (Skyrim has REFR XCWT overrides + `RiverWater` named
//!   WATR records; FNV uses NAM2-suffixed names).
//! - TLAS exclusion: water planes are spawned with no `in_tlas`
//!   semantics because the renderer skips this entity in the regular
//!   draw path (`DrawCommand.is_water == true`), and the water
//!   pipeline doesn't read its own surface from TLAS.
//!
//! Returns the number of water-plane entities spawned (0 or 1 today).

use byroredux_core::ecs::components::water::{
    SubmersionState, WaterFlow, WaterKind, WaterMaterial, WaterPlane, WaterVolume,
};
use byroredux_core::ecs::{GlobalTransform, MeshHandle, Transform, World};
use byroredux_plugin::esm;
use byroredux_core::math::{Quat, Vec3};
use byroredux_renderer::{Vertex, VulkanContext};
use std::collections::HashMap;

use crate::asset_provider::{resolve_texture, TextureProvider};

/// World extent of one exterior cell tile in Bethesda units. Matches
/// the cell-loader-wide constant (see `cell_loader_terrain.rs`).
const CELL_SIZE: f32 = 4096.0;

/// Default interior water-plane half-extent in Bethesda units when
/// the cell loader has not yet computed the cell's reference bounds.
///
/// Was 1024 before the first live-engine smoke test surfaced the
/// "everything underwater" failure mode: a 1024-unit plane centred
/// at world origin covered every camera-reachable position in a
/// typical interior, and any cell with a non-zero XCLW height
/// (sewer, flooded ruin, pool) flagged the camera as submerged
/// even when standing on dry floor. 256 wu is the typical
/// Bethesda interior-pool diameter — tighter than the worst case,
/// but the right side of the trade-off until WorldBound
/// aggregation lands.
const DEFAULT_INTERIOR_HALF_EXTENT: f32 = 256.0;

/// Default interior water-volume depth below the surface, in
/// Bethesda units. Most interior pools / baths / sewer channels are
/// well under 200 wu deep; the pre-fix value (4096) was a copy of
/// the exterior open-ocean default and was the second contributor
/// to the spurious-submerged regression — the volume column reached
/// far enough down to engulf any camera that happened to share the
/// interior plane's XZ extent.
const DEFAULT_INTERIOR_VOLUME_DEPTH: f32 = 200.0;

/// Spawn one water-plane entity for the given cell.
///
/// `xclw_height` is the cell's parsed `water_height` in Bethesda
/// Z-up. `xcwt_form` is the optional `water_type_form` FormID into
/// the parsed `WATR` records table. `cell_origin_world_xz` is the
/// already-converted Y-up origin of the cell (X, Z components) —
/// for interior cells, this is `(0, 0)`; for exterior cells, the
/// renderer's grid translation.
///
/// Returns `Some(1)` on a successful spawn, `None` when mesh upload
/// fails (matches the terrain helper's signature).
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_water_plane(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    waters: &HashMap<u32, esm::records::misc::WatrRecord>,
    xclw_height: f32,
    xcwt_form: Option<u32>,
    cell_origin_world_xz: (f32, f32),
    half_extent: f32,
    blas_specs: &mut Vec<(u32, u32, u32)>,
) -> Option<usize> {
    // ── Resolve WATR → engine WaterMaterial ──
    let (material, kind, flow, normal_texture_path) = resolve_water_material(waters, xcwt_form);

    let allocator = ctx.allocator.as_ref()?;

    // ── Build the flat quad mesh ──
    // Local-space mesh: 4 verts on Y=0, square covering [-1, 1] × [-1, 1].
    // The entity's Transform places the quad at world Y = xclw_height
    // and scales it by `half_extent`. Normal map UV scrolls in world
    // space, so the mesh UVs don't matter visually — they're set to
    // [-half_extent, half_extent] so the perturbed-normal blend in
    // the fragment shader has a consistent UV-derivative magnitude
    // across the plane.
    let uv = half_extent;
    let vertices = vec![
        // Position is local space; the model matrix scales by
        // half_extent on X/Z. UV mirrors local position so world-
        // space UV derivatives behave.
        Vertex {
            position: [-1.0, 0.0, -1.0],
            color: [1.0, 1.0, 1.0],
            normal: [0.0, 1.0, 0.0],
            uv: [-uv, -uv],
            bone_indices: [0, 0, 0, 0],
            bone_weights: [0.0, 0.0, 0.0, 0.0],
            splat_weights_0: [0, 0, 0, 0],
            splat_weights_1: [0, 0, 0, 0],
            // World +X tangent — water.frag re-orthogonalises against
            // the world normal, so any non-degenerate tangent works.
            tangent: [1.0, 0.0, 0.0, 1.0],
        },
        Vertex {
            position: [1.0, 0.0, -1.0],
            color: [1.0, 1.0, 1.0],
            normal: [0.0, 1.0, 0.0],
            uv: [uv, -uv],
            bone_indices: [0, 0, 0, 0],
            bone_weights: [0.0, 0.0, 0.0, 0.0],
            splat_weights_0: [0, 0, 0, 0],
            splat_weights_1: [0, 0, 0, 0],
            tangent: [1.0, 0.0, 0.0, 1.0],
        },
        Vertex {
            position: [-1.0, 0.0, 1.0],
            color: [1.0, 1.0, 1.0],
            normal: [0.0, 1.0, 0.0],
            uv: [-uv, uv],
            bone_indices: [0, 0, 0, 0],
            bone_weights: [0.0, 0.0, 0.0, 0.0],
            splat_weights_0: [0, 0, 0, 0],
            splat_weights_1: [0, 0, 0, 0],
            tangent: [1.0, 0.0, 0.0, 1.0],
        },
        Vertex {
            position: [1.0, 0.0, 1.0],
            color: [1.0, 1.0, 1.0],
            normal: [0.0, 1.0, 0.0],
            uv: [uv, uv],
            bone_indices: [0, 0, 0, 0],
            bone_weights: [0.0, 0.0, 0.0, 0.0],
            splat_weights_0: [0, 0, 0, 0],
            splat_weights_1: [0, 0, 0, 0],
            tangent: [1.0, 0.0, 0.0, 1.0],
        },
    ];
    // Two triangles, CCW after the engine's Z→Y up swizzle would
    // negate winding — emit CW so it becomes CCW post-conversion.
    // For this mesh we don't apply the negate (we author already in
    // Y-up local), so emit CCW directly.
    let indices = vec![0u32, 2, 1, 1, 2, 3];

    let mesh_handle = match ctx.mesh_registry.upload_scene_mesh(
        &ctx.device,
        allocator,
        &ctx.graphics_queue,
        ctx.transfer_pool,
        &vertices,
        &indices,
        // Water meshes do NOT need BLAS — they're skipped from TLAS
        // (water-on-water self-hits are avoided by the CP2077-style
        // terminate-on-hit policy on water rays).
        false,
        None,
    ) {
        Ok(h) => h,
        Err(e) => {
            log::warn!("Water plane mesh upload failed: {e}");
            return None;
        }
    };
    // Suppress unused warning when ray tracing is on — water never
    // adds a BLAS entry, but other spawn helpers in the same call
    // chain accumulate into the same vec.
    let _ = blas_specs;

    // Texture resolve — the water material's normal_map_index points
    // here. When the WATR record's TNAM is unset (e.g., default
    // interior water with no XCWT), fall back to the canonical
    // engine water normal map path.
    let resolved_normal_idx = if let Some(path) = normal_texture_path {
        resolve_texture(ctx, tex_provider, Some(path.as_str()))
    } else {
        // Empty path → resolve_texture returns 0 (placeholder), which
        // the shader interprets as `u32::MAX` via floatBitsToUint —
        // *but* we want the procedural fallback in that case. Encode
        // u32::MAX directly into the material instead of letting it
        // pass through the texture registry.
        0
    };

    let mut material = material;
    if resolved_normal_idx != 0 {
        material.normal_map_index = resolved_normal_idx;
    } // else material.normal_map_index stays at u32::MAX (default — triggers shader procedural)

    // ── Spawn the entity ──
    let position = Vec3::new(
        cell_origin_world_xz.0,
        xclw_height,
        // Bethesda Z-up → Y-up: world_y → −Z. `cell_origin_world_xz.1`
        // is already pre-converted by the caller, so we use it as-is
        // (callers from interior path pass `(0, 0)`; exterior callers
        // pass `(grid_x * 4096, grid_y * 4096 * -1)` already swizzled).
        cell_origin_world_xz.1,
    );
    let scale = half_extent;

    let entity = world.spawn();
    world.insert(
        entity,
        Transform::new(position, Quat::IDENTITY, scale),
    );
    world.insert(
        entity,
        GlobalTransform::new(position, Quat::IDENTITY, scale),
    );
    world.insert(entity, MeshHandle(mesh_handle));
    world.insert(entity, WaterPlane { kind, material });
    if let Some(flow) = flow {
        world.insert(entity, flow);
    }
    // Volume extends from the surface down to a per-mode floor.
    // Interior planes get a tight 200-wu column (typical pool
    // depth); exterior planes get the full 4096-wu column so deep
    // ocean cells remain detectable. The exterior heuristic is
    // "half-extent > 1024 = exterior" — captures the spawn caller
    // contract without an explicit flag.
    let volume_depth = if half_extent > 1024.0 {
        4096.0
    } else {
        DEFAULT_INTERIOR_VOLUME_DEPTH
    };
    let volume_floor_y = xclw_height - volume_depth;
    world.insert(
        entity,
        WaterVolume {
            min: [
                position.x - half_extent,
                volume_floor_y,
                position.z - half_extent,
            ],
            max: [
                position.x + half_extent,
                xclw_height,
                position.z + half_extent,
            ],
        },
    );

    // RenderLayer::Decal pushes water onto a slightly biased depth
    // ladder so it stays above coincident architectural geometry
    // (lake floor mesh, river bed) without z-fighting. The engine-
    // wide depth-bias ladder treats `Decal` as a soft over-bias.
    world.insert(
        entity,
        byroredux_core::ecs::components::RenderLayer::Decal,
    );

    log::debug!(
        "Water plane spawned: pos={:?}, half_extent={}, kind={:?}, normalIdx={}",
        position,
        half_extent,
        kind,
        material.normal_map_index
    );

    Some(1)
}

/// Resolve a cell's `XCWT` FormID to an engine [`WaterMaterial`]
/// plus a [`WaterKind`] (currently always `Calm`) plus an optional
/// [`WaterFlow`] and an optional normal-texture path the cell loader
/// should attempt to bind.
///
/// `xcwt_form == None` (no WATR reference on the cell) falls back to
/// engine defaults — same shape Skyrim uses for unmodded cells that
/// rely on the worldspace water-default cascade.
fn resolve_water_material(
    waters: &HashMap<u32, esm::records::misc::WatrRecord>,
    xcwt_form: Option<u32>,
) -> (WaterMaterial, WaterKind, Option<WaterFlow>, Option<String>) {
    let mut mat = WaterMaterial::default();
    let mut kind = WaterKind::Calm;
    let mut flow: Option<WaterFlow> = None;
    let mut normal_path: Option<String> = None;

    if let Some(form) = xcwt_form {
        if let Some(rec) = waters.get(&form) {
            mat.shallow_color = rec.params.shallow_color;
            mat.deep_color = rec.params.deep_color;
            mat.fog_near = rec.params.fog_near;
            mat.fog_far = rec.params.fog_far;
            mat.fresnel_f0 = rec.params.fresnel.clamp(0.001, 0.20);
            mat.reflectivity = rec.params.reflectivity;
            mat.reflection_tint = rec.params.reflection_color;
            mat.source_form = rec.form_id;

            // ── WaterKind heuristic from EDID naming convention ──
            //
            // Cell-level water planes are **always horizontal**
            // (XCLW provides a Y height; the mesh is a flat quad).
            // The `Waterfall` kind in the shader is for vertical
            // sheet geometry (cliff-side falling water), which the
            // cell loader does NOT spawn — those land as standalone
            // mesh refs through the regular NIF import path. So
            // any EDID match that would otherwise promote a cell
            // plane to `Waterfall` is demoted to `River` here: the
            // horizontal plane below a waterfall is a fast,
            // turbulent pool, not a falling sheet, and the River
            // shader path is the correct visual.
            //
            // Skyrim has many WATR records whose names contain
            // "fall"/"waterfall" but are applied to horizontal
            // bodies of water (e.g. `DLC2WaterFallingStream`,
            // `WaterFallingPool`, `WaterRiverFallingSlow`). The
            // pre-fix heuristic mis-classified these and the
            // shader's Waterfall mode painted heavy fizz foam
            // across whole exterior cells — see the May 2026
            // smoke-test screenshot reported alongside this
            // change.
            let lowered = rec.editor_id.to_ascii_lowercase();
            if lowered.contains("rapid") {
                kind = WaterKind::Rapids;
                mat.foam_strength = 0.85;
            } else if lowered.contains("waterfall")
                || lowered.contains("falls")
                || lowered.contains("river")
                || lowered.contains("stream")
            {
                kind = WaterKind::River;
                mat.foam_strength = 0.20;
            }
            // Synthesise a flow vector from WATR's wind speed +
            // direction when the kind implies flow. Bethesda's
            // wind_direction is in radians from north (UESP).
            if !matches!(kind, WaterKind::Calm) {
                let theta = rec.params.wind_direction;
                flow = Some(WaterFlow {
                    direction: [theta.cos(), 0.0, theta.sin()],
                    speed: rec.params.wind_speed.abs().max(0.5),
                });
                // Rebuild scroll vectors to bias along the flow axis.
                let dir = (theta.cos(), theta.sin());
                let speed = rec.params.wind_speed.abs().max(0.5);
                mat.scroll_a = [dir.0 * speed * 0.5, dir.1 * speed * 0.5];
                // Perpendicular shear at half speed for the second layer.
                mat.scroll_b = [-dir.1 * speed * 0.25, dir.0 * speed * 0.25];
            }
            // TNAM is the diffuse / noise texture — used as the
            // bindless normal map for the shader. Empty path =
            // procedural fallback.
            if !rec.texture_path.is_empty() {
                normal_path = Some(rec.texture_path.clone());
            }
        }
    }

    // SubmersionState is per-actor, not per-plane — but seed a
    // sentinel value on the material itself so debug overlays can
    // see "water without a parsed XCWT" cells.
    let _ = SubmersionState::default();

    (mat, kind, flow, normal_path)
}

/// Convenience for the interior path — picks a default half-extent
/// when the cell-load step doesn't yet know the actual reference
/// bounds (most interior cells with water are small pools or
/// flooded rooms; the half-extent is generous so the plane covers
/// the whole room).
#[inline]
pub(super) fn default_interior_half_extent() -> f32 {
    DEFAULT_INTERIOR_HALF_EXTENT
}

/// Convenience for the exterior path — one exterior cell quad.
#[inline]
pub(super) fn exterior_half_extent() -> f32 {
    CELL_SIZE * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::misc::{WatrRecord, WaterParams};

    /// Regression for #1069 / F-WAT-09 — `reflection_color` parsed from
    /// WATR DATA must reach `WaterMaterial.reflection_tint` via
    /// `resolve_water_material`. Pre-fix the field was silently dropped.
    #[test]
    fn resolve_water_material_transfers_reflection_color() {
        let lava_tint = [0.85_f32, 0.30, 0.10]; // orange-red lava pool

        let rec = WatrRecord {
            form_id: 0x000A_BCDE,
            editor_id: "LavaPool01".to_string(),
            full_name: "Lava Pool".to_string(),
            texture_path: String::new(),
            noise_textures: [u32::MAX; 3],
            params: WaterParams {
                shallow_color: [1.0, 0.4, 0.1],
                deep_color: [0.6, 0.1, 0.0],
                reflection_color: lava_tint,
                fog_near: 20.0,
                fog_far: 80.0,
                reflectivity: 0.40,
                fresnel: 0.04,
                wind_speed: 0.0,
                wind_direction: 0.0,
                wave_amplitude: 0.0,
                wave_frequency: 0.0,
            },
            raw_dnam: Vec::new(),
            raw_data: Vec::new(),
        };

        let mut waters = HashMap::new();
        waters.insert(rec.form_id, rec);

        let (mat, _kind, _flow, _normal) =
            resolve_water_material(&waters, Some(0x000A_BCDE));

        assert_eq!(
            mat.reflection_tint, lava_tint,
            "reflection_tint must round-trip from WATR DATA reflection_color"
        );
    }

    /// Default WaterMaterial (no XCWT / no WATR record) uses the neutral
    /// grey that matches the pre-#1069 hard-coded shader value.
    #[test]
    fn default_water_material_has_neutral_reflection_tint() {
        let (mat, _, _, _) = resolve_water_material(&HashMap::new(), None);
        assert_eq!(
            mat.reflection_tint,
            [0.65, 0.70, 0.75],
            "default reflection_tint must match the pre-fix shader hard-code"
        );
    }
}
