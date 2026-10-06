//! #5094 — fog-volume clustering, split out of the volumetrics driver
//! (regression of #2256): the GPU fog-volume row, the cluster grid build,
//! the portal sweep and the grid filter that decides which volumes a
//! frame's froxels see.

use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use byroredux_core::math::{Mat3, Quat, Vec3};
use crate::shader_constants::{
    FOG_VOLUME_CLUSTER_DIM as GLSL_FOG_VOLUME_CLUSTER_DIM, FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR,
    FOG_VOLUME_PROFILE_HOMOGENEOUS, FOG_VOLUME_PROFILE_LIGHT_SHAFT, FOG_VOLUME_PROFILE_SMOKE, FOG_VOLUME_SHAPE_BOX,
    FOG_VOLUME_SHAPE_CONE,
    FOG_VOLUME_SHAPE_SPHERE, MAX_FOG_PORTALS_PER_CLUSTER as GLSL_MAX_FOG_PORTALS_PER_CLUSTER,
    MAX_FOG_VOLUMES_PER_CLUSTER as GLSL_MAX_FOG_VOLUMES_PER_CLUSTER,
};

/// Maximum authored local volumes uploaded after CPU frustum/distance culling.
pub const MAX_GPU_FOG_VOLUMES: usize = 512;

/// Camera-centered world-space cluster resolution used for local fog.
/// #2229 / REN-D3-02 — derived from `shader_constants_data.rs`'s
/// `FOG_VOLUME_CLUSTER_DIM` (the single source of truth shared with
/// `volumetrics_inject.comp`'s generated `#define`) rather than a second
/// hand-written literal, which previously risked silently desyncing CPU
/// cluster-list indexing from the GPU shader's own copy.
pub const FOG_VOLUME_CLUSTER_DIM: usize = GLSL_FOG_VOLUME_CLUSTER_DIM as usize;

pub const FOG_VOLUME_CLUSTER_COUNT: usize =
    FOG_VOLUME_CLUSTER_DIM * FOG_VOLUME_CLUSTER_DIM * FOG_VOLUME_CLUSTER_DIM;

/// Bounded primitive references per cluster. Overflow keeps the nearest
/// volumes because the CPU input list is distance-sorted. See
/// `FOG_VOLUME_CLUSTER_DIM` doc for why this derives from the shared
/// constant instead of a local literal.
pub const MAX_FOG_VOLUMES_PER_CLUSTER: usize = GLSL_MAX_FOG_VOLUMES_PER_CLUSTER as usize;

pub const MAX_FOG_PORTALS_PER_CLUSTER: usize = GLSL_MAX_FOG_PORTALS_PER_CLUSTER as usize;

pub(crate) const FOG_CLUSTER_INDEX_STRIDE: usize =
    MAX_FOG_VOLUMES_PER_CLUSTER + MAX_FOG_PORTALS_PER_CLUSTER;

pub(crate) const FOG_VOLUME_INDEX_COUNT: usize = FOG_VOLUME_CLUSTER_COUNT * FOG_CLUSTER_INDEX_STRIDE;

