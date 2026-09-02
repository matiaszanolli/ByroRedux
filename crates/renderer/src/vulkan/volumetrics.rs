//! Volumetric lighting pipeline (M55, Tier 8).
//!
//! Frostbite-style froxel volumetrics: a 3D texture indexed by
//! `(screenUV.x, screenUV.y, sliceZ)` where `sliceZ` is a non-linear
//! function of view-space depth (denser slices near the camera).
//! Each froxel stores RGB scattered radiance + alpha transmittance.
//! The composite fragment shader samples the integrated volume and
//! modulates the scene color: `final = scene * vol.a + vol.rgb`.
//!
//! The raw V-buffer stores `(in-scattered radiance.rgb, sigma_t)` and the
//! integrated volume stores `(accumulated radiance.rgb, transmittance)`.
//! Horizontal dimensions derive from the post-upscaler-query render extent;
//! depth uses a 5 m linear floor followed by exponential slices. Injection
//! evaluates procedural density, dual-lobe HG scattering, clustered local
//! lights, and directional/local TLAS visibility. One sample per froxel is
//! blue-noise jittered and reprojected from the previous raw V-buffer.
//!
//! `composite.frag` consumes one integrated 3D sample, adds analytic
//! beyond-grid height fog, and writes volumetric coverage into FSR's reactive
//! and transparency/composition masks.

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::descriptors::{
    image_barrier_general_write_to_read, image_barrier_undef_to_general, memory_barrier,
    write_acceleration_structure, write_combined_image_sampler, write_storage_buffer,
    write_storage_image, write_uniform_buffer, DescriptorPoolBuilder,
};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::scene_buffer::GpuLight;
use super::sync::MAX_FRAMES_IN_FLIGHT;
use super::upscaling::VolumetricsConfig;
use crate::shader_constants::{
    ATTENUATION_MODEL_INVERSE_SQUARE, COMBUSTION_AEROSOL_LINGER_SECONDS,
    COMBUSTION_LIGHT_FIXED_SCALE, COMBUSTION_LIGHT_GRID_COUNT as GLSL_COMBUSTION_LIGHT_GRID_COUNT,
    COMBUSTION_LIGHT_GRID_X, COMBUSTION_LIGHT_GRID_Y, COMBUSTION_LIGHT_GRID_Z,
    COMBUSTION_LIGHT_HALF_EXTENT_XZ_METERS, COMBUSTION_LIGHT_HALF_EXTENT_Y_METERS,
    COMBUSTION_LIGHT_VOLUME_FIXED_SCALE, FOG_VOLUME_CLUSTER_DIM as GLSL_FOG_VOLUME_CLUSTER_DIM,
    MAX_FOG_VOLUMES_PER_CLUSTER as GLSL_MAX_FOG_VOLUMES_PER_CLUSTER, MAX_LIGHTS,
    VISIBILITY_MASK_FULL, WORKGROUP_X, WORKGROUP_Y, WORKGROUP_Z,
};
pub use crate::shader_constants::{
    FOG_VOLUME_PROFILE_EXPLOSION, FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR,
    FOG_VOLUME_PROFILE_EXPLOSION_OIL, FOG_VOLUME_PROFILE_FLAME, FOG_VOLUME_PROFILE_HOMOGENEOUS,
    FOG_VOLUME_PROFILE_SMOKE,
};
use anyhow::{Context, Result};
use ash::vk;
use gpu_allocator::vulkan as vk_alloc;

mod noise;
use noise::{
    cached_base_density_noise, cached_detail_density_noise, BASE_NOISE_SIZE, DETAIL_NOISE_SIZE,
};

const VOLUMETRICS_INJECT_COMP_SPV: &[u8] =
    include_bytes!("../../shaders/volumetrics_inject.comp.spv");
const VOLUMETRICS_INTEGRATE_COMP_SPV: &[u8] =
    include_bytes!("../../shaders/volumetrics_integrate.comp.spv");

/// Parameters uploaded to the volumetric injection shader as a UBO
/// each frame. Layout matches `VolumetricsParams` in
/// `volumetrics_inject.comp` — `validate_set_layout` enforces the
/// binding shape, but the std140 field layout is the host's
/// responsibility (each `vec4` is 16-byte aligned, `mat4` is
/// 4 × vec4 = 64 bytes).
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct VolumetricsParams {
    /// Inverse view-projection matrix; reconstructs world-space rays
    /// from screen-space (uv, NDC z = 1 = far) per froxel.
    pub inv_view_proj: [[f32; 4]; 4],
    /// Previous view-projection matrix rebased to consume positions relative
    /// to this frame's render origin. Used to reproject V-buffer history.
    pub prev_view_proj: [[f32; 4]; 4],
    /// xyz = camera world position (Bethesda units), w = base extinction
    /// coefficient (1 / world unit).
    pub camera_pos: [f32; 4],
    /// xyz = previous absolute camera position; w = history-valid flag.
    pub prev_camera_pos: [f32; 4],
    /// xyz = direction TO the sun (world space, unit; matches
    /// GpuCamera.sun_direction / GpuLight.direction_angle, #1937), w = HG
    /// phase asymmetry parameter g in (-1, 1).
    pub sun_dir: [f32; 4],
    /// rgb = sun radiance (already scaled by intensity), a = the cell's
    /// XCLL fog-far distance (matches the `screen.w` field
    /// `cluster_cull.comp` uses for its exponential depth-slice
    /// distribution — see `clusters.glsl::getClusterIndex`). Phase 2b
    /// point-light injection needs to compute the SAME cluster index a
    /// froxel's world position would map to, to look up that cluster's
    /// pre-built light list; reusing the identical fog-far basis keeps
    /// the two slicing schemes aligned instead of drifting apart.
    pub sun_color: [f32; 4],
    /// x = grid far plane, y = linear-depth floor, z = fraction of slices
    /// assigned to the linear section, w = monotonic frame index. Distances
    /// are in Bethesda world units.
    pub volume_params: [f32; 4],
    /// #markarth-precision — xyz = camera-relative render origin; the inject
    /// shader adds it to the `inv_view_proj`-reconstructed far point so froxel
    /// world positions (and their TLAS shadow rays) are absolute. w = 1.0 for
    /// an exterior cell, 0.0 for interior — selects the inject shader's
    /// shadow-ray strategy: exterior keeps the single opaque-mask query
    /// (no-hit = lit, real open sky); interior runs the two-pass query
    /// (opaque-mask first; only if that misses, a bounded glass-mask pass —
    /// no-hit-in-either = occluded) so a geometry gap (e.g. a missing
    /// ceiling mesh) can't masquerade as a window. See the interior-godray
    /// investigation note in `context/draw.rs` near `VolumetricsParams`
    /// construction.
    pub render_origin: [f32; 4],
    /// x = single-scatter albedo, y = backward HG lobe g,
    /// z = forward-lobe mixture, w = height-fog scale height in world units.
    pub medium_params: [f32; 4],
    /// rgb = normalized authored fog chromaticity. The inject shader
    /// multiplies this by `medium_params.x` to recover spectral
    /// single-scatter albedo without letting tint alter extinction or the
    /// authored medium's peak scattering energy. w = the sanitized peak of
    /// the authored apparent fog colour, allowing the homogeneous-medium
    /// source term to converge back to that colour even in sunless interiors.
    pub fog_tint: [f32; 4],
    /// x = temporal history weight, y = relative density rejection strength,
    /// z = total time in seconds, w = procedural coverage.
    pub temporal_params: [f32; 4],
    /// xyz = minimum corner of the camera-centered local-volume cluster cube;
    /// w = reciprocal world-space cluster-cell size.
    pub local_volume_grid: [f32; 4],
    /// x = world-space height-fog reference altitude (REN-D16-01 / #2225)
    /// — absolute space, matching `camera_pos`/`render_origin`. A downward
    /// ray-cast ground height near the camera, or the camera's own Y as a
    /// fallback when no ground is found. Consumed by
    /// `proceduralDensityScale`'s `refHeight` parameter instead of
    /// `camera_pos.y`, which made froxel fog density peak at eye level and
    /// follow the player vertically instead of thinning with real
    /// altitude. y = temporal history weight applied where the froxel's
    /// source term is dominated by thermal emission (see
    /// [`DEFAULT_EMISSIVE_HISTORY_WEIGHT`]). z = adaptive maximum local
    /// lights evaluated per froxel. w = simulation delta seconds, filled and
    /// clamped by [`VolumetricsPipeline::dispatch`] after successful-history
    /// time accounting.
    pub fog_reference: [f32; 4],
}

/// Maximum authored local volumes uploaded after CPU frustum/distance culling.
pub const MAX_GPU_FOG_VOLUMES: usize = 128;
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
const FOG_VOLUME_INDEX_COUNT: usize = FOG_VOLUME_CLUSTER_COUNT * MAX_FOG_VOLUMES_PER_CLUSTER;

/// World-space analytic medium primitive consumed by
/// `volumetrics_inject.comp`.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GpuFogVolume {
    /// xyz = absolute center; w = shape (0 sphere, 1 ellipsoid, 2 box).
    pub center_shape: [f32; 4],
    /// xyz = world-space half extents; w = extinction per world unit.
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
    pub profile_params: [f32; 4],
}

