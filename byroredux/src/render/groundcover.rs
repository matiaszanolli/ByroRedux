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

// #4607 — this file is the per-frame render path: every hashed collection
// here is FxHash end to end per the #2923 hot-path rule, and the
// per-frame intermediates live in caller/`GroundCoverResidency`-owned
// scratch (cleared, not reallocated) instead of fresh locals.
use rustc_hash::{FxHashMap, FxHashSet};

use byroredux_core::ecs::components::groundcover::{GroundCoverDimmer, GroundCoverPalette};
use byroredux_core::ecs::{MeshHandle, World};
use byroredux_core::math::Vec3;
use byroredux_renderer::shader_constants::{
    GROUNDCOVER_BLADES_PER_POINT, GROUNDCOVER_CHUNKS_PER_CELL_SIDE, GROUNDCOVER_CHUNK_UNITS, GROUNDCOVER_DETAIL_ATLAS_EDGE,
    GROUNDCOVER_DRAW_DISTANCE, GROUNDCOVER_INTERACTION_MAX_DISTURBERS,
    GROUNDCOVER_INTERACTION_HALF_LIFE_SECONDS, GROUNDCOVER_INTERACTION_UNITS,
    GROUNDCOVER_MAX_CHUNKS,
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
const CHUNK_BOUND_RADIUS: f32 = GROUNDCOVER_CHUNK_UNITS * std::f32::consts::FRAC_1_SQRT_2;

/// A chunk that survived the distance and behind-camera culls, before the
/// per-frame caps are applied.
#[derive(Clone, Copy)]
pub(crate) struct ChunkCandidate {
    /// Position in the cull walk — origin-sorted cell, then row, then column.
    /// Survivors are emitted in this order, so a frame that stays under the
    /// cap produces exactly the chunk list it always did.
    order: usize,
    /// Index into the origin-sorted resident cell list.
    cell: usize,
    base_xz: [f32; 2],
    /// Horizontal camera distance to the chunk centre, as the cull measured it.
    horizontal: f32,
}

/// Identity of a resident chunk.  The base coordinate is integral in normal
/// terrain generation, but its bit representation is the actual renderer
/// identity: it is exactly what `chunk_seed` and the GPU record use.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ChunkKey {
    x: u32,
    z: u32,
}

impl ChunkKey {
    fn from_base(base_xz: [f32; 2]) -> Self {
        Self {
            x: base_xz[0].to_bits(),
            z: base_xz[1].to_bits(),
        }
    }
}

/// Camera-centred ground-cover residency ring.
///
/// The blade SSBO is partitioned into equal slabs by the chunk record index.
/// Keeping a chunk in the same slot while it remains in the ring therefore
/// keeps its slab ownership stable; only eviction releases a slab.  The ring
/// itself is host-side because terrain-cell streaming is host-side too.
#[derive(Default)]
pub(crate) struct GroundCoverResidency {
    slots: Vec<Option<ChunkKey>>,
    entry_progress: Vec<f32>,
    /// #4607 — reconcile's intermediates, cleared on entry each frame so
    /// their allocations persist instead of rebuilding per exterior frame.
    desired: FxHashMap<ChunkKey, ChunkCandidate>,
    wanted: FxHashSet<ChunkKey>,
    resident: FxHashSet<ChunkKey>,
    pending: Vec<(ChunkKey, ChunkCandidate)>,
}

impl GroundCoverResidency {
    /// Start a fresh worldspace ring.  The next collection fills it through
    /// the normal placement budget rather than issuing a one-frame burst.
    pub(crate) fn clear(&mut self) {
        self.slots.clear();
        self.entry_progress.clear();
    }

    fn ensure_slot_count(&mut self) {
        let radius_chunks = ((GROUNDCOVER_DRAW_DISTANCE + CHUNK_BOUND_RADIUS)
            / GROUNDCOVER_CHUNK_UNITS)
            .ceil() as usize;
        // A square is a conservative allocation for the circular desired
        // ring.  It is derived solely from the draw radius and chunk extent,
        // not an independently tuned cap.
        let side = radius_chunks * 2 + 1;
        let required = side * side;
        debug_assert!(required <= GROUNDCOVER_MAX_CHUNKS as usize);
        self.slots.resize(required, None);
        self.entry_progress.resize(required, 0.0);
    }