/// World-space analytic medium primitive consumed by
/// `volumetrics_inject.comp`.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GpuFogVolume {
    /// xyz = absolute center; w = shape (0 sphere, 1 ellipsoid, 2 box,
    /// 3 cone).
    pub center_shape: [f32; 4],
    /// xyz = world-space half extents; w = extinction per world unit. For a
    /// cone, x = bottom radius, y = half height, z = top radius.
    pub half_extents_extinction: [f32; 4],
    /// Quaternion rotating world offsets into primitive-local space.
    pub inverse_rotation: [f32; 4],
    /// rgb = single-scatter albedo; w = normalized edge softness.
    pub albedo_edge: [f32; 4],
    /// rgb = emitted radiance `L_e` in linear RGB; w = source blackbody
    /// temperature in kelvin. The inject pass uses it to couple local flame
    /// colour and radiance to the same temperature field that shapes density.
    ///
    /// The shader multiplies `rgb` by the froxel's locally evaluated
    /// absorption coefficient `sigma_a = sigma_t * (1 - albedo)` to form the
    /// emission source term of the radiative transfer equation, so emission
    /// inherits the same procedural density profile as extinction instead of
    /// filling the primitive uniformly.
    ///
    /// All-zero for passive media (fog, mist, cooled smoke), which is the
    /// overwhelming majority — the emission branch is skipped for them.
    pub emission_temperature: [f32; 4],
    /// x = procedural density profile (`FOG_VOLUME_PROFILE_*`); for an
    /// explosion, y = normalized age and z = lifetime seconds. Oil and
    /// nuclear explosion profiles share the timeline but not the shader
    /// morphology/dynamics.
    ///
    /// Source provenance belongs here instead of being inferred from albedo or
    /// emission in the shader: passive particle smoke and an authored fog box
    /// can have identical radiometric coefficients but need very different
    /// silhouettes and advection.
    /// Profile, explosion age, lifetime, and explicit sky-aperture marker.
    pub profile_params: [f32; 4],
}

// SAFETY: six `[f32; 4]` fields — homogeneous vec4-shaped arrays already
// satisfy the struct's `align(16)`, so no implicit padding is introduced
// (#3761).
unsafe impl crate::vulkan::buffer::NoUninit for GpuFogVolume {}

pub(crate) fn has_transport_emitter(volumes: &[GpuFogVolume]) -> bool {
    volumes.iter().any(|volume| {
        let profile = volume.profile_params[0];
        profile.is_finite()
            && (FOG_VOLUME_PROFILE_SMOKE - 0.5..=FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR + 0.5)
                .contains(&profile)
    })
}

/// Whether `transportCombustion`'s RK2 advection block — the 18-fetch
/// neighbour gather gated in-shader on `dt > 0.0` — should run this frame.
/// True while an emitter is present, or while transported soot from a
/// recently-removed emitter is still within its linger window; false once
/// both conditions lapse, in which case the caller sends
/// [`TRANSPORT_EXPIRED_DT`]: the shader's whole RK2 block collapses to a
/// no-op and the residual field is dropped (#4775). See #3131 (PERF-D5-01):
/// without this the stencil ran unconditionally, and its most expensive
/// branch (`incomingDynamicsFromNeighbors`) is itself gated on *low*
/// combustion activity — so the quiet majority of froxels in every
/// fog-bearing cell paid the full 18-fetch cost for a uniformly-zero field.
/// `fog_reference.w` sentinel for "combustion transport has lapsed" (#4775).
/// A zero `dt` alone means *hold* — a paused frame must not move or decay
/// the field — so expiry needs its own signal. The inject shader reads any
/// negative `w` as expired and writes the empty transport state instead of
/// carrying the ≤ 3 % residual `AEROSOL_LINGER_SECONDS` was sized to drop;
/// its `simulationDt` clamps the sentinel to 0.
pub(crate) const TRANSPORT_EXPIRED_DT: f32 = -1.0;

/// A submitted expiry write has already emptied the target field. The
/// shader may skip both transport reads and stores for this frame slot.
pub(crate) const TRANSPORT_KNOWN_EMPTY_DT: f32 = -2.0;

#[derive(Default)]
pub(crate) struct TransportFieldState {
    pub(crate) empty: [bool; MAX_FRAMES_IN_FLIGHT],
    pub(crate) pending_clear: Option<usize>,
}

impl TransportFieldState {
    pub(crate) fn prepare(&mut self, frame: usize, active: bool) -> bool {
        self.pending_clear = None;
        if active {
            // A new source invalidates the global empty proof. Be conservative
            // even if this command buffer is later abandoned.
            self.empty.fill(false);
            return false;
        }
        if self.empty[frame] {
            return true;
        }
        self.pending_clear = Some(frame);
        false
    }

    pub(crate) fn submitted(&mut self) {
        if let Some(frame) = self.pending_clear.take() {
            self.empty[frame] = true;
        }
    }
}

