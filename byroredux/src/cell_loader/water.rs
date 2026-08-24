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
//!   sized to their reference bounds; exterior cells use the LAND 33×33
//!   height field to omit fully dry triangles and bound the matching
//!   `WaterVolume`, while neighboring wet tiles still share world-XZ UVs.
//! - `WaterKind` is resolved at the canonical WATR boundary. Authored
//!   linear flow (NAM0), flow-normal textures, and the cross-generation
//!   EDID compatibility tokens classify rivers/rapids; horizontal cell
//!   planes deliberately demote waterfall names to river water. Dedicated
//!   vertical waterfall meshes use geometry classification in the NIF path.
//! - TLAS exclusion: water planes are spawned with no `in_tlas`
//!   semantics because the renderer skips this entity in the regular
//!   draw path (`DrawCommand.is_water == true`), and the water
//!   pipeline doesn't read its own surface from TLAS.
//!
//! Returns the number of water-plane entities spawned (0 or 1 today).

use byroredux_core::ecs::components::water::{WaterFlow, WaterKind, WaterPlane, WaterVolume};
use byroredux_core::ecs::components::ParticleEmitter;
use byroredux_core::ecs::components::RenderLayer;
use byroredux_core::ecs::{GlobalTransform, MeshHandle, Transform, World};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm;
use byroredux_renderer::vulkan::GpuUploadCtx;
use byroredux_renderer::{Vertex, VulkanContext};
use std::collections::HashMap;

use crate::asset_provider::{resolve_texture, TextureProvider};
use crate::components::{NormalMapHandle, WaterLodInfo, WaterNoiseMapHandles};
use crate::streaming::LodWaterPlane;
use byroredux_core::math::coord::{zup_to_yup_pos, EXTERIOR_CELL_UNITS};

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

/// Full-detail water needs enough vertices for the authored vertex waves to
/// affect the silhouette. A four-vertex quad turns the whole 256–4096 wu
/// surface into one broad saddle, so the displacement is effectively absent
/// at the shoreline. Sixteen cells per side puts a vertex every 256 world
/// units across a 4096-unit exterior cell, avoiding the visible 512-unit
/// wave facets of the former 8×8 grid while keeping the mesh bounded; the
/// fragment normal path still supplies the fine ripples.
const FULL_DETAIL_WATER_GRID_SEGMENTS: usize = 16;

/// Subdivisions per side of the distant-water annulus' outer-to-hole bands.
/// Eight keeps the single worldspace mesh small while preventing its wave
/// displacement from collapsing into four giant corner panels.
const LOD_WATER_RING_SUBDIVISIONS: usize = 8;

/// Derive a conservative horizontal placement for an interior water plane
/// from the cell's authored REFR positions. Interior references are stored in
/// Bethesda Z-up coordinates; the renderer's horizontal axes are X/Z after
/// the shared coordinate conversion, so source Y becomes renderer -Z.
///
/// The cell format does not provide a shoreline polygon. This is therefore a
/// bounded coverage estimate rather than a claim that every interior room is
/// water-filled: it fixes the much worse historical `(0, 0)` placement while
/// retaining the legacy 256-unit minimum for sparse pool cells and capping
/// pathological marker spread at one room-scale extent.
pub(super) fn interior_water_placement<I>(positions: I) -> ((f32, f32), f32)
where
    I: IntoIterator<Item = [f32; 3]>,
{
    const MIN_HALF_EXTENT: f32 = DEFAULT_INTERIOR_HALF_EXTENT;
    const MAX_HALF_EXTENT: f32 = 2048.0;
    const EDGE_MARGIN: f32 = 32.0;

    let mut min_x = f32::INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    for p in positions {
        if !p.iter().all(|v| v.is_finite()) {
            continue;
        }
        let converted = zup_to_yup_pos(p);
        min_x = min_x.min(converted[0]);
        min_z = min_z.min(converted[2]);
        max_x = max_x.max(converted[0]);
        max_z = max_z.max(converted[2]);
    }

    if !min_x.is_finite() {
        return ((0.0, 0.0), MIN_HALF_EXTENT);
    }

    let center = ((min_x + max_x) * 0.5, (min_z + max_z) * 0.5);
    let span = (max_x - min_x).max(max_z - min_z);
    let half_extent = ((span * 0.5) + EDGE_MARGIN).clamp(MIN_HALF_EXTENT, MAX_HALF_EXTENT);
    (center, half_extent)
}