    /// Reconcile ring slots with this frame's camera-centred desired set.
    ///
    /// The placement budget is the plan's cited starting value (24 chunks per
    /// frame): a teleport fills in over several frames instead of forcing one
    /// large scatter dispatch.  Existing residents are never re-slotted.
    fn reconcile(&mut self, candidates: &[ChunkCandidate], delta_seconds: f32) -> Vec<(usize, ChunkCandidate)> {
        const PLACEMENTS_PER_FRAME: usize = 24;

        self.ensure_slot_count();
        // #4607 — scratch fields, cleared (not reallocated) each frame.
        let desired = &mut self.desired;
        let wanted = &mut self.wanted;
        desired.clear();
        desired.extend(
            candidates
                .iter()
                .map(|candidate| (ChunkKey::from_base(candidate.base_xz), *candidate)),
        );
        wanted.clear();
        wanted.extend(desired.keys().copied());

        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.is_some_and(|key| !wanted.contains(&key)) {
                *slot = None;
                self.entry_progress[index] = 0.0;
            }
        }

        // Reuse the cited interaction-field half-life as the bounded entry
        // ramp: it is already the renderer's established short temporal
        // response. New slots stay at zero until after this advance.
        let step = delta_seconds.clamp(0.0, 0.25) / GROUNDCOVER_INTERACTION_HALF_LIFE_SECONDS;
        for (slot, progress) in self.slots.iter().zip(&mut self.entry_progress) {
            if slot.is_some() {
                *progress = (*progress + step).min(1.0);
            }
        }

        let resident = &mut self.resident;
        resident.clear();
        resident.extend(self.slots.iter().flatten().copied());
        let pending = &mut self.pending;
        pending.clear();
        pending.extend(
            desired
                .iter()
                .filter(|(key, _)| !resident.contains(key))
                .map(|(key, candidate)| (*key, *candidate)),
        );
        pending.sort_by(|a, b| {
            a.1.horizontal
                .total_cmp(&b.1.horizontal)
                .then(a.1.order.cmp(&b.1.order))
        });

        let mut pending = pending.iter().copied();
        let mut placed = 0usize;
        for (slot_index, slot) in self.slots.iter_mut().enumerate() {
            if placed == PLACEMENTS_PER_FRAME {
                break;
            }
            if slot.is_none() {
                if let Some((key, _)) = pending.next() {
                    *slot = Some(key);
                    self.entry_progress[slot_index] = 0.0;
                    placed += 1;
                } else {
                    break;
                }
            }
        }

        self.slots
            .iter()
            .enumerate()
            .filter_map(|(slot, key)| {
                let key = (*key)?;
                // A slot can only contain a key from `wanted`: stale slots
                // were evicted above, and newly placed keys came from it.
                desired
                    .get(&key)
                    .copied()
                    .map(|candidate| (slot, candidate))
            })
            .collect()
    }

    fn entry_progress(&self, slot: usize) -> f32 {
        self.entry_progress.get(slot).copied().unwrap_or(0.0)
    }
}

/// Fit the culled chunk list to `cap`: keep the nearest chunks, then restore
/// walk order among the survivors. Returns how many were dropped (#4338).
///
/// Nearest-first for the reason the disturber list is sorted: whatever falls
/// off the end is lost, and losing the far rim of the field is the only
/// truncation that reads as distance. Walk order alone drops by cell origin —
/// west to east — so the grass on one side of the camera went instead.
#[cfg(test)]
fn keep_nearest_chunks(candidates: &mut Vec<ChunkCandidate>, cap: usize) -> u32 {
    if candidates.len() <= cap {
        return 0;
    }
    let dropped = candidates.len() - cap;
    candidates.sort_by(|a, b| {
        a.horizontal
            .total_cmp(&b.horizontal)
            .then(a.order.cmp(&b.order))
    });
    candidates.truncate(cap);
    candidates.sort_unstable_by_key(|candidate| candidate.order);
    dropped as u32
}

/// #4607 — the per-frame collection intermediates, caller-owned and
/// cleared on entry so their allocations persist across exterior frames
/// (the same pattern the output Vecs already follow).
pub(crate) struct GroundCoverCollectScratch {
    pub(crate) resident_cells: Vec<(EntityCell, [f32; 2])>,
    pub(crate) candidates: Vec<ChunkCandidate>,
    pub(crate) emitted: FxHashMap<usize, u32>,
    pub(crate) disturber_found: Vec<(f32, GpuGroundCoverDisturber)>,
}

impl Default for GroundCoverCollectScratch {
    fn default() -> Self {
        Self {
            resident_cells: Vec::new(),
            candidates: Vec::new(),
            emitted: FxHashMap::default(),
            disturber_found: Vec::new(),
        }
    }
}