/// The `fog_reference.w` the inject shader receives: the history-accounted
/// step while transport is active — including `0.0` on a paused frame, which
/// holds the field — and [`TRANSPORT_EXPIRED_DT`] once it has lapsed.
pub(crate) fn transport_simulation_dt(combustion_active: bool, simulation_dt: f32) -> f32 {
    if combustion_active {
        simulation_dt
    } else {
        TRANSPORT_EXPIRED_DT
    }
}

pub(crate) fn combustion_transport_active(
    fog_volumes: &[GpuFogVolume],
    simulation_time: f32,
    combustion_active_until_seconds: f32,
) -> bool {
    has_transport_emitter(fog_volumes) || simulation_time <= combustion_active_until_seconds
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct GpuFogVolumeUpload {
    /// x: volume count; y: adaptive/pinned ray quality tier; zw: padding.
    pub(crate) count: [u32; 4],
    pub(crate) volumes: [GpuFogVolume; MAX_GPU_FOG_VOLUMES],
}

impl Default for GpuFogVolumeUpload {
    fn default() -> Self {
        Self {
            count: [0; 4],
            volumes: [GpuFogVolume::default(); MAX_GPU_FOG_VOLUMES],
        }
    }
}

// SAFETY: `count` is `[u32; 4]` (16 B) and `volumes` is an array of the
// already-`NoUninit` `GpuFogVolume` (each 96 B, a multiple of 16) — both
// fields' sizes are 16-byte multiples, so `#[repr(C)]` places no padding
// between or after them (#3761).
unsafe impl crate::vulkan::buffer::NoUninit for GpuFogVolumeUpload {}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct GpuFogClusterEntry {
    pub(crate) offset: u32,
    pub(crate) count: u32,
    pub(crate) portal_offset: u32,
    pub(crate) portal_count: u32,
}

// SAFETY: four `u32` fields — tiles the struct's declared size with no
// implicit padding (#3761).
unsafe impl crate::vulkan::buffer::NoUninit for GpuFogClusterEntry {}

/// #3834 — bytes of `GpuFogVolumeUpload` that are meaningful when `count`
/// volumes are populated: the 16-byte `count` header plus the leading
/// `count` entries of the trailing `volumes` array.
///
/// A free function, not an inline expression, so the arithmetic is pinned by
/// `fog_volume_upload_bytes_*` below rather than only by a GPU no one runs in
/// CI. Clamped to `MAX_GPU_FOG_VOLUMES` because `write_mapped_prefix` treats
/// an over-large request as "write it all" — the clamp keeps that path
/// unreachable from a corrupt count.
pub(crate) fn fog_volume_upload_bytes(volume_count: usize) -> usize {
    std::mem::size_of::<[u32; 4]>()
        + volume_count.min(MAX_GPU_FOG_VOLUMES) * std::mem::size_of::<GpuFogVolume>()
}

/// A fresh, all-zero fog-cluster entry array. #4792 — offsets are no longer
/// permanent per-slot values (#3133's `cluster_index * stride` seeding):
/// `build_fog_volume_clusters` packs each frame's index lists densely and
/// writes the touched clusters' offsets itself, so nothing needs seeding.
pub(crate) fn fog_cluster_entries() -> Box<[GpuFogClusterEntry; FOG_VOLUME_CLUSTER_COUNT]> {
    Box::new([GpuFogClusterEntry::default(); FOG_VOLUME_CLUSTER_COUNT])
}

/// One admitted (cluster, volume) reference, recorded in admission order by
/// `build_fog_volume_clusters`' intersection pass and scattered into the
/// dense index list once every cluster's counts — and so its offsets — are
/// known (#4792).
#[derive(Debug, Clone, Copy)]
pub(crate) struct FogClusterRef {
    pub(crate) cluster: u16,
    pub(crate) volume: u16,
    pub(crate) portal: bool,
}

/// What one `build_fog_volume_clusters` call produced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FogClusterBuild {
    /// xyz = grid minimum corner, w = 1 / cell size (`local_volume_grid`).
    pub(crate) grid: [f32; 4],
    /// Highest touched cluster index + 1 — the entry prefix that can hold a
    /// non-zero count (#3834).
    pub(crate) cluster_hi: usize,
    /// Lowest touched entry, or FOG_VOLUME_CLUSTER_COUNT for an empty build.
    pub(crate) cluster_lo: usize,
    /// Live length of the dense index list: every touched cluster's density
    /// and portal segments lie in `indices[..index_len]` (#4792).
    pub(crate) index_len: usize,
}

