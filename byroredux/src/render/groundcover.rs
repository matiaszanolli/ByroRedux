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

use byroredux_core::ecs::components::groundcover::{GroundCoverDimmer, GroundCoverPalette};
use byroredux_core::ecs::{MeshHandle, World};
use byroredux_core::math::Vec3;
use byroredux_renderer::shader_constants::{
    GROUNDCOVER_CHUNKS_PER_CELL_SIDE, GROUNDCOVER_CHUNK_UNITS, GROUNDCOVER_DRAW_DISTANCE,
    GROUNDCOVER_INTERACTION_MAX_DISTURBERS, GROUNDCOVER_INTERACTION_UNITS, GROUNDCOVER_MAX_CHUNKS,
};
use byroredux_renderer::vulkan::groundcover::{
    GpuGroundCoverCell, GpuGroundCoverChunk, GpuGroundCoverDisturber, GpuGroundCoverSpecies,
    CHUNKS_PER_CELL, MAX_GROUNDCOVER_CELLS, MAX_GROUNDCOVER_SPECIES,
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

/// Collect this frame's §12.4 interaction disturbers (#4058), nearest first.
///
/// # Which entities feed the field
///
/// **Every live actor** — anything carrying [`ActorValues`] and a world
/// transform, the player included. The issue left this open as a cost/quality
/// trade; the two alternatives it named lose for reasons that do not need a
/// render to see:
///
/// - *Player only* would make the world feel dead the moment an NPC walked
///   past you through the same grass and left none of it moved. The whole
///   point of the term is that the stratum reacts to the world, not to the
///   camera.
/// - *All physics bodies* would spend the budget on every dropped bottle and
///   every settled prop, most of which never move again and none of which the
///   eye is tracking. A disturber costs one loop iteration per field texel.
///
/// Actors are what the eye follows, and an actor's motion through grass is the
/// only motion that reads as motion. The remaining refinements — a cart, a
/// rolling boulder, a spell effect — are entities that would each want their
/// own radius anyway, so they are additions to this list rather than reasons
/// to have picked a different one.
///
/// # Radius
///
/// The actor's own character-controller capsule when it has one, and
/// [`CharacterController::HUMAN`]'s radius when it does not — a canonical
/// constant that already describes a vanilla actor capsule (36 units wide),
/// not a number invented here.
pub(crate) fn collect_groundcover_disturbers(
    world: &World,
    camera_pos: Vec3,
    out: &mut Vec<GpuGroundCoverDisturber>,
) {
    use byroredux_core::ecs::components::{ActorValues, GlobalTransform};
    out.clear();
    let (Some(actor_q), Some(xform_q)) = (
        world.query::<ActorValues>(),
        world.query::<GlobalTransform>(),
    ) else {
        return;
    };
    let controller_q = world.query::<byroredux_physics::CharacterController>();
    // Anything past this cannot reach a texel of the field, so it would cost a
    // per-texel loop iteration to contribute exactly zero.
    let reach = GROUNDCOVER_INTERACTION_UNITS * 0.5 + MAX_DISTURBER_RADIUS;
    let mut found: Vec<(f32, GpuGroundCoverDisturber)> = Vec::new();
    for (entity, _) in actor_q.iter() {
        let Some(xform) = xform_q.get(entity) else {
            continue;
        };
        let pos = xform.translation;
        let radius = controller_q
            .as_ref()
            .and_then(|q| q.get(entity))
            .map_or(DEFAULT_DISTURBER_RADIUS, |cc| cc.radius)
            .clamp(1.0, MAX_DISTURBER_RADIUS);
        let dx = pos.x - camera_pos.x;
        let dz = pos.z - camera_pos.z;
        // Horizontal only, for the same reason the chunk cull is horizontal:
        // an actor on a ledge above the camera is standing in grass the camera
        // can see, and a 3-D distance would drop it.
        let dist_sq = dx * dx + dz * dz;
        if dist_sq > reach * reach {
            continue;
        }
        found.push((
            dist_sq,
            GpuGroundCoverDisturber {
                world_xz: [pos.x, pos.z],
                radius,
                strength: 1.0,
            },
        ));
    }
    // Nearest first, because the renderer truncates at
    // `GROUNDCOVER_INTERACTION_MAX_DISTURBERS` and the ones nearest the camera
    // are the ones whose trails are legible.
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.extend(
        found
            .into_iter()
            .take(GROUNDCOVER_INTERACTION_MAX_DISTURBERS as usize)
            .map(|(_, d)| d),
    );
}

/// Fallback disturber radius — a vanilla actor capsule.
const DEFAULT_DISTURBER_RADIUS: f32 = byroredux_physics::CharacterController::HUMAN.radius;

/// Ceiling on an authored capsule radius, so a mod's giant does not open a
/// channel wider than the field can represent.
const MAX_DISTURBER_RADIUS: f32 = 256.0;

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

/// Flatten the resolved palette into the GPU species array, applying the live
/// per-weather grass dimmer.
///
/// `GroundCoverPalette::resolve` guarantees at least one entry, so the shader
/// has no empty-palette branch to get wrong; this preserves that by falling
/// back to the built-in default if the resource is somehow absent (an interior
/// frame that reached here, say).
///
/// # Why the dimmer is applied here and not in the palette
///
/// [`GroundCoverDimmer`] is Oblivion's `WTHR.HNAM.grassDimmer` — a per-weather
/// colour multiplier the source engine applies at the grass-generator stage,
/// before scene lighting. Folding it into `GroundCoverPalette` would freeze it
/// at whichever weather was active when the worldspace was entered, because
/// that is the one time the palette resolves. This function runs every frame
/// off the live resource `weather_system` writes, which is the same slot
/// `WindField` rides, so a `WTHR` cross-fade moves the sward's tint with it.
///
/// It multiplies **every** authored colour, transmission included: it is a
/// property of the light the weather is passing, not of the reflectance, so
/// dimming what a blade reflects while leaving what it transmits alone would
/// make backlit grass brighten relative to its surroundings under overcast.
pub(crate) fn collect_groundcover_species(
    world: &World,
    dimmer: GroundCoverDimmer,
    out: &mut Vec<GpuGroundCoverSpecies>,
) {
    use byroredux_core::ecs::components::groundcover::GroundCoverSpecies;
    out.clear();
    let fallback = [GroundCoverSpecies::DEFAULT_TEMPERATE];
    let palette = world.try_resource::<GroundCoverPalette>();
    let species: &[GroundCoverSpecies] = match palette.as_ref() {
        Some(p) if !p.species.is_empty() => &p.species,
        _ => &fallback,
    };
    let k = dimmer.0;
    for s in species.iter().take(MAX_GROUNDCOVER_SPECIES) {
        out.push(GpuGroundCoverSpecies {
            size_range: [
                s.height_range.0,
                s.height_range.1,
                s.width_range.0,
                s.width_range.1,
            ],
            base_colour: [
                s.colour_gradient[0][0] * k,
                s.colour_gradient[0][1] * k,
                s.colour_gradient[0][2] * k,
                s.bend_stiffness,
            ],
            tip_colour: [
                s.colour_gradient[1][0] * k,
                s.colour_gradient[1][1] * k,
                s.colour_gradient[1][2] * k,
                // §12.3's ground-coupling weight. Landed with the canonical
                // type in #4056; older palettes read the type's default.
                s.ground_coupling,
            ],
            transmission_sheen: [
                s.transmission_colour[0] * k,
                s.transmission_colour[1] * k,
                s.transmission_colour[2] * k,
                // §12.6's sheen amount is a *lobe strength*, not a colour —
                // the dimmer must not scale it, or an overcast weather would
                // also flatten the silvering that overcast light produces.
                s.sheen,
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
        collect_groundcover_species(&world, GroundCoverDimmer::NEUTRAL, &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].size_range[1] >= out[0].size_range[0]);
    }

    /// The grass dimmer has to reach the GPU record every frame, or a `WTHR`
    /// cross-fade leaves the sward tinted for whichever weather happened to be
    /// active when the worldspace resolved its palette (#4057).
    #[test]
    fn grass_dimmer_scales_every_authored_colour_but_not_the_sheen() {
        let world = World::new();
        let mut neutral = Vec::new();
        let mut dimmed = Vec::new();
        collect_groundcover_species(&world, GroundCoverDimmer::NEUTRAL, &mut neutral);
        collect_groundcover_species(&world, GroundCoverDimmer(0.5), &mut dimmed);
        for c in 0..3 {
            assert!((dimmed[0].base_colour[c] - neutral[0].base_colour[c] * 0.5).abs() < 1.0e-6);
            assert!((dimmed[0].tip_colour[c] - neutral[0].tip_colour[c] * 0.5).abs() < 1.0e-6);
            assert!(
                (dimmed[0].transmission_sheen[c] - neutral[0].transmission_sheen[c] * 0.5).abs()
                    < 1.0e-6
            );
        }
        // The `.w` lanes are not colours: bend stiffness, ground coupling and
        // the sheen lobe strength all have to survive the multiply untouched.
        assert_eq!(dimmed[0].base_colour[3], neutral[0].base_colour[3]);
        assert_eq!(dimmed[0].tip_colour[3], neutral[0].tip_colour[3]);
        assert_eq!(
            dimmed[0].transmission_sheen[3],
            neutral[0].transmission_sheen[3]
        );
    }

    /// §12.4's disturber list is the actors, nearest first, capped. The cap
    /// is why the sort matters: an unsorted list truncated at 64 would drop
    /// whichever actors the ECS happened to iterate last, which is not stable
    /// between frames — so a distant crowd could evict the actor standing next
    /// to you, and their trail would flicker in and out (#4058).
    #[test]
    fn disturbers_are_actors_sorted_by_horizontal_distance() {
        use byroredux_core::ecs::components::{ActorValues, GlobalTransform};
        let mut world = World::new();
        // Three actors at increasing horizontal distance, plus one entity with
        // a transform but no ActorValues (a prop) that must not appear.
        for (i, x) in [900.0f32, 100.0, 400.0].into_iter().enumerate() {
            let e = world.spawn();
            world.insert(e, ActorValues::from_pairs([(7, 5.0 + i as f32)]));
            world.insert(
                e,
                GlobalTransform {
                    translation: Vec3::new(x, 0.0, 0.0),
                    ..GlobalTransform::IDENTITY
                },
            );
        }
        let prop = world.spawn();
        world.insert(
            prop,
            GlobalTransform {
                translation: Vec3::new(50.0, 0.0, 0.0),
                ..GlobalTransform::IDENTITY
            },
        );

        let mut out = Vec::new();
        collect_groundcover_disturbers(&world, Vec3::ZERO, &mut out);
        assert_eq!(out.len(), 3, "the prop must not disturb anything");
        assert_eq!(out[0].world_xz[0], 100.0);
        assert_eq!(out[1].world_xz[0], 400.0);
        assert_eq!(out[2].world_xz[0], 900.0);
        // No CharacterController on any of them, so each takes the vanilla
        // actor capsule rather than a number invented at the call site.
        assert_eq!(out[0].radius, DEFAULT_DISTURBER_RADIUS);
    }

    /// An actor far enough out cannot reach a texel of the field, so it would
    /// cost a per-texel loop iteration to contribute exactly zero.
    #[test]
    fn distant_actors_are_culled_from_the_field() {
        use byroredux_core::ecs::components::{ActorValues, GlobalTransform};
        let mut world = World::new();
        let far = world.spawn();
        world.insert(far, ActorValues::from_pairs([(7, 5.0)]));
        world.insert(
            far,
            GlobalTransform {
                translation: Vec3::new(GROUNDCOVER_INTERACTION_UNITS * 2.0, 0.0, 0.0),
                ..GlobalTransform::IDENTITY
            },
        );
        // Vertically distant but horizontally close: an actor on a ledge above
        // the camera is standing in grass the camera can see, and culling on
        // 3-D distance would drop it — the same reason the chunk cull is
        // horizontal.
        let above = world.spawn();
        world.insert(above, ActorValues::from_pairs([(7, 5.0)]));
        world.insert(
            above,
            GlobalTransform {
                translation: Vec3::new(64.0, 5000.0, 0.0),
                ..GlobalTransform::IDENTITY
            },
        );

        let mut out = Vec::new();
        collect_groundcover_disturbers(&world, Vec3::ZERO, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].world_xz, [64.0, 0.0]);
    }

    /// §12.1 supersedes §7's baked dark base. If the gradient still carried a
    /// stand-in for self-shadowing the two would compound and the base of
    /// every blade would go black under the real occlusion term.
    #[test]
    fn the_default_gradients_no_longer_bake_a_self_shadow() {
        use byroredux_core::ecs::components::groundcover::GroundCoverSpecies;
        for sp in [
            GroundCoverSpecies::DEFAULT_TEMPERATE,
            GroundCoverSpecies::DEFAULT_ARID,
        ] {
            let lum = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            let base = lum(sp.colour_gradient[0]);
            let tip = lum(sp.colour_gradient[1]);
            assert!(
                base > tip * 0.7,
                "base {base} is still a darkened tip {tip} — §12.1 now computes \
                 that term and the two would compound (#4057)"
            );
            assert!(base < tip, "the sheath is still paler than the lamina");
        }
    }
}