/// Collect this frame's ground-cover scatter input.
///
/// `cells` and `chunks` are caller-owned scratch, cleared on entry so their
/// allocations persist across frames — the same pattern `draw_commands` and
/// the light buffers use.
///
/// Returns only cell-table overflow for `GroundCoverStats::chunks_truncated`.
/// The GPU blade arena remains statically sized, but its slots are now owned
/// by [`GroundCoverResidency`] instead of by a per-frame nearest-chunk cap.
#[allow(clippy::too_many_arguments)] // Caller-owned per-frame scratch plus frame inputs.
pub(crate) fn collect_groundcover_frame(
    world: &World,
    mesh_registry: &MeshRegistry,
    camera_pos: Vec3,
    _camera_forward: Vec3,
    delta_seconds: f32,
    residency: &mut GroundCoverResidency,
    scratch: &mut GroundCoverCollectScratch,
    cells: &mut Vec<GpuGroundCoverCell>,
    chunks: &mut Vec<GpuGroundCoverChunk>,
) -> u32 {
    cells.clear();
    chunks.clear();
    let resident = &mut scratch.resident_cells;
    let candidates = &mut scratch.candidates;
    resident.clear();
    candidates.clear();

    let (Some(origin_q), Some(cover_q), Some(mesh_q)) = (
        world.query::<TerrainCellOrigin>(),
        world.query::<TerrainCoverInputs>(),
        world.query::<MeshHandle>(),
    ) else {
        return 0;
    };

    // Sorted by origin so the cell indices chunk records point at are stable
    // frame to frame. ECS iteration order is not, and an unstable index would
    // re-key every chunk whenever the resident set changed — which is
    // invisible in a still frame and shows up as the whole field reshuffling
    // when a cell streams in.
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
    for (cell_ordinal, (cell, _)) in resident.iter().enumerate() {
        for cz in 0..GROUNDCOVER_CHUNKS_PER_CELL_SIDE {
            for cx in 0..GROUNDCOVER_CHUNKS_PER_CELL_SIDE {
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
                candidates.push(ChunkCandidate {
                    order: candidates.len(),
                    cell: cell_ordinal,
                    base_xz,
                    horizontal,
                });
            }
        }
    }
    // The residency ring owns the fixed GPU slabs.  Do not retain the old
    // nearest-first truncation here: it still made a teleport or a larger
    // draw radius silently drop coverage at one edge instead of queuing it.
    let residents = residency.reconcile(&candidates, delta_seconds);
    let mut truncated = 0;

    // A cell contributes nothing if none of its chunks survive, so the cell
    // record is only emitted once one does — otherwise a 49-cell ring would
    // fill the 128-cell cap with cells whose chunks are all a kilometre behind
    // the camera. #4607 — survivors arrive in SLOT order (reconcile walks
    // the residency ring, not the candidates), so one cell's chunks are NOT
    // contiguous and any earlier emitted cell can match: a real map, not the
    // last-cell fast path the pre-ring comment described.
    let emitted = &mut scratch.emitted;
    emitted.clear();
    // Preserve the residency slot as the GPU record index.  Compacting this
    // list would make a hole at (say) slot 3 move slot 4's chunk into slab 3,
    // defeating the ring's no-move ownership guarantee.  Inactive records
    // are explicitly skipped by scatter and retain an empty indirect draw.
    let slot_count = residents
        .iter()
        .map(|(slot, _)| *slot + 1)
        .max()
        .unwrap_or(0);
    chunks.resize(slot_count, GpuGroundCoverChunk::default());
    for (slot, candidate) in residents {
        let index = match emitted.get(&candidate.cell).copied() {
            Some(index) => index,
            None => {
                if cells.len() >= MAX_GROUNDCOVER_CELLS {
                    truncated += 1;
                    continue;
                }
                let cell = &resident[candidate.cell].0;
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
                let index = (cells.len() - 1) as u32;
                emitted.insert(candidate.cell, index);
                index
            }
        };
        chunks[slot] = GpuGroundCoverChunk {
            base_xz: candidate.base_xz,
            cell_index: index,
            seed: chunk_seed(candidate.base_xz),
            active: 1,
            entry_progress: residency.entry_progress(slot),
            pad: [0; 2],
        };
    }
    debug_assert!(chunks.len() <= GROUNDCOVER_MAX_CHUNKS as usize);
    debug_assert!(cells.len() <= MAX_GROUNDCOVER_CELLS);
    let _ = CHUNKS_PER_CELL;
    truncated
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
    scratch: &mut GroundCoverCollectScratch,
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
    let found = &mut scratch.disturber_found;
    found.clear();
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
            .iter()
            .take(GROUNDCOVER_INTERACTION_MAX_DISTURBERS as usize)
            .map(|(_, d)| *d),
    );
}