/// #2242 (REN-D16-04) — every call resets every entry's `count` to 0 before
/// repopulating from `volumes` (and rewrites the touched clusters' offsets:
/// the index list is packed densely per frame, #4792). There is no incremental/partial update to `count`: a cell
/// transition that changes the fog-volume list (or shifts the camera-centred
/// grid origin under it) can never leave a stale nonzero `count` from a
/// previous frame's call sitting in a cluster cell this frame's rebuild
/// didn't happen to touch. Combined with `fogVolumeCount` (`upload.count`)
/// being unconditionally rewritten every frame in `dispatch` below —
/// including the branch that skips calling this function entirely — this is
/// why `volumetrics_inject.comp`'s `localFogCluster` early-out is a complete
/// guard, not a fragile lone line of defense against replaying a previous
/// cell's local fog. `indices` needs no per-frame reset at all — see the
/// comment at its `count = 0` reset loop below for why.
/// Conservative world-space box for cluster assignment. The old enclosing
/// sphere expanded a tall, narrow beam's horizontal reach to its full height,
/// so a few authored shafts filled unrelated clusters before nearby media
/// could enter the bounded list. A rotated local box remains conservative for
/// spheres, ellipsoids, boxes, and the cone's base-radius cylinder.
/// Whether `volume` carries analytic shape `id` (a generated `FOG_VOLUME_SHAPE_*`
/// constant). An exact compare, not an `x.5` threshold, so reordering
/// `FogShape` cannot silently re-route a shape (#4955). The id is written as
/// `shape as u32 as f32`, which is exact.
pub(crate) fn fog_volume_is_shape(volume: &GpuFogVolume, id: u32) -> bool {
    volume.center_shape[3] == id as f32
}

pub(crate) fn fog_volume_world_aabb_half_extents(volume: &GpuFogVolume) -> Option<[f32; 3]> {
    let extent = Vec3::from_array([
        volume.half_extents_extinction[0],
        volume.half_extents_extinction[1],
        volume.half_extents_extinction[2],
    ]);
    if !extent.is_finite() || extent.min_element() <= 0.0 {
        return None;
    }
    let local = if fog_volume_is_shape(volume, FOG_VOLUME_SHAPE_SPHERE) {
        Vec3::splat(extent.x)
    } else if fog_volume_is_shape(volume, FOG_VOLUME_SHAPE_CONE) {
        Vec3::new(extent.x, extent.y, extent.x)
    } else {
        extent
    };
    let inverse = Quat::from_array(volume.inverse_rotation);
    let rotation = if inverse.is_finite() && inverse.length_squared() > 1.0e-8 {
        inverse.normalize().conjugate()
    } else {
        Quat::IDENTITY
    };
    let basis = Mat3::from_quat(rotation);
    let world = basis.x_axis.abs() * local.x
        + basis.y_axis.abs() * local.y
        + basis.z_axis.abs() * local.z;
    world.is_finite().then_some(world.to_array())
}

/// Cull before both the dispatch/transport gate and cluster construction.
/// A distant source must not arm the grid-wide simulation or its linger.
/// Keep remote apertures whose sunlight sweep can still reach this grid.
pub(crate) fn filter_fog_volumes_for_grid(
    volumes: &[GpuFogVolume],
    camera_pos: [f32; 3],
    far_distance: f32,
    sun_direction: [f32; 3],
    portal_sweep: bool,
    out: &mut Vec<GpuFogVolume>,
) {
    let camera = Vec3::from_array(camera_pos);
    let far = far_distance.max(1.0);
    out.clear();
    out.extend(volumes.iter().filter(|volume| {
        fog_volume_within_grid_reach(volume, camera, far)
            || (portal_sweep && fog_portal_swept_bounds(
                volume,
                Vec3::from_array(sun_direction),
                camera - Vec3::splat(far),
                far,
            ).is_some())
    }).copied());
}