fn has_transport_emitter(volumes: &[GpuFogVolume]) -> bool {
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
/// both conditions lapse, in which case the caller zeroes `simulationDt` and
/// the shader's whole RK2 block collapses to a no-op. See #3131 (PERF-D5-01):
/// without this the stencil ran unconditionally, and its most expensive
/// branch (`incomingDynamicsFromNeighbors`) is itself gated on *low*
/// combustion activity — so the quiet majority of froxels in every
/// fog-bearing cell paid the full 18-fetch cost for a uniformly-zero field.
fn combustion_transport_active(
    fog_volumes: &[GpuFogVolume],
    simulation_time: f32,
    combustion_active_until_seconds: f32,
) -> bool {
    has_transport_emitter(fog_volumes) || simulation_time <= combustion_active_until_seconds
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GpuFogVolumeUpload {
    count: [u32; 4],
    volumes: [GpuFogVolume; MAX_GPU_FOG_VOLUMES],
}

impl Default for GpuFogVolumeUpload {
    fn default() -> Self {
        Self {
            count: [0; 4],
            volumes: [GpuFogVolume::default(); MAX_GPU_FOG_VOLUMES],
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct GpuFogClusterEntry {
    offset: u32,
    count: u32,
}

/// A fresh fog-cluster entry array with `offset` seeded to its final,
/// permanent value (`cluster_index * MAX_FOG_VOLUMES_PER_CLUSTER`) — a pure
/// function of array position that never changes for the lifetime of the
/// grid. #3133 (PERF-D4-01): `build_fog_volume_clusters` used to recompute
/// every entry's `offset` from scratch each frame (a 4096-iteration scalar
/// loop on top of a full-array memset) even though the value it wrote was
/// always identical to what was already there. Seeding it once here — used
/// both by `VolumetricsPipeline::new` and by tests that exercise
/// `build_fog_volume_clusters` directly — lets the per-frame rebuild reset
/// only `count`, which is the one field a full-rebuild architecture (see the
/// doc comment on `build_fog_volume_clusters`) actually needs touched every
/// frame.
fn fog_cluster_entries_with_offsets() -> Box<[GpuFogClusterEntry; FOG_VOLUME_CLUSTER_COUNT]> {
    let mut entries = Box::new([GpuFogClusterEntry::default(); FOG_VOLUME_CLUSTER_COUNT]);
    for (cluster_index, entry) in entries.iter_mut().enumerate() {
        entry.offset = (cluster_index * MAX_FOG_VOLUMES_PER_CLUSTER) as u32;
    }
    entries
}

/// Gamebryo/Fallout world-coordinate scale. The renderer keeps positions in
/// Bethesda units all the way through TLAS and shader reconstruction.
pub const WORLD_UNITS_PER_METER: f32 = byroredux_core::lighting::BETHESDA_UNITS_PER_METER;
pub const DEFAULT_GRID_FAR_METERS: f32 = 128.0;
pub const DEFAULT_VOLUME_FAR: f32 = DEFAULT_GRID_FAR_METERS * WORLD_UNITS_PER_METER;
pub const LINEAR_DEPTH_METERS: f32 = 5.0;
pub const LINEAR_DEPTH: f32 = LINEAR_DEPTH_METERS * WORLD_UNITS_PER_METER;
pub const LINEAR_SLICE_FRACTION: f32 = 0.125;

/// Default forward Henyey-Greenstein asymmetry for atmospheric scattering.
pub const DEFAULT_PHASE_G: f32 = 0.8;
pub const DEFAULT_BACKWARD_PHASE_G: f32 = -0.3;
pub const DEFAULT_DUAL_LOBE_MIX: f32 = 0.7;
pub const DEFAULT_SINGLE_SCATTER_ALBEDO: f32 = 0.95;
pub const DEFAULT_SCALE_HEIGHT_METERS: f32 = 30.0;
pub const DEFAULT_TEMPORAL_HISTORY_WEIGHT: f32 = 0.92;
pub const DEFAULT_DENSITY_REJECTION: f32 = 4.0;

/// Temporal history weight for froxels whose source term is dominated by
/// thermal emission, interpolated against [`DEFAULT_TEMPORAL_HISTORY_WEIGHT`]
/// by the emissive fraction in `volumetrics_inject.comp`.
///
/// This is a filter time-constant choice, not a physical constant, so it is
/// stated rather than derived. An exponential filter with weight `w` reaches
/// ~86% of a step change in `-1/ln(w)` frames. A 0.75 base is deliberately
/// calmer than the old 0.5 response; emission-weighted disagreement rejection
/// in the shader still cuts history quickly at a flash/explosion edge.
pub const DEFAULT_EMISSIVE_HISTORY_WEIGHT: f32 = 0.75;

/// Convert authored fog colour into a finite, energy-neutral chromaticity.
///
/// Bethesda's fog colour is an apparent in-scattering tint, while the
/// canonical medium stores a separate scalar single-scatter albedo. Dividing
/// by the strongest channel preserves the authored channel ratios and leaves
/// that scalar as the peak spectral albedo. Black remains black absorption;
/// invalid and negative channels contribute no scattering.
pub fn normalize_fog_tint(color: [f32; 3]) -> [f32; 3] {
    let sanitized = color.map(|channel| {
        if channel.is_finite() {
            channel.max(0.0)
        } else {
            0.0
        }
    });
    let peak = sanitized[0].max(sanitized[1]).max(sanitized[2]);
    if peak <= 1.0e-6 {
        [0.0; 3]
    } else {
        sanitized.map(|channel| (channel / peak).clamp(0.0, 1.0))
    }
}

/// Pack an authored apparent fog colour for volumetric injection.
///
/// RGB stores energy-neutral chromaticity for directional/local-light
/// scattering, while A stores the original peak radiance. Their product
/// reconstructs the sanitized authored colour for the medium's equilibrium
/// source term. This is what keeps interior fog visible when a CELL has no
/// directional source: extinction alone can only darken the scene.
pub fn pack_fog_tint(color: [f32; 3]) -> [f32; 4] {
    let sanitized = color.map(|channel| {
        if channel.is_finite() {
            channel.max(0.0)
        } else {
            0.0
        }
    });
    let peak = sanitized[0].max(sanitized[1]).max(sanitized[2]);
    let tint = normalize_fog_tint(sanitized);
    [tint[0], tint[1], tint[2], peak]
}

/// Optical depth through an exponential height medium
/// `sigma_t(y) = sigma0 * exp(-(y - base_height) / scale_height)`.
///
/// The explicit horizontal branch is intentional: evaluating the analytic
/// quotient at `ray_direction_y → 0` is the classic 0/0 NaN that turns an
/// otherwise ordinary exterior frame white.
pub fn height_fog_optical_depth(
    sigma0: f32,
    ray_origin_y: f32,
    ray_direction_y: f32,
    distance: f32,
    base_height: f32,
    scale_height: f32,
) -> f32 {
    if sigma0 <= 0.0 || distance <= 0.0 {
        return 0.0;
    }
    let safe_scale = scale_height.max(1.0e-4);
    let log_sigma_at_origin = sigma0.ln() - (ray_origin_y - base_height) / safe_scale;
    let sigma_at_origin = log_sigma_at_origin.clamp(-80.0, 80.0).exp();
    let length = distance.max(0.0);
    let slope = ray_direction_y / safe_scale;
    if slope.abs() < 1.0e-6 {
        return sigma_at_origin * length;
    }
    if slope > 0.0 {
        let integral = -(-slope * length).exp_m1() / slope;
        return (sigma_at_origin * integral).max(0.0);
    }
    let end_y = ray_origin_y + ray_direction_y * length;
    let log_sigma_at_end = sigma0.ln() - (end_y - base_height) / safe_scale;
    let sigma_at_end = log_sigma_at_end.clamp(-80.0, 80.0).exp();
    ((sigma_at_end - sigma_at_origin) / -slope).max(0.0)
}

pub fn hybrid_slice_distance(
    normalized_slice: f32,
    far_distance: f32,
    linear_depth: f32,
    linear_fraction: f32,
) -> f32 {
    let u = normalized_slice.clamp(0.0, 1.0);
    let far = far_distance.max(1.0e-4);
    let linear = linear_depth.clamp(1.0e-4, far);
    let fraction = linear_fraction.clamp(1.0e-4, 0.9999);
    if u <= fraction {
        linear * (u / fraction)
    } else {
        let q = (u - fraction) / (1.0 - fraction);
        linear * (far / linear).powf(q)
    }
}

pub fn hybrid_slice_coordinate(
    distance: f32,
    far_distance: f32,
    linear_depth: f32,
    linear_fraction: f32,
) -> f32 {
    let far = far_distance.max(1.0e-4);
    let linear = linear_depth.clamp(1.0e-4, far);
    let fraction = linear_fraction.clamp(1.0e-4, 0.9999);
    let d = distance.clamp(0.0, far);
    if d <= linear {
        fraction * (d / linear)
    } else {
        fraction + (1.0 - fraction) * (d / linear).ln() / (far / linear).ln()
    }
}

/// #2242 (REN-D16-04) — every call resets every entry's `count` to 0 before
/// repopulating from `volumes` (`offset` is a caller-seeded invariant as of
/// #3133, see `fog_cluster_entries_with_offsets` — it never needs touching
/// here). There is no incremental/partial update to `count`: a cell
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
fn build_fog_volume_clusters(
    volumes: &[GpuFogVolume],
    camera_pos: [f32; 3],
    far_distance: f32,
    upload: &mut GpuFogVolumeUpload,
    entries: &mut [GpuFogClusterEntry; FOG_VOLUME_CLUSTER_COUNT],
    indices: &mut [u32; FOG_VOLUME_INDEX_COUNT],
) -> [f32; 4] {
    let far = far_distance.max(1.0);
    let cell_size = (2.0 * far) / FOG_VOLUME_CLUSTER_DIM as f32;
    let grid_min = [
        camera_pos[0] - far,
        camera_pos[1] - far,
        camera_pos[2] - far,
    ];

    // #3133 (PERF-D4-01) — `offset` is a pure function of array position
    // (see `fog_cluster_entries_with_offsets`) and is seeded once by every
    // caller (`VolumetricsPipeline::new` in production, the same helper in
    // tests below); only `count` needs resetting every frame, so this no
    // longer re-derives `offset` nor memsets it. `indices` needs no reset at
    // all: the shader only ever reads `fogClusterIndices[cluster.offset +
    // i]` for `i < min(cluster.count, MAX_FOG_VOLUMES_PER_CLUSTER)`
    // (`sampleLocalMedium`, volumetrics_inject.comp), and `count` is reset
    // to 0 for every cluster below before any of this frame's volumes are
    // clustered — so a slot past this frame's `count` for a given cluster
    // is never observed, stale contents or not.
    for entry in entries.iter_mut() {
        entry.count = 0;
    }

    let volume_count = volumes.len().min(MAX_GPU_FOG_VOLUMES);
    upload.count = [volume_count as u32, 0, 0, 0];
    upload.volumes[..volume_count].copy_from_slice(&volumes[..volume_count]);

    for (volume_index, volume) in upload.volumes[..volume_count].iter().enumerate() {
        let center = [
            volume.center_shape[0],
            volume.center_shape[1],
            volume.center_shape[2],
        ];
        let radius = (volume.half_extents_extinction[0] * volume.half_extents_extinction[0]
            + volume.half_extents_extinction[1] * volume.half_extents_extinction[1]
            + volume.half_extents_extinction[2] * volume.half_extents_extinction[2])
            .sqrt();
        if !center.iter().all(|value| value.is_finite()) || !radius.is_finite() {
            continue;
        }

        let mut ranges = [(0usize, 0usize); 3];
        let mut intersects_grid = true;
        for axis in 0..3 {
            let lower = (center[axis] - radius - grid_min[axis]) / cell_size;
            let upper = (center[axis] + radius - grid_min[axis]) / cell_size;
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

        for z in ranges[2].0..=ranges[2].1 {
            for y in ranges[1].0..=ranges[1].1 {
                for x in ranges[0].0..=ranges[0].1 {
                    let cluster_index = x
                        + y * FOG_VOLUME_CLUSTER_DIM
                        + z * FOG_VOLUME_CLUSTER_DIM * FOG_VOLUME_CLUSTER_DIM;
                    let entry = &mut entries[cluster_index];
                    if entry.count as usize >= MAX_FOG_VOLUMES_PER_CLUSTER {
                        continue;
                    }
                    indices[entry.offset as usize + entry.count as usize] = volume_index as u32;
                    entry.count += 1;
                }
            }
        }
    }

    [grid_min[0], grid_min[1], grid_min[2], cell_size.recip()]
}

/// Single source of truth for whether the composite shader actually
/// consumes the integrated volumetric output.
///
/// History: gated off 2026-05-09 after a diagnosed per-froxel single-
/// shadow-ray banding artifact on Prospector Saloon cup-and-lantern
/// interior content (commits `f62d4bd`, `33f48b5`). Re-enabled once
/// M-LIGHT v2 — a 3x3 XY spatial blur over the injection buffer in
/// `volumetrics_integrate.comp` — resolved the banding, and #1462
/// (the inject/integrate/composite depth-slice convention mismatch,
/// a ~half-slab fog-depth bias) was reconciled by moving `inject`'s
/// per-slice world-distance sample from slice-CENTER to slice-FRONT-
/// EDGE, matching what `integrate` and `composite` already assumed.
///
/// Also resolved alongside: the blanket `is_exterior` zero that used
/// to suppress ALL sun/scattering contribution for interior cells
/// (too blunt — it also blocked real sun-through-window godrays) is
/// gone. `volumetrics_inject.comp`'s shadow ray now runs a two-pass
/// opaque-then-glass-masked query for interiors specifically, so a
/// geometry gap (e.g. a `--cell`-loaded interior's typically-missing
/// ceiling mesh) can't masquerade as a window — see
/// `VolumetricsParams::render_origin`'s doc comment and the
/// interior-godray investigation note in `context/draw.rs`.
///
/// Integration parameters are per-FIF; changing reach or slice distribution
/// cannot race a prior frame's in-flight UBO read.
///
/// Per-froxel ray budget: despite `volumetrics_inject.comp`'s header
/// describing shadow visibility as the standard single "trace toward
/// light, miss = lit" test, the shader actually casts up to 10 ray-query
/// traversals per froxel in the worst case (1 opaque + 1 glass-masked sun
/// ray, plus up to `MAX_FROXEL_LIGHTS` local lights x up to 2 rays each) —
/// ~9.2M ray queries/frame at the default `froxel_xy_divisor = 8` 160x90x64
/// grid for a 1280x720 render extent. See
/// REN-D16-2026-08-07-02 / #2509.
pub const VOLUMETRIC_OUTPUT_CONSUMED: bool = true;

/// Integration shader uniform — slab thickness `dt` shared across all
/// slices under linear distribution. Phase 5 will replace this with
/// an exponential per-slice `dt[]` array. std140 alignment: vec4 only.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct IntegrationParams {
    /// x = grid far, y = linear-depth floor, z = linear slice fraction,
    /// w = depth-slice count. Distances are in world units.
    grid: [f32; 4],
}

/// Derive the froxel volume from the *render* extent. This is deliberately
/// downstream of the FSR preset query: using output resolution here would
/// silently overspend whenever FSR Quality/Balanced/Performance is active.
pub fn froxel_extent(render_extent: vk::Extent2D, config: VolumetricsConfig) -> vk::Extent3D {
    vk::Extent3D {
        width: render_extent
            .width
            .div_ceil(config.froxel_xy_divisor)
            .max(1),
        height: render_extent
            .height
            .div_ceil(config.froxel_xy_divisor)
            .max(1),
        depth: config.froxel_z_slices,
    }
}

/// RGB scattered radiance (HDR) + alpha transmittance. RGBA16F
/// matches Frostbite's reference layout — 8 bytes per froxel,
/// half-float precision is ample for both scattering ([0, ~10]) and
/// transmittance ([0, 1]). R11G11B10F was considered but its 10-bit
/// alpha-equivalent (the implicit 0.0 we'd reconstruct) loses the
/// transmittance channel entirely.
const FROXEL_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
/// Transported thermochemical and velocity fields share RGBA16F with the
/// V-buffer. Chemistry stores `(fuel, temperature K, absorption sigma_a,
/// visible-radiance calibration)`; velocity stores world-units/second in xyz
/// plus scattering sigma_s in w. Half precision covers the bounded canonical
/// ranges while keeping the two history fields affordable.
const COMBUSTION_FIELD_FORMAT: vk::Format = vk::Format::R16G16B16A16_SFLOAT;
/// One scalar per froxel recording what fraction of the current source term
/// came from deterministic thermal emission. R32 is a core storage-image
/// format, avoiding a `shaderStorageImageExtendedFormats` requirement for the
/// temporal correctness sidecar. #2809 (REN-D16-04).
const EMISSION_HISTORY_FORMAT: vk::Format = vk::Format::R32_SFLOAT;

/// Froxel volumes allocated per frame-in-flight slot: `lighting`,
/// `emission_history`, `combustion_state`, `combustion_dynamics`,
/// `combustion_optical`, `integrated`.
pub(crate) const FROXEL_VOLUMES_PER_SLOT: usize = 6;
/// Bytes of VRAM per froxel per slot — five RGBA16F volumes at 8 B plus the
/// R32F emission-history sidecar at 4 B.
///
/// #3117 — named so the boot log and `docs/engine/memory-budget.md` cannot
/// silently disagree. The doc's ledger row sat at the pre-Session-62
/// two-volume figure while this grew to six, understating the froxel grid by
/// ~24x; `froxel_grid_cost_matches_the_memory_budget_doc` now pins them
/// together. Multiply by `MAX_FRAMES_IN_FLIGHT` for the resident total.
pub(crate) const FROXEL_BYTES_PER_SLOT: u64 = 44;
const DENSITY_NOISE_FORMAT: vk::Format = vk::Format::R8_UNORM;
/// Continue advancing transported smoke after the source entity disappears.
/// Core owns the matching aerosol decay/cutoff contract, avoiding a visible
/// tail cut when the renderer stops the otherwise-idle transport dispatch.
const COMBUSTION_HISTORY_LINGER_SECONDS: f32 = COMBUSTION_AEROSOL_LINGER_SECONDS;
const COMBUSTION_LIGHT_GRID_COUNT: usize = GLSL_COMBUSTION_LIGHT_GRID_COUNT as usize;
const MAX_COMBUSTION_SURFACE_LIGHTS: usize = 64;
const COMBUSTION_LIGHT_CUTOFF_IRRADIANCE: f32 = 1.0e-3;
const COMBUSTION_LIGHT_MIN_RANGE_METERS: f32 = 0.25;
const COMBUSTION_LIGHT_MAX_RANGE_METERS: f32 = 64.0;
/// `pointSpotAtten` treats half the uploaded cull radius as the physical
/// inverse-square reach and uses the second half as a smooth cull window.
const COMBUSTION_LIGHT_RANGE_EXTENSION: f32 = 2.0;
/// Art-directed gain for converting the transported fire field into a point
/// light. The moment is a froxel-integrated, escape-weighted source, while
/// the surface path evaluates it as one inverse-square emitter; without this
/// calibration the flame is visible but contributes almost no room light.
/// Keep this separate from the volumetric emission so fire visibility and
/// surface illumination can be tuned independently.
const COMBUSTION_SURFACE_LIGHT_BOOST: f32 = 4.0;
/// A nuclear cloud deliberately creates more luminous bins than an oil
/// fireball. Keep each derived point-light contribution bounded while the
/// source is alive; the broad volume still lights the room through multiple
/// bins, but does not flatten every wall to white before the mushroom shape
/// can be read.
const NUCLEAR_COMBUSTION_SURFACE_LIGHT_SCALE: f32 = 0.22;

/// Fixed-point ABI mirrored by `CombustionLightMoment` in
/// `volumetrics_inject.comp` (std430: eight tightly packed uints).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GpuCombustionLightMoment {
    weight: u32,
    weighted_x: u32,
    weighted_y: u32,
    weighted_z: u32,
    radiant_r: u32,
    radiant_g: u32,
    radiant_b: u32,
    luminous_volume: u32,
}

fn decode_combustion_light_moment(bytes: &[u8]) -> GpuCombustionLightMoment {
    debug_assert!(bytes.len() >= std::mem::size_of::<GpuCombustionLightMoment>());
    let word = |index: usize| {
        let offset = index * std::mem::size_of::<u32>();
        u32::from_ne_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .expect("four-byte word"),
        )
    };
    GpuCombustionLightMoment {
        weight: word(0),
        weighted_x: word(1),
        weighted_y: word(2),
        weighted_z: word(3),
        radiant_r: word(4),
        radiant_g: word(5),
        radiant_b: word(6),
        luminous_volume: word(7),
    }
}

fn combustion_light_from_moment(
    bin_index: usize,
    moment: GpuCombustionLightMoment,
    camera_world: [f32; 3],
) -> Option<GpuLight> {
    if moment.weight == 0 {
        return None;
    }
    let radiant = [
        moment.radiant_r as f32 / COMBUSTION_LIGHT_FIXED_SCALE,
        moment.radiant_g as f32 / COMBUSTION_LIGHT_FIXED_SCALE,
        moment.radiant_b as f32 / COMBUSTION_LIGHT_FIXED_SCALE,
    ]
    .map(|channel| channel * COMBUSTION_SURFACE_LIGHT_BOOST);
    let luma = radiant[0] * 0.2126 + radiant[1] * 0.7152 + radiant[2] * 0.0722;
    if !luma.is_finite() || luma < COMBUSTION_LIGHT_CUTOFF_IRRADIANCE {
        return None;
    }

    let grid_x = COMBUSTION_LIGHT_GRID_X as usize;
    let grid_y = COMBUSTION_LIGHT_GRID_Y as usize;
    let bin_x = bin_index % grid_x;
    let yz = bin_index / grid_x;
    let bin_y = yz % grid_y;
    let bin_z = yz / grid_y;
    if bin_z >= COMBUSTION_LIGHT_GRID_Z as usize {
        return None;
    }
    let inverse_weight = 1.0 / moment.weight as f32;
    let within = [
        (moment.weighted_x as f32 * inverse_weight).clamp(0.0, 1.0),
        (moment.weighted_y as f32 * inverse_weight).clamp(0.0, 1.0),
        (moment.weighted_z as f32 * inverse_weight).clamp(0.0, 1.0),
    ];
    let half = [
        COMBUSTION_LIGHT_HALF_EXTENT_XZ_METERS,
        COMBUSTION_LIGHT_HALF_EXTENT_Y_METERS,
        COMBUSTION_LIGHT_HALF_EXTENT_XZ_METERS,
    ];
    let dims = [
        COMBUSTION_LIGHT_GRID_X as f32,
        COMBUSTION_LIGHT_GRID_Y as f32,
        COMBUSTION_LIGHT_GRID_Z as f32,
    ];
    let bins = [bin_x as f32, bin_y as f32, bin_z as f32];
    let position: [f32; 3] = std::array::from_fn(|axis| {
        let offset_metres =
            -half[axis] + (bins[axis] + within[axis]) * 2.0 * half[axis] / dims[axis];
        camera_world[axis] + offset_metres * WORLD_UNITS_PER_METER
    });

    let luminous_volume = moment.luminous_volume as f32 / COMBUSTION_LIGHT_VOLUME_FIXED_SCALE;
    let source_radius_metres = if luminous_volume > 0.0 && luminous_volume.is_finite() {
        (3.0 * luminous_volume / (4.0 * std::f32::consts::PI))
            .cbrt()
            .clamp(0.02, 8.0)
    } else {
        0.02
    };
    let range_metres = (luma / COMBUSTION_LIGHT_CUTOFF_IRRADIANCE).sqrt().clamp(
        COMBUSTION_LIGHT_MIN_RANGE_METERS,
        COMBUSTION_LIGHT_MAX_RANGE_METERS,
    );

    Some(GpuLight {
        position_radius: [
            position[0],
            position[1],
            position[2],
            range_metres * WORLD_UNITS_PER_METER * COMBUSTION_LIGHT_RANGE_EXTENSION,
        ],
        color_type: [radiant[0], radiant[1], radiant[2], 0.0],
        direction_angle: [0.0; 4],
        params: [
            1.0,
            source_radius_metres * WORLD_UNITS_PER_METER,
            VISIBILITY_MASK_FULL as f32,
            ATTENUATION_MODEL_INVERSE_SQUARE as f32,
        ],
    })
}

fn volume_contains_position(volume: &GpuFogVolume, position: [f32; 3]) -> bool {
    let dx = position[0] - volume.center_shape[0];
    let dy = position[1] - volume.center_shape[1];
    let dz = position[2] - volume.center_shape[2];
    let radius_squared = volume.half_extents_extinction[0] * volume.half_extents_extinction[0]
        + volume.half_extents_extinction[1] * volume.half_extents_extinction[1]
        + volume.half_extents_extinction[2] * volume.half_extents_extinction[2];
    dx * dx + dy * dy + dz * dz <= radius_squared
}

fn combustion_light_is_inside_authored_source(
    light: &GpuLight,
    source_volumes: &[GpuFogVolume],
    authored_lights: &[GpuLight],
) -> bool {
    let light_position = [
        light.position_radius[0],
        light.position_radius[1],
        light.position_radius[2],
    ];
    source_volumes.iter().any(|volume| {
        let profile = volume.profile_params[0];
        let transported = profile.is_finite()
            && (FOG_VOLUME_PROFILE_SMOKE - 0.5..=FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR + 0.5)
                .contains(&profile);
        transported
            && volume_contains_position(volume, light_position)
            && authored_lights.iter().any(|candidate| {
                candidate.color_type[3] <= 1.5
                    && volume_contains_position(
                        volume,
                        [
                            candidate.position_radius[0],
                            candidate.position_radius[1],
                            candidate.position_radius[2],
                        ],
                    )
            })
    })
}

struct FroxelSlot {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Option<vk_alloc::Allocation>,
}

pub struct VolumetricsPipeline {
    // ── Injection pass ───────────────────────────────────────────────
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    history_sampler: vk::Sampler,
    transport_sampler: vk::Sampler,
    density_noise_sampler: vk::Sampler,
    base_noise_volume: Option<FroxelSlot>,
    detail_noise_volume: Option<FroxelSlot>,
    extent: vk::Extent3D,
    config: VolumetricsConfig,
    /// Per frame-in-flight injection-pass output: per-froxel
    /// `(rgb=inscatter, a=extinction)`. Read by the integration pass and by
    /// the next frame's temporal reprojection.
    lighting_volumes: Vec<FroxelSlot>,
    /// Per frame-in-flight scalar emissive-source share. Kept separate from
    /// `lighting_volumes` because its RGBA channels are already committed to
    /// in-scatter plus extinction. The injector writes the current slot and
    /// reprojects the previous slot to make fire/explosion history symmetric
    /// on leading and trailing edges.
    emission_history_volumes: Vec<FroxelSlot>,
    /// Canonical transported `(fuel, temperature K, total extinction sigma_t,
    /// visible-radiance calibration)` state, double-buffered by
    /// frame-in-flight slot.
    combustion_state_volumes: Vec<FroxelSlot>,
    /// Canonical combustion dynamics: world-space velocity in xyz (world
    /// units / second) and transported specific overpressure in w (m²/s²).
    /// Sampled linearly for RK2 backtracing and pressure-gradient forces.
    combustion_dynamics_volumes: Vec<FroxelSlot>,
    /// Canonical transported spectral scattering coefficient `sigma_s.rgb`,
    /// double-buffered independently from chemistry and dynamics so optical
    /// tint survives advection without overloading either concern.
    combustion_optical_volumes: Vec<FroxelSlot>,
    /// Per frame-in-flight host-mapped UBO carrying
    /// `VolumetricsParams`. Written each frame from `dispatch()`.
    param_buffers: Vec<GpuBuffer>,
    /// Per-frame host-mapped local-volume primitive, cluster-grid, and
    /// cluster-index SSBOs. Bindings are static; contents are rebuilt for the
    /// idle frame slot before injection.
    fog_volume_buffers: Vec<GpuBuffer>,
    fog_cluster_buffers: Vec<GpuBuffer>,
    fog_cluster_index_buffers: Vec<GpuBuffer>,
    /// GPU-written fixed-point radiant moments, one camera-centred coarse
    /// grid per frame-in-flight slot. The host drains and zeroes a slot only
    /// after its fence, so extraction never stalls the active submission.
    combustion_light_moment_buffers: Vec<GpuBuffer>,
    combustion_light_grid_centers: [[f32; 3]; MAX_FRAMES_IN_FLIGHT],
    combustion_light_grid_valid: [bool; MAX_FRAMES_IN_FLIGHT],
    combustion_light_candidates: Vec<(f32, GpuLight)>,
    /// Last reduction counters emitted at debug level. Logging only changes
    /// keeps the GPU-readback diagnostic useful without producing one line
    /// per frame during a steady flame.
    /// Counts plus strongest-centroid quarter-metre cell. Including position
    /// makes debug telemetry report real plume motion even when the number of
    /// occupied reduction bins stays constant.
    last_combustion_light_topology: Option<[i32; 7]>,
    /// Reused CPU staging state for cluster construction.
    fog_volume_upload: Box<GpuFogVolumeUpload>,
    fog_cluster_entries: Box<[GpuFogClusterEntry; FOG_VOLUME_CLUSTER_COUNT]>,
    fog_cluster_indices: Box<[u32; FOG_VOLUME_INDEX_COUNT]>,

    // ── Integration pass (Phase 3) ───────────────────────────────────
    integration_pipeline: vk::Pipeline,
    integration_pipeline_layout: vk::PipelineLayout,
    integration_descriptor_set_layout: vk::DescriptorSetLayout,
    integration_descriptor_pool: vk::DescriptorPool,
    integration_descriptor_sets: Vec<vk::DescriptorSet>,
    /// Per frame-in-flight integration-pass output: per-slice
    /// cumulative `(rgb=∫inscatter, a=T_cum)`. Composite samples this
    /// once per fragment with a sampler3D.
    integrated_volumes: Vec<FroxelSlot>,
    /// Per-frame-in-flight integration UBO holding the hybrid-Z grid
    /// parameters. The shader derives each slab's exact view-space width from
    /// these values while walking the column.
    integration_param_buffers: Vec<GpuBuffer>,
    history_valid: bool,
    dispatched_this_frame: bool,
    /// Last successfully submitted simulation time and the time recorded by
    /// the current command buffer. Promoting pending -> last only from
    /// `mark_frame_completed` keeps failed submissions from advancing dt.
    last_simulation_time_seconds: Option<f32>,
    pending_simulation_time_seconds: Option<f32>,
    /// CPU-side lifetime latch for transported soot after emitters vanish.
    combustion_active_until_seconds: f32,

    /// Per-frame-in-flight latch: `true` once `write_tlas` has written
    /// binding 2 for this slot. The injection descriptor set is created
    /// with bindings 0/1 written but binding 2 (TLAS) deferred — the
    /// caller is required to call `write_tlas` before `dispatch` each
    /// frame the gate is on. `dispatch` debug_asserts this. (#1105 /
    /// REN-D18-003)
    tlas_written: [bool; MAX_FRAMES_IN_FLIGHT],
    /// Same latch shape as `tlas_written`, for bindings 3/4/5 (lights /
    /// cluster grid / light-index SSBOs) written via
    /// `write_lights_and_clusters`. See that method's doc comment.
    lights_written: [bool; MAX_FRAMES_IN_FLIGHT],
    /// Same deferred-write latch for bindings 19/20/21: the current
    /// per-frame instance table and canonical global geometry SSBOs used to
    /// recover rigid triangle normals at combustion boundaries.
    boundary_geometry_written: [bool; MAX_FRAMES_IN_FLIGHT],
}

impl VolumetricsPipeline {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        render_extent: vk::Extent2D,
        config: VolumetricsConfig,
    ) -> Result<Self> {
        let config = config.validate()?;
        let result = Self::new_inner(device, allocator, pipeline_cache, render_extent, config);
        if let Err(ref e) = result {
            log::debug!("Volumetrics pipeline creation failed at: {e}");
        }
        result
    }

    fn new_inner(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        render_extent: vk::Extent2D,
        config: VolumetricsConfig,
    ) -> Result<Self> {
        let extent = froxel_extent(render_extent, config);
        let mut partial = Self {
            pipeline: vk::Pipeline::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_sets: Vec::new(),
            history_sampler: vk::Sampler::null(),
            transport_sampler: vk::Sampler::null(),
            density_noise_sampler: vk::Sampler::null(),
            base_noise_volume: None,
            detail_noise_volume: None,
            extent,
            config,
            lighting_volumes: Vec::new(),
            emission_history_volumes: Vec::new(),
            combustion_state_volumes: Vec::new(),
            combustion_dynamics_volumes: Vec::new(),
            combustion_optical_volumes: Vec::new(),
            param_buffers: Vec::new(),
            fog_volume_buffers: Vec::new(),
            fog_cluster_buffers: Vec::new(),
            fog_cluster_index_buffers: Vec::new(),
            combustion_light_moment_buffers: Vec::new(),
            combustion_light_grid_centers: [[0.0; 3]; MAX_FRAMES_IN_FLIGHT],
            combustion_light_grid_valid: [false; MAX_FRAMES_IN_FLIGHT],
            combustion_light_candidates: Vec::new(),
            last_combustion_light_topology: None,
            fog_volume_upload: Box::new(GpuFogVolumeUpload::default()),
            fog_cluster_entries: fog_cluster_entries_with_offsets(),
            fog_cluster_indices: Box::new([0; FOG_VOLUME_INDEX_COUNT]),
            integration_pipeline: vk::Pipeline::null(),
            integration_pipeline_layout: vk::PipelineLayout::null(),
            integration_descriptor_set_layout: vk::DescriptorSetLayout::null(),
            integration_descriptor_pool: vk::DescriptorPool::null(),
            integration_descriptor_sets: Vec::new(),
            integrated_volumes: Vec::new(),
            integration_param_buffers: Vec::new(),
            history_valid: false,
            dispatched_this_frame: false,
            last_simulation_time_seconds: None,
            pending_simulation_time_seconds: None,
            combustion_active_until_seconds: f32::NEG_INFINITY,
            tlas_written: [false; MAX_FRAMES_IN_FLIGHT],
            lights_written: [false; MAX_FRAMES_IN_FLIGHT],
            boundary_geometry_written: [false; MAX_FRAMES_IN_FLIGHT],
        };

        macro_rules! try_or_cleanup {
            ($expr:expr) => {
                match $expr {
                    Ok(v) => v,
                    Err(e) => {
                        // SAFETY: cleanup path on construction failure; `partial` owns only
                        // objects created so far in this fn, none submitted to the GPU, and
                        // `device`/`allocator` outlive this call.
                        unsafe { partial.destroy(device, allocator) };
                        return Err(e.into());
                    }
                }
            };
        }

        // ── 1. Allocate per-frame-in-flight froxel volumes ────────────
        // Six volumes per frame: lighting (injection output → integration
        // input), integrated (integration output → composite read), scalar
        // emission provenance, and three transported-combustion history fields.
        for i in 0..MAX_FRAMES_IN_FLIGHT {
            let slot = try_or_cleanup!(Self::create_volume(
                device,
                allocator,
                &format!("volumetrics_lighting_{i}"),
                extent,
                FROXEL_FORMAT,
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            ));
            partial.lighting_volumes.push(slot);
            let emission_history = try_or_cleanup!(Self::create_volume(
                device,
                allocator,
                &format!("volumetrics_emission_history_{i}"),
                extent,
                EMISSION_HISTORY_FORMAT,
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            ));
            partial.emission_history_volumes.push(emission_history);
            let combustion_state = try_or_cleanup!(Self::create_volume(
                device,
                allocator,
                &format!("volumetrics_combustion_state_{i}"),
                extent,
                COMBUSTION_FIELD_FORMAT,
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            ));
            partial.combustion_state_volumes.push(combustion_state);
            let combustion_dynamics = try_or_cleanup!(Self::create_volume(
                device,
                allocator,
                &format!("volumetrics_combustion_dynamics_{i}"),
                extent,
                COMBUSTION_FIELD_FORMAT,
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            ));
            partial
                .combustion_dynamics_volumes
                .push(combustion_dynamics);
            let combustion_optical = try_or_cleanup!(Self::create_volume(
                device,
                allocator,
                &format!("volumetrics_combustion_optical_{i}"),
                extent,
                COMBUSTION_FIELD_FORMAT,
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            ));
            partial.combustion_optical_volumes.push(combustion_optical);
            let integrated = try_or_cleanup!(Self::create_volume(
                device,
                allocator,
                &format!("volumetrics_integrated_{i}"),
                extent,
                FROXEL_FORMAT,
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_DST,
            ));
            partial.integrated_volumes.push(integrated);
        }
        partial.base_noise_volume = Some(try_or_cleanup!(Self::create_volume(
            device,
            allocator,
            "volumetrics_base_noise",
            vk::Extent3D {
                width: BASE_NOISE_SIZE,
                height: BASE_NOISE_SIZE,
                depth: BASE_NOISE_SIZE,
            },
            DENSITY_NOISE_FORMAT,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
        )));
        partial.detail_noise_volume = Some(try_or_cleanup!(Self::create_volume(
            device,
            allocator,
            "volumetrics_detail_noise",
            vk::Extent3D {
                width: DETAIL_NOISE_SIZE,
                height: DETAIL_NOISE_SIZE,
                depth: DETAIL_NOISE_SIZE,
            },
            DENSITY_NOISE_FORMAT,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
        )));

        // History must never filter across XY froxel boundaries: adjacent
        // columns can be separated by opaque structure. The shader performs
        // explicit nearest-XY/linear-Z texelFetch reconstruction, and the
        // nearest sampler keeps accidental future texture() calls conservative.
        // All volumes remain in GENERAL for storage writes and history reads.
        // SAFETY: `device` is live; the create info contains no borrowed
        // extension chain, and the resulting sampler is owned by `partial`
        // until construction rollback or `destroy`.
        partial.history_sampler = try_or_cleanup!(unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::NEAREST)
                        .min_filter(vk::Filter::NEAREST)
                        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .context("Volumetrics history sampler")
        });

        // Transport needs sub-froxel motion: nearest sampling would keep
        // velocity pinned until it crossed a whole cell and produce blocky
        // smoke. A dedicated linear/clamp sampler is safe here because the
        // chemistry field is continuous; raw radiance history deliberately
        // keeps its separate nearest-column sampler to avoid wall bleeding.
        // SAFETY: same ownership/lifetime contract as `history_sampler`.
        partial.transport_sampler = try_or_cleanup!(unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                    None,
                )
                .context("Volumetrics transport sampler")
        });

        // The density generator is periodic at the voxel boundary, so
        // trilinear filtering remains continuous across every repeat seam.
        // SAFETY: `device` is live, the create info has no extension chain,
        // and the sampler handle is owned by `partial` until rollback or
        // explicit pipeline destruction.
        partial.density_noise_sampler = try_or_cleanup!(unsafe {
            device
                .create_sampler(
                    &vk::SamplerCreateInfo::default()
                        .mag_filter(vk::Filter::LINEAR)
                        .min_filter(vk::Filter::LINEAR)
                        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                        .address_mode_u(vk::SamplerAddressMode::REPEAT)
                        .address_mode_v(vk::SamplerAddressMode::REPEAT)
                        .address_mode_w(vk::SamplerAddressMode::REPEAT),
                    None,
                )
                .context("Volumetrics density-noise sampler")
        });

        // ── 2. Per-frame parameter UBOs ───────────────────────────────
        let param_size = std::mem::size_of::<VolumetricsParams>() as vk::DeviceSize;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let buf = try_or_cleanup!(GpuBuffer::create_host_visible(
                device,
                allocator,
                param_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            ));
            partial.param_buffers.push(buf);
            partial
                .fog_volume_buffers
                .push(try_or_cleanup!(GpuBuffer::create_host_visible(
                    device,
                    allocator,
                    std::mem::size_of::<GpuFogVolumeUpload>() as vk::DeviceSize,
                    vk::BufferUsageFlags::STORAGE_BUFFER,
                )));
            partial
                .fog_cluster_buffers
                .push(try_or_cleanup!(GpuBuffer::create_host_visible(
                    device,
                    allocator,
                    std::mem::size_of::<[GpuFogClusterEntry; FOG_VOLUME_CLUSTER_COUNT]>()
                        as vk::DeviceSize,
                    vk::BufferUsageFlags::STORAGE_BUFFER,
                )));
            partial.fog_cluster_index_buffers.push(try_or_cleanup!(
                GpuBuffer::create_host_visible(
                    device,
                    allocator,
                    std::mem::size_of::<[u32; FOG_VOLUME_INDEX_COUNT]>() as vk::DeviceSize,
                    vk::BufferUsageFlags::STORAGE_BUFFER,
                )
            ));
            let mut moment_buffer = try_or_cleanup!(GpuBuffer::create_host_readback(
                device,
                allocator,
                std::mem::size_of::<[GpuCombustionLightMoment; COMBUSTION_LIGHT_GRID_COUNT]>()
                    as vk::DeviceSize,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            ));
            try_or_cleanup!((|| -> Result<()> {
                moment_buffer.mapped_slice_mut()?.fill(0);
                moment_buffer.flush_if_needed(device)
            })());
            partial.combustion_light_moment_buffers.push(moment_buffer);
        }

        // ── 3. Descriptor set layout ──────────────────────────────────
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 2: scene TLAS (Phase 2c). Updated each frame via
            // `write_tlas` from draw.rs before dispatch — same flow
            // as `caustic.write_tlas` (caustic.rs:627). Used by the
            // injection shader's shadow visibility ray query.
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 3/4/5: Phase 2b point/spot light injection — the same
            // per-frame lights SSBO + cluster grid + light-index list
            // `ClusterCullPipeline` already builds for the fragment
            // shader (`triangle.frag`'s `lights[]` / `clusters[]` /
            // `clusterLightIndices[]`), reused here rather than
            // building a separate froxel-space light-culling
            // structure. Written per-frame via `write_lights_and_clusters`
            // (same deferred-write flow as `write_tlas`, since the
            // buffer *contents* are rebuilt every frame even though the
            // handles are frame-in-flight-stable).
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(5)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 6: previous frame's raw V-buffer for temporal reprojection.
            vk::DescriptorSetLayoutBinding::default()
                .binding(6)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 7/8/9: analytic local fog primitives and their camera-centered
            // 16^3 world-space clustered cull list.
            vk::DescriptorSetLayoutBinding::default()
                .binding(7)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(8)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(9)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // Immutable boot-generated Perlin-Worley base density and
            // higher-frequency erosion detail.
            vk::DescriptorSetLayoutBinding::default()
                .binding(10)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(11)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 12/13: current write + previous sampled scalar emission share.
            // This provenance cannot be reconstructed from the RGBA V-buffer
            // after thermal emission and stochastic lighting are combined.
            vk::DescriptorSetLayoutBinding::default()
                .binding(12)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(13)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 14/15: current write + previous sampled canonical chemistry
            // `(fuel, temperature, soot extinction, radiance calibration)`.
            vk::DescriptorSetLayoutBinding::default()
                .binding(14)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(15)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 16/17: current write + previous sampled canonical dynamics
            // `(world-space velocity xyz, specific overpressure)`.
            vk::DescriptorSetLayoutBinding::default()
                .binding(16)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(17)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 18: fixed-point transported-field radiant moments. GPU writes
            // this slot; the host drains it only after the slot fence.
            vk::DescriptorSetLayoutBinding::default()
                .binding(18)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 19/20/21: current GpuInstance table plus canonical global
            // vertex/index SSBOs. A TLAS committed hit's custom index lands
            // directly in binding 19, and its primitive index addresses the
            // matching rigid triangle in bindings 20/21.
            vk::DescriptorSetLayoutBinding::default()
                .binding(19)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(20)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(21)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 22/23: current write + previous sampled spectral scattering
            // coefficient. Keeping optics separate leaves bindings 14-17 as
            // single-purpose thermochemical and kinematic state.
            vk::DescriptorSetLayoutBinding::default()
                .binding(22)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(23)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "volumetrics_inject.comp",
                spirv: VOLUMETRICS_INJECT_COMP_SPV,
            }],
            "volumetrics",
            &[],
        )
        .expect("volumetrics descriptor layout drifted against volumetrics_inject.comp (see #427)");
        // SAFETY: `device` is live; `bindings` outlives the call; the layout
        // is owned by `partial` and destroyed on the error path / in destroy().
        partial.descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("Volumetrics descriptor set layout")
        });

        // SAFETY: `device` is live; the referenced descriptor set layout was
        // just created above and is still live; result owned by `partial`.
        partial.pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&partial.descriptor_set_layout)),
                    None,
                )
                .context("Volumetrics pipeline layout")
        });

        // ── 4. Compute pipeline ───────────────────────────────────────
        // load-module → create → destroy-module centralized in
        // pipeline::create_compute_pipeline (#1751); try_or_cleanup! adds the
        // partial-struct rollback on error.
        partial.pipeline = try_or_cleanup!(super::pipeline::create_compute_pipeline(
            device,
            pipeline_cache,
            VOLUMETRICS_INJECT_COMP_SPV,
            partial.pipeline_layout,
            "Volumetrics clear",
        ));

        // ── 5. Descriptor pool + sets ─────────────────────────────────
        // Pool sizes derived from `bindings` (#1030 / REN-D10-NEW-09).
        partial.descriptor_pool = try_or_cleanup!(DescriptorPoolBuilder::from_layout_bindings(
            &bindings,
            MAX_FRAMES_IN_FLIGHT as u32,
        )
        .max_sets(MAX_FRAMES_IN_FLIGHT as u32)
        .build(device, "Volumetrics descriptor pool"));

        let layouts = vec![partial.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: `device` is live; `partial.descriptor_pool` was just built and
        // `layouts` (clones of the live set layout) outlive the call.
        partial.descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.descriptor_pool)
                        .set_layouts(&layouts),
                )
                .context("Volumetrics descriptor sets")
        });

        // ── 6. Write descriptor sets ──────────────────────────────────
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            let previous = (f + MAX_FRAMES_IN_FLIGHT - 1) % MAX_FRAMES_IN_FLIGHT;
            let lighting_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.lighting_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let history_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.history_sampler)
                .image_view(partial.lighting_volumes[previous].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let emission_history_write_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.emission_history_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let previous_emission_history_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.history_sampler)
                .image_view(partial.emission_history_volumes[previous].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let combustion_state_write_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.combustion_state_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let previous_combustion_state_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.transport_sampler)
                .image_view(partial.combustion_state_volumes[previous].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let combustion_dynamics_write_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.combustion_dynamics_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let previous_combustion_dynamics_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.transport_sampler)
                .image_view(partial.combustion_dynamics_volumes[previous].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let combustion_optical_write_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.combustion_optical_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let previous_combustion_optical_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.transport_sampler)
                .image_view(partial.combustion_optical_volumes[previous].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let params_info = [vk::DescriptorBufferInfo {
                buffer: partial.param_buffers[f].buffer,
                offset: 0,
                range: param_size,
            }];
            let fog_volume_info = [vk::DescriptorBufferInfo {
                buffer: partial.fog_volume_buffers[f].buffer,
                offset: 0,
                range: std::mem::size_of::<GpuFogVolumeUpload>() as vk::DeviceSize,
            }];
            let fog_cluster_info = [vk::DescriptorBufferInfo {
                buffer: partial.fog_cluster_buffers[f].buffer,
                offset: 0,
                range: std::mem::size_of::<[GpuFogClusterEntry; FOG_VOLUME_CLUSTER_COUNT]>()
                    as vk::DeviceSize,
            }];
            let fog_index_info = [vk::DescriptorBufferInfo {
                buffer: partial.fog_cluster_index_buffers[f].buffer,
                offset: 0,
                range: std::mem::size_of::<[u32; FOG_VOLUME_INDEX_COUNT]>() as vk::DeviceSize,
            }];
            let combustion_light_moment_info = [vk::DescriptorBufferInfo {
                buffer: partial.combustion_light_moment_buffers[f].buffer,
                offset: 0,
                range: std::mem::size_of::<[GpuCombustionLightMoment; COMBUSTION_LIGHT_GRID_COUNT]>(
                ) as vk::DeviceSize,
            }];
            let set = partial.descriptor_sets[f];
            let base_noise_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.density_noise_sampler)
                .image_view(partial.base_noise_volume.as_ref().expect("base noise").view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let detail_noise_info = [vk::DescriptorImageInfo::default()
                .sampler(partial.density_noise_sampler)
                .image_view(
                    partial
                        .detail_noise_volume
                        .as_ref()
                        .expect("detail noise")
                        .view,
                )
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
            let writes = [
                write_storage_image(set, 0, &lighting_info),
                write_uniform_buffer(set, 1, &params_info),
                write_combined_image_sampler(set, 6, &history_info),
                write_storage_buffer(set, 7, &fog_volume_info),
                write_storage_buffer(set, 8, &fog_cluster_info),
                write_storage_buffer(set, 9, &fog_index_info),
                write_combined_image_sampler(set, 10, &base_noise_info),
                write_combined_image_sampler(set, 11, &detail_noise_info),
                write_storage_image(set, 12, &emission_history_write_info),
                write_combined_image_sampler(set, 13, &previous_emission_history_info),
                write_storage_image(set, 14, &combustion_state_write_info),
                write_combined_image_sampler(set, 15, &previous_combustion_state_info),
                write_storage_image(set, 16, &combustion_dynamics_write_info),
                write_combined_image_sampler(set, 17, &previous_combustion_dynamics_info),
                write_storage_buffer(set, 18, &combustion_light_moment_info),
                write_storage_image(set, 22, &combustion_optical_write_info),
                write_combined_image_sampler(set, 23, &previous_combustion_optical_info),
            ];
            // SAFETY: the written descriptor sets and the referenced froxel image
            // view + param UBO are freshly created here and not yet in use by any
            // in-flight frame.
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        // ── 7. Per-FIF integration parameters ─────────────────────────
        // The hybrid distribution makes slab thickness depend on Z. Keep the
        // parameters per-FIF so future weather-driven reach changes cannot
        // introduce a host-write / in-flight-read WAR hazard.
        let int_param_size = std::mem::size_of::<IntegrationParams>() as vk::DeviceSize;
        let int_params = IntegrationParams {
            grid: [
                config.grid_far_meters as f32 * WORLD_UNITS_PER_METER,
                LINEAR_DEPTH,
                LINEAR_SLICE_FRACTION,
                extent.depth as f32,
            ],
        };
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            let mut buffer = try_or_cleanup!(GpuBuffer::create_host_visible(
                device,
                allocator,
                int_param_size,
                vk::BufferUsageFlags::UNIFORM_BUFFER,
            ));
            try_or_cleanup!(buffer
                .write_mapped(device, std::slice::from_ref(&int_params))
                .context("write integration params"));
            partial.integration_param_buffers.push(buffer);
        }

        // ── 8. Integration descriptor set layout ──────────────────────
        let int_bindings = [
            // 0: read-only injection volume
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 1: write-only integrated volume
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            // 2: dt UBO
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        validate_set_layout(
            0,
            &int_bindings,
            &[ReflectedShader {
                name: "volumetrics_integrate.comp",
                spirv: VOLUMETRICS_INTEGRATE_COMP_SPV,
            }],
            "volumetrics_integrate",
            &[],
        )
        .expect(
            "volumetrics integration layout drifted against volumetrics_integrate.comp (see #427)",
        );
        // SAFETY: `device` is live; `int_bindings` outlives the call; result is
        // owned by `partial` and destroyed on error / in destroy().
        partial.integration_descriptor_set_layout = try_or_cleanup!(unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&int_bindings),
                    None,
                )
                .context("Volumetrics integration descriptor set layout")
        });

        // SAFETY: `device` is live; the integration descriptor set layout was
        // just created above and is still live; result owned by `partial`.
        partial.integration_pipeline_layout = try_or_cleanup!(unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(std::slice::from_ref(
                        &partial.integration_descriptor_set_layout,
                    )),
                    None,
                )
                .context("Volumetrics integration pipeline layout")
        });

        // ── 9. Integration compute pipeline ───────────────────────────
        // Shared builder (#1751); try_or_cleanup! rolls back `partial` on error.
        partial.integration_pipeline = try_or_cleanup!(super::pipeline::create_compute_pipeline(
            device,
            pipeline_cache,
            VOLUMETRICS_INTEGRATE_COMP_SPV,
            partial.integration_pipeline_layout,
            "Volumetrics integration",
        ));

        // ── 10. Integration descriptor pool + sets ────────────────────
        // Pool sizes derived from `int_bindings` (#1030 / REN-D10-NEW-09).
        partial.integration_descriptor_pool =
            try_or_cleanup!(DescriptorPoolBuilder::from_layout_bindings(
                &int_bindings,
                MAX_FRAMES_IN_FLIGHT as u32,
            )
            .max_sets(MAX_FRAMES_IN_FLIGHT as u32)
            .build(device, "Volumetrics integration descriptor pool"));

        let int_layouts = vec![partial.integration_descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: `device` is live; the integration descriptor pool was just
        // built and `int_layouts` (clones of the live set layout) outlive the call.
        partial.integration_descriptor_sets = try_or_cleanup!(unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(partial.integration_descriptor_pool)
                        .set_layouts(&int_layouts),
                )
                .context("Volumetrics integration descriptor sets")
        });

        // ── 11. Write integration descriptor sets ─────────────────────
        for f in 0..MAX_FRAMES_IN_FLIGHT {
            let inj_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.lighting_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let int_info = [vk::DescriptorImageInfo::default()
                .image_view(partial.integrated_volumes[f].view)
                .image_layout(vk::ImageLayout::GENERAL)];
            let ubo_info = [vk::DescriptorBufferInfo {
                buffer: partial.integration_param_buffers[f].buffer,
                offset: 0,
                range: int_param_size,
            }];
            let set = partial.integration_descriptor_sets[f];
            let int_writes = [
                write_storage_image(set, 0, &inj_info),
                write_storage_image(set, 1, &int_info),
                write_uniform_buffer(set, 2, &ubo_info),
            ];
            // SAFETY: the written integration descriptor sets and the referenced
            // froxel image views + dt UBO are freshly created here and not yet in
            // use by any in-flight frame.
            unsafe { device.update_descriptor_sets(&int_writes, &[]) };
        }

        log::info!(
            "Volumetrics pipeline created from render {}x{}: {}x{}x{} froxels (1/{} XY), {} MiB / slot ({} volumes: 5×RGBA16F + R32F), far={} m",
            render_extent.width,
            render_extent.height,
            extent.width,
            extent.height,
            extent.depth,
            config.froxel_xy_divisor,
            (extent.width as u64
                * extent.height as u64
                * extent.depth as u64
                * FROXEL_BYTES_PER_SLOT)
                / (1024 * 1024),
            FROXEL_VOLUMES_PER_SLOT,
            config.grid_far_meters,
        );

        Ok(partial)
    }

    fn create_volume(
        device: &ash::Device,
        allocator: &SharedAllocator,
        name: &str,
        extent: vk::Extent3D,
        format: vk::Format,
        usage: vk::ImageUsageFlags,
    ) -> Result<FroxelSlot> {
        let img_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_3D)
            .format(format)
            .extent(extent)
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        // SAFETY: `device` is live; `img_info` outlives the call; the returned
        // image is owned by the FroxelSlot and destroyed on error / in destroy().
        let image = unsafe {
            device
                .create_image(&img_info, None)
                .with_context(|| format!("create {name}"))?
        };

        // SAFETY (get_image_memory_requirements below): `image` was just created
        // above by us and is still live; `device` outlives the call.
        let alloc = match allocator
            .lock()
            .expect("allocator lock")
            .allocate(&vk_alloc::AllocationCreateDesc {
                name,
                // SAFETY: pure query — `device` is the live logical device and
                // `image` was just created by it above; the call only reads the
                // memory requirements into a caller-owned struct.
                requirements: unsafe { device.get_image_memory_requirements(image) },
                location: gpu_allocator::MemoryLocation::GpuOnly,
                linear: false,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            })
            .with_context(|| format!("allocate {name}"))
        {
            Ok(a) => a,
            Err(e) => {
                // SAFETY: `image` was created above by us and not yet destroyed;
                // `device` is live on this allocation-failure cleanup path.
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };

        // SAFETY: `image` was just created and `alloc` was just allocated; both
        // are live, the offset/memory come from the same allocation; `device` is live.
        if let Err(e) = unsafe {
            device
                .bind_image_memory(image, alloc.memory(), alloc.offset())
                .with_context(|| format!("bind {name}"))
        } {
            allocator.lock().expect("allocator lock").free(alloc).ok();
            // SAFETY: `image` was created above by us and not yet destroyed;
            // `device` is live on this bind-failure cleanup path.
            unsafe { device.destroy_image(image, None) };
            return Err(e);
        }

        // SAFETY: `device` is live; `image` was created+bound above and is still
        // live; the view is owned by the FroxelSlot and destroyed on error / in destroy().
        let view = match unsafe {
            device
                .create_image_view(
                    &vk::ImageViewCreateInfo::default()
                        .image(image)
                        .view_type(vk::ImageViewType::TYPE_3D)
                        .format(format)
                        .subresource_range(super::descriptors::color_subresource_single_mip()),
                    None,
                )
                .with_context(|| format!("view {name}"))
        } {
            Ok(v) => v,
            Err(e) => {
                allocator.lock().expect("allocator lock").free(alloc).ok();
                // SAFETY: `image` was created above by us and not yet destroyed;
                // `device` is live on this view-creation-failure cleanup path.
                unsafe { device.destroy_image(image, None) };
                return Err(e);
            }
        };

        Ok(FroxelSlot {
            image,
            view,
            allocation: Some(alloc),
        })
    }

    /// One-time UNDEFINED → GENERAL transition for every writable froxel
    /// volume followed by a `cmd_clear_color_image`: lighting/integration use
    /// `(rgb=0 inscatter, a=1 transmittance)`, while emission provenance and
    /// transported combustion fields use zero (the shader interprets empty
    /// chemistry as ambient air). Without the integrated clear, uninitialized
    /// `vol.a ≈ 0`
    /// makes the composite formula `final = scene * vol.a + vol.rgb`
    /// collapse the scene to black on the first frame volumetrics is
    /// enabled (#1082). Call once after `new()`.
    ///
    /// This neutral value is also the entire safety net for
    /// `VOLUMETRIC_OUTPUT_CONSUMED == false`: #1926 removed
    /// `composite.frag`'s shader-side gate, so `combined * vol.a +
    /// vol.rgb` runs unconditionally regardless of the const.
    /// `post_passes.rs` skips both volumetric dispatches when the const
    /// is false, which means composite reads whatever this clear left
    /// behind — do not "optimize" it to a plain zero-fill, that would
    /// make the off-path black out the scene.
    ///
    /// # Safety
    ///
    /// Caller must ensure all passed Vulkan handles (`device`, `cmd`) are
    /// valid and live, `cmd` is in the recording state, the device is not
    /// lost, and the froxel images are not concurrently accessed by another
    /// command buffer.
    pub unsafe fn initialize_layouts(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        pool: vk::CommandPool,
    ) -> Result<()> {
        // #2231 / REN-D5-03 — memoized: this function reruns on every window
        // resize (the whole pipeline is rebuilt because the froxel grid
        // follows render resolution), but the noise itself is resolution-
        // independent, so regenerating it via ~10^7 hash evaluations per
        // resize was pure waste. See `noise::cached_base_density_noise`.
        let base_texels = cached_base_density_noise();
        let detail_texels = cached_detail_density_noise();
        let make_staging = |bytes: &[u8]| -> Result<GpuBuffer> {
            let mut buffer = GpuBuffer::create_host_visible(
                device,
                allocator,
                bytes.len() as vk::DeviceSize,
                vk::BufferUsageFlags::TRANSFER_SRC,
            )?;
            if let Err(error) = buffer.write_mapped(device, bytes) {
                buffer.destroy(device, allocator);
                return Err(error);
            }
            Ok(buffer)
        };
        let mut base_staging = make_staging(base_texels)?;
        let mut detail_staging = match make_staging(detail_texels) {
            Ok(buffer) => buffer,
            Err(error) => {
                base_staging.destroy(device, allocator);
                return Err(error);
            }
        };

        let upload_result = super::texture::with_one_time_commands(device, queue, pool, |cmd| {
            let full_range = super::descriptors::color_subresource_single_mip();
            let base_noise = self.base_noise_volume.as_ref().expect("base noise");
            let detail_noise = self.detail_noise_volume.as_ref().expect("detail noise");

            // ── 1. Initialize writable froxels and upload-only noise images.
            let mut barriers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT * 6 + 2);
            for slot in self
                .lighting_volumes
                .iter()
                .chain(self.integrated_volumes.iter())
                .chain(self.emission_history_volumes.iter())
                .chain(self.combustion_state_volumes.iter())
                .chain(self.combustion_dynamics_volumes.iter())
                .chain(self.combustion_optical_volumes.iter())
            {
                barriers.push(image_barrier_undef_to_general(slot.image).dst_access_mask(
                    vk::AccessFlags::SHADER_READ
                        | vk::AccessFlags::SHADER_WRITE
                        | vk::AccessFlags::TRANSFER_WRITE,
                ));
            }
            for noise in [base_noise, detail_noise] {
                // #2413 / TD2-116 SIBLING — same shape as the three sites
                // the issue named; `full_range` is
                // `color_subresource_single_mip()`, which is what the helper
                // builds for (mips=1, layers=1).
                barriers.push(super::descriptors::image_barrier_undef_to_transfer_dst(
                    noise.image,
                    1,
                ));
            }
            // SAFETY: `cmd` is recording; every image is freshly allocated
            // and exclusively owned by this pipeline. The barriers move
            // froxels/noise from UNDEFINED to the layouts used by the
            // immediately following transfer commands.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::NONE,
                    vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &barriers,
                );
            }

            // ── 2. Clear froxels to the no-fog sentinel.
            // Zero inscatter + unit transmittance is the correct "no fog"
            // sentinel: composite `final = scene * vol.a + vol.rgb`
            // becomes `scene * 1 + 0 = scene`.
            let clear_value = vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 1.0],
            };
            for slot in self
                .lighting_volumes
                .iter()
                .chain(self.integrated_volumes.iter())
            {
                // SAFETY: this freshly allocated froxel is in GENERAL, has
                // TRANSFER_DST usage, and cannot be referenced by an in-flight
                // frame before initialization publishes the pipeline.
                unsafe {
                    device.cmd_clear_color_image(
                        cmd,
                        slot.image,
                        vk::ImageLayout::GENERAL,
                        &clear_value,
                        &[full_range],
                    );
                }
            }
            let clear_history = vk::ClearColorValue { float32: [0.0; 4] };
            for slot in self
                .emission_history_volumes
                .iter()
                .chain(self.combustion_state_volumes.iter())
                .chain(self.combustion_dynamics_volumes.iter())
                .chain(self.combustion_optical_volumes.iter())
            {
                // SAFETY: same ownership/layout/usage contract as the RGBA
                // froxels above; R32F consumes only the first clear channel,
                // while the RGBA16F transport fields consume all four.
                unsafe {
                    device.cmd_clear_color_image(
                        cmd,
                        slot.image,
                        vk::ImageLayout::GENERAL,
                        &clear_history,
                        &[full_range],
                    );
                }
            }

            // ── 3. Copy deterministic R8 fields into their immutable images.
            let subresource = vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .mip_level(0)
                .base_array_layer(0)
                .layer_count(1);
            let base_copy = vk::BufferImageCopy::default()
                .image_subresource(subresource)
                .image_extent(vk::Extent3D {
                    width: BASE_NOISE_SIZE,
                    height: BASE_NOISE_SIZE,
                    depth: BASE_NOISE_SIZE,
                });
            let detail_copy = vk::BufferImageCopy::default()
                .image_subresource(subresource)
                .image_extent(vk::Extent3D {
                    width: DETAIL_NOISE_SIZE,
                    height: DETAIL_NOISE_SIZE,
                    depth: DETAIL_NOISE_SIZE,
                });
            // SAFETY: both staging buffers are live and fully populated; both
            // destination images are in TRANSFER_DST_OPTIMAL with matching R8
            // extents and TRANSFER_DST usage.
            unsafe {
                device.cmd_copy_buffer_to_image(
                    cmd,
                    base_staging.buffer,
                    base_noise.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[base_copy],
                );
                device.cmd_copy_buffer_to_image(
                    cmd,
                    detail_staging.buffer,
                    detail_noise.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[detail_copy],
                );
            }

            // ── 4. Publish transfer writes to the compute injector.
            memory_barrier(
                device,
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
            );
            let noise_ready = [base_noise, detail_noise].map(|noise| {
                vk::ImageMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .image(noise.image)
                    .subresource_range(full_range)
            });
            // SAFETY: `cmd` is recording and the noise images were written by
            // the preceding copies. This makes those writes visible and moves
            // both images into their descriptor-declared sampled layout.
            unsafe {
                device.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &noise_ready,
                );
            }

            Ok(())
        });

        // The one-time submit waits for the queue before returning, so neither
        // staging buffer can still be referenced here.
        base_staging.destroy(device, allocator);
        detail_staging.destroy(device, allocator);
        upload_result?;
        log::info!(
            "Volumetric density noise uploaded: {}^3 base + {}^3 detail ({} KiB R8)",
            BASE_NOISE_SIZE,
            DETAIL_NOISE_SIZE,
            (base_texels.len() + detail_texels.len()) / 1024,
        );
        Ok(())
    }

    /// Dispatch both volumetric compute passes for this frame:
    ///   1. **Injection** writes per-froxel `(rgb=inscatter, a=extinction)`
    ///      from sun lighting + Henyey-Greenstein phase function into
    ///      the lighting volume.
    ///   2. **Integration** Z-scans the lighting volume per (x,y) column
    ///      and writes `(rgb=∫inscatter, a=T_cum)` into the integrated
    ///      volume — the result composite samples once per fragment.
    ///
    /// Must be called AFTER the main render pass ends (so caustic /
    /// SVGF have already scheduled their reads against the G-buffer)
    /// and BEFORE composite (so the integrated volume is ready to
    /// sample). Natural slot: between caustic and TAA in `draw.rs`.
    ///
    /// # Safety
    ///
    /// Caller must ensure all passed Vulkan handles (`device`, `cmd`) are
    /// valid and live, `cmd` is in the recording state, the device is not
    /// lost, and the froxel images and bound buffers are not in use by
    /// another in-flight command buffer.
    pub unsafe fn dispatch(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        params: &VolumetricsParams,
        fog_volumes: &[GpuFogVolume],
    ) -> Result<()> {
        // #1105 / REN-D18-003 — injection descriptor binding 2 (TLAS) is
        // not written at construction; caller is required to call
        // write_tlas() each frame before dispatch. Without this latch
        // the validation layer reports "descriptor not updated" noise in
        // debug, and the injection shader's shadow ray would read an
        // undefined acceleration structure.
        debug_assert!(
            self.tlas_written[frame],
            "VolumetricsPipeline::dispatch called without prior write_tlas() for frame {}",
            frame,
        );
        // Reset latch so the assert fires if write_tlas() is skipped next frame.
        self.tlas_written[frame] = false;
        // Same shape for bindings 3/4/5 (Phase 2b lights/clusters) —
        // see `write_lights_and_clusters`.
        debug_assert!(
            self.lights_written[frame],
            "VolumetricsPipeline::dispatch called without prior write_lights_and_clusters() for frame {}",
            frame,
        );
        self.lights_written[frame] = false;
        debug_assert!(
            self.boundary_geometry_written[frame],
            "VolumetricsPipeline::dispatch called without prior write_boundary_geometry() for frame {}",
            frame,
        );
        self.boundary_geometry_written[frame] = false;
        // ── Stage A: write injection-pass UBO ────────────────────────
        // The buffer is HOST_VISIBLE + HOST_COHERENT via
        // `GpuBuffer::create_host_visible`, but the execution
        // dependency (HOST → COMPUTE) is still required by the spec
        // to make the write visible to the compute stage.
        let mut frame_params = *params;
        frame_params.prev_camera_pos[3] = if self.history_valid { 1.0 } else { 0.0 };
        let simulation_time = if frame_params.temporal_params[2].is_finite() {
            frame_params.temporal_params[2].max(0.0)
        } else {
            self.last_simulation_time_seconds.unwrap_or(0.0)
        };
        let simulation_dt = if self.history_valid {
            self.last_simulation_time_seconds
                .map(|previous| (simulation_time - previous).clamp(0.0, 1.0 / 15.0))
                .unwrap_or(1.0 / 60.0)
        } else {
            // Seed a newly created/reset field with one nominal frame of
            // source input so a flame is visible immediately.
            1.0 / 60.0
        };
        // #3131 (PERF-D5-01) — `transportCombustion`'s whole RK2 advection
        // block (source/midpoint/destination probes, 18-fetch neighbour
        // gather) is gated in-shader on `dt > 0.0`. Without a scene-level
        // gate here, every froxel of every fog-bearing cell pays it even
        // when nothing is burning: `combustionActivity(...) < 0.08` is the
        // *quiet* case, so the expensive neighbour gather fires precisely
        // on the froxels with no combustion. `combustion_active_until_seconds`
        // is exactly the CPU-side "combustion is (or was recently) active"
        // signal `requires_dispatch` already maintains — reuse it to zero
        // `dt` and skip the stencil outright when nothing is transporting.
        let combustion_active = combustion_transport_active(
            fog_volumes,
            simulation_time,
            self.combustion_active_until_seconds,
        );
        frame_params.fog_reference[3] = if combustion_active {
            simulation_dt
        } else {
            0.0
        };
        self.pending_simulation_time_seconds = Some(simulation_time);
        let camera_position = [
            frame_params.camera_pos[0],
            frame_params.camera_pos[1],
            frame_params.camera_pos[2],
        ];
        self.combustion_light_grid_centers[frame] = camera_position;
        self.combustion_light_grid_valid[frame] = true;
        let fog_far = self.far_distance_world().max(1.0);
        // #2242 (REN-D16-04) — `fog_volume_upload.count` (the shader's
        // `fogVolumeCount`) is rewritten unconditionally in BOTH branches
        // below, even though the cluster/index GPU buffer writes further
        // down are skipped in this empty branch as a pure optimisation
        // (nothing to cluster). That's what makes it safe to skip them:
        // `localFogCluster`'s `fogVolumeCount == 0u` early-out always
        // gates the exact case where the cluster/index buffers weren't
        // touched this frame, so their contents (which could be a
        // previous frame's, or a previous cell's) are never read.
        frame_params.local_volume_grid = if fog_volumes.is_empty() {
            self.fog_volume_upload.count = [0; 4];
            let cell_size = (2.0 * fog_far) / FOG_VOLUME_CLUSTER_DIM as f32;
            [
                camera_position[0] - fog_far,
                camera_position[1] - fog_far,
                camera_position[2] - fog_far,
                cell_size.recip(),
            ]
        } else {
            build_fog_volume_clusters(
                fog_volumes,
                camera_position,
                fog_far,
                &mut self.fog_volume_upload,
                &mut self.fog_cluster_entries,
                &mut self.fog_cluster_indices,
            )
        };
        self.param_buffers[frame].write_mapped(device, std::slice::from_ref(&frame_params))?;
        self.fog_volume_buffers[frame].write_mapped(
            device,
            std::slice::from_ref(self.fog_volume_upload.as_ref()),
        )?;
        if !fog_volumes.is_empty() {
            self.fog_cluster_buffers[frame].write_mapped(
                device,
                std::slice::from_ref(self.fog_cluster_entries.as_ref()),
            )?;
            self.fog_cluster_index_buffers[frame].write_mapped(
                device,
                std::slice::from_ref(self.fog_cluster_indices.as_ref()),
            )?;
        }
        // HOST → COMPUTE_SHADER (UBO flush; execution dependency required even
        // for HOST_COHERENT memory to make UBO/SSBO writes visible to compute).
        memory_barrier(
            device,
            cmd,
            vk::PipelineStageFlags::HOST,
            vk::AccessFlags::HOST_WRITE,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::UNIFORM_READ
                | vk::AccessFlags::SHADER_READ
                | vk::AccessFlags::SHADER_WRITE,
        );

        let subresource = super::descriptors::color_subresource_single_mip();

        // ── Stage B: pre-injection history barriers ─────────────────
        // The raw lighting volumes, scalar emission-provenance sidecars, and
        // transported chemistry/velocity/optical fields
        // live in GENERAL across their lifetime (set by
        // `initialize_layouts`), so no layout transitions occur. Sequence
        // the slot being recycled from last frame's history READ to this
        // frame's WRITE, and publish the previous slot's WRITE to both
        // reprojected history reads.
        let pre_inject = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.lighting_volumes[frame].image)
            .subresource_range(subresource);
        let previous = (frame + MAX_FRAMES_IN_FLIGHT - 1) % MAX_FRAMES_IN_FLIGHT;
        let history_ready = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.lighting_volumes[previous].image)
            .subresource_range(subresource);
        let pre_emission_history_write = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.emission_history_volumes[frame].image)
            .subresource_range(subresource);
        let emission_history_ready = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.emission_history_volumes[previous].image)
            .subresource_range(subresource);
        let pre_combustion_state_write = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.combustion_state_volumes[frame].image)
            .subresource_range(subresource);
        let combustion_state_ready = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.combustion_state_volumes[previous].image)
            .subresource_range(subresource);
        let pre_combustion_dynamics_write = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.combustion_dynamics_volumes[frame].image)
            .subresource_range(subresource);
        let combustion_dynamics_ready = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.combustion_dynamics_volumes[previous].image)
            .subresource_range(subresource);
        let pre_combustion_optical_write = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.combustion_optical_volumes[frame].image)
            .subresource_range(subresource);
        let combustion_optical_ready = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.combustion_optical_volumes[previous].image)
            .subresource_range(subresource);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[
                pre_inject,
                history_ready,
                pre_emission_history_write,
                emission_history_ready,
                pre_combustion_state_write,
                combustion_state_ready,
                pre_combustion_dynamics_write,
                combustion_dynamics_ready,
                pre_combustion_optical_write,
                combustion_optical_ready,
            ],
        );

        // ── Stage C: dispatch injection ──────────────────────────────
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.descriptor_sets[frame]],
            &[],
        );
        let inj_groups_x = self.extent.width.div_ceil(WORKGROUP_X);
        let inj_groups_y = self.extent.height.div_ceil(WORKGROUP_Y);
        let inj_groups_z = self.extent.depth.div_ceil(WORKGROUP_Z);
        device.cmd_dispatch(cmd, inj_groups_x, inj_groups_y, inj_groups_z);

        // ── Stage D: barrier between injection and integration ──────
        // Sequence the injection WRITE on the lighting volume against
        // the integration READ of the same image. The integration
        // shader reads every froxel of the lighting volume; without
        // this barrier the recurrence reads stale (or partially-
        // written) data.
        let inj_to_int = image_barrier_general_write_to_read(self.lighting_volumes[frame].image);
        // Plus a barrier on the integrated volume so last frame's
        // composite READ (sampler3D in fragment shader) is sequenced
        // against this frame's integration WRITE. #3647 — the slot's
        // prior use may instead have been `record_neutral_frame`'s
        // `vkCmdClearColorImage` (a TRANSFER_WRITE), which is always the
        // case at scene load while no TLAS exists yet; without TRANSFER
        // in the source scope that clear has no dependency chain into
        // this write.
        let pre_int_write = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(self.integrated_volumes[frame].image)
            .subresource_range(subresource);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER
                | vk::PipelineStageFlags::FRAGMENT_SHADER
                | vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[inj_to_int, pre_int_write],
        );

        // ── Stage E: dispatch integration ────────────────────────────
        device.cmd_bind_pipeline(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.integration_pipeline,
        );
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.integration_pipeline_layout,
            0,
            &[self.integration_descriptor_sets[frame]],
            &[],
        );
        // 2D dispatch: one thread per (x, y) column; each thread Z-marches
        // all depth slices internally.
        let int_groups_x = self.extent.width.div_ceil(WORKGROUP_X);
        let int_groups_y = self.extent.height.div_ceil(WORKGROUP_Y);
        device.cmd_dispatch(cmd, int_groups_x, int_groups_y, 1);

        // ── Stage F: post-integration barrier ────────────────────────
        // Make the integrated-volume WRITE visible to the composite
        // fragment shader's sampler3D READ.
        let post_int = image_barrier_general_write_to_read(self.integrated_volumes[frame].image);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[post_int],
        );
        // Publish binding 18's atomic writes to the host. The actual read is
        // deferred until this slot's fence is waited on at its next reuse;
        // the barrier supplies the device->host memory dependency that the
        // fence's device-only access scope does not create by itself.
        memory_barrier(
            device,
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::AccessFlags::SHADER_WRITE,
            vk::PipelineStageFlags::HOST,
            vk::AccessFlags::HOST_READ,
        );

        self.dispatched_this_frame = true;
        Ok(())
    }

    pub fn signal_history_reset(&mut self) {
        self.history_valid = false;
        self.dispatched_this_frame = false;
        self.last_simulation_time_seconds = None;
        self.pending_simulation_time_seconds = None;
        self.combustion_active_until_seconds = f32::NEG_INFINITY;
        self.combustion_light_grid_valid.fill(false);
    }

    /// Advance temporal validity only after the command buffer that wrote the
    /// new V-buffer was submitted successfully.
    pub fn mark_frame_completed(&mut self) {
        if self.dispatched_this_frame {
            self.history_valid = true;
            self.dispatched_this_frame = false;
            self.last_simulation_time_seconds = self.pending_simulation_time_seconds.take();
        }
    }

    pub fn config(&self) -> VolumetricsConfig {
        self.config
    }

    pub fn extent(&self) -> vk::Extent3D {
        self.extent
    }

    pub fn far_distance_world(&self) -> f32 {
        self.config.grid_far_meters as f32 * WORLD_UNITS_PER_METER
    }

    /// Drain the prior use of `frame`'s transported-field radiant moments,
    /// reset the buffer for this frame's injector, and append a bounded set of
    /// inverse-square surface lights. The caller must have waited the frame
    /// slot's fence. One frame-slot of latency is deliberate: it removes a
    /// GPU->CPU stall while still following the advected/cooled field rather
    /// than the current source primitive.