fn build_full_detail_water_grid(
    half_extent: f32,
    terrain: Option<&esm::cell::LandscapeData>,
    water_height_zup: f32,
) -> (Vec<Vertex>, Vec<u32>) {
    let segments = FULL_DETAIL_WATER_GRID_SEGMENTS;
    let side = segments + 1;
    let vertex = |x: f32, z: f32| Vertex {
        position: [x, 0.0, z],
        color: [1.0, 1.0, 1.0, 1.0],
        normal: [0.0, 1.0, 0.0],
        // UVs are world-distance based so adjacent cells keep the same
        // normal-map derivative and wave phase.
        uv: [x * half_extent, z * half_extent],
        bone_indices: [0, 0, 0, 0],
        bone_weights: [0.0, 0.0, 0.0, 0.0],
        splat_weights_0: [0, 0, 0, 0],
        splat_weights_1: [0, 0, 0, 0],
        tangent: [1.0, 0.0, 0.0, -1.0],
    };

    // Keep the regular shared grid for interiors and for callers without a
    // valid LAND height field. Exterior shoreline cells use the clipped path
    // below, where each surviving polygon gets its own vertices.
    let mut vertices = if terrain.is_none() {
        let mut grid = Vec::with_capacity(side * side);
        for row in 0..=segments {
            let z = row as f32 / segments as f32 * 2.0 - 1.0;
            for col in 0..=segments {
                let x = col as f32 / segments as f32 * 2.0 - 1.0;
                grid.push(vertex(x, z));
            }
        }
        grid
    } else {
        Vec::with_capacity(segments * segments * 4)
    };
    let mut indices = Vec::with_capacity(segments * segments * 6);
    for row in 0..segments {
        for col in 0..segments {
            let Some(land) = terrain else {
                let top_left = (row * side + col) as u32;
                let top_right = top_left + 1;
                let bottom_left = top_left + side as u32;
                let bottom_right = bottom_left + 1;
                indices.extend_from_slice(&[
                    top_left,
                    bottom_left,
                    top_right,
                    top_right,
                    bottom_left,
                    bottom_right,
                ]);
                continue;
            };

            const LAND_SEGMENTS: usize = 32;
            let sample = |r: usize, c: usize| {
                land.heights.get(
                    (r * LAND_SEGMENTS / segments) * (LAND_SEGMENTS + 1)
                        + c * LAND_SEGMENTS / segments,
                )
            };
            let x0 = col as f32 / segments as f32 * 2.0 - 1.0;
            let x1 = (col + 1) as f32 / segments as f32 * 2.0 - 1.0;
            let z0 = row as f32 / segments as f32 * 2.0 - 1.0;
            let z1 = (row + 1) as f32 / segments as f32 * 2.0 - 1.0;
            // Polygon order matches the original +Y winding: top-left,
            // bottom-left, bottom-right, top-right.
            let corners = [
                ([x0, z0], sample(row, col)),
                ([x0, z1], sample(row + 1, col)),
                ([x1, z1], sample(row + 1, col + 1)),
                ([x1, z0], sample(row, col + 1)),
            ];
            let Some(heights) = corners
                .iter()
                .map(|(_, height)| height.copied())
                .collect::<Option<Vec<f32>>>()
            else {
                // A malformed/incomplete LAND payload should not erase a
                // water surface. Fall back to the conservative full quad.
                let base = vertices.len() as u32;
                for (point, _) in corners {
                    vertices.push(vertex(point[0], point[1]));
                }
                indices.extend_from_slice(&[
                    base,
                    base + 1,
                    base + 3,
                    base + 1,
                    base + 2,
                    base + 3,
                ]);
                continue;
            };
            let mut polygon = Vec::with_capacity(6);
            for edge in 0..4 {
                let next = (edge + 1) % 4;
                let a = corners[edge].0;
                let b = corners[next].0;
                let ha = heights[edge];
                let hb = heights[next];
                let a_wet = ha <= water_height_zup;
                let b_wet = hb <= water_height_zup;
                if a_wet {
                    polygon.push(a);
                }
                if a_wet != b_wet {
                    let denom = hb - ha;
                    let t = if denom.abs() > f32::EPSILON {
                        ((water_height_zup - ha) / denom).clamp(0.0, 1.0)
                    } else {
                        0.5
                    };
                    polygon.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
                }
            }
            if polygon.len() < 3 {
                continue;
            }
            let base = vertices.len() as u32;
            for point in polygon.iter().copied() {
                vertices.push(vertex(point[0], point[1]));
            }
            for i in 1..polygon.len() - 1 {
                indices.extend_from_slice(&[base, base + i as u32, base + i as u32 + 1]);
            }
        }
    }
    (vertices, indices)
}