/// The distance half of [`filter_fog_volumes_for_grid`]: whether `volume`'s
/// conservative world box can overlap the camera-centred grid of reach `far`.
pub(crate) fn fog_volume_within_grid_reach(volume: &GpuFogVolume, camera: Vec3, far: f32) -> bool {
    let center = Vec3::new(volume.center_shape[0], volume.center_shape[1], volume.center_shape[2]);
    let Some(extent) = fog_volume_world_aabb_half_extents(volume) else { return false };
    if !center.is_finite() {
        return false;
    }
    let radius = Vec3::from_array(extent).length();
    center.distance(camera) <= far + radius
}

/// Whether a nuclear source can have contributed to a combustion field
/// accumulated on the grid centred at `grid_center` (#4968). Uses the same
/// reach test as [`filter_fog_volumes_for_grid`] — the portal sweep admits
/// only light shafts, never a nuclear profile — so a cloud the grid culled,
/// which contributed no moments, does not dim the fires that did.
pub(crate) fn nuclear_source_reaches_grid(volumes: &[GpuFogVolume], grid_center: [f32; 3], far: f32) -> bool {
    let camera = Vec3::from_array(grid_center);
    let far = far.max(1.0);
    volumes.iter().any(|volume| {
        (volume.profile_params[0] - FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR).abs() < 0.5
            && fog_volume_within_grid_reach(volume, camera, far)
    })
}

/// #4784 — CPU twin of the shader's `isTransportedProfile`: every authored
/// profile except homogeneous dust and light shafts feeds the transported
/// combustion field, so its clusters must seed the occupancy mask even on
/// the ignition frame (the GPU marks are one frame behind).
pub(crate) fn is_transported_profile(volume: &GpuFogVolume) -> bool {
    (volume.profile_params[0] - FOG_VOLUME_PROFILE_HOMOGENEOUS).abs() >= 0.5
        && (volume.profile_params[0] - FOG_VOLUME_PROFILE_LIGHT_SHAFT).abs() >= 0.5
}

/// World-space candidate envelope of an authored cone ring or window plane
/// swept along incoming sunlight through the camera-centred grid.
pub(crate) struct FogPortalSweep {
    pub(crate) source: Vec3,
    pub(crate) direction: Vec3,
    pub(crate) radius: f32,
    pub(crate) lower: Vec3,
    pub(crate) upper: Vec3,
}