pub fn append_combustion_surface_lights(
        &mut self,
        device: &ash::Device,
        frame: usize,
        source_volumes: &[GpuFogVolume],
        lights: &mut Vec<GpuLight>,
    ) -> Result<usize> {
        let camera_world = self.combustion_light_grid_centers[frame];
        let had_grid = std::mem::take(&mut self.combustion_light_grid_valid[frame]);
        let buffer = &mut self.combustion_light_moment_buffers[frame];
        buffer.invalidate_if_needed(device)?;
        let mut moments = [GpuCombustionLightMoment::default(); COMBUSTION_LIGHT_GRID_COUNT];
        {
            let bytes = buffer.mapped_slice_mut()?;
            let stride = std::mem::size_of::<GpuCombustionLightMoment>();
            anyhow::ensure!(
                bytes.len() >= stride * COMBUSTION_LIGHT_GRID_COUNT,
                "combustion light moment buffer is truncated"
            );
            for (index, moment) in moments.iter_mut().enumerate() {
                let start = index * stride;
                *moment = decode_combustion_light_moment(&bytes[start..start + stride]);
            }
            bytes[..stride * COMBUSTION_LIGHT_GRID_COUNT].fill(0);
        }
        buffer.flush_if_needed(device)?;
        if !had_grid {
            return Ok(0);
        }

        let occupied_bins = moments.iter().filter(|moment| moment.weight > 0).count();
        let weight_quanta: u64 = moments.iter().map(|moment| u64::from(moment.weight)).sum();
        let radiance_quanta: u64 = moments
            .iter()
            .map(|moment| {
                u64::from(moment.radiant_r)
                    + u64::from(moment.radiant_g)
                    + u64::from(moment.radiant_b)
            })
            .sum();
        let authored_count = lights.len();
        let nuclear_source_active = source_volumes.iter().any(|volume| {
            (volume.profile_params[0] - FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR).abs() < 0.5
        });
        let mut decoded_candidates = 0;
        let mut suppressed_candidates = 0;
        self.combustion_light_candidates.clear();
        for (bin_index, moment) in moments.into_iter().enumerate() {
            let Some(mut light) = combustion_light_from_moment(bin_index, moment, camera_world) else {
                continue;
            };
            if nuclear_source_active {
                light.color_type[..3]
                    .iter_mut()
                    .for_each(|channel| *channel *= NUCLEAR_COMBUSTION_SURFACE_LIGHT_SCALE);
                light.position_radius[3] *= NUCLEAR_COMBUSTION_SURFACE_LIGHT_SCALE.sqrt();
            }
            decoded_candidates += 1;
            // An authored LIGH inside a steady source volume is already the
            // content's surface-light surrogate. Suppress only while this
            // centroid is also inside that same canonical volume; once the
            // transported field leaves it (plume or blast), it contributes.
            if combustion_light_is_inside_authored_source(
                &light,
                source_volumes,
                &lights[..authored_count],
            ) {
                suppressed_candidates += 1;
                continue;
            }
            self.combustion_light_candidates
                .push((light.gi_priority_score(), light));
        }
        self.combustion_light_candidates
            .sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
        let available = MAX_LIGHTS.saturating_sub(lights.len());
        let append_count = available
            .min(MAX_COMBUSTION_SURFACE_LIGHTS)
            .min(self.combustion_light_candidates.len());
        lights.extend(
            self.combustion_light_candidates[..append_count]
                .iter()
                .map(|(_, light)| *light),
        );
        let strongest_centroid_metres = self
            .combustion_light_candidates
            .first()
            .map(|(_, light)| {
                [
                    light.position_radius[0] / WORLD_UNITS_PER_METER,
                    light.position_radius[1] / WORLD_UNITS_PER_METER,
                    light.position_radius[2] / WORLD_UNITS_PER_METER,
                ]
            })
            .unwrap_or([0.0; 3]);
        let topology = [
            occupied_bins as i32,
            decoded_candidates,
            suppressed_candidates,
            append_count as i32,
            (strongest_centroid_metres[0] * 4.0).round() as i32,
            (strongest_centroid_metres[1] * 4.0).round() as i32,
            (strongest_centroid_metres[2] * 4.0).round() as i32,
        ];
        if self.last_combustion_light_topology != Some(topology) {
            log::debug!(
                "combustion light reduction: occupied_bins={occupied_bins} \
                 decoded_candidates={decoded_candidates} \
                 suppressed_candidates={suppressed_candidates} appended_lights={append_count} \
                 weight_quanta={weight_quanta} radiance_quanta={radiance_quanta} \
                 strongest_centroid_m={:.2},{:.2},{:.2}",
                strongest_centroid_metres[0],
                strongest_centroid_metres[1],
                strongest_centroid_metres[2],
            );
            self.last_combustion_light_topology = Some(topology);
        }
        Ok(append_count)
    }

    /// Decide whether the two volumetric passes need to run this frame.
    /// Transported soot must outlive its emitter, so the old
    /// `global fog || current volumes` test was insufficient: deleting an
    /// explosion entity immediately froze/cleared its smoke. The latch is
    /// canonical-profile based and therefore carries no source-game policy.
    pub fn requires_dispatch(
        &mut self,
        total_time_seconds: f32,
        has_global_medium: bool,
        fog_volumes: &[GpuFogVolume],
    ) -> bool {
        let now = if total_time_seconds.is_finite() {
            total_time_seconds.max(0.0)
        } else {
            self.last_simulation_time_seconds.unwrap_or(0.0)
        };
        if has_transport_emitter(fog_volumes) {
            self.combustion_active_until_seconds = now + COMBUSTION_HISTORY_LINGER_SECONDS;
        }
        has_global_medium
            || !fog_volumes.is_empty()
            || (self.history_valid && now <= self.combustion_active_until_seconds)
    }

    /// Clear a frame slot to the neutral composite value when required RT or
    /// clustered-light inputs are unavailable. This prevents the last valid
    /// fog volume from hanging over a newly loaded scene.
    ///
    /// # Safety
    ///
    /// `cmd` must be recording outside a render pass and `frame` must name an
    /// idle frame-in-flight slot.
    pub unsafe fn record_neutral_frame(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
    ) {
        let subresource = super::descriptors::color_subresource_single_mip();
        let image = self.integrated_volumes[frame].image;
        // #3647 — TRANSFER_WRITE in the source scope covers a repeat
        // neutral frame on the same slot (the common case at load: every
        // frame is neutral until the TLAS exists).
        let to_clear = vk::ImageMemoryBarrier::default()
            .src_access_mask(
                vk::AccessFlags::SHADER_READ
                    | vk::AccessFlags::SHADER_WRITE
                    | vk::AccessFlags::TRANSFER_WRITE,
            )
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(image)
            .subresource_range(subresource);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER
                | vk::PipelineStageFlags::FRAGMENT_SHADER
                | vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_clear],
        );
        device.cmd_clear_color_image(
            cmd,
            image,
            vk::ImageLayout::GENERAL,
            &vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 1.0],
            },
            &[subresource],
        );
        let to_sample = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .image(image)
            .subresource_range(subresource);
        device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[to_sample],
        );
        self.history_valid = false;
        self.dispatched_this_frame = false;
        self.last_simulation_time_seconds = None;
        self.pending_simulation_time_seconds = None;
        self.combustion_active_until_seconds = f32::NEG_INFINITY;
        self.combustion_light_grid_valid[frame] = false;
    }

    /// All per-frame-in-flight integration-output views, in slot order.
    /// Composite consumes this at construction time to bind one view
    /// per frame-in-flight descriptor set. This is the volume composite
    /// SAMPLES — not the injection-output (which is internal to the
    /// volumetrics pipeline; integration consumes it).
    pub fn integrated_views(&self) -> Vec<vk::ImageView> {
        self.integrated_volumes.iter().map(|s| s.view).collect()
    }

    /// Update the injection descriptor set's binding 2 (TLAS) for
    /// `frame`. Mirrors `CausticPipeline::write_tlas` (caustic.rs:627)
    /// — the TLAS is rebuilt each frame, so this MUST be called every
    /// frame from `draw.rs` before `dispatch`. If the caller has no
    /// TLAS available for this frame (RT unsupported, scene not yet
    /// built), they should skip both `write_tlas` AND `dispatch`;
    /// composite will reuse the prior frame's integrated volume.
    pub fn write_tlas(
        &mut self,
        device: &ash::Device,
        frame: usize,
        tlas: vk::AccelerationStructureKHR,
    ) {
        let accel_structs = [tlas];
        let mut accel_write = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&accel_structs);
        let write = write_acceleration_structure(self.descriptor_sets[frame], 2, &mut accel_write);
        // SAFETY: the injection descriptor set for `frame` is live and not in use
        // by an in-flight frame (caller invokes write_tlas before this frame's
        // dispatch); `accel_structs`/`accel_write` outlive the call.
        unsafe { device.update_descriptor_sets(&[write], &[]) };
        // Latch so dispatch's debug_assert can verify the binding was
        // written this session. (#1105 / REN-D18-003)
        self.tlas_written[frame] = true;
    }

    /// Update the injection descriptor set's bindings 3/4/5 (lights /
    /// cluster grid / light-index SSBOs) for `frame` — Phase 2b
    /// point/spot light injection. Reuses the exact same per-frame
    /// buffers `ClusterCullPipeline` already builds for the fragment
    /// shader (`scene_buffers.light_buffers()`,
    /// `cluster_cull.scene_cluster_grid_buffers`,
    /// `cluster_cull.scene_light_index_buffers`) rather than a
    /// separate froxel-space light-culling structure — see the
    /// Phase 2b design note on `volumetrics_inject.comp`.
    ///
    /// Mirrors `write_tlas`: the buffer *contents* are rebuilt every
    /// frame (cluster_cull's compute dispatch runs earlier in the
    /// same command buffer), so this MUST be called every frame from
    /// `draw.rs` before `dispatch`, after `cluster_cull.dispatch()`
    /// has recorded and after the compute→compute barrier that makes
    /// its writes visible (see the barrier note in `draw.rs`).
    #[allow(clippy::too_many_arguments)]
    pub fn write_lights_and_clusters(
        &mut self,
        device: &ash::Device,
        frame: usize,
        light_buffer: vk::Buffer,
        light_buffer_size: vk::DeviceSize,
        cluster_grid_buffer: vk::Buffer,
        cluster_grid_size: vk::DeviceSize,
        light_index_buffer: vk::Buffer,
        light_index_size: vk::DeviceSize,
    ) {
        let light_info = [vk::DescriptorBufferInfo {
            buffer: light_buffer,
            offset: 0,
            range: light_buffer_size,
        }];
        let cluster_info = [vk::DescriptorBufferInfo {
            buffer: cluster_grid_buffer,
            offset: 0,
            range: cluster_grid_size,
        }];
        let index_info = [vk::DescriptorBufferInfo {
            buffer: light_index_buffer,
            offset: 0,
            range: light_index_size,
        }];
        let set = self.descriptor_sets[frame];
        let writes = [
            write_storage_buffer(set, 3, &light_info),
            write_storage_buffer(set, 4, &cluster_info),
            write_storage_buffer(set, 5, &index_info),
        ];
        // SAFETY: the injection descriptor set for `frame` is live and not in
        // use by an in-flight frame (caller invokes this before this frame's
        // dispatch); the *_info arrays outlive the call.
        unsafe { device.update_descriptor_sets(&writes, &[]) };
        self.lights_written[frame] = true;
    }

    /// Update bindings 19/20/21 with the exact geometry backing this frame's
    /// TLAS. `instance_custom_index` is defined to index the supplied
    /// `instance_buffer`; the vertex/index offsets in that entry address the
    /// two global geometry buffers. This keeps combustion collision response
    /// on the renderer's canonical scene representation rather than building
    /// a parallel obstacle grid or importing source-game rules into runtime.
    #[allow(clippy::too_many_arguments)]
    pub fn write_boundary_geometry(
        &mut self,
        device: &ash::Device,
        frame: usize,
        instance_buffer: vk::Buffer,
        instance_buffer_size: vk::DeviceSize,
        vertex_buffer: vk::Buffer,
        vertex_buffer_size: vk::DeviceSize,
        index_buffer: vk::Buffer,
        index_buffer_size: vk::DeviceSize,
    ) {
        let instance_info = [vk::DescriptorBufferInfo {
            buffer: instance_buffer,
            offset: 0,
            range: instance_buffer_size,
        }];
        let vertex_info = [vk::DescriptorBufferInfo {
            buffer: vertex_buffer,
            offset: 0,
            range: vertex_buffer_size,
        }];
        let index_info = [vk::DescriptorBufferInfo {
            buffer: index_buffer,
            offset: 0,
            range: index_buffer_size,
        }];
        let set = self.descriptor_sets[frame];
        let writes = [
            write_storage_buffer(set, 19, &instance_info),
            write_storage_buffer(set, 20, &vertex_info),
            write_storage_buffer(set, 21, &index_info),
        ];
        // SAFETY: this frame slot is idle and all three buffers are live for
        // the subsequent dispatch; the descriptor infos outlive the call.
        unsafe { device.update_descriptor_sets(&writes, &[]) };
        self.boundary_geometry_written[frame] = true;
    }

    /// Destroy all froxel images, views, buffers, and pipeline objects.
    ///
    /// # Safety
    ///
    /// Caller must ensure `device` and `allocator` are valid and live, the
    /// device is not lost, and that no object owned by `self` is still in use
    /// by an in-flight command buffer.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for slot in self
            .lighting_volumes
            .drain(..)
            .chain(self.emission_history_volumes.drain(..))
            .chain(self.combustion_state_volumes.drain(..))
            .chain(self.combustion_dynamics_volumes.drain(..))
            .chain(self.combustion_optical_volumes.drain(..))
            .chain(self.integrated_volumes.drain(..))
            .chain(self.base_noise_volume.take())
            .chain(self.detail_noise_volume.take())
        {
            device.destroy_image_view(slot.view, None);
            device.destroy_image(slot.image, None);
            if let Some(a) = slot.allocation {
                allocator.lock().expect("allocator lock").free(a).ok();
            }
        }
        for buf in &mut self.param_buffers {
            buf.destroy(device, allocator);
        }
        // #732 LIFE-N1 pattern — drop the GpuBuffer structs after
        // their GPU allocations are freed so each one's
        // `Arc<Mutex<Allocator>>` clone releases now, not when
        // VulkanContext::Drop's `Arc::try_unwrap` has already given up.
        self.param_buffers.clear();
        for buf in &mut self.fog_volume_buffers {
            buf.destroy(device, allocator);
        }
        self.fog_volume_buffers.clear();
        for buf in &mut self.fog_cluster_buffers {
            buf.destroy(device, allocator);
        }
        self.fog_cluster_buffers.clear();
        for buf in &mut self.fog_cluster_index_buffers {
            buf.destroy(device, allocator);
        }
        self.fog_cluster_index_buffers.clear();
        for buf in &mut self.combustion_light_moment_buffers {
            buf.destroy(device, allocator);
        }
        self.combustion_light_moment_buffers.clear();
        for buf in &mut self.integration_param_buffers {
            buf.destroy(device, allocator);
        }
        self.integration_param_buffers.clear();
        if self.integration_pipeline != vk::Pipeline::null() {
            device.destroy_pipeline(self.integration_pipeline, None);
            self.integration_pipeline = vk::Pipeline::null();
        }
        if self.integration_pipeline_layout != vk::PipelineLayout::null() {
            device.destroy_pipeline_layout(self.integration_pipeline_layout, None);
            self.integration_pipeline_layout = vk::PipelineLayout::null();
        }
        if self.integration_descriptor_pool != vk::DescriptorPool::null() {
            device.destroy_descriptor_pool(self.integration_descriptor_pool, None);
            self.integration_descriptor_pool = vk::DescriptorPool::null();
        }
        if self.integration_descriptor_set_layout != vk::DescriptorSetLayout::null() {
            device.destroy_descriptor_set_layout(self.integration_descriptor_set_layout, None);
            self.integration_descriptor_set_layout = vk::DescriptorSetLayout::null();
        }
        if self.pipeline != vk::Pipeline::null() {
            device.destroy_pipeline(self.pipeline, None);
            self.pipeline = vk::Pipeline::null();
        }
        if self.pipeline_layout != vk::PipelineLayout::null() {
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            self.pipeline_layout = vk::PipelineLayout::null();
        }
        if self.descriptor_pool != vk::DescriptorPool::null() {
            device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.descriptor_pool = vk::DescriptorPool::null();
        }
        if self.descriptor_set_layout != vk::DescriptorSetLayout::null() {
            device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.descriptor_set_layout = vk::DescriptorSetLayout::null();
        }
        if self.history_sampler != vk::Sampler::null() {
            device.destroy_sampler(self.history_sampler, None);
            self.history_sampler = vk::Sampler::null();
        }
        if self.transport_sampler != vk::Sampler::null() {
            device.destroy_sampler(self.transport_sampler, None);
            self.transport_sampler = vk::Sampler::null();
        }
        if self.density_noise_sampler != vk::Sampler::null() {
            device.destroy_sampler(self.density_noise_sampler, None);
            self.density_noise_sampler = vk::Sampler::null();
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn physical_volume_reach_and_extinction_are_converted_to_world_units() {
        let reach_metres = DEFAULT_VOLUME_FAR / WORLD_UNITS_PER_METER;
        assert!((reach_metres - 128.0).abs() < 1.0e-4);

        // Unit conversion must preserve the authored optical depth:
        // sigma_world * distance_world == sigma_m * distance_m.
        let sigma_per_metre = 0.02;
        let sigma_per_world_unit = sigma_per_metre / WORLD_UNITS_PER_METER;
        let world_optical_depth = sigma_per_world_unit * DEFAULT_VOLUME_FAR;
        let metre_optical_depth = sigma_per_metre * reach_metres;
        assert!((world_optical_depth - metre_optical_depth).abs() < 1.0e-6);
    }

    #[test]
    fn authored_fog_tint_preserves_chromaticity_and_peak_energy() {
        let tint = normalize_fog_tint([0.65, 0.7, 0.8]);
        assert!((tint[0] - 0.8125).abs() < 1.0e-6);
        assert!((tint[1] - 0.875).abs() < 1.0e-6);
        assert_eq!(tint[2], 1.0);

        let spectral_albedo = tint.map(|channel| channel * DEFAULT_SINGLE_SCATTER_ALBEDO);
        assert_eq!(
            spectral_albedo[2], DEFAULT_SINGLE_SCATTER_ALBEDO,
            "fog tint must not increase the medium's peak scattering energy"
        );
    }

    #[test]
    fn packed_fog_tint_reconstructs_authored_equilibrium_radiance() {
        let authored = [0.65, 0.7, 0.8];
        let packed = pack_fog_tint(authored);
        assert!((packed[3] - 0.8).abs() < 1.0e-6);
        for channel in 0..3 {
            assert!((packed[channel] * packed[3] - authored[channel]).abs() < 1.0e-6);
        }

        assert_eq!(pack_fog_tint([f32::NAN, -1.0, f32::INFINITY]), [0.0; 4]);
    }

    #[test]
    fn black_or_invalid_fog_tint_stays_finite_and_absorptive() {
        assert_eq!(normalize_fog_tint([0.0; 3]), [0.0; 3]);
        assert_eq!(
            normalize_fog_tint([f32::NAN, -1.0, f32::INFINITY]),
            [0.0; 3]
        );
        assert_eq!(normalize_fog_tint([f32::NAN, 2.0, -4.0]), [0.0, 1.0, 0.0]);
    }

    #[test]
    fn injector_consumes_authored_spectral_albedo_and_density_volumes() {
        let shader = include_str!("../../shaders/volumetrics_inject.comp");
        for contract in [
            "vec4 fog_tint;",
            "params.fog_tint.rgb",
            "params.fog_tint.a",
            "vec3 authored_inscatter = global_extinction",
            "global_extinction * global_albedo",
            "sampler3D baseDensityNoise",
            "sampler3D detailDensityNoise",
            "sampledDensity(metres, 96.0, 23.0)",
        ] {
            assert!(
                shader.contains(contract),
                "procedural fog lost its authored spectral-albedo contract: {contract}"
            );
        }
    }

    #[test]
    fn combustion_profiles_feed_one_canonical_transported_contract() {
        let shader = include_str!("../../shaders/volumetrics_inject.comp");
        for contract in [
            "bool isLocalProfile(float actual, float expected)",
            "FOG_VOLUME_PROFILE_EXPLOSION",
            "layout(rgba16f, set = 0, binding = 14) uniform writeonly image3D combustionState;",
            "layout(set = 0, binding = 15) uniform sampler3D previousCombustionState;",
            "layout(rgba16f, set = 0, binding = 16) uniform writeonly image3D combustionDynamics;",
            "layout(set = 0, binding = 17) uniform sampler3D previousCombustionDynamics;",
            "layout(rgba16f, set = 0, binding = 22) uniform writeonly image3D combustionOptical;",
            "layout(set = 0, binding = 23) uniform sampler3D previousCombustionOptical;",
            "vec3 blackbodyChromaticity(float kelvin)",
            "void transportCombustion(",
            "intersectUnitSphereSegment",
            "intersectUnitBoxSegment",
            "densityProfile *= segmentCoverage;",
            "out float stateProfile",
            "reactionStateMask * sourceRadianceLuma",
            "FLAME_FUEL_BOUNDARY_HEIGHT_FRACTION",
            "FLAME_REACTION_ZONE_HEIGHT_FRACTION",
            "FLAME_REACTION_ZONE_FADE_START_FRACTION",
            "FLAME_SOURCE_LATERAL_SPEED_MPS",
            "FLAME_SOURCE_VELOCITY_RESPONSE_PER_SECOND",
            "chemistry.z = max(chemistry.z, sourceSigmaT);",
            "optical.rgb = max(optical.rgb, sourceSigmaT * sourceAlbedo);",
            "dynamics.xz = mix(dynamics.xz, lateralTarget, velocityResponse);",
            "vec3 sourcePosition = worldPos - midpointDynamics.xyz * dt;",
            "bool sourceInHistory = samplePreviousTransport(",
            "&& carriesCombustion(sourceChemistry, sourceOptical)",
            "bool incomingDynamicsFromNeighbors(",
            "float closingSpeed = dot(",
            "combustionPathBlocked(\n                candidatePosition,",
            "float combustionActivity(vec4 chemistry, vec4 optical)",
            "bool destinationDynamicsExtended = false;",
            "if (destinationDynamicsExtended",
            "vec3 neighbor_step_z = field_segment_end - field_segment_start;",
            "bool combustionPathBlocked(",
            "VISIBILITY_MASK_SOLID",
            "layout(std430, set = 0, binding = 19) readonly buffer BoundaryInstanceBuffer",
            "layout(std430, set = 0, binding = 20) readonly buffer BoundaryVertexBuffer",
            "layout(std430, set = 0, binding = 21) readonly buffer BoundaryIndexBuffer",
            "rayQueryGetIntersectionInstanceCustomIndexEXT(boundaryQuery, true)",
            "bool rigidBoundaryNormal(",
            "float inwardSpeed = min(dot(dynamics.xyz, boundaryNormal), 0.0);",
            "dynamics.xyz -= boundaryNormal * inwardSpeed;",
            "vec3 predictedPosition = worldPos + dynamics.xyz * dt;",
            "float ageSeconds = age * lifetime;",
            "EXPLOSION_IMPULSE_DURATION_SECONDS * 0.35",
            "radius / EXPLOSION_EXPANSION_TIME_SECONDS",
            "compactCore * impulseEnvelope",
            ": densityProfile * smokeEnvelope * 0.38;",
            "float fuelFraction = clamp(chemistry.x, 0.0, 1.0);",
            "float oxidizer = 1.0 - fuelFraction;",
            "COMBUSTION_REACTION_RATE_PER_SECOND",
            "float reactionRate = COMBUSTION_REACTION_RATE_PER_SECOND",
            "ADIABATIC_FLAME_TEMPERATURE_K - chemistry.y",
            "float reactionHeatFraction = 1.0 - exp(",
            "COMBUSTION_REACTION_HEAT_RESPONSE",
            "chemistry.y += temperatureDeficit * reactionHeatFraction;",
            "float sootYield = mix(",
            "COMBUSTION_RICH_SOOT_YIELD",
            "COMBUSTION_LEAN_SOOT_YIELD",
            "float hotSootOxidation = smoothstep(",
            "COMBUSTION_SOOT_OXIDATION_RATE_PER_SECOND",
            "COMBUSTION_SOOT_SINGLE_SCATTER_ALBEDO",
            "COMBUSTION_SOOT_OXIDATION_START_TEMPERATURE_K",
            "COMBUSTION_SOOT_OXIDATION_FULL_TEMPERATURE_K",
            "float sootRetention = exp(",
            "chemistry.z *= exp(-COMBUSTION_AEROSOL_DISSIPATION_PER_SECOND * dt);",
            "optical.rgb *= exp(-COMBUSTION_AEROSOL_DISSIPATION_PER_SECOND * dt);",
            "float specificOverpressure = 0.5 * expansionSpeedMps * expansionSpeedMps;",
            "dynamics.w = max(",
            "struct CombustionDifferential {",
            "bool previousCombustionDifferential(",
            "mat3 inverseBasis = inverse(basis);",
            "mat3 velocityJacobian = velocityDifferences * inverseBasis;",
            "differential.velocityDivergence = velocityJacobian[0][0]",
            "differential.activityGradient =",
            "vec3 pressureAcceleration = -differential.pressureGradient",
            "COMBUSTION_MAX_PRESSURE_ACCELERATION_MPS2",
            "COMBUSTION_MAX_DILUTION_RATE_PER_SECOND",
            "float dilutionRate = clamp(",
            "float dilution = exp(-dilutionRate * dt);",
            "COMBUSTION_OVERPRESSURE_DISSIPATION_PER_SECOND",
            "vec3 vorticityAcceleration = cross(",
            "COMBUSTION_VORTICITY_CONFINEMENT_SPEED_MPS",
            "COMBUSTION_MAX_VORTICITY_ACCELERATION_MPS2",
            "float decompressedWeight = 1.0 - smoothstep(",
            "COMBUSTION_TURBULENCE_COARSE_EDDY_SCALE_METERS",
            "COMBUSTION_TURBULENCE_DETAIL_EDDY_SCALE_METERS",
            "COMBUSTION_TURBULENCE_COARSE_RISE_SPEED_MPS",
            "COMBUSTION_TURBULENCE_DETAIL_RISE_SPEED_MPS",
            "coarseCurl * 0.72 + detailCurl * 0.42",
            "blackbodyVisibleRadianceRatio(chemistry.y, 1850.0)",
            "float buoyantAcceleration = min(temperatureExcess, 7.0)",
            "COMBUSTION_AEROSOL_LIFT_EXTINCTION_SCALE",
            "COMBUSTION_AEROSOL_LIFT_ACCELERATION_MPS2",
            "float aerosolLift = clamp(",
            "COMBUSTION_FUEL_VAPOUR_REMOVAL_PER_SECOND",
            "COMBUSTION_RADIANCE_REMOVAL_PER_SECOND",
            "COMBUSTION_THERMAL_COOLING_PER_SECOND",
            "COMBUSTION_THERMAL_BUOYANCY_ACCELERATION_MPS2",
            "COMBUSTION_VELOCITY_DAMPING_PER_SECOND",
            "COMBUSTION_REACTION_HEAT_RESPONSE",
            "LocalMedium combustionMedium(vec4 chemistry, vec4 optical)",
            "float sigmaT = chemistry.z + fuelVapourSigmaT;",
            "vec3 sigmaS = optical.rgb",
            "vec3 sigmaA = vec3(sigmaT) - sigmaS;",
            "medium.emission = sigmaA * thermalRadiance;",
            "float flameSourceSampleRatio(vec3 fieldWorldPos, vec3 sampleWorldPos)",
            "sampleDensitySum / centerDensitySum",
            "transported_combustion.emission *= flame_source_sample_ratio;",
            "imageStore(combustionState, coord, chemistry);",
            "imageStore(combustionOptical, coord, combustion_optical);",
            "layout(std430, set = 0, binding = 18) buffer CombustionLightMomentBuffer",
            "void accumulateCombustionLightMoment(",
            "medium.emission * cellVolumeWorld",
            "combustionLightMoments[binIndex].radiant_r",
        ] {
            assert!(
                shader.contains(contract),
                "transported combustion lost a coupled physical contract: {contract}"
            );
        }
        assert!(
            shader.contains("|| isTransportedProfile(profileKind))"),
            "dynamic profiles must not also render their old analytic body"
        );
        assert!(
            !shader.contains("bool blocked = carriesCombustion(probeChemistry)"),
            "solid crossing must be gated by advected source chemistry, not the destination cell"
        );
        assert!(
            !shader.contains("chemistry.y + burnedFuel * 2350.0"),
            "combustion heat must approach equilibrium rather than add a runaway fixed increment"
        );
        assert!(
            !shader.contains("radius / lifetime"),
            "smoke lifetime must not slow the explosion's radial impulse"
        );
        assert!(
            !shader.contains("cooledSmokeAlbedo")
                && !shader.contains("oxidizer fraction"),
            "transport must preserve canonical optical coefficients instead of inventing a cooled-smoke material"
        );
        assert!(
            shader.contains(
                "vec3 sourceAlbedo = clamp(volume.albedo_edge.rgb, vec3(0.0), vec3(1.0));"
            ) && !shader.contains("dot(clamp(volume.albedo_edge.rgb"),
            "transport must retain authored RGB aerosol albedo rather than collapse it to luminance"
        );
        assert!(
            !shader.contains("float sigmaS = dynamics.w")
                && !shader.contains("float sigmaT = dynamics.w"),
            "canonical dynamics must not regain an optical side channel"
        );
        let differential_solver = shader
            .split_once("bool previousCombustionDifferential(")
            .and_then(|(_, tail)| tail.split_once("bool carriesCombustion("))
            .map(|(body, _)| body)
            .expect("canonical combustion differential body");
        assert!(
            !differential_solver.contains("profileKind")
                && !differential_solver.contains("FOG_VOLUME_PROFILE_")
                && !differential_solver.contains("GameKind"),
            "local flow differentials must depend on transported state, never source identity"
        );
        assert!(
            !shader.contains("GameKind")
                && !shader.contains("Fallout")
                && !shader.contains("Skyrim"),
            "the combustion solver must consume canonical profiles, never source-game identity"
        );
    }

    #[test]
    fn transported_combustion_attenuates_directional_and_local_light_paths() {
        let shader = include_str!("../../shaders/volumetrics_inject.comp");
        for contract in [
            "const float COMBUSTION_FUEL_VAPOUR_SIGMA_T = 0.0035;",
            "const int COMBUSTION_LIGHT_TRANSMITTANCE_STEPS = 8;",
            "const float COMBUSTION_LIGHT_TRANSMITTANCE_REACH_METERS = 2.0;",
            "float transportedCombustionTransmittance(",
            "texture(previousCombustionState, previousUvw)",
            "opticalDepth += sigmaT * (distanceEnd - distanceStart);",
            "return exp(-clamp(opticalDepth, 0.0, 20.0));",
            "bool transportedMediumActive = transported_combustion.extinction > 1.0e-7;",
            "visibility *= transportedCombustionTransmittance(",
            "localVisibility *= transportedCombustionTransmittance(",
        ] {
            assert!(
                shader.contains(contract),
                "transported combustion lost light-space attenuation: {contract}"
            );
        }
        assert_eq!(
            shader.matches("0.0035").count(),
            1,
            "camera- and light-space transport must share one fuel-vapour extinction constant"
        );
        let transmittance_body = shader
            .split_once("float transportedCombustionTransmittance(")
            .and_then(|(_, tail)| tail.split_once("\n}\n\nuint quantizeCombustionMoment"))
            .map(|(body, _)| body)
            .expect("transmittance function must remain independently inspectable");
        for forbidden in ["profileKind", "FogVolume", "GameKind", "Fallout", "Skyrim"] {
            assert!(
                !transmittance_body.contains(forbidden),
                "light transmittance must consume canonical state, not {forbidden}"
            );
        }
    }

    #[test]
    fn combustion_light_moment_abi_is_eight_std430_words() {
        assert_eq!(std::mem::size_of::<GpuCombustionLightMoment>(), 32);
        assert_eq!(std::mem::align_of::<GpuCombustionLightMoment>(), 4);
        assert_eq!(COMBUSTION_LIGHT_GRID_COUNT, 256);
    }

    #[test]
    fn transported_moment_decodes_to_bin_centroid_and_physical_light() {
        let fixed_scale = COMBUSTION_LIGHT_FIXED_SCALE as u32;
        let moment = GpuCombustionLightMoment {
            weight: 16,
            weighted_x: 8,
            weighted_y: 8,
            weighted_z: 8,
            radiant_r: fixed_scale,
            radiant_g: fixed_scale / 2,
            radiant_b: fixed_scale / 4,
            luminous_volume: COMBUSTION_LIGHT_VOLUME_FIXED_SCALE as u32,
        };
        let light = combustion_light_from_moment(0, moment, [0.0; 3])
            .expect("nonzero radiant moment must produce a light");
        let expected = [-28.0, -12.0, -28.0].map(|metres| metres * WORLD_UNITS_PER_METER);
        for (actual, expected) in light.position_radius[..3].iter().zip(expected) {
            assert!((actual - expected).abs() < 1.0e-3);
        }
        assert_eq!(
            &light.color_type[..3],
            &[
                COMBUSTION_SURFACE_LIGHT_BOOST,
                COMBUSTION_SURFACE_LIGHT_BOOST * 0.5,
                COMBUSTION_SURFACE_LIGHT_BOOST * 0.25,
            ]
        );
        assert_eq!(light.params[2], VISIBILITY_MASK_FULL as f32);
        assert_eq!(light.params[3], ATTENUATION_MODEL_INVERSE_SQUARE as f32);
        assert!(
            light.params[1] > 0.0,
            "luminous volume must derive source radius"
        );
    }

    #[test]
    fn authored_companion_suppresses_only_inside_its_canonical_source() {
        let field_light = GpuLight {
            position_radius: [10.0, 20.0, 30.0, 100.0],
            params: [1.0, 8.0, VISIBILITY_MASK_FULL as f32, 1.0],
            ..GpuLight::default()
        };
        let source = GpuFogVolume {
            center_shape: [10.0, 20.0, 30.0, 0.0],
            half_extents_extinction: [20.0, 20.0, 20.0, 0.01],
            profile_params: [FOG_VOLUME_PROFILE_FLAME, 0.0, 0.0, 0.0],
            ..GpuFogVolume::default()
        };
        let nearby = GpuLight {
            position_radius: [20.0, 20.0, 30.0, 100.0],
            ..GpuLight::default()
        };
        assert!(combustion_light_is_inside_authored_source(
            &field_light,
            &[source],
            &[nearby]
        ));
        let far = GpuLight {
            position_radius: [500.0, 20.0, 30.0, 100.0],
            ..GpuLight::default()
        };
        assert!(!combustion_light_is_inside_authored_source(
            &field_light,
            &[source],
            &[far]
        ));
        let directional = GpuLight {
            color_type: [1.0, 1.0, 1.0, 2.0],
            position_radius: field_light.position_radius,
            ..GpuLight::default()
        };
        assert!(!combustion_light_is_inside_authored_source(
            &field_light,
            &[source],
            &[directional]
        ));
        let transported_light = GpuLight {
            position_radius: [200.0, 20.0, 30.0, 100.0],
            ..field_light
        };
        assert!(!combustion_light_is_inside_authored_source(
            &transported_light,
            &[source],
            &[nearby],
        ));
    }

    #[test]
    fn transport_emitter_detection_uses_canonical_profiles() {
        let volume = |profile| GpuFogVolume {
            profile_params: [profile, 0.0, 0.0, 0.0],
            ..GpuFogVolume::default()
        };
        assert!(!has_transport_emitter(&[volume(
            FOG_VOLUME_PROFILE_HOMOGENEOUS
        )]));
        for profile in [
            FOG_VOLUME_PROFILE_SMOKE,
            FOG_VOLUME_PROFILE_FLAME,
            FOG_VOLUME_PROFILE_EXPLOSION,
        ] {
            assert!(has_transport_emitter(&[volume(profile)]));
        }
        assert!(!has_transport_emitter(&[volume(f32::NAN)]));
    }

    /// Regression: #3131 (PERF-D5-01). No emitter and an expired linger
    /// window must zero the shader's `dt` and skip the combustion transport
    /// stencil; a live emitter or an unexpired linger window must not.
    #[test]
    fn combustion_transport_skips_only_once_nothing_is_burning_or_lingering() {
        // No emitter, never active (fresh/reset state) -> inactive.
        assert!(!combustion_transport_active(&[], 10.0, f32::NEG_INFINITY));
        // No emitter, but still inside the linger window -> stays active.
        assert!(combustion_transport_active(&[], 10.0, 12.0));
        // Exactly at the linger boundary -> still active (inclusive).
        assert!(combustion_transport_active(&[], 12.0, 12.0));
        // Linger window expired -> inactive again.
        assert!(!combustion_transport_active(&[], 13.0, 12.0));
        // A live transport emitter overrides an expired/absent linger window.
        let smoke = GpuFogVolume {
            profile_params: [FOG_VOLUME_PROFILE_SMOKE, 0.0, 0.0, 0.0],
            ..GpuFogVolume::default()
        };
        assert!(combustion_transport_active(
            &[smoke],
            10.0,
            f32::NEG_INFINITY
        ));
    }

    #[test]
    fn every_contributing_local_fog_light_obeys_structural_visibility() {
        let shader = include_str!("../../shaders/volumetrics_inject.comp");
        for contract in [
            "visibilityMaskNeedsTrace(lights[li].params.z)",
            "visibilityOpaqueMask(lights[li].params.z)",
            "visibilityMaskUsesGlass(lights[li].params.z)",
            "world_pos, toLightDir, 0.05, shadowDist, opaqueMask",
        ] {
            assert!(
                shader.contains(contract),
                "local volumetric light lost structural visibility contract: {contract}"
            );
        }
        assert!(
            !shader.contains("MAX_SHADOWED_FROXEL_LIGHTS"),
            "a contributing local fog light must not bypass visibility by list position"
        );
    }

    #[test]
    fn temporal_history_never_filters_across_camera_ray_columns() {
        let shader = include_str!("../../shaders/volumetrics_inject.comp");
        for contract in [
            "vec4 sampleHistoryColumn(vec3 uvw)",
            "ivec2 column = clamp(",
            "texelFetch(previousFroxel, ivec3(column, z0), 0)",
            "vec4 history = sampleHistoryColumn(previousUvw);",
        ] {
            assert!(
                shader.contains(contract),
                "volumetric history lost its wall-safe column contract: {contract}"
            );
        }
        assert!(
            !shader.contains("texture(previousFroxel"),
            "trilinear history sampling can blend light through opaque XY boundaries"
        );
    }

    #[test]
    fn emissive_history_uses_previous_provenance_on_trailing_edges() {
        let shader = include_str!("../../shaders/volumetrics_inject.comp");
        for contract in [
            "layout(r32f, set = 0, binding = 12) uniform writeonly image3D emissionHistory;",
            "layout(set = 0, binding = 13) uniform sampler3D previousEmissionHistory;",
            "imageStore(emissionHistory, coord, vec4(currentEmissionFraction",
            "float previousEmissionFraction = clamp(",
            "float temporalEmissionFraction = max(",
            "currentEmissionFraction,",
            "previousEmissionFraction",
            "mix(steadyWeight, emissiveWeight, temporalEmissionFraction)",
        ] {
            assert!(
                shader.contains(contract),
                "emissive history lost its symmetric leading/trailing-edge contract: {contract}"
            );
        }
        assert!(
            shader.contains("texelFetch(previousEmissionHistory, ivec3(column, z0), 0).r"),
            "emission provenance must use the same wall-safe column reprojection as radiance"
        );
    }

    #[test]
    fn exponential_slab_transport_saturates_dense_smoke() {
        fn source_integral(extinction: f32, width: f32) -> f32 {
            let optical_depth = extinction * width;
            if optical_depth < 1.0e-3 {
                width * (1.0 - 0.5 * optical_depth)
            } else {
                (1.0 - (-optical_depth).exp()) / extinction
            }
        }

        // For q = sigma_t * L_eq, an optically thick homogeneous slab must
        // approach L_eq rather than the rectangle rule's unbounded q*dt.
        let extinction = 100.0;
        let width = 1.0;
        let equilibrium_radiance = 3.5;
        let source = extinction * equilibrium_radiance;
        let exact = source * source_integral(extinction, width);
        assert!((exact - equilibrium_radiance).abs() < 1.0e-5);
        assert!(source * width > exact * 50.0);

        // The series branch preserves the vacuum/thin-medium limit.
        let thin = source_integral(1.0e-7, 2.0);
        assert!((thin - 2.0).abs() < 1.0e-6);

        let shader = include_str!("../../shaders/volumetrics_integrate.comp");
        for contract in [
            "float slabTransmittance = exp(-opticalDepth);",
            "float sourceIntegral = opticalDepth < 1.0e-3",
            ": (1.0 - slabTransmittance) / sigmaT;",
            "inscatter_total += inscatter * trans_cumulative * sourceIntegral;",
        ] {
            assert!(
                shader.contains(contract),
                "integration shader lost exact homogeneous-slab transport: {contract}"
            );
        }
        assert!(
            !shader.contains("inscatter * trans_cumulative * dt"),
            "rectangle-rule source integration would over-brighten dense smoke"
        );
    }

    #[test]
    fn horizontal_height_fog_ray_is_finite_and_matches_constant_density() {
        let tau = height_fog_optical_depth(0.01, 140.0, 0.0, 1_000.0, 0.0, 2_100.0);
        let expected = 0.01 * (-140.0_f32 / 2_100.0).exp() * 1_000.0;
        assert!(tau.is_finite());
        assert!((tau - expected).abs() < 1.0e-5);
    }

    #[test]
    fn steep_downward_height_fog_ray_stays_finite() {
        let tau = height_fog_optical_depth(0.01, 10_000.0, -1.0, 1.0e8, 0.0, 2_100.0);
        assert!(tau.is_finite());
        assert!(tau >= 0.0);
    }

    #[test]
    fn hybrid_slice_mapping_round_trips_and_keeps_first_five_metres_linear() {
        let far = DEFAULT_VOLUME_FAR;
        for u in [0.0, 0.03, LINEAR_SLICE_FRACTION, 0.25, 0.5, 1.0] {
            let distance = hybrid_slice_distance(u, far, LINEAR_DEPTH, LINEAR_SLICE_FRACTION);
            let reconstructed =
                hybrid_slice_coordinate(distance, far, LINEAR_DEPTH, LINEAR_SLICE_FRACTION);
            assert!((reconstructed - u).abs() < 1.0e-5);
        }
        assert_eq!(
            hybrid_slice_distance(
                LINEAR_SLICE_FRACTION,
                far,
                LINEAR_DEPTH,
                LINEAR_SLICE_FRACTION,
            ),
            LINEAR_DEPTH
        );
    }

    #[test]
    fn froxel_extent_uses_render_resolution_and_configured_divisor() {
        // Derive from the config rather than snapshotting the default: this
        // test is named for the *relationship*, and hardcoding `[320, 180, 64]`
        // made it a tripwire on the default's value instead (it broke when the
        // divisor moved 4 -> 8, which is a retune, not a regression).
        let config = VolumetricsConfig::default();
        let render = vk::Extent2D {
            width: 1280,
            height: 720,
        };
        let extent = froxel_extent(render, config);
        assert_eq!(
            [extent.width, extent.height, extent.depth],
            [
                render.width.div_ceil(config.froxel_xy_divisor),
                render.height.div_ceil(config.froxel_xy_divisor),
                config.froxel_z_slices,
            ]
        );
        // And pin an explicit divisor so the relationship itself is covered
        // independently of whatever the default happens to be.
        let explicit = froxel_extent(
            render,
            VolumetricsConfig {
                froxel_xy_divisor: 4,
                ..config
            },
        );
        assert_eq!([explicit.width, explicit.height], [320, 180]);
    }

    #[test]
    fn gpu_fog_volume_layout_matches_std430_shader_contract() {
        assert_eq!(std::mem::size_of::<GpuFogVolume>(), 96);
        assert_eq!(std::mem::align_of::<GpuFogVolume>(), 16);
        assert_eq!(
            std::mem::size_of::<GpuFogVolumeUpload>(),
            16 + MAX_GPU_FOG_VOLUMES * 96
        );
        assert_eq!(std::mem::size_of::<GpuFogClusterEntry>(), 8);
    }

    /// #2228 / REN-D3-01 — the size/align pin above cannot catch a within-
    /// struct field reorder: every `GpuFogVolume` field is an identically
    /// sized 16 B `vec4`, so any permutation still sums to 96 B / align 16
    /// and passes that test while silently swapping what each field means
    /// on the GPU (e.g. `inverse_rotation` read as `albedo_edge`). Pin each
    /// field's exact byte offset here — the source-of-truth order the GLSL
    /// struct below is checked against.
    #[test]
    fn gpu_fog_volume_field_offsets_match_declared_order() {
        use std::mem::offset_of;
        assert_eq!(offset_of!(GpuFogVolume, center_shape), 0);
        assert_eq!(offset_of!(GpuFogVolume, half_extents_extinction), 16);
        assert_eq!(offset_of!(GpuFogVolume, inverse_rotation), 32);
        assert_eq!(offset_of!(GpuFogVolume, albedo_edge), 48);
        assert_eq!(offset_of!(GpuFogVolume, emission_temperature), 64);
        assert_eq!(offset_of!(GpuFogVolume, profile_params), 80);
    }

    /// #2228 / REN-D3-01 — cross-checks `volumetrics_inject.comp`'s
    /// `struct GpuFogVolume` declaration order against the Rust field order
    /// pinned above (the offset source of truth). A GLSL-only reorder is
    /// invisible to every Rust-side test — including the one above — since
    /// nothing on the Rust side reads the shader source. This closes that
    /// gap the same way `gpu_material_glsl_field_order_matches_rust_struct`
    /// does for `GpuMaterial` (scene_buffer/gpu_instance_layout_tests.rs).
    #[test]
    fn gpu_fog_volume_glsl_field_order_matches_rust_struct() {
        const RUST_FIELD_ORDER: [&str; 6] = [
            "center_shape",
            "half_extents_extinction",
            "inverse_rotation",
            "albedo_edge",
            "emission_temperature",
            "profile_params",
        ];

        let glsl_src = include_str!("../../shaders/volumetrics_inject.comp");
        let struct_start = glsl_src
            .find("struct GpuFogVolume {")
            .expect("volumetrics_inject.comp must declare `struct GpuFogVolume`");
        let body_end = glsl_src[struct_start..]
            .find("};")
            .expect("GpuFogVolume struct body must be closed with `};`");
        let body = &glsl_src[struct_start..struct_start + body_end];

        let glsl_fields: Vec<&str> = body
            .lines()
            .skip(1) // the `struct GpuFogVolume {` line itself
            .filter_map(|line| {
                let line = line.split("//").next().unwrap_or("").trim();
                if line.is_empty() {
                    return None;
                }
                // "vec4 center_shape;" -> "center_shape"
                line.trim_end_matches(';').split_whitespace().nth(1)
            })
            .collect();

        assert_eq!(
            glsl_fields, RUST_FIELD_ORDER,
            "volumetrics_inject.comp's `struct GpuFogVolume` field order must match the \
             Rust struct's offset order {RUST_FIELD_ORDER:?} — a reorder on either side \
             compiles clean and passes the size/align pin while silently corrupting every \
             fog-volume sample on the GPU (#2228 / REN-D3-01)",
        );
    }

    #[test]
    fn local_volume_cluster_grid_references_center_primitive() {
        let volume = GpuFogVolume {
            center_shape: [0.0, 0.0, 0.0, 1.0],
            half_extents_extinction: [10.0, 20.0, 10.0, 0.01],
            inverse_rotation: [0.0, 0.0, 0.0, 1.0],
            albedo_edge: [0.9, 0.9, 0.9, 0.4],
            emission_temperature: [0.0; 4],
            profile_params: [FOG_VOLUME_PROFILE_SMOKE, 0.0, 0.0, 0.0],
        };
        let mut upload = GpuFogVolumeUpload::default();
        let mut entries = fog_cluster_entries_with_offsets();
        let mut indices = Box::new([0; FOG_VOLUME_INDEX_COUNT]);
        let grid = build_fog_volume_clusters(
            &[volume],
            [0.0; 3],
            160.0,
            &mut upload,
            &mut entries,
            &mut indices,
        );

        assert_eq!(upload.count[0], 1);
        assert_eq!(grid, [-160.0, -160.0, -160.0, 0.05]);
        let center = FOG_VOLUME_CLUSTER_DIM / 2;
        let center_cluster = center
            + center * FOG_VOLUME_CLUSTER_DIM
            + center * FOG_VOLUME_CLUSTER_DIM * FOG_VOLUME_CLUSTER_DIM;
        assert_eq!(entries[center_cluster].count, 1);
        assert_eq!(indices[entries[center_cluster].offset as usize], 0);
    }

    #[test]
    fn volume_outside_cluster_cube_produces_no_references() {
        let volume = GpuFogVolume {
            center_shape: [10_000.0, 0.0, 0.0, 1.0],
            half_extents_extinction: [1.0, 1.0, 1.0, 0.01],
            inverse_rotation: [0.0, 0.0, 0.0, 1.0],
            albedo_edge: [0.9, 0.9, 0.9, 0.4],
            emission_temperature: [0.0; 4],
            profile_params: [FOG_VOLUME_PROFILE_HOMOGENEOUS, 0.0, 0.0, 0.0],
        };
        let mut upload = GpuFogVolumeUpload::default();
        let mut entries = fog_cluster_entries_with_offsets();
        let mut indices = Box::new([0; FOG_VOLUME_INDEX_COUNT]);
        build_fog_volume_clusters(
            &[volume],
            [0.0; 3],
            160.0,
            &mut upload,
            &mut entries,
            &mut indices,
        );
        assert!(entries.iter().all(|entry| entry.count == 0));
    }

    /// Regression: #3133 (PERF-D4-01). `build_fog_volume_clusters` used to
    /// recompute every entry's `offset` from scratch each call; now it is
    /// seeded once by `fog_cluster_entries_with_offsets` and never rewritten
    /// by the per-frame rebuild. Pins that invariant across repeated calls
    /// (simulating N frames without re-initialising the caller's buffer),
    /// and that a cluster whose volume moved out of it — the one case the
    /// removed `entries.fill(default())` used to guarantee stays
    /// correct — reports `count == 0` and is therefore never read (per
    /// `localFogCluster`'s `fogVolumeCount`/`count` early-out).
    #[test]
    fn fog_cluster_offsets_persist_and_stale_counts_clear_across_frames() {
        let volume = GpuFogVolume {
            center_shape: [0.0, 0.0, 0.0, 1.0],
            half_extents_extinction: [10.0, 20.0, 10.0, 0.01],
            inverse_rotation: [0.0, 0.0, 0.0, 1.0],
            albedo_edge: [0.9, 0.9, 0.9, 0.4],
            emission_temperature: [0.0; 4],
            profile_params: [FOG_VOLUME_PROFILE_SMOKE, 0.0, 0.0, 0.0],
        };
        let mut upload = GpuFogVolumeUpload::default();
        let mut entries = fog_cluster_entries_with_offsets();
        let mut indices = Box::new([0; FOG_VOLUME_INDEX_COUNT]);
        let center = FOG_VOLUME_CLUSTER_DIM / 2;
        let center_cluster = center
            + center * FOG_VOLUME_CLUSTER_DIM
            + center * FOG_VOLUME_CLUSTER_DIM * FOG_VOLUME_CLUSTER_DIM;

        // Frame 1: a volume sits at the grid center.
        build_fog_volume_clusters(
            &[volume],
            [0.0; 3],
            160.0,
            &mut upload,
            &mut entries,
            &mut indices,
        );
        assert_eq!(entries[center_cluster].count, 1);
        let expected_offset = (center_cluster * MAX_FOG_VOLUMES_PER_CLUSTER) as u32;
        assert_eq!(entries[center_cluster].offset, expected_offset);

        // Frame 2: no volumes at all (e.g. the emitter despawned). The
        // buffer is reused as-is, exactly like the real per-frame call —
        // `offset` must still be correct and `count` must have cleared.
        build_fog_volume_clusters(
            &[],
            [0.0; 3],
            160.0,
            &mut upload,
            &mut entries,
            &mut indices,
        );
        assert_eq!(
            entries[center_cluster].count, 0,
            "a cluster's volume list must clear once its volume is gone, even though \
             offset is no longer re-derived every frame"
        );
        assert_eq!(
            entries[center_cluster].offset, expected_offset,
            "offset must remain the permanent per-slot invariant across frames"
        );

        // Every entry's offset must still be correct, not just the center's.
        for (cluster_index, entry) in entries.iter().enumerate() {
            assert_eq!(
                entry.offset,
                (cluster_index * MAX_FOG_VOLUMES_PER_CLUSTER) as u32
            );
        }
    }

    /// #3117 — `docs/engine/memory-budget.md` is the authoritative VRAM source
    /// (`/audit-performance` Dimension 3 forbids re-deriving ceilings from
    /// anywhere else), and it sat at the pre-Session-62 two-volume figure while
    /// the froxel grid grew along two axes: `froxel_xy_divisor` 8 → 4 and the
    /// per-slot volume set 2 → 6. The ledger row understated the grid ~24x, and
    /// the doc's own detail section disagreed with its ledger by a further ~9x.
    ///
    /// The code already knew the right number — the boot log prints it. This
    /// pins the doc to the same constants so they cannot drift apart silently.
    #[test]
    fn froxel_grid_cost_matches_the_memory_budget_doc() {
        const DOC: &str = include_str!("../../../../docs/engine/memory-budget.md");

        // The six volume vectors this cost is derived from must still exist.
        const SRC: &str = include_str!("volumetrics.rs");
        let volume_fields = [
            "lighting_volumes: Vec<FroxelSlot>",
            "integrated_volumes: Vec<FroxelSlot>",
            "emission_history_volumes: Vec<FroxelSlot>",
            "combustion_state_volumes: Vec<FroxelSlot>",
            "combustion_dynamics_volumes: Vec<FroxelSlot>",
            "combustion_optical_volumes: Vec<FroxelSlot>",
        ];
        assert_eq!(
            volume_fields.len(),
            FROXEL_VOLUMES_PER_SLOT,
            "FROXEL_VOLUMES_PER_SLOT disagrees with the list this test checks"
        );
        for field in volume_fields {
            assert!(
                SRC.contains(field),
                "`{field}` is gone — the per-slot volume set changed, so \
                 FROXEL_BYTES_PER_SLOT ({FROXEL_BYTES_PER_SLOT}) and the \
                 memory-budget ledger both need re-deriving (#3117)"
            );
        }
        // 5 x RGBA16F (8 B) + 1 x R32F (4 B).
        assert_eq!(FROXEL_BYTES_PER_SLOT, 5 * 8 + 4);

        // The doc must state the same per-slot cost and volume count.
        assert!(
            DOC.contains(&format!("{FROXEL_BYTES_PER_SLOT} B/froxel/slot")),
            "memory-budget.md does not state `{FROXEL_BYTES_PER_SLOT} B/froxel/slot` \
             — the froxel cost changed and the ledger was not updated (#3117)"
        );
        assert!(
            DOC.contains(&format!("{FROXEL_VOLUMES_PER_SLOT} volumes")),
            "memory-budget.md does not state `{FROXEL_VOLUMES_PER_SLOT} volumes` (#3117)"
        );
        // The exact stale strings this issue was filed for must not come back.
        assert!(
            !DOC.contains("(2 volumes, 2 FIF)"),
            "memory-budget.md's ledger row is back to the pre-Session-62 \
             two-volume figure (#3117)"
        );

        // And the headline totals must match the formula, so a divisor change
        // cannot leave the table behind. Decimal MB, as the doc uses.
        // Read the divisor from the config rather than hardcoding it, so a
        // future retune fails on the DOC assertions below — which is the point
        // — instead of on stale arithmetic here.
        let config = crate::vulkan::upscaling::VolumetricsConfig::default();
        let total_mb = |w: u64, h: u64| -> f64 {
            let d = config.froxel_xy_divisor as u64;
            let (fx, fy) = (w.div_ceil(d), h.div_ceil(d));
            (fx * fy
                * config.froxel_z_slices as u64
                * FROXEL_BYTES_PER_SLOT
                * MAX_FRAMES_IN_FLIGHT as u64) as f64
                / 1.0e6
        };
        assert!(
            (total_mb(1920, 1080) - 182.5).abs() < 1.0,
            "1080p native: {} MB",
            total_mb(1920, 1080)
        );
        assert!(
            (total_mb(3840, 2160) - 729.9).abs() < 1.0,
            "4K native: {} MB",
            total_mb(3840, 2160)
        );
        assert!(
            DOC.contains("~183 MB") && DOC.contains("~730 MB"),
            "memory-budget.md's volumetrics totals no longer match the formula \
             at the shipped froxel_xy_divisor of {} (#3117)",
            config.froxel_xy_divisor
        );
    }
}

// Shader workgroup drift tests moved to shader_constants::tests after #1038
// folded all shared constants into the build.rs codegen path. The canonical
// checks are now:
//   shader_constants::tests::affected_shaders_include_constants_header
//   shader_constants::tests::generated_header_contains_all_defines