/// Return connected normalized X/Z footprints of LAND cells intersecting XCLW.
/// The components feed gameplay `WaterVolume`s as well as the render mask,
/// preventing an inherited worldspace sea level from making dry terrain
/// between disconnected ponds buoyant. `None` means no below-surface cells.
fn terrain_water_components(
    land: &esm::cell::LandscapeData,
    water_height_zup: f32,
) -> Option<Vec<(f32, f32, f32, f32)>> {
    const WATER_SEGMENTS: usize = FULL_DETAIL_WATER_GRID_SEGMENTS;
    const LAND_SEGMENTS: usize = 32;
    let sample = |r: usize, c: usize| {
        land.heights
            .get(
                (r * LAND_SEGMENTS / WATER_SEGMENTS) * (LAND_SEGMENTS + 1)
                    + c * LAND_SEGMENTS / WATER_SEGMENTS,
            )
            .copied()
    };
    let mut wet = vec![false; WATER_SEGMENTS * WATER_SEGMENTS];
    for row in 0..WATER_SEGMENTS {
        for col in 0..WATER_SEGMENTS {
            wet[row * WATER_SEGMENTS + col] = [
                sample(row, col),
                sample(row, col + 1),
                sample(row + 1, col),
                sample(row + 1, col + 1),
            ]
            .into_iter()
            .flatten()
            .any(|height| height <= water_height_zup);
        }
    }
    if !wet.iter().any(|is_wet| *is_wet) {
        return None;
    }

    let mut components = Vec::new();
    for seed in 0..wet.len() {
        if !wet[seed] {
            continue;
        }
        wet[seed] = false;
        let mut stack = vec![seed];
        let mut min_col = seed % WATER_SEGMENTS;
        let mut max_col = min_col;
        let mut min_row = seed / WATER_SEGMENTS;
        let mut max_row = min_row;
        while let Some(index) = stack.pop() {
            let row = index / WATER_SEGMENTS;
            let col = index % WATER_SEGMENTS;
            min_col = min_col.min(col);
            max_col = max_col.max(col);
            min_row = min_row.min(row);
            max_row = max_row.max(row);
            for (next_row, next_col) in [
                (row.wrapping_sub(1), col),
                (row + 1, col),
                (row, col.wrapping_sub(1)),
                (row, col + 1),
            ] {
                if next_row >= WATER_SEGMENTS || next_col >= WATER_SEGMENTS {
                    continue;
                }
                let next = next_row * WATER_SEGMENTS + next_col;
                if wet[next] {
                    wet[next] = false;
                    stack.push(next);
                }
            }
        }
        components.push((
            min_col as f32 / WATER_SEGMENTS as f32 * 2.0 - 1.0,
            (max_col + 1) as f32 / WATER_SEGMENTS as f32 * 2.0 - 1.0,
            min_row as f32 / WATER_SEGMENTS as f32 * 2.0 - 1.0,
            (max_row + 1) as f32 / WATER_SEGMENTS as f32 * 2.0 - 1.0,
        ));
    }
    Some(components)
}

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
    cell_water_velocity: Option<[f32; 3]>,
    cell_origin_world_xz: (f32, f32),
    half_extent: f32,
    terrain: Option<&esm::cell::LandscapeData>,
) -> Option<usize> {
    // ── Resolve WATR → engine WaterMaterial (EXAL boundary) ──
    let (mut material, mut kind, mut flow, normal_texture_path, noise_texture_paths) =
        crate::env_translate::resolve_water_material(waters, xcwt_form);
    // CELL.XWCU is a local current vector and takes precedence over the
    // WATR-level synthesized current for this plane. Convert Gamebryo Z-up
    // horizontal axes (X/Y) into renderer Y-up (X/Z) once at this boundary;
    // cell planes are horizontal, so the source vertical component is not a
    // current target for this path.
    if let Some(cell_flow) = cell_water_flow(cell_water_velocity) {
        // XWCU is an authored current on the cell, so it is stronger
        // classification evidence than a neutral/localized WATR EDID. A
        // calm WATR with a non-zero cell current must take a flow shader
        // path (River or Rapids by speed); otherwise the flow reaches
        // physics and UV scroll but the renderer suppresses its aligned foam
        // response.
        kind = kind_with_cell_flow(kind, cell_flow.speed);
        material.foam_strength = kind.canonical_foam_strength();
        // Keep the authored WATR layer motion, but add the cell-local
        // current as a world-space UV bias so XWCU affects both physics and
        // the visible surface rather than only drifting debris.
        const CELL_CURRENT_UV_PER_BU_S: f32 = 0.0015;
        let scroll = cell_flow.speed * CELL_CURRENT_UV_PER_BU_S;
        let dx = cell_flow.direction[0] * scroll;
        let dz = cell_flow.direction[2] * scroll;
        material.scroll_a[0] += dx;
        material.scroll_a[1] += dz;
        material.scroll_b[0] += dx * 0.65;
        material.scroll_b[1] += dz * 0.65;
        material.scroll_c[0] += dx * 0.45;
        material.scroll_c[1] += dz * 0.45;
        flow = Some(cell_flow);
    }

    let allocator = ctx.allocator.as_ref()?;

    // ── Build the tessellated full-detail water mesh ──
    // The entity's Transform places the grid at `xclw_height` and scales it
    // by `half_extent`; tessellation lets the vertex shader's two authored
    // waves produce real near-field silhouette motion instead of only moving
    // the four corners of a cell plane.
    let (vertices, indices) = build_full_detail_water_grid(half_extent, terrain, xclw_height);
    if indices.is_empty() {
        // A cell can inherit a worldspace water height while its LAND tile is
        // entirely above that level. Keep the caller's successful-load path,
        // but do not create a render or physics surface on dry terrain.
        log::debug!("Water plane suppressed: LAND tile is entirely above water");
        return Some(0);
    }

    let upload_ctx = GpuUploadCtx {
        device: &ctx.device,
        allocator,
        queue: &ctx.graphics_queue,
        command_pool: ctx.transfer_pool,
    };
    let mesh_handle = match ctx.mesh_registry.upload_scene_mesh(
        upload_ctx, &vertices, &indices,
        // Water meshes do NOT need BLAS — they're skipped from TLAS
        // (water-on-water self-hits are avoided by the CP2077-style
        // terminate-on-hit policy on water rays).
        false, None,
    ) {
        Ok(h) => h,
        Err(e) => {
            log::warn!("Water plane mesh upload failed: {e}");
            return None;
        }
    };

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

    let mut resolved_noise = [0u32; 3];
    for (idx, path) in noise_texture_paths.into_iter().enumerate() {
        if let Some(path) = path {
            resolved_noise[idx] = resolve_texture(ctx, tex_provider, Some(path.as_str()));
        }
    }

    let mut material = material;
    if resolved_normal_idx != 0 {
        material.normal_map_index = resolved_normal_idx;
    } // else material.normal_map_index stays at u32::MAX (default — triggers shader procedural)
    material.noise_map_indices = resolved_noise.map(|idx| {
        if idx != 0 {
            idx
        } else {
            material.normal_map_index
        }
    });

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
    world.insert(entity, Transform::new(position, Quat::IDENTITY, scale));
    world.insert(
        entity,
        GlobalTransform::new(position, Quat::IDENTITY, scale),
    );
    world.insert(entity, MeshHandle(mesh_handle));
    let damage_per_second = xcwt_form
        .and_then(|form| waters.get(&form))
        .filter(|record| {
            record
                .water_flags
                .or(record.legacy_flags)
                .is_some_and(|flags| flags & 0x01 != 0)
        })
        .and_then(|record| record.legacy_damage)
        .map(f32::from)
        .unwrap_or(0.0);
    world.insert(
        entity,
        WaterPlane {
            kind,
            material,
            damage_per_second,
        },
    );
    // Keep a dormant, textureless spray emitter resident on the plane. The
    // water interaction system only raises its rate while the active camera
    // is near the surface, so this produces localized ripples without
    // structural ECS mutation during the frame.
    world.insert(entity, ParticleEmitter::water_splash());
    // #1338 — pair the normal-map `resolve_texture` refcount bump above
    // with a handle component the cell-unload victim walk can reach.
    // The water plane is drawn by the water pipeline from
    // `WaterPlane.material.normal_map_index` (its static `DrawCommand`
    // is skipped via the `is_water` flag in `reemit_water_planes`), so
    // this handle is consumed only by `unload_cell`'s `NormalMapHandle`
    // sweep — without it the texture refcount + bindless slot leak on
    // every cell unload. Gated on `!= 0` to mirror the acquire gate
    // (the procedural-fallback path leaves `resolved_normal_idx == 0`).
    if resolved_normal_idx != 0 {
        world.insert(entity, NormalMapHandle(resolved_normal_idx));
    }
    if resolved_noise.iter().any(|&idx| idx != 0) {
        world.insert(entity, WaterNoiseMapHandles(resolved_noise));
    }
    if let Some(flow) = flow {
        world.insert(entity, flow);
    }
    // Volume extends from the surface down to a per-mode floor.
    // Interior planes get a tight 200-wu column (typical pool
    // depth); exterior planes get the full cell-width column so deep
    // ocean cells remain detectable. The exterior heuristic is
    // "half-extent > 1024 = exterior" — captures the spawn caller
    // contract without an explicit flag.
    let volume_depth = if half_extent > 1024.0 {
        EXTERIOR_CELL_UNITS
    } else {
        DEFAULT_INTERIOR_VOLUME_DEPTH
    };
    let volume_floor_y = xclw_height - volume_depth;
    let water_components = terrain
        .and_then(|land| terrain_water_components(land, xclw_height))
        .unwrap_or_else(|| vec![(-1.0, 1.0, -1.0, 1.0)]);
    let (coverage_min_x, coverage_max_x, coverage_min_z, coverage_max_z) = water_components[0];
    world.insert(
        entity,
        WaterVolume {
            min: [
                position.x + coverage_min_x * half_extent,
                volume_floor_y,
                position.z + coverage_min_z * half_extent,
            ],
            max: [
                position.x + coverage_max_x * half_extent,
                xclw_height,
                position.z + coverage_max_z * half_extent,
            ],
        },
    );
    // A single render mesh can contain several disconnected ponds, but one
    // AABB cannot represent their dry gap. Keep the primary plane as the
    // first component and attach the remaining components to volume-only
    // entities. They carry the same material/flow for physics and gameplay,
    // but no MeshHandle, so the water renderer naturally skips them.
    for &(min_x, max_x, min_z, max_z) in water_components.iter().skip(1) {
        let component_entity = world.spawn();
        world.insert(
            component_entity,
            WaterPlane {
                kind,
                material,
                damage_per_second,
            },
        );
        if let Some(flow) = flow {
            world.insert(component_entity, flow);
        }
        world.insert(
            component_entity,
            WaterVolume {
                min: [
                    position.x + min_x * half_extent,
                    volume_floor_y,
                    position.z + min_z * half_extent,
                ],
                max: [
                    position.x + max_x * half_extent,
                    xclw_height,
                    position.z + max_z * half_extent,
                ],
            },
        );
    }

    // RenderLayer::Decal here is purely a draw-order placement — it
    // sorts water late, after opaque architectural geometry (lake floor
    // mesh, river bed). It is NOT a depth-bias guard: the water pipeline
    // sets `depth_bias_enable(false)` and never binds DEPTH_BIAS dynamic
    // state (see `crates/renderer/src/vulkan/water.rs::build_pipeline`),
    // so no bias is actually applied here (#1998). Water also doesn't
    // write depth, which is what actually keeps it from z-fighting the
    // bed mesh; if real shoreline z-fighting is ever observed, add
    // genuine DEPTH_BIAS dynamic state + `cmd_set_depth_bias` instead of
    // relying on RenderLayer::Decal.
    world.insert(entity, byroredux_core::ecs::components::RenderLayer::Decal);

    log::debug!(
        "Water plane spawned: pos={:?}, half_extent={}, kind={:?}, normalIdx={}",
        position,
        half_extent,
        kind,
        material.normal_map_index
    );

    Some(1)
}