pub(crate) fn fog_portal_swept_bounds(
    volume: &GpuFogVolume,
    sun_direction: Vec3,
    grid_min: Vec3,
    far: f32,
) -> Option<FogPortalSweep> {
    if (volume.profile_params[0] - FOG_VOLUME_PROFILE_LIGHT_SHAFT).abs() >= 0.5
        || !(fog_volume_is_shape(volume, FOG_VOLUME_SHAPE_CONE)
            || (fog_volume_is_shape(volume, FOG_VOLUME_SHAPE_BOX)
                && volume.profile_params[3] >= 0.5))
        || !sun_direction.is_finite()
        || sun_direction.length_squared() <= 1.0e-8
    {
        return None;
    }
    let half_extents = Vec3::from_array([
        volume.half_extents_extinction[0],
        volume.half_extents_extinction[1],
        volume.half_extents_extinction[2],
    ]);
    if !half_extents.is_finite() || half_extents.min_element() <= 0.0 {
        return None;
    }
    let inverse = Quat::from_array(volume.inverse_rotation);
    if !inverse.is_finite() || inverse.length_squared() <= 1.0e-8 {
        return None;
    }
    let center = Vec3::from_array([
        volume.center_shape[0],
        volume.center_shape[1],
        volume.center_shape[2],
    ]);
    let is_cone = fog_volume_is_shape(volume, FOG_VOLUME_SHAPE_CONE);
    let radius = if is_cone {
        half_extents.z
    } else {
        half_extents.x.hypot(half_extents.y)
    };
    let source = if is_cone {
        center + inverse.normalize().conjugate() * Vec3::Y * half_extents.y
    } else {
        center
    };
    if !source.is_finite() {
        return None;
    }
    let direction = -sun_direction.normalize();
    let expanded_min = grid_min - Vec3::splat(radius);
    let expanded_max = grid_min + Vec3::splat(2.0 * far + radius);
    let mut t_near = 0.0f32;
    let mut t_far = f32::INFINITY;
    for axis in 0..3 {
        let start = source[axis];
        let step = direction[axis];
        if step.abs() < 1.0e-6 {
            if start < expanded_min[axis] || start > expanded_max[axis] {
                return None;
            }
        } else {
            let a = (expanded_min[axis] - start) / step;
            let b = (expanded_max[axis] - start) / step;
            t_near = t_near.max(a.min(b));
            t_far = t_far.min(a.max(b));
        }
    }
    if !t_far.is_finite() || t_far < t_near {
        return None;
    }
    let first = source + direction * t_near;
    let last = source + direction * t_far;
    let lower = first.min(last) - Vec3::splat(radius);
    let upper = first.max(last) + Vec3::splat(radius);
    (lower.is_finite() && upper.is_finite()).then_some(FogPortalSweep {
        source,
        direction,
        radius,
        lower,
        upper,
    })
}

/// Slab-test each candidate cluster against the sun-swept source ring.
/// The coarse whole-sweep AABB alone would fill a diagonal rectangle of
/// unrelated clusters, evicting nearby apertures from their bounded lists.
pub(crate) fn fog_portal_intersects_cluster(sweep: &FogPortalSweep, cell_min: Vec3, cell_size: f32) -> bool {
    let lower = cell_min - Vec3::splat(sweep.radius);
    let upper = cell_min + Vec3::splat(cell_size + sweep.radius);
    let mut t_near = 0.0f32;
    let mut t_far = f32::INFINITY;
    for axis in 0..3 {
        let start = sweep.source[axis];
        let step = sweep.direction[axis];
        if step.abs() < 1.0e-6 {
            if start < lower[axis] || start > upper[axis] {
                return false;
            }
        } else {
            let a = (lower[axis] - start) / step;
            let b = (upper[axis] - start) / step;
            t_near = t_near.max(a.min(b));
            t_far = t_far.min(a.max(b));
        }
    }
    t_far >= t_near
}