/// Fallback disturber radius — a vanilla actor capsule.
const DEFAULT_DISTURBER_RADIUS: f32 = byroredux_physics::CharacterController::HUMAN.radius;

/// Ceiling on an authored capsule radius, so a mod's giant does not open a
/// channel wider than the field can represent.
const MAX_DISTURBER_RADIUS: f32 = 256.0;

/// A resident cell's scatter inputs, before chunking.
pub(crate) struct EntityCell {
    origin_xz: [f32; 2],
    vertex_offset: u32,
    water_y: f32,
    layer_affinity: [f32; 8],
}

/// CPU payload for the bindless Tier-3 detail atlas. Its rows are generated
/// from the active engine species rather than guessed from GRAS card assets:
/// the palette deliberately owns the blade identity (§12.12).
pub(crate) struct GroundCoverDetailAtlas {
    pub(crate) pixels: Vec<u8>,
    pub(crate) species_count: u32,
    pub(crate) signature: u64,
}

/// Build one repeatable texture row per palette species. RGB is the species'
/// own base/tip gradient. Alpha is a small deterministic blade silhouette:
/// terrain consumes it as compact height detail while Tier 2 uses the same
/// palette-owned rows as clump-card cutouts. `d_ground`, evaluated in the
/// terrain fragment, remains the sole density authority.
pub(crate) fn build_groundcover_detail_atlas(world: &World) -> GroundCoverDetailAtlas {
    use byroredux_core::ecs::components::groundcover::GroundCoverSpecies;

    let fallback = [GroundCoverSpecies::DEFAULT_TEMPERATE];
    let palette = world.try_resource::<GroundCoverPalette>();
    let species: &[GroundCoverSpecies] = match palette.as_ref() {
        Some(p) if !p.species.is_empty() => &p.species,
        _ => &fallback,
    };
    let count = species.len().min(MAX_GROUNDCOVER_SPECIES);
    let edge = GROUNDCOVER_DETAIL_ATLAS_EDGE as usize;
    let mut pixels = vec![0u8; edge * edge * count * 4];
    let mut signature = count as u64;
    for (species_index, species) in species.iter().take(count).enumerate() {
        for channel in species.colour_gradient.iter().flatten() {
            signature = signature.rotate_left(7) ^ u64::from(channel.to_bits());
        }
        for y in 0..edge {
            for x in 0..edge {
                // Integer-only checker phase: immutable texture detail must
                // never use time or frame state, for the same reason the
                // blade seed cannot (§3's AMD determinism requirement).
                let bit = ((x.wrapping_mul(13) ^ y.wrapping_mul(17) ^ species_index) & 1) as f32;
                let colour = [0, 1, 2].map(|channel| {
                    species.colour_gradient[0][channel]
                        + (species.colour_gradient[1][channel]
                            - species.colour_gradient[0][channel])
                            * bit
                });
                let texel = ((species_index * edge + y) * edge + x) * 4;
                for channel in 0..3 {
                    pixels[texel + channel] = linear_to_srgb8(colour[channel]);
                }
                // Tier 2 samples this same immutable atlas as a clump card.
                // Four narrow full-height strokes are derived from Outerra's
                // already-cited `GROUNDCOVER_BLADES_PER_POINT`, so the alpha
                // lane is a deterministic grass silhouette rather than the
                // old checker that only served Tier 3 normal detail. The
                // terrain still reads it as a compact local height field.
                let blade_count = GROUNDCOVER_BLADES_PER_POINT as usize;
                let lane = x * blade_count / edge;
                let lane_start = lane * edge / blade_count;
                let lane_width = (edge / (blade_count * blade_count)).max(1);
                let lane_centre = lane_start + edge / (blade_count * 2);
                let left = lane_centre.saturating_sub(lane_width / 2);
                let right = (left + lane_width).min(edge);
                pixels[texel + 3] = if x >= left && x < right { u8::MAX } else { 0 };
            }
        }
    }
    GroundCoverDetailAtlas {
        pixels,
        species_count: count as u32,
        signature,
    }
}

fn linear_to_srgb8(value: f32) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
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
    card_atlas_handle: u32,
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
            card_atlas: [card_atlas_handle, 0, 0, 0],
        });
    }
}