/// Convert a CELL.XWCU Gamebryo velocity into the canonical renderer flow.
/// X/Y are the horizontal Z-up axes; the source Z component is vertical and
/// is intentionally ignored for horizontal cell planes.
fn cell_water_flow(velocity: Option<[f32; 3]>) -> Option<WaterFlow> {
    let [x, y, _z] = velocity?;
    let speed = x.hypot(y);
    (speed.is_finite() && speed > 1.0e-5).then(|| WaterFlow::new([x, 0.0, -y], speed))
}

#[inline]
fn kind_with_cell_flow(kind: WaterKind, speed: f32) -> WaterKind {
    if matches!(kind, WaterKind::Calm) && speed.is_finite() && speed > 1.0e-5 {
        if speed >= WaterFlow::SPEED_RAPIDS {
            WaterKind::Rapids
        } else {
            WaterKind::River
        }
    } else {
        kind
    }
}

/// Extra cushion (in cells) beyond `radius_unload` the LOD-water hole cuts
/// out, so the annulus doesn't touch right at the streaming boundary —
/// mirrors the conservative-by-one-cell margin the terrain LOD ring's own
/// `radius_unload` gate already relies on (#1871 / LC0703-02).
const LOD_WATER_HOLE_MARGIN_CELLS: i32 = 1;