/// `portal_sweep` gates the sun-swept LightShaft/aperture candidate lists
/// (#4792): their only reader is the inject shader's `localSkyAperture`,
/// which runs only in sealed interiors and whose visibility result only
/// scales `sun_color`, so an open-sky frame or a zero-radiance sun needs none.
///
/// `refs` is caller-owned scratch (cleared here) so the per-frame build does
/// not allocate once it has reached its high-water mark.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_fog_volume_clusters(
    volumes: &[GpuFogVolume],
    camera_pos: [f32; 3],
    far_distance: f32,
    sun_direction: [f32; 3],
    portal_sweep: bool,
    upload: &mut GpuFogVolumeUpload,
    entries: &mut [GpuFogClusterEntry; FOG_VOLUME_CLUSTER_COUNT],
    indices: &mut [u32],
    refs: &mut Vec<FogClusterRef>,
    occupancy: &mut [u32; FOG_VOLUME_CLUSTER_COUNT],
) -> FogClusterBuild {
    let far = far_distance.max(1.0);
    let cell_size = (2.0 * far) / FOG_VOLUME_CLUSTER_DIM as f32;
    let grid_min = [
        camera_pos[0] - far,
        camera_pos[1] - far,
        camera_pos[2] - far,
    ];

    // Only the counts reset here; offsets are rewritten below for the
    // touched prefix, and indices need no reset at all: the shader only ever
    // reads `fogClusterIndices[cluster.offset + i]` for `i <
    // min(cluster.count, MAX_FOG_VOLUMES_PER_CLUSTER)` (and the portal twin),
    // so an untouched cluster's stale offset or index slot is never observed.
    for entry in entries.iter_mut() {
        entry.count = 0;
        entry.portal_count = 0;
    }
    // #4784 — reset alongside the entry counts: the mask this frame's
    // dispatch uploads is exactly the CPU seed marks below (GPU marks for the
    // NEXT frame accumulate on top of it in the inject pass).
    occupancy.fill(0);
    refs.clear();

    // #3834 — highest touched cluster index + 1, i.e. the length of the
    // `entries` prefix this frame can leave non-zero. `dispatch` uploads only
    // that prefix instead of all 4096 entries and their index segments. Reported rather
    // than rediscovered by a post-hoc scan because the clustering loop below
    // already visits exactly the touched set.
    let mut cluster_hi = 0usize;
    let mut cluster_lo = FOG_VOLUME_CLUSTER_COUNT;

    let volume_count = volumes.len().min(MAX_GPU_FOG_VOLUMES);
    upload.count = [volume_count as u32, 0, 0, 0];
    upload.volumes[..volume_count].copy_from_slice(&volumes[..volume_count]);

    for (volume_index, volume) in upload.volumes[..volume_count].iter().enumerate() {
        let center = [
            volume.center_shape[0],
            volume.center_shape[1],
            volume.center_shape[2],
        ];
        let Some(world_extent) = fog_volume_world_aabb_half_extents(volume) else {
            continue;
        };
        if !center.iter().all(|value| value.is_finite()) {
            continue;
        }

        if let Some(sweep) = portal_sweep
            .then(|| {
                fog_portal_swept_bounds(
                    volume,
                    Vec3::from_array(sun_direction),
                    Vec3::from_array(grid_min),
                    far,
                )
            })
            .flatten()
        {
            let mut ranges = [(0usize, 0usize); 3];
            let mut intersects_grid = true;
            for axis in 0..3 {
                let lo = (sweep.lower[axis] - grid_min[axis]) / cell_size;
                let hi = (sweep.upper[axis] - grid_min[axis]) / cell_size;
                if hi < 0.0 || lo >= FOG_VOLUME_CLUSTER_DIM as f32 {
                    intersects_grid = false;
                    break;
                }
                ranges[axis] = (
                    lo.floor().clamp(0.0, (FOG_VOLUME_CLUSTER_DIM - 1) as f32) as usize,
                    hi.floor().clamp(0.0, (FOG_VOLUME_CLUSTER_DIM - 1) as f32) as usize,
                );
            }
            if intersects_grid {
                for z in ranges[2].0..=ranges[2].1 {
                    for y in ranges[1].0..=ranges[1].1 {
                        for x in ranges[0].0..=ranges[0].1 {
                            let cell_min = Vec3::from_array(grid_min)
                                + Vec3::new(x as f32, y as f32, z as f32) * cell_size;
                            if !fog_portal_intersects_cluster(&sweep, cell_min, cell_size) {
                                continue;
                            }
                            let cluster_index = x
                                + y * FOG_VOLUME_CLUSTER_DIM
                                + z * FOG_VOLUME_CLUSTER_DIM * FOG_VOLUME_CLUSTER_DIM;
                            let entry = &mut entries[cluster_index];
                            if entry.portal_count as usize >= MAX_FOG_PORTALS_PER_CLUSTER {
                                continue;
                            }
                            refs.push(FogClusterRef {
                                cluster: cluster_index as u16,
                                volume: volume_index as u16,
                                portal: true,
                            });
                            entry.portal_count += 1;
                            cluster_hi = cluster_hi.max(cluster_index + 1);
                            cluster_lo = cluster_lo.min(cluster_index);
                        }
                    }
                }
            }
        }

        // #4807 — an aperture with no extinction contributes no scattering
        // or emission (both are multiplied by sigma_t in sampleLocalMedium).
        // Keep its sun-swept portal references above, but do not consume the
        // density list's capacity or evaluate its procedural profile. Positive
        // densities remain exact; choosing a low-density cutoff needs images
        // and measurements, not an arbitrary epsilon here.
        if (volume.profile_params[0] - FOG_VOLUME_PROFILE_LIGHT_SHAFT).abs() < 0.5
            && volume.half_extents_extinction[3] <= 0.0
        {
            continue;
        }

        let mut ranges = [(0usize, 0usize); 3];
        let mut intersects_grid = true;
        for axis in 0..3 {
            let lower = (center[axis] - world_extent[axis] - grid_min[axis]) / cell_size;
            let upper = (center[axis] + world_extent[axis] - grid_min[axis]) / cell_size;
            if upper < 0.0 || lower >= FOG_VOLUME_CLUSTER_DIM as f32 {
                intersects_grid = false;
                break;
            }
            ranges[axis] = (
                lower
                    .floor()
                    .clamp(0.0, (FOG_VOLUME_CLUSTER_DIM - 1) as f32) as usize,
                upper
                    .floor()
                    .clamp(0.0, (FOG_VOLUME_CLUSTER_DIM - 1) as f32) as usize,
            );
        }
        if !intersects_grid {
            continue;
        }

        let transported_source = is_transported_profile(volume);
        for z in ranges[2].0..=ranges[2].1 {
            for y in ranges[1].0..=ranges[1].1 {
                for x in ranges[0].0..=ranges[0].1 {
                    let cluster_index = x
                        + y * FOG_VOLUME_CLUSTER_DIM
                        + z * FOG_VOLUME_CLUSTER_DIM * FOG_VOLUME_CLUSTER_DIM;
                    // #4784 — seed the occupancy mask for every cluster a
                    // transported source spans, BEFORE the capacity check
                    // below: ignition must reach the mask even when the
                    // density list is full, or the first frame of a fire in a
                    // busy cell would transport nowhere.
                    if transported_source {
                        occupancy[cluster_index] = 1;
                    }
                    let entry = &mut entries[cluster_index];
                    if entry.count as usize >= MAX_FOG_VOLUMES_PER_CLUSTER {
                        continue;
                    }
                    refs.push(FogClusterRef {
                        cluster: cluster_index as u16,
                        volume: volume_index as u16,
                        portal: false,
                    });
                    entry.count += 1;
                    cluster_hi = cluster_hi.max(cluster_index + 1);
                    cluster_lo = cluster_lo.min(cluster_index);
                }
            }
        }
    }

    // #4792 — pack the lists densely: each touched cluster's density segment
    // then its portal segment, in cluster order. The old fixed 192-slot
    // segment per cluster made the upload O(touched clusters × capacity);
    // this makes it O(live references). Admission order within a cluster is
    // preserved, so overflow still keeps the nearest volumes.
    let mut next = 0u32;
    for entry in entries[..cluster_hi].iter_mut() {
        entry.offset = next;
        next += entry.count;
        entry.portal_offset = next;
        next += entry.portal_count;
        entry.count = 0;
        entry.portal_count = 0;
    }
    for reference in refs.iter() {
        let entry = &mut entries[reference.cluster as usize];
        if reference.portal {
            indices[(entry.portal_offset + entry.portal_count) as usize] = reference.volume as u32;
            entry.portal_count += 1;
        } else {
            indices[(entry.offset + entry.count) as usize] = reference.volume as u32;
            entry.count += 1;
        }
    }

    FogClusterBuild {
        grid: [grid_min[0], grid_min[1], grid_min[2], cell_size.recip()],
        cluster_hi,
        cluster_lo,
        index_len: next as usize,
    }
}

pub(crate) fn fog_cluster_write_range(current: (usize, usize), previous: (usize, usize)) -> (usize, usize) {
    (current.0.min(previous.0), current.1.max(previous.1))
}