/// Build the scatter's species selection table from the palette's climate
/// weights (§7).
///
/// `GroundCoverSpecies::climate_weight` was resolved at the translate boundary
/// and then never read: the scatter picked `hash % species_count`, so every
/// species in the load order was equally likely everywhere — Skyrim's
/// underwater kelp on dry tundra as often as the tundra grass. The weight in
/// the palette's own climate is the selection probability §7 specifies.
///
/// Indexed in the same order and truncated at the same
/// [`MAX_GROUNDCOVER_SPECIES`] as [`collect_groundcover_species`], so a table
/// entry always names a species the GPU buffer holds.
pub(crate) fn collect_groundcover_species_table(world: &World, out: &mut Vec<u32>) {
    out.clear();
    let weights: Vec<f32> = match world.try_resource::<GroundCoverPalette>() {
        Some(palette) if !palette.species.is_empty() => palette
            .species
            .iter()
            .take(MAX_GROUNDCOVER_SPECIES)
            .map(|s| s.climate_weight.weight_for(palette.climate))
            .collect(),
        // Matches `collect_groundcover_species`' single built-in fallback.
        _ => vec![1.0],
    };
    out.extend_from_slice(&species_selection_table(&weights));
}

/// #4413 — the authored-model tier's records and record-selection table for
/// this frame, from the worldspace's [`AuthoredCover`]. Returns the candidate
/// grid spacing, or `None` (and empty outputs) when the worldspace has no
/// authored cover.
///
/// Each record's cover-test reach is its tallest scaled instance: its authored
/// height scaled by its full height variation, or — for the records that
/// author no height — the blade palette's tallest blade, the reach the blade
/// scatter already uses.
///
/// [`AuthoredCover`]: byroredux_core::ecs::components::groundcover::AuthoredCover
pub(crate) fn collect_groundcover_model_records(
    world: &World,
    records: &mut Vec<byroredux_renderer::vulkan::groundcover_models::GpuGroundCoverModelRecord>,
    table: &mut Vec<u32>,
) -> Option<f32> {
    use byroredux_core::ecs::components::groundcover::AuthoredCover;
    use byroredux_renderer::shader_constants::{
        GROUNDCOVER_MODEL_RECORD_FLAG_FIT_TO_SLOPE, GROUNDCOVER_MODEL_RECORD_FLAG_UNIFORM_SCALING,
    };
    use byroredux_renderer::vulkan::groundcover_models::GpuGroundCoverModelRecord;
    records.clear();
    table.clear();
    let cover = world.try_resource::<AuthoredCover>()?;
    let blade_reach = world
        .try_resource::<GroundCoverPalette>()
        .map(|palette| {
            palette
                .species
                .iter()
                .map(|s| s.height_range.1)
                .fold(0.0_f32, f32::max)
        })
        .unwrap_or(0.0);
    let count = cover
        .records
        .len()
        .min(byroredux_renderer::shader_constants::GROUNDCOVER_MODEL_MAX_RECORDS as usize);
    let used = &cover.records[..count];
    records.extend(used.iter().map(|record| {
        let mut flags = 0;
        if record.uniform_scaling {
            flags |= GROUNDCOVER_MODEL_RECORD_FLAG_UNIFORM_SCALING;
        }
        if record.fit_to_slope {
            flags |= GROUNDCOVER_MODEL_RECORD_FLAG_FIT_TO_SLOPE;
        }
        GpuGroundCoverModelRecord {
            density: record.density,
            water_rule: record.water_rule.gpu_code(),
            water_distance: record.water_distance,
            height_range: record.height_range,
            position_range: record.position_range,
            flags,
            shape_first: 0,
            shape_count: 0,
            cover_reach: record
                .nominal_height
                .map_or(blade_reach, |h| h * (1.0 + record.height_range)),
            pad0: 0,
            pad1: 0,
            pad2: 0,
        }
    }));
    let weights: Vec<f32> = used
        .iter()
        .map(|record| record.climate_weight.weight_for(cover.climate))
        .collect();
    table.extend_from_slice(&species_selection_table(&weights));
    Some(cover.grid_spacing)
}