/// Spawn the worldspace-wide distant LOD water quad (#2449 / EXAL-01) — the
/// `NAM3`/`NAM4` counterpart of [`spawn_water_plane`]'s per-cell `XCLW`/
/// `XCWT`. A single square annulus ("picture frame"): its outer edge
/// matches the distant-terrain LOD ring's total reach
/// (`super::terrain_lod::lod_ring_reach_cells`) for visual consistency, and its inner edge is a hole cut out around
/// `player_grid` sized to `radius_unload` (+ a one-cell margin) so it
/// doesn't overlap/double-blend against the near, full-detail per-cell
/// water. Called once at worldspace entry — see [`LodWaterPlane`]'s doc for
/// why the hole is a fixed snapshot rather than continuously re-centered.
///
/// Built as a 4×4 vertex grid (3×3 quads), holing out only the center quad
/// — the same row-major `tl/tr/bl/br` two-triangle-per-quad topology
/// `terrain_lod::spawn_lod_block` uses, so the winding convention is
/// reused rather than re-derived. Uses the SAME safe upload path
/// [`spawn_water_plane`] does (`rt_enabled: false`, per-mesh buffer) — see
/// [`LodWaterPlane`]'s doc for why that matters.
///
/// Returns `None` when the worldspace has no LOD water, the requested
/// radius leaves no annulus to draw (a huge streaming radius relative to
/// the LOD ring — degenerate on real content), or the mesh upload fails.
/// Pure geometry builder for [`spawn_lod_water_plane`]'s annulus mesh —
/// split out so the degenerate-guard and hole-cutout/winding logic is
/// unit-testable without a `VulkanContext`. `center_x_zup`/`center_y_zup`
/// are cell-grid-index-based Z-up world coordinates (pre-conversion), the
/// same convention `spawn_lod_block` uses for its block origin. Returns
/// `None` when `inner >= outer` (degenerate — see the call site's doc).
fn build_lod_water_frame(
    outer: f32,
    inner: f32,
    center_x_zup: f32,
    center_y_zup: f32,
    lod_height: f32,
) -> Option<(Vec<Vertex>, Vec<u32>)> {
    if inner >= outer {
        return None;
    }

    let mut axis = Vec::with_capacity(2 * (LOD_WATER_RING_SUBDIVISIONS + 1));
    for i in 0..=LOD_WATER_RING_SUBDIVISIONS {
        let t = i as f32 / LOD_WATER_RING_SUBDIVISIONS as f32;
        axis.push(-outer + (outer - inner) * t);
    }
    for i in 0..=LOD_WATER_RING_SUBDIVISIONS {
        let t = i as f32 / LOD_WATER_RING_SUBDIVISIONS as f32;
        axis.push(inner + (outer - inner) * t);
    }
    let cols: Vec<f32> = axis.iter().map(|offset| center_x_zup + offset).collect();
    let rows: Vec<f32> = axis.iter().map(|offset| center_y_zup + offset).collect();
    let n = axis.len();

    let mut vertices: Vec<Vertex> = Vec::with_capacity(n * n);
    for &world_y_zup in &rows {
        for &world_x in &cols {
            vertices.push(Vertex {
                position: zup_to_yup_pos([world_x, world_y_zup, lod_height]),
                color: [1.0, 1.0, 1.0, 1.0],
                normal: [0.0, 1.0, 0.0],
                // World-space UV, matching `spawn_water_plane`'s "UVs don't
                // matter visually, only their derivative magnitude"
                // rationale for the normal-map perturb blend.
                uv: [world_x - center_x_zup, world_y_zup - center_y_zup],
                bone_indices: [0, 0, 0, 0],
                bone_weights: [0.0, 0.0, 0.0, 0.0],
                splat_weights_0: [0, 0, 0, 0],
                splat_weights_1: [0, 0, 0, 0],
                // Non-degenerate placeholder — water.frag re-orthogonalises
                // against the world normal (matches `spawn_water_plane`).
                tangent: [1.0, 0.0, 0.0, -1.0],
            });
        }
    }

    // The central quad band is the hole cut out for the full-detail streamed
    // area. Same tl/tr/bl/br two-triangle winding as
    // `spawn_lod_block`.
    let hole_band = LOD_WATER_RING_SUBDIVISIONS;
    let mut indices: Vec<u32> = Vec::with_capacity((n - 1) * (n - 1) * 6);
    for r in 0..(n - 1) {
        for c in 0..(n - 1) {
            if r == hole_band && c == hole_band {
                continue; // the hole
            }
            let tl = (r * n + c) as u32;
            let tr = tl + 1;
            let bl = ((r + 1) * n + c) as u32;
            let br = bl + 1;
            indices.extend_from_slice(&[tl, tr, bl, tr, br, bl]);
        }
    }

    Some((vertices, indices))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_lod_water_plane(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    waters: &HashMap<u32, esm::records::misc::WatrRecord>,
    lod_height: f32,
    lod_water_form: Option<u32>,
    player_grid: (i32, i32),
    radius_unload: i32,
    game: esm::reader::GameKind,
) -> Option<LodWaterPlane> {
    let (material, kind, flow, normal_texture_path, noise_texture_paths) =
        crate::env_translate::resolve_water_material(waters, lod_water_form);

    let allocator = ctx.allocator.as_ref()?;

    // Track the terrain LOD ring's own reach, which since #2371 depends on
    // whether the game bakes a quadtree (Skyrim/FO4 reach their vanilla
    // `fBlockMaximumDistance`, the rest keep the synth ring). Reading it from
    // `terrain_lod` keeps the water frame from falling short of the terrain
    // it is supposed to meet.
    let outer = super::terrain_lod::lod_ring_reach_cells(game) as f32 * EXTERIOR_CELL_UNITS;
    let inner = (radius_unload + LOD_WATER_HOLE_MARGIN_CELLS).max(0) as f32 * EXTERIOR_CELL_UNITS;
    // Cell-grid-index-based Z-up world coordinates (pre-conversion), same
    // convention `spawn_lod_block` uses for its block origin.
    let center_x_zup = player_grid.0 as f32 * EXTERIOR_CELL_UNITS;
    let center_y_zup = player_grid.1 as f32 * EXTERIOR_CELL_UNITS;
    // Degenerate: the streamed area already covers (or exceeds) the LOD
    // ring's own radius — no annulus left to draw. Not expected on real
    // content (the LOD ring is sized far larger than any sane streaming
    // radius), but a corrupt/extreme config must not build an inverted or
    // zero-area mesh.
    let (vertices, indices) =
        build_lod_water_frame(outer, inner, center_x_zup, center_y_zup, lod_height)?;

    let upload_ctx = GpuUploadCtx {
        device: &ctx.device,
        allocator,
        queue: &ctx.graphics_queue,
        command_pool: ctx.transfer_pool,
    };
    let mesh_handle = match ctx
        .mesh_registry
        .upload_scene_mesh(upload_ctx, &vertices, &indices, false, None)
    {
        Ok(h) => h,
        Err(e) => {
            log::warn!("LOD water plane mesh upload failed: {e}");
            return None;
        }
    };

    let resolved_normal_idx = if let Some(path) = normal_texture_path {
        resolve_texture(ctx, tex_provider, Some(path.as_str()))
    } else {
        0
    };
    let mut resolved_noise = [0u32; 3];
    for (idx, path) in noise_texture_paths.into_iter().enumerate() {
        if let Some(path) = path {
            resolved_noise[idx] = resolve_texture(ctx, tex_provider, Some(path.as_str()));
        }
    }
    let mut material = material;
    if resolved_normal_idx != 0 {
        material.normal_map_index = resolved_normal_idx;
    }
    material.noise_map_indices = resolved_noise.map(|idx| {
        if idx != 0 {
            idx
        } else {
            material.normal_map_index
        }
    });

    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);
    world.insert(entity, GlobalTransform::IDENTITY);
    world.insert(entity, MeshHandle(mesh_handle));
    let damage_per_second = lod_water_form
        .and_then(|form| waters.get(&form))
        .filter(|record| {
            record
                .water_flags
                .or(record.legacy_flags)
                .is_some_and(|flags| flags & 0x01 != 0)
        })
        .and_then(|record| record.legacy_damage)
        .map(f32::from)
        .unwrap_or(0.0);
    world.insert(
        entity,
        WaterPlane {
            kind,
            material,
            damage_per_second,
        },
    );
    world.insert(
        entity,
        WaterLodInfo {
            height: lod_height,
            water_form: lod_water_form,
        },
    );
    world.insert(entity, ParticleEmitter::water_splash());
    if resolved_normal_idx != 0 {
        world.insert(entity, NormalMapHandle(resolved_normal_idx));
    }
    if resolved_noise.iter().any(|&idx| idx != 0) {
        world.insert(entity, WaterNoiseMapHandles(resolved_noise));
    }
    if let Some(flow) = flow {
        world.insert(entity, flow);
    }
    // Distant water is a render-only annulus. It has no shoreline geometry,
    // so a matching AABB `WaterVolume` would falsely submerge actors/cameras
    // on dry land anywhere inside the square (the annulus itself cannot be
    // represented by the canonical AABB). Near, streamed cell planes remain
    // the authoritative source for swimming, buoyancy, currents, and splash
    // interaction.
    world.insert(entity, RenderLayer::Decal);

    log::info!(
        "LOD water plane spawned: height={lod_height}, outer={outer:.0} BU, inner_hole={inner:.0} \
         BU @ grid {player_grid:?}, kind={kind:?}",
    );

    Some(LodWaterPlane {
        entity,
        mesh_handle,
        normal_map_handle: (resolved_normal_idx != 0).then_some(resolved_normal_idx),
        noise_map_handles: resolved_noise,
        center_grid: player_grid,
    })
}

