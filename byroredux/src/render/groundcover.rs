//! EXAL ground cover — the host half of the per-frame scatter input (#4054).
//!
//! §4 makes the 512-unit chunk the unit of dispatch, culling and LOD
//! selection. This walks the resident exterior terrain cells, subdivides each
//! into its 8×8 chunk grid, culls to the ground-cover radius, and produces the
//! two record arrays the scatter reads.
//!
//! The `GpuInstance.vertex_offset` locator is resolved here, every frame,
//! against the live `MeshRegistry` — never cached. #4052 settled that path
//! (read the global vertex SSBO directly rather than baking an attribute
//! texture) and the registry compacts, so a cached offset would silently point
//! into another mesh's vertices and grow grass out of a rock.

use byroredux_core::ecs::components::groundcover::GroundCoverPalette;
use byroredux_core::ecs::{MeshHandle, World};
use byroredux_core::math::Vec3;
use byroredux_renderer::shader_constants::{
    GROUNDCOVER_CHUNKS_PER_CELL_SIDE, GROUNDCOVER_CHUNK_UNITS, GROUNDCOVER_DRAW_DISTANCE,
    GROUNDCOVER_MAX_CHUNKS,
};
use byroredux_renderer::vulkan::groundcover::{
    GpuGroundCoverCell, GpuGroundCoverChunk, GpuGroundCoverSpecies, CHUNKS_PER_CELL,
    MAX_GROUNDCOVER_CELLS, MAX_GROUNDCOVER_SPECIES,
};
use byroredux_renderer::MeshRegistry;

use crate::components::{TerrainCellOrigin, TerrainCoverInputs};

/// Half-diagonal of a chunk's XZ footprint — the radius of the sphere that
/// bounds it horizontally. Used to keep the distance cull conservative: a
/// chunk whose *centre* is past the draw distance can still have a near corner
/// inside it, and culling on the centre alone eats a visible wedge out of the
/// far edge of the field.
const CHUNK_BOUND_RADIUS: f32 = GROUNDCOVER_CHUNK_UNITS * 0.70711;