/// Quantise relative weights into the scatter's fixed-size selection table.
///
/// Largest-remainder apportionment, after reserving one entry for every
/// species with a positive weight: quantisation to 1/256 must not silently
/// drop a species the palette says grows here, and a pure proportional round
/// would do exactly that to any share under 1/512. Non-positive or
/// non-finite weights get no entries. If no weight is positive the table is
/// uniform, which is what the scatter did before weights were read — the
/// neutral outcome for a palette that says nothing.
fn species_selection_table(
    weights: &[f32],
) -> [u32; byroredux_renderer::shader_constants::GROUNDCOVER_SPECIES_TABLE_SIZE as usize] {
    const SIZE: usize =
        byroredux_renderer::shader_constants::GROUNDCOVER_SPECIES_TABLE_SIZE as usize;
    let mut table = [0u32; SIZE];
    let count = weights.len().min(SIZE);
    if count == 0 {
        return table;
    }
    let clean: Vec<f64> = weights[..count]
        .iter()
        .map(|&w| {
            if w.is_finite() && w > 0.0 {
                f64::from(w)
            } else {
                0.0
            }
        })
        .collect();
    let total: f64 = clean.iter().sum();
    if total <= 0.0 {
        for (i, slot) in table.iter_mut().enumerate() {
            *slot = (i % count) as u32;
        }
        return table;
    }
    let positive = clean.iter().filter(|&&w| w > 0.0).count();
    let spare = (SIZE - positive) as f64;
    let quotas: Vec<f64> = clean.iter().map(|w| w / total * spare).collect();
    let mut counts: Vec<usize> = clean
        .iter()
        .zip(&quotas)
        .map(|(&w, &q)| if w > 0.0 { 1 + q.floor() as usize } else { 0 })
        .collect();
    let mut remaining = SIZE - counts.iter().sum::<usize>();
    let mut order: Vec<usize> = (0..count).filter(|&i| clean[i] > 0.0).collect();
    // Largest fractional part first; index breaks ties so the table is
    // identical across runs.
    order.sort_by(|&a, &b| {
        (quotas[b] - quotas[b].floor())
            .total_cmp(&(quotas[a] - quotas[a].floor()))
            .then(a.cmp(&b))
    });
    for &i in order.iter().cycle() {
        if remaining == 0 {
            break;
        }
        counts[i] += 1;
        remaining -= 1;
    }
    let mut cursor = 0;
    for (species, &n) in counts.iter().enumerate() {
        for slot in &mut table[cursor..cursor + n] {
            *slot = species as u32;
        }
        cursor += n;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shares(table: &[u32], species: usize) -> Vec<usize> {
        let mut out = vec![0; species];
        for &s in table {
            out[s as usize] += 1;
        }
        out
    }

    /// §7: selection probability is the climate weight. A table that fills
    /// every entry and splits them in proportion is that probability at the
    /// scatter's 8-bit resolution.
    #[test]
    fn species_table_is_proportional_to_weight() {
        let table = species_selection_table(&[2.0, 0.4, 0.1]);
        let s = shares(&table, 3);
        assert_eq!(s.iter().sum::<usize>(), 256);
        // 2.0 / 2.5 of the 253 unreserved entries plus its reserved one.
        assert!((s[0] as f64 - (1.0 + 253.0 * 0.8)).abs() <= 1.0, "{s:?}");
        assert!((s[1] as f64 - (1.0 + 253.0 * 0.16)).abs() <= 1.0, "{s:?}");
        assert!((s[2] as f64 - (1.0 + 253.0 * 0.04)).abs() <= 1.0, "{s:?}");
    }

    /// Quantisation must not erase a species the palette says belongs here,
    /// however small its share.
    #[test]
    fn species_table_keeps_every_positive_weight() {
        let mut weights = vec![1000.0];
        weights.extend(std::iter::repeat(0.001).take(20));
        let s = shares(&species_selection_table(&weights), weights.len());
        assert!(s.iter().all(|&n| n >= 1), "{s:?}");
        assert_eq!(s.iter().sum::<usize>(), 256);
    }

    /// Zero, negative and non-finite weights mean "never here", and a palette
    /// with no positive weight at all falls back to uniform selection.
    #[test]
    fn species_table_ignores_unusable_weights_and_falls_back_to_uniform() {
        let s = shares(&species_selection_table(&[0.0, 3.0, f32::NAN, -1.0]), 4);
        assert_eq!(s, vec![0, 256, 0, 0]);
        let s = shares(&species_selection_table(&[0.0, 0.0]), 2);
        assert_eq!(s, vec![128, 128]);
    }

    /// The table is sized to the scatter's 8 hash bits; anything else leaves
    /// entries unreachable or indexes past the end.
    #[test]
    fn species_table_matches_the_scatter_hash_bits() {
        assert_eq!(
            byroredux_renderer::shader_constants::GROUNDCOVER_SPECIES_TABLE_SIZE,
            1 << 8
        );
    }

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
    /// #4338 — the chunk cap must hold every chunk the distance cull can keep
    /// at the shipped chunk size and draw distance. A kept chunk's centre lies
    /// within `GROUNDCOVER_DRAW_DISTANCE + CHUNK_BOUND_RADIUS` horizontally, so
    /// its whole footprint lies within one more bound radius; footprints do
    /// not overlap, so that disc's area over one footprint bounds the count
    /// (167 today). §11.2's 256-unit chunk sweep bounds at ~268, and fails here
    /// rather than silently dropping chunks at runtime.
    #[test]
    fn chunk_cap_covers_every_chunk_in_reach() {
        let reach = GROUNDCOVER_DRAW_DISTANCE + 2.0 * CHUNK_BOUND_RADIUS;
        let bound = (std::f32::consts::PI * reach * reach
            / (GROUNDCOVER_CHUNK_UNITS * GROUNDCOVER_CHUNK_UNITS))
            .ceil() as u32;
        assert!(
            bound <= GROUNDCOVER_MAX_CHUNKS,
            "up to {bound} chunks can survive the distance cull, but \
             GROUNDCOVER_MAX_CHUNKS is {GROUNDCOVER_MAX_CHUNKS} — raise the cap (and \
             the blade buffer with it) alongside the chunk size or draw distance"
        );
    }

    /// #4338 — over the cap, the farthest chunks go, and the survivors keep
    /// walk order so the chunk list is unchanged on frames under the cap.
    #[test]
    fn chunk_cap_keeps_the_nearest_chunks_in_walk_order() {
        let candidate = |order: usize, cell: usize, horizontal: f32| ChunkCandidate {
            order,
            cell,
            base_xz: [order as f32, 0.0],
            horizontal,
        };
        // Walk order is cell origin, so a west-side cell's far chunks come
        // first when the camera stands to the east — exactly what a walk-order
        // cut used to keep.
        let mut over = vec![
            candidate(0, 0, 900.0),
            candidate(1, 0, 100.0),
            candidate(2, 1, 800.0),
            candidate(3, 1, 200.0),
            candidate(4, 2, 300.0),
        ];
        assert_eq!(keep_nearest_chunks(&mut over, 3), 2);
        assert_eq!(
            over.iter().map(|c| c.order).collect::<Vec<_>>(),
            vec![1, 3, 4],
            "the nearest three, back in walk order"
        );

        let mut under = vec![candidate(0, 0, 5.0), candidate(1, 0, 1.0)];
        assert_eq!(keep_nearest_chunks(&mut under, 3), 0);
        assert_eq!(
            under.iter().map(|c| c.order).collect::<Vec<_>>(),
            vec![0, 1],
            "under the cap nothing is dropped or reordered"
        );
    }

    /// #4338 — the structural fix is a fixed-slot ring, not a different
    /// ordering for the old cap. A cold ring takes the documented 24-chunk
    /// placement budget, then keeps those slot assignments while admitting
    /// more work on the next frame.
    #[test]
    fn residency_ring_places_in_budget_and_never_moves_residents() {
        let candidate = |order: usize, horizontal: f32| ChunkCandidate {
            order,
            cell: 0,
            base_xz: [order as f32 * GROUNDCOVER_CHUNK_UNITS, 0.0],
            horizontal,
        };
        let candidates: Vec<_> = (0..60)
            .map(|order| candidate(order, order as f32))
            .collect();
        let mut ring = GroundCoverResidency::default();
        let first = ring.reconcile(&candidates, 0.0);
        assert_eq!(first.len(), 24, "cold ring obeys the placement budget");
        let first_slots: Vec<_> = first
            .iter()
            .map(|(slot, chunk)| (*slot, ChunkKey::from_base(chunk.base_xz)))
            .collect();

        let second = ring.reconcile(&candidates, 0.0);
        assert_eq!(second.len(), 48, "the next frame admits another budget");
        for (slot, key) in first_slots {
            assert_eq!(ring.slots[slot], Some(key), "resident chunk moved slots");
        }
    }

    #[test]
    fn residency_ring_grows_existing_slots_but_not_newly_placed_ones() {
        let candidate = |order: usize| ChunkCandidate {
            order,
            cell: 0,
            base_xz: [order as f32 * GROUNDCOVER_CHUNK_UNITS, 0.0],
            horizontal: order as f32,
        };
        let candidates: Vec<_> = (0..25).map(candidate).collect();
        let mut ring = GroundCoverResidency::default();
        let first = ring.reconcile(&candidates[..24], 0.0);
        let existing_slot = first[0].0;
        assert_eq!(ring.entry_progress(existing_slot), 0.0);

        let second = ring.reconcile(&candidates, GROUNDCOVER_INTERACTION_HALF_LIFE_SECONDS);
        assert!(
            (ring.entry_progress(existing_slot)
                - (0.25 / GROUNDCOVER_INTERACTION_HALF_LIFE_SECONDS))
                .abs()
                < f32::EPSILON
        );
        let new_slot = second
            .iter()
            .find_map(|(slot, candidate)| (candidate.order == 24).then_some(*slot))
            .expect("the next placement budget must admit the 25th candidate");
        assert_eq!(ring.entry_progress(new_slot), 0.0);
    }

    #[test]
    fn residency_ring_capacity_is_derived_from_draw_radius() {
        let mut ring = GroundCoverResidency::default();
        ring.ensure_slot_count();
        let radius_chunks = ((GROUNDCOVER_DRAW_DISTANCE + CHUNK_BOUND_RADIUS)
            / GROUNDCOVER_CHUNK_UNITS)
            .ceil() as usize;
        assert_eq!(ring.slots.len(), (radius_chunks * 2 + 1).pow(2));
        assert!(ring.slots.len() <= GROUNDCOVER_MAX_CHUNKS as usize);
    }

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
        collect_groundcover_species(&world, GroundCoverDimmer::NEUTRAL, 0, &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].size_range[1] >= out[0].size_range[0]);
    }

    #[test]
    fn tier3_detail_atlas_is_palette_sized_and_carries_height_detail() {
        let world = World::new();
        let atlas = build_groundcover_detail_atlas(&world);
        let edge = GROUNDCOVER_DETAIL_ATLAS_EDGE as usize;
        assert_eq!(atlas.species_count, 1);
        assert_eq!(atlas.pixels.len(), edge * edge * 4);
        assert!(atlas.pixels.chunks_exact(4).any(|pixel| pixel[3] == 0));
        assert!(atlas
            .pixels
            .chunks_exact(4)
            .any(|pixel| pixel[3] == u8::MAX));
        // Tier 2 relies on this being a blade silhouette rather than the old
        // checker alpha: every occupied lane extends continuously up a row.
        let first_row_alpha: Vec<u8> = (0..edge)
            .map(|x| atlas.pixels[x * 4 + 3])
            .collect();
        for y in 1..edge {
            let row_alpha: Vec<u8> = (0..edge)
                .map(|x| atlas.pixels[(y * edge + x) * 4 + 3])
                .collect();
            assert_eq!(row_alpha, first_row_alpha);
        }

        let mut palette_world = World::new();
        palette_world.insert_resource(GroundCoverPalette::resolve(
            vec![
                byroredux_core::ecs::components::groundcover::GroundCoverSpecies::DEFAULT_TEMPERATE,
                byroredux_core::ecs::components::groundcover::GroundCoverSpecies::DEFAULT_ARID,
            ],
            byroredux_core::ecs::components::groundcover::Climate::Temperate,
        ));
        let two = build_groundcover_detail_atlas(&palette_world);
        assert_eq!(two.species_count, 2);
        assert_eq!(two.pixels.len(), edge * edge * 2 * 4);
        assert_ne!(two.signature, atlas.signature);
    }

    /// The grass dimmer has to reach the GPU record every frame, or a `WTHR`
    /// cross-fade leaves the sward tinted for whichever weather happened to be
    /// active when the worldspace resolved its palette (#4057).
    #[test]
    fn grass_dimmer_scales_every_authored_colour_but_not_the_sheen() {
        let world = World::new();
        let mut neutral = Vec::new();
        let mut dimmed = Vec::new();
        collect_groundcover_species(&world, GroundCoverDimmer::NEUTRAL, 0, &mut neutral);
        collect_groundcover_species(&world, GroundCoverDimmer(0.5), 0, &mut dimmed);
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

    #[test]
    fn species_collection_carries_the_palette_card_atlas_handle() {
        let world = World::new();
        let mut out = Vec::new();
        collect_groundcover_species(&world, GroundCoverDimmer::NEUTRAL, 37, &mut out);
        assert!(out.iter().all(|species| species.card_atlas == [37, 0, 0, 0]));
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
        collect_groundcover_disturbers(
            &world,
            Vec3::ZERO,
            &mut GroundCoverCollectScratch::default(),
            &mut out,
        );
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
        collect_groundcover_disturbers(
            &world,
            Vec3::ZERO,
            &mut GroundCoverCollectScratch::default(),
            &mut out,
        );
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