/// Tear down the worldspace-wide LOD water quad (#2449 / EXAL-01): release
/// its normal-map texture refcount (mirrors `NormalMapHandle`'s contract —
/// `World::despawn` has no GPU side effects), free its mesh, and despawn
/// the entity. The only reclaim path — like `LodBlock`, this entity carries
/// no `CellRoot`, so `unload_cell`'s victim walk can't reach it.
pub(crate) fn unload_lod_water_plane(
    world: &mut World,
    ctx: &mut VulkanContext,
    plane: &LodWaterPlane,
) {
    if let Some(normal_idx) = plane.normal_map_handle {
        ctx.texture_registry.drop_texture(&ctx.device, normal_idx);
    }
    for &noise_idx in &plane.noise_map_handles {
        if noise_idx != 0 {
            ctx.texture_registry.drop_texture(&ctx.device, noise_idx);
        }
    }
    ctx.mesh_registry.drop_mesh(plane.mesh_handle);
    world.despawn(plane.entity);
}

/// Convenience for the exterior path — one exterior cell quad.
#[inline]
pub(super) fn exterior_half_extent() -> f32 {
    EXTERIOR_CELL_UNITS * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::water::{WaterKind, WaterMaterial};

    #[test]
    fn cell_water_velocity_converts_zup_horizontal_current() {
        let flow = cell_water_flow(Some([3.0, 4.0, 99.0])).expect("non-zero XWCU");
        assert_eq!(flow.direction, [0.6, 0.0, -0.8]);
        assert_eq!(flow.speed, 5.0);
    }

    #[test]
    fn zero_cell_water_velocity_keeps_watr_flow_fallback() {
        assert!(cell_water_flow(Some([0.0, 0.0, 2.0])).is_none());
        assert!(cell_water_flow(None).is_none());
    }

    #[test]
    fn authored_cell_current_promotes_neutral_water_to_river_shader() {
        assert_eq!(kind_with_cell_flow(WaterKind::Calm, 2.0), WaterKind::River);
        assert_eq!(
            kind_with_cell_flow(WaterKind::Calm, WaterFlow::SPEED_RAPIDS),
            WaterKind::Rapids
        );
        assert_eq!(
            kind_with_cell_flow(WaterKind::Rapids, 2.0),
            WaterKind::Rapids
        );
        assert_eq!(kind_with_cell_flow(WaterKind::Calm, 0.0), WaterKind::Calm);
    }

    // `resolve_water_material` (+ its WATR reflection-tint / default-tint
    // regressions for #1069) moved to the EXAL boundary in
    // `crate::env_translate`; the tests moved with it.

    /// Regression for #1338 / D3-01 — the normal map `spawn_water_plane`
    /// resolves (bumping a texture refcount) must be reachable by the
    /// SAME `NormalMapHandle` query `unload_cell`'s victim walk uses, so
    /// the refcount is released on cell unload. Pre-fix the index lived
    /// only in `WaterMaterial.normal_map_index`, which the walk can't
    /// reach → one leaked texture + bindless slot per water cell unload.
    ///
    /// The Vulkan half of spawn/unload can't run in a unit test (no
    /// headless `VulkanContext`), so we assert the reachability invariant
    /// directly: a water entity built like `spawn_water_plane` (MeshHandle,
    /// WaterPlane, and NormalMapHandle) is found by both the mesh-drop and
    /// the texture-drop queries the walk fans out to.
    #[test]
    fn water_normal_map_handle_reachable_by_unload_walk_query() {
        let mut world = World::new();
        let entity = world.spawn();
        // Mirror the production component set attached by spawn_water_plane.
        world.insert(entity, MeshHandle(7));
        world.insert(
            entity,
            WaterPlane {
                kind: WaterKind::Calm,
                material: WaterMaterial::default(),
                damage_per_second: 0.0,
            },
        );
        // The fix: a non-zero resolved normal index becomes a handle.
        let resolved_normal_idx: u32 = 42;
        if resolved_normal_idx != 0 {
            world.insert(entity, NormalMapHandle(resolved_normal_idx));
        }

        // `unload_cell` reaches mesh handles via `query::<MeshHandle>()`
        // and texture handles via `query::<NormalMapHandle>()`. Both must
        // find this water entity for cleanup to be complete.
        let mq = world.query::<MeshHandle>().expect("MeshHandle storage");
        assert_eq!(
            mq.get(entity).map(|m| m.0),
            Some(7),
            "water entity's mesh handle must be reachable by the unload walk"
        );
        let nq = world
            .query::<NormalMapHandle>()
            .expect("NormalMapHandle storage");
        assert_eq!(
            nq.get(entity).map(|n| n.0),
            Some(resolved_normal_idx),
            "water entity's normal-map handle must be reachable by the unload \
             walk's NormalMapHandle query so the texture refcount is released"
        );
    }

    #[test]
    fn lod_water_is_render_only_and_cannot_create_false_submersion() {
        let src = include_str!("water.rs");
        let start = src
            .find("pub(crate) fn spawn_lod_water_plane")
            .expect("LOD-water spawn function");
        let end = src
            .find("pub(crate) fn unload_lod_water_plane")
            .expect("LOD-water unload function");
        let lod_body = &src[start..end];
        assert!(
            !lod_body.contains("world.insert(\n        entity,\n        WaterVolume"),
            "distant LOD water has no shoreline geometry and must not drive physics/submersion"
        );
    }

    #[test]
    fn interior_water_placement_converts_horizontal_axes_and_adds_margin() {
        // Bethesda Z-up: source X is renderer X and source Y becomes
        // renderer -Z. The vertical source Z must not affect the footprint.
        let positions = [[0.0, 300.0, -500.0], [600.0, -300.0, 900.0]];
        let (center, half_extent) = interior_water_placement(positions.iter().copied());
        assert_eq!(center, (300.0, 0.0));
        assert_eq!(half_extent, 332.0); // max span 600 / 2 + 32
    }

    #[test]
    fn interior_water_placement_keeps_safe_default_for_empty_or_invalid_cells() {
        let positions = [[f32::NAN, 0.0, 0.0], [0.0, f32::INFINITY, 0.0]];
        assert_eq!(
            interior_water_placement(positions.iter().copied()),
            ((0.0, 0.0), DEFAULT_INTERIOR_HALF_EXTENT)
        );
    }

    #[test]
    fn full_detail_water_grid_supports_vertex_wave_silhouette() {
        let (vertices, indices) = build_full_detail_water_grid(256.0, None, 0.0);
        let side = FULL_DETAIL_WATER_GRID_SEGMENTS + 1;
        assert_eq!(vertices.len(), side * side);
        assert_eq!(
            indices.len(),
            FULL_DETAIL_WATER_GRID_SEGMENTS * FULL_DETAIL_WATER_GRID_SEGMENTS * 6
        );
        assert_eq!(vertices[0].position, [-1.0, 0.0, -1.0]);
        assert_eq!(vertices[0].uv, [-256.0, -256.0]);
        assert_eq!(
            vertices.last().expect("grid has corners").position,
            [1.0, 0.0, 1.0]
        );
        // The first cell uses the same +Y winding as the former quad.
        assert_eq!(
            &indices[..6],
            &[0, side as u32, 1, 1, side as u32, side as u32 + 1]
        );
    }

    #[test]
    fn terrain_mask_removes_fully_dry_water_cells_and_preserves_shoreline_triangles() {
        let dry = esm::cell::LandscapeData {
            heights: vec![100.0; 33 * 33],
            normals: None,
            vertex_colors: None,
            quadrants: Default::default(),
        };
        let (_, dry_indices) = build_full_detail_water_grid(2048.0, Some(&dry), 0.0);
        assert!(dry_indices.is_empty());

        let mut mixed = dry.clone();
        mixed.heights[16 * 33 + 16] = -100.0;
        let (_, mixed_indices) = build_full_detail_water_grid(2048.0, Some(&mixed), 0.0);
        assert!(!mixed_indices.is_empty());
        assert!(
            mixed_indices.len()
                < FULL_DETAIL_WATER_GRID_SEGMENTS * FULL_DETAIL_WATER_GRID_SEGMENTS * 6
        );
    }

    #[test]
    fn terrain_mask_clips_mixed_cells_at_the_waterline() {
        let mut land = esm::cell::LandscapeData {
            heights: vec![100.0; 33 * 33],
            normals: None,
            vertex_colors: None,
            quadrants: Default::default(),
        };
        // Only the top-left LAND sample is below XCLW. The first water cell
        // must therefore be a clipped polygon, not the original full quad.
        land.heights[0] = -100.0;
        let (vertices, indices) = build_full_detail_water_grid(2048.0, Some(&land), 0.0);
        assert!(!indices.is_empty());
        assert!(vertices.iter().any(|vertex| {
            (vertex.position[0] + 1.0).abs() < 1.0e-6
                && (vertex.position[2] + 0.9375).abs() < 1.0e-6
        }));
        assert!(vertices.iter().all(|vertex| {
            vertex.position[0] >= -1.0
                && vertex.position[0] <= 1.0
                && vertex.position[2] >= -1.0
                && vertex.position[2] <= 1.0
        }));
    }

    #[test]
    fn terrain_water_components_bound_the_physics_footprint() {
        let mut land = esm::cell::LandscapeData {
            heights: vec![100.0; 33 * 33],
            normals: None,
            vertex_colors: None,
            quadrants: Default::default(),
        };
        land.heights[0] = -10.0;
        land.heights[32 * 33 + 32] = -20.0;
        assert_eq!(
            terrain_water_components(&land, 0.0),
            Some(vec![(-1.0, -0.875, -1.0, -0.875), (0.875, 1.0, 0.875, 1.0)])
        );
    }

    #[test]
    fn terrain_water_components_keep_disconnected_ponds_separate() {
        let mut land = esm::cell::LandscapeData {
            heights: vec![100.0; 33 * 33],
            normals: None,
            vertex_colors: None,
            quadrants: Default::default(),
        };
        land.heights[2] = -10.0;
        land.heights[32 * 33 + 32] = -10.0;
        let components = terrain_water_components(&land, 0.0).expect("two wet cells");
        assert_eq!(components.len(), 2);
    }

    // ── build_lod_water_frame (#2449 / EXAL-01) ─────────────────────

    /// A degenerate request (inner hole at or beyond the outer edge) must
    /// build nothing rather than an inverted/zero-area mesh.
    #[test]
    fn degenerate_inner_not_less_than_outer_builds_nothing() {
        assert!(build_lod_water_frame(1000.0, 1000.0, 0.0, 0.0, 0.0).is_none());
        assert!(build_lod_water_frame(1000.0, 2000.0, 0.0, 0.0, 0.0).is_none());
    }

    /// A valid annulus request produces the expected vertex/triangle counts:
    /// the two eight-subdivision bands form an 18×18 grid, with one central
    /// quad removed for the hole.
    #[test]
    fn valid_annulus_has_expected_vertex_and_triangle_counts() {
        let (vertices, indices) = build_lod_water_frame(2000.0, 500.0, 0.0, 0.0, 100.0)
            .expect("outer > inner must build a frame");
        let side = 2 * (LOD_WATER_RING_SUBDIVISIONS + 1);
        assert_eq!(vertices.len(), side * side);
        assert_eq!(
            indices.len(),
            ((side - 1) * (side - 1) - 1) * 6,
            "all grid quads except the center hole"
        );
    }

    /// The center quad (the hole) must never appear as a triangle — its
    /// four corner indices around the center hole must never all-three appear
    /// together as one emitted triangle.
    #[test]
    fn center_quad_is_never_emitted() {
        let (_, indices) = build_lod_water_frame(2000.0, 500.0, 0.0, 0.0, 100.0)
            .expect("outer > inner must build a frame");
        let side = 2 * (LOD_WATER_RING_SUBDIVISIONS + 1);
        let h = LOD_WATER_RING_SUBDIVISIONS as u32;
        let side = side as u32;
        let hole_corners: std::collections::HashSet<u32> = [
            h * side + h,
            h * side + h + 1,
            (h + 1) * side + h,
            (h + 1) * side + h + 1,
        ]
        .into_iter()
        .collect();
        for tri in indices.chunks_exact(3) {
            let all_in_hole = tri.iter().all(|i| hole_corners.contains(i));
            assert!(
                !all_in_hole,
                "triangle {tri:?} must not be built entirely from the hole's own corners"
            );
        }
    }

    /// Every emitted vertex's Y (the engine's up axis after the Z-up→Y-up
    /// swap) must equal the authored LOD water height — a flat plane.
    #[test]
    fn every_vertex_sits_at_the_authored_height() {
        let (vertices, _) = build_lod_water_frame(2000.0, 500.0, 0.0, 0.0, -1234.5)
            .expect("outer > inner must build a frame");
        for v in &vertices {
            assert_eq!(
                v.position[1], -1234.5,
                "vertex {v:?} must sit at lod_height"
            );
        }
    }

    /// The outermost corner vertex's world position and UV must both
    /// reflect the requested `outer` extent, offset by the requested
    /// center — pins the coordinate convention (`center ± outer`) against
    /// a future refactor silently swapping outer/inner or dropping the
    /// center offset.
    #[test]
    fn outer_corner_position_and_uv_match_requested_extent() {
        let (vertices, _) = build_lod_water_frame(2000.0, 500.0, 100.0, 200.0, 0.0)
            .expect("outer > inner must build a frame");
        // Row 0, col 0 = the (-outer, -outer) corner relative to center,
        // i.e. world (100 - 2000, 200 - 2000) = (-1900, -1800) in Z-up X/Y.
        let corner = vertices[0];
        // Z-up→Y-up: (x, height, -y_zup).
        assert_eq!(corner.position[0], -1900.0);
        assert_eq!(corner.position[2], 1800.0);
        // UV is center-relative, matching `spawn_water_plane`'s convention.
        assert_eq!(corner.uv, [-2000.0, -2000.0]);
    }

    // ── translate_lod_water (#2449 / EXAL-01) ────────────────────────

    fn lod_map(
        wrld: byroredux_plugin::esm::cell::WorldspaceRecord,
    ) -> std::collections::HashMap<String, byroredux_plugin::esm::cell::WorldspaceRecord> {
        std::collections::HashMap::from([("w".to_string(), wrld)])
    }

    #[test]
    fn translate_lod_water_passes_through_authored_fields() {
        let wrld = byroredux_plugin::esm::cell::WorldspaceRecord {
            lod_water_height: Some(-500.0),
            lod_water_form: Some(0x0001_2345),
            ..Default::default()
        };
        assert_eq!(
            crate::env_translate::translate_lod_water(&lod_map(wrld), "w"),
            (Some(-500.0), Some(0x0001_2345))
        );
    }

    /// Oblivion WRLD authors no NAM3/NAM4 — the record's fields are
    /// already `None` at parse time, so no per-game branch is needed here.
    #[test]
    fn translate_lod_water_is_none_when_unauthored() {
        let wrld = byroredux_plugin::esm::cell::WorldspaceRecord::default();
        assert_eq!(
            crate::env_translate::translate_lod_water(&lod_map(wrld), "w"),
            (None, None)
        );
        assert_eq!(
            crate::env_translate::translate_lod_water(&Default::default(), "missing"),
            (None, None)
        );
    }
}