/// Collect this frame's ground-cover scatter input.
///
/// `cells` and `chunks` are caller-owned scratch, cleared on entry so their
/// allocations persist across frames — the same pattern `draw_commands` and
/// the light buffers use.
pub(crate) fn collect_groundcover_frame(
    world: &World,
    mesh_registry: &MeshRegistry,
    camera_pos: Vec3,
    camera_forward: Vec3,
    cells: &mut Vec<GpuGroundCoverCell>,
    chunks: &mut Vec<GpuGroundCoverChunk>,
) {
    cells.clear();
    chunks.clear();

    let (Some(origin_q), Some(cover_q), Some(mesh_q)) = (
        world.query::<TerrainCellOrigin>(),
        world.query::<TerrainCoverInputs>(),
        world.query::<MeshHandle>(),
    ) else {
        return;
    };

    // Sorted by origin so the cell indices chunk records point at are stable
    // frame to frame. ECS iteration order is not, and an unstable index would
    // re-key every chunk whenever the resident set changed — which is
    // invisible in a still frame and shows up as the whole field reshuffling
    // when a cell streams in.
    let mut resident: Vec<(EntityCell, [f32; 2])> = Vec::new();
    for (entity, origin) in origin_q.iter() {
        let (Some(cover), Some(mesh)) = (cover_q.get(entity), mesh_q.get(entity)) else {
            continue;
        };
        let Some(gpu_mesh) = mesh_registry.get(mesh.0) else {
            continue;
        };
        resident.push((
            EntityCell {
                origin_xz: origin.origin_xz,
                vertex_offset: gpu_mesh.global_vertex_offset,
                water_y: cover.water_y,
                layer_affinity: cover.layer_affinity,
            },
            origin.origin_xz,
        ));
    }
    resident.sort_by(|a, b| a.1[0].total_cmp(&b.1[0]).then(a.1[1].total_cmp(&b.1[1])));

    let max_dist = GROUNDCOVER_DRAW_DISTANCE + CHUNK_BOUND_RADIUS;
    for (cell, _) in resident.into_iter().take(MAX_GROUNDCOVER_CELLS) {
        // A cell contributes nothing if none of its chunks survive the cull,
        // so the cell record is only emitted once one does — otherwise a
        // 49-cell ring would fill the 128-cell cap with cells whose chunks are
        // all a kilometre behind the camera.
        let mut cell_index: Option<u32> = None;
        for cz in 0..GROUNDCOVER_CHUNKS_PER_CELL_SIDE {
            for cx in 0..GROUNDCOVER_CHUNKS_PER_CELL_SIDE {
                if chunks.len() >= GROUNDCOVER_MAX_CHUNKS as usize {
                    return;
                }
                // +X and −Z from the cell origin, matching the terrain grid's
                // row direction.
                let base_xz = [
                    cell.origin_xz[0] + cx as f32 * GROUNDCOVER_CHUNK_UNITS,
                    cell.origin_xz[1] - cz as f32 * GROUNDCOVER_CHUNK_UNITS,
                ];
                let centre = Vec3::new(
                    base_xz[0] + GROUNDCOVER_CHUNK_UNITS * 0.5,
                    camera_pos.y,
                    base_xz[1] - GROUNDCOVER_CHUNK_UNITS * 0.5,
                );
                let to_chunk = centre - camera_pos;
                // Horizontal distance only. Ground cover sits on the terrain,
                // and a hilltop cell 800 units below the camera is not further
                // away in the sense the draw distance means — culling on the
                // 3-D distance would strip the field off every valley floor
                // seen from a ridge, which is exactly the view that shows it.
                let horizontal = Vec3::new(to_chunk.x, 0.0, to_chunk.z).length();
                if horizontal > max_dist {
                    continue;
                }
                // Cheap behind-the-camera reject. Not a full frustum cull:
                // this is a hemisphere test, so it keeps everything the side
                // planes would also keep. The scatter is one workgroup per
                // chunk and the draw is indirect, so the cost of a kept-but-
                // offscreen chunk is small — while a chunk wrongly culled at
                // the screen edge is a visible bite out of the field.
                if to_chunk.dot(camera_forward) < -(CHUNK_BOUND_RADIUS + GROUNDCOVER_CHUNK_UNITS) {
                    continue;
                }
                let index = *cell_index.get_or_insert_with(|| {
                    cells.push(GpuGroundCoverCell {
                        origin_xz: cell.origin_xz,
                        vertex_offset: cell.vertex_offset,
                        pad0: 0,
                        affinity0: [
                            cell.layer_affinity[0],
                            cell.layer_affinity[1],
                            cell.layer_affinity[2],
                            cell.layer_affinity[3],
                        ],
                        affinity1: [
                            cell.layer_affinity[4],
                            cell.layer_affinity[5],
                            cell.layer_affinity[6],
                            cell.layer_affinity[7],
                        ],
                        water_y: cell.water_y,
                        pad1: [0.0; 3],
                    });
                    (cells.len() - 1) as u32
                });
                chunks.push(GpuGroundCoverChunk {
                    base_xz,
                    cell_index: index,
                    seed: chunk_seed(base_xz),
                });
            }
        }
        if cells.len() >= MAX_GROUNDCOVER_CELLS {
            break;
        }
    }
    debug_assert!(chunks.len() <= GROUNDCOVER_MAX_CHUNKS as usize);
    debug_assert!(cells.len() <= MAX_GROUNDCOVER_CELLS);
    let _ = CHUNKS_PER_CELL;
}

/// A resident cell's scatter inputs, before chunking.
struct EntityCell {
    origin_xz: [f32; 2],
    vertex_offset: u32,
    water_y: f32,
    layer_affinity: [f32; 8],
}

/// Scramble seed for a chunk's candidate sequence.
///
/// §4 requires blade placement stable frame to frame **and across sessions** —
/// a blade must not move when the camera does, or when the cell is unloaded
/// and reloaded. So this is a hash of the chunk's world position and nothing
/// else: no frame index, no entity id, no iteration order.
fn chunk_seed(base_xz: [f32; 2]) -> u32 {
    let mut h = base_xz[0].to_bits() ^ base_xz[1].to_bits().rotate_left(16);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    h
}

/// Flatten the resolved palette into the GPU species array.
///
/// `GroundCoverPalette::resolve` guarantees at least one entry, so the shader
/// has no empty-palette branch to get wrong; this preserves that by falling
/// back to the built-in default if the resource is somehow absent (an interior
/// frame that reached here, say).
pub(crate) fn collect_groundcover_species(world: &World, out: &mut Vec<GpuGroundCoverSpecies>) {
    use byroredux_core::ecs::components::groundcover::GroundCoverSpecies;
    out.clear();
    let fallback = [GroundCoverSpecies::DEFAULT_TEMPERATE];
    let palette = world.try_resource::<GroundCoverPalette>();
    let species: &[GroundCoverSpecies] = match palette.as_ref() {
        Some(p) if !p.species.is_empty() => &p.species,
        _ => &fallback,
    };
    for s in species.iter().take(MAX_GROUNDCOVER_SPECIES) {
        out.push(GpuGroundCoverSpecies {
            size_range: [
                s.height_range.0,
                s.height_range.1,
                s.width_range.0,
                s.width_range.1,
            ],
            base_colour: [
                s.colour_gradient[0][0],
                s.colour_gradient[0][1],
                s.colour_gradient[0][2],
                s.bend_stiffness,
            ],
            tip_colour: [
                s.colour_gradient[1][0],
                s.colour_gradient[1][1],
                s.colour_gradient[1][2],
                // §12.3's ground-coupling weight. Landed with the canonical
                // type in #4056; older palettes read the type's default.
                s.ground_coupling,
            ],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §4 requires placement stable frame to frame and across sessions. The
    /// seed is what guarantees it, so it must be a pure function of the
    /// chunk's world position — nothing else is stable across a reload.
    #[test]
    fn chunk_seed_is_positional_and_stable() {
        let a = chunk_seed([4096.0, -8192.0]);
        assert_eq!(a, chunk_seed([4096.0, -8192.0]));
        assert_ne!(a, chunk_seed([4608.0, -8192.0]));
        assert_ne!(a, chunk_seed([4096.0, -8704.0]));
        // Neighbouring chunks must not collide, or two adjacent chunks draw
        // the identical point set and the seam reads as a mirror line.
        let mut seen = std::collections::HashSet::new();
        for cz in 0..8 {
            for cx in 0..8 {
                let base = [cx as f32 * 512.0, -(cz as f32) * 512.0];
                assert!(seen.insert(chunk_seed(base)), "seed collision at {base:?}");
            }
        }
    }

    /// The cull radius has to bound the chunk, not its centre: a chunk whose
    /// centre is just past the draw distance still has a near corner inside
    /// it, and culling on the centre eats a visible wedge out of the far edge.
    #[test]
    fn chunk_bound_radius_covers_the_footprint() {
        let half_diagonal = (2.0f32).sqrt() * GROUNDCOVER_CHUNK_UNITS * 0.5;
        assert!(
            CHUNK_BOUND_RADIUS >= half_diagonal - 1.0e-3,
            "{CHUNK_BOUND_RADIUS} must bound a {GROUNDCOVER_CHUNK_UNITS}-unit chunk's \
             half-diagonal {half_diagonal}"
        );
    }

    /// An empty palette would index `gcSpecies[0]` out of bounds in the blade
    /// shader. `GroundCoverPalette::resolve` already guarantees non-empty;
    /// this pins that the collection preserves the guarantee even with no
    /// resource installed at all.
    #[test]
    fn species_collection_is_never_empty() {
        let world = World::new();
        let mut out = Vec::new();
        collect_groundcover_species(&world, &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].size_range[1] >= out[0].size_range[0]);
    }
}
