//! Application-specific marker components and resources.

use byroredux_audio::Sound;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Component, Resource, SparseSetStorage};
use byroredux_core::math::Vec3;
use byroredux_core::string::FixedString;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use winit::keyboard::KeyCode;

/// Marker component for entities that should spin in the demo scene.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Spinning;
impl Component for Spinning {
    type Storage = SparseSetStorage<Self>;
}

/// Component for entities that use alpha blending, carrying the Gamebryo
/// blend factors extracted from NiAlphaProperty flags.
///
/// Gamebryo AlphaFunction enum (bits 1–4 = src, bits 5–8 = dst):
///   0=ONE, 1=ZERO, 2=SRC_COLOR, 3=INV_SRC_COLOR, 4=DEST_COLOR,
///   5=INV_DEST_COLOR, 6=SRC_ALPHA, 7=INV_SRC_ALPHA, 8=DEST_ALPHA,
///   9=INV_DEST_ALPHA, 10=SRC_ALPHA_SATURATE.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AlphaBlend {
    pub(crate) src_blend: u8,
    pub(crate) dst_blend: u8,
}
impl Component for AlphaBlend {
    type Storage = SparseSetStorage<Self>;
}

/// Marker component for entities that need two-sided rendering (no backface culling).
#[derive(Debug, Clone, Copy)]
pub(crate) struct TwoSided;
impl Component for TwoSided {
    type Storage = SparseSetStorage<Self>;
}

// `Decal` marker retired in #renderlayer — its semantic ("renders on
// top of coplanar surfaces via depth bias") is now expressed as
// `RenderLayer::Decal` from `byroredux_core::ecs::components::RenderLayer`.
// The render-side `is_decal: bool` on `DrawCommand` (consumed by
// shader / GpuInstance flag paths) is now derived from
// `render_layer == RenderLayer::Decal` at DrawCommand construction.

/// Bindless texture handle for a normal map (parallels TextureHandle for diffuse).
#[derive(Debug, Clone, Copy)]
pub(crate) struct NormalMapHandle(pub(crate) u32);
impl Component for NormalMapHandle {
    type Storage = SparseSetStorage<Self>;
}

/// Bindless texture handle for a dark/lightmap (NiTexturingProperty slot 1).
/// Multiplicative modulation: `albedo.rgb *= dark_sample.rgb`. See #264.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DarkMapHandle(pub(crate) u32);
impl Component for DarkMapHandle {
    type Storage = SparseSetStorage<Self>;
}

/// Bindless texture indices for the three NiTexturingProperty slots that
/// previously populated `Material` but never reached `GpuInstance`:
/// glow (slot 4 — emissive overlay), detail (slot 2 — high-frequency
/// 2× UV overlay), and gloss (slot 3 — per-texel specular mask). All
/// three default to `0` (= no map; shader falls through to the inline
/// material constants). Combined into a single component to keep the
/// per-frame query count fixed regardless of which slots a mesh uses.
/// See #399 (OBL-D4-H3).
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExtraTextureMaps {
    pub(crate) glow: u32,
    pub(crate) detail: u32,
    pub(crate) gloss: u32,
    /// Parallax / height map (`BSShaderTextureSet` slot 3). 0 = no POM.
    /// See #453 (renderer-side plumbing) and #452 (import-side path).
    pub(crate) parallax: u32,
    /// Env reflection map (slot 4). 0 = no env map. See #453.
    pub(crate) env: u32,
    /// Env reflection mask (slot 5). 0 = unmasked. See #453.
    pub(crate) env_mask: u32,
    /// POM height scale (default 0.04). See #453.
    pub(crate) parallax_height_scale: f32,
    /// POM ray-march sample budget (default 4.0). See #453.
    pub(crate) parallax_max_passes: f32,
}
impl Component for ExtraTextureMaps {
    type Storage = SparseSetStorage<Self>;
}

/// Terrain splat-layer tile index into the renderer's
/// `GpuTerrainTile` SSBO (scene set 1, binding 10). Attached only to
/// LAND terrain entities when ATXT/VTXT splat layers are present.
/// `render.rs` forwards this into `DrawCommand::terrain_tile_index`,
/// which `draw.rs` packs into the top 16 bits of `GpuInstance.flags`
/// alongside the `INSTANCE_FLAG_TERRAIN_SPLAT` bit. See #470.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TerrainTileSlot(pub(crate) u32);
impl Component for TerrainTileSlot {
    type Storage = SparseSetStorage<Self>;
}

// SystemList moved to byroredux_core::ecs::resources::SystemList

/// Cell lighting from the ESM (ambient + directional + fog).
///
/// The base block (`ambient`/`directional_*`/`fog_*`) is populated for
/// every cell — interior XCLL on `load_cell_with_masters`, exterior
/// WTHR on `apply_worldspace_weather`, or engine-default constants
/// when no plugin data is available.
///
/// The optional Skyrim/FNV extended block (`directional_fade` ..
/// `fresnel_power`) is populated only when the source cell carries an
/// XCLL with the matching tail length. Pre-#861 every consumer dropped
/// these on the floor at the renderer-facing boundary even though the
/// plugin parser had been extracting them since #379 (FNV) / #367
/// (Skyrim). Now flowed through end-to-end on the CPU side; shader
/// consumption follows in #865 (fog curve) and a future Skyrim
/// ambient-cube uniform. Per-field `#[allow(dead_code)]` is the
/// staged-rollout marker — removed in lockstep with each shader-side
/// consumer landing, so an unused-after-shader-lands field surfaces
/// as a warning instead of silently growing dead.
pub(crate) struct CellLightingRes {
    pub(crate) ambient: [f32; 3],
    pub(crate) directional_color: [f32; 3],
    /// Direction vector in Y-up space (computed from rotation).
    pub(crate) directional_dir: [f32; 3],
    /// True when the cell is interior. Interior XCLL directional is a
    /// subtle tint, not a physical sun — we skip it as a scene light to
    /// avoid leak artifacts on walls that shouldn't see the sky.
    pub(crate) is_interior: bool,
    /// Fog color (RGB 0-1).
    pub(crate) fog_color: [f32; 3],
    /// Fog near distance (game units).
    pub(crate) fog_near: f32,
    /// Fog far distance (game units).
    pub(crate) fog_far: f32,
    // ── Extended XCLL fields (FNV 40-byte tail + Skyrim 92-byte tail) ──
    // Each `#[allow(dead_code)]` is removed in lockstep with the
    // matching shader-side consumer landing (see #865 for fog curve;
    // ambient-cube + light-fade + specular follow as their own issues).
    /// Directional light fade multiplier — bytes 28-31 of the XCLL.
    /// FNV+ XCLL.
    #[allow(dead_code)]
    pub(crate) directional_fade: Option<f32>,
    /// Cubic-fog clip distance — bytes 32-35. FNV+ XCLL. Used together
    /// with `fog_power` to drive a non-linear fog curve in place of
    /// the linear `fog_near..fog_far` ramp. See #865.
    #[allow(dead_code)]
    pub(crate) fog_clip: Option<f32>,
    /// Cubic-fog falloff exponent — bytes 36-39. FNV+ XCLL. See #865.
    #[allow(dead_code)]
    pub(crate) fog_power: Option<f32>,
    /// Fog far color (RGB 0-1) — bytes 72-74. Skyrim+ XCLL. Distinct
    /// from `fog_color` (which is the near-distance fog tint).
    #[allow(dead_code)]
    pub(crate) fog_far_color: Option<[f32; 3]>,
    /// Maximum fog opacity — bytes 76-79. Skyrim+ XCLL.
    #[allow(dead_code)]
    pub(crate) fog_max: Option<f32>,
    /// Light fade begin distance — bytes 80-83. Skyrim+ XCLL.
    #[allow(dead_code)]
    pub(crate) light_fade_begin: Option<f32>,
    /// Light fade end distance — bytes 84-87. Skyrim+ XCLL.
    #[allow(dead_code)]
    pub(crate) light_fade_end: Option<f32>,
    /// Directional ambient cube — `[+X, -X, +Y, -Y, +Z, -Z]` RGB
    /// triplets from bytes 40-63. Drives the per-cell ambient probe;
    /// ±Z asymmetry is what makes Skyrim cave floors read warm while
    /// ceilings read cool without a dedicated IBL pass. Skyrim+ XCLL.
    #[allow(dead_code)]
    pub(crate) directional_ambient: Option<[[f32; 3]; 6]>,
    /// Specular tint (RGB) — bytes 64-66. Skyrim+ XCLL.
    #[allow(dead_code)]
    pub(crate) specular_color: Option<[f32; 3]>,
    /// Specular alpha — byte 67. Stored separately so consumers can
    /// decide whether the RGBA packing was intentional or padding.
    #[allow(dead_code)]
    pub(crate) specular_alpha: Option<f32>,
    /// Fresnel power exponent — bytes 68-71. Skyrim+ XCLL.
    #[allow(dead_code)]
    pub(crate) fresnel_power: Option<f32>,
}

impl CellLightingRes {
    /// Construct a `CellLightingRes` from a fully-resolved
    /// [`byroredux_plugin::esm::cell::CellLighting`] (the plugin
    /// layer's parser output) plus the renderer-facing direction
    /// vector and the interior/exterior flag. Carries the 9 extended
    /// XCLL fields through verbatim — see #861. Producers in
    /// `scene.rs` use this helper for the interior `--esm --cell`
    /// arm; exterior weather (`apply_worldspace_weather`) and the
    /// engine-default constant path build the struct directly with
    /// `extended_*` set to `None`.
    pub(crate) fn from_cell_lighting(
        lit: &byroredux_plugin::esm::cell::CellLighting,
        directional_dir: [f32; 3],
        is_interior: bool,
    ) -> Self {
        Self {
            ambient: lit.ambient,
            directional_color: lit.directional_color,
            directional_dir,
            is_interior,
            fog_color: lit.fog_color,
            fog_near: lit.fog_near,
            fog_far: lit.fog_far,
            directional_fade: lit.directional_fade,
            fog_clip: lit.fog_clip,
            fog_power: lit.fog_power,
            fog_far_color: lit.fog_far_color,
            fog_max: lit.fog_max,
            light_fade_begin: lit.light_fade_begin,
            light_fade_end: lit.light_fade_end,
            directional_ambient: lit.directional_ambient,
            specular_color: lit.specular_color,
            specular_alpha: lit.specular_alpha,
            fresnel_power: lit.fresnel_power,
        }
    }
}

impl Resource for CellLightingRes {}

#[cfg(test)]
mod cell_lighting_res_tests {
    //! Regression for #861 — extended XCLL fields propagate through the
    //! `CellLighting → CellLightingRes` boundary (FNV 40-byte tail and
    //! Skyrim 92-byte tail). Pre-fix every Optional past the 3 base
    //! fog fields was dropped on the floor at the renderer-facing
    //! resource layer.
    use super::*;
    use byroredux_plugin::esm::cell::CellLighting;

    fn fnv_xcll_with_fog_curve() -> CellLighting {
        // Mirrors the FNV 40-byte fixture in
        // crates/plugin/src/esm/cell/tests.rs:1683-1740 — directional_fade,
        // fog_clip, fog_power populated; Skyrim-only fields stay None.
        CellLighting {
            ambient: [0.10, 0.10, 0.12],
            directional_color: [1.0, 0.95, 0.80],
            directional_rotation: [0.0, 0.0],
            fog_color: [0.50, 0.45, 0.30],
            fog_near: 100.0,
            fog_far: 8000.0,
            directional_fade: Some(0.80),
            fog_clip: Some(7500.0),
            fog_power: Some(2.0),
            fog_far_color: None,
            fog_max: None,
            light_fade_begin: None,
            light_fade_end: None,
            directional_ambient: None,
            specular_color: None,
            specular_alpha: None,
            fresnel_power: None,
        }
    }

    fn skyrim_xcll_with_full_extension() -> CellLighting {
        CellLighting {
            ambient: [0.05, 0.05, 0.06],
            directional_color: [0.8, 0.85, 1.0],
            directional_rotation: [0.5, 0.3],
            fog_color: [0.20, 0.25, 0.35],
            fog_near: 50.0,
            fog_far: 5000.0,
            directional_fade: Some(0.65),
            fog_clip: Some(4500.0),
            fog_power: Some(1.5),
            fog_far_color: Some([0.10, 0.10, 0.15]),
            fog_max: Some(0.85),
            light_fade_begin: Some(800.0),
            light_fade_end: Some(2400.0),
            directional_ambient: Some([
                [0.20, 0.18, 0.15], // +X
                [0.18, 0.16, 0.13], // -X
                [0.10, 0.10, 0.10], // +Y
                [0.05, 0.05, 0.05], // -Y
                [0.25, 0.25, 0.30], // +Z (warmer ceiling)
                [0.15, 0.13, 0.10], // -Z (cooler floor)
            ]),
            specular_color: Some([0.30, 0.30, 0.32]),
            specular_alpha: Some(1.0),
            fresnel_power: Some(2.5),
        }
    }

    #[test]
    fn from_cell_lighting_propagates_fnv_fog_curve_fields() {
        let lit = fnv_xcll_with_fog_curve();
        let res = CellLightingRes::from_cell_lighting(&lit, [0.0, 1.0, 0.0], true);

        // Base block.
        assert_eq!(res.ambient, [0.10, 0.10, 0.12]);
        assert_eq!(res.directional_color, [1.0, 0.95, 0.80]);
        assert_eq!(res.directional_dir, [0.0, 1.0, 0.0]);
        assert!(res.is_interior);
        assert_eq!(res.fog_color, [0.50, 0.45, 0.30]);
        assert_eq!(res.fog_near, 100.0);
        assert_eq!(res.fog_far, 8000.0);

        // FNV 40-byte tail — must be Some(...) and round-trip exactly.
        assert_eq!(res.directional_fade, Some(0.80));
        assert_eq!(res.fog_clip, Some(7500.0));
        assert_eq!(res.fog_power, Some(2.0));

        // Skyrim 92-byte tail — must stay None on a 40-byte FNV XCLL.
        assert!(res.fog_far_color.is_none());
        assert!(res.fog_max.is_none());
        assert!(res.light_fade_begin.is_none());
        assert!(res.light_fade_end.is_none());
        assert!(res.directional_ambient.is_none());
        assert!(res.specular_color.is_none());
        assert!(res.specular_alpha.is_none());
        assert!(res.fresnel_power.is_none());
    }

    #[test]
    fn from_cell_lighting_propagates_skyrim_92byte_extension() {
        let lit = skyrim_xcll_with_full_extension();
        let res = CellLightingRes::from_cell_lighting(&lit, [0.1, 0.9, -0.3], true);

        // FNV 40-byte tail.
        assert_eq!(res.directional_fade, Some(0.65));
        assert_eq!(res.fog_clip, Some(4500.0));
        assert_eq!(res.fog_power, Some(1.5));

        // Skyrim 92-byte tail.
        assert_eq!(res.fog_far_color, Some([0.10, 0.10, 0.15]));
        assert_eq!(res.fog_max, Some(0.85));
        assert_eq!(res.light_fade_begin, Some(800.0));
        assert_eq!(res.light_fade_end, Some(2400.0));
        let cube = res.directional_ambient.expect("Skyrim XCLL ambient cube");
        assert_eq!(cube[4], [0.25, 0.25, 0.30], "+Z (ceiling) face");
        assert_eq!(cube[5], [0.15, 0.13, 0.10], "-Z (floor) face");
        assert_eq!(res.specular_color, Some([0.30, 0.30, 0.32]));
        assert_eq!(res.specular_alpha, Some(1.0));
        assert_eq!(res.fresnel_power, Some(2.5));
    }

    #[test]
    fn from_cell_lighting_handles_pre_skyrim_xcll_with_no_extension() {
        // Oblivion-shape XCLL (28-byte head, no tail at all) — every
        // optional must stay None even though the parser emits the
        // CellLighting struct with the same field set.
        let lit = CellLighting {
            ambient: [0.30, 0.30, 0.30],
            directional_color: [0.90, 0.90, 0.85],
            directional_rotation: [0.0, 0.0],
            fog_color: [0.40, 0.40, 0.50],
            fog_near: 200.0,
            fog_far: 4000.0,
            directional_fade: None,
            fog_clip: None,
            fog_power: None,
            fog_far_color: None,
            fog_max: None,
            light_fade_begin: None,
            light_fade_end: None,
            directional_ambient: None,
            specular_color: None,
            specular_alpha: None,
            fresnel_power: None,
        };
        let res = CellLightingRes::from_cell_lighting(&lit, [0.0, 1.0, 0.0], true);
        assert!(res.directional_fade.is_none());
        assert!(res.fog_clip.is_none());
        assert!(res.fog_power.is_none());
        assert!(res.directional_ambient.is_none());
    }
}

/// Sky rendering parameters from WTHR records (exterior cells).
/// Stored as an ECS resource so the render loop can read it per-frame.
pub(crate) struct SkyParamsRes {
    pub(crate) zenith_color: [f32; 3],
    pub(crate) horizon_color: [f32; 3],
    /// Below-horizon ground / lower-hemisphere tint from WTHR's
    /// `SKY_LOWER` group (real Sky-Lower at NAM0 slot 7 per nif.xml,
    /// post-#729). Per-frame `weather_system` interpolates the
    /// authored TOD slots; the renderer's `compute_sky` branches on
    /// negative elevation and uses this colour instead of the pre-#541
    /// `horizon * 0.3` fake.
    pub(crate) lower_color: [f32; 3],
    pub(crate) sun_direction: [f32; 3],
    pub(crate) sun_color: [f32; 3],
    pub(crate) sun_size: f32,
    pub(crate) sun_intensity: f32,
    pub(crate) is_exterior: bool,
    /// Cloud layer 0 UV tile scale. `0.0` disables clouds (shader skips the sample).
    pub(crate) cloud_tile_scale: f32,
    /// Bindless texture handle for cloud_textures[0]. Only meaningful when
    /// `cloud_tile_scale > 0.0`.
    pub(crate) cloud_texture_index: u32,
    /// Bindless texture handle for the CLMT FNAM sun sprite. `0` = use
    /// the composite shader's procedural sun disc (pre-#478 behaviour).
    /// Populated at cell load when the worldspace has a CLMT with a
    /// resolvable FNAM path. See #478.
    pub(crate) sun_texture_index: u32,
    /// Cloud layer 1 UV tile scale. `0.0` disables the layer (shader
    /// branch-skips the sample). Set to `0.0` when no CNAM texture
    /// is available for the current weather.
    pub(crate) cloud_tile_scale_1: f32,
    /// Bindless texture handle for cloud_textures[1] (WTHR CNAM).
    /// Only meaningful when `cloud_tile_scale_1 > 0.0`.
    pub(crate) cloud_texture_index_1: u32,
    /// Cloud layer 2 UV tile scale. `0.0` disables the layer.
    /// Set to `0.0` when no ANAM texture is available.
    pub(crate) cloud_tile_scale_2: f32,
    /// Bindless texture handle for cloud_textures[2] (WTHR ANAM).
    pub(crate) cloud_texture_index_2: u32,
    /// Cloud layer 3 UV tile scale. `0.0` disables the layer.
    /// Set to `0.0` when no BNAM texture is available.
    pub(crate) cloud_tile_scale_3: f32,
    /// Bindless texture handle for cloud_textures[3] (WTHR BNAM).
    pub(crate) cloud_texture_index_3: u32,
}
impl Resource for SkyParamsRes {}

/// Continuous-simulation cloud scroll accumulators — survive cell
/// transitions because the player exiting an exterior cell to an
/// interior shouldn't snap the cloud frame back to origin on
/// re-entry. Mirrors the `GameTimeRes` survives-transitions pattern.
///
/// Pre-#803 the four scroll fields lived on `SkyParamsRes`, which
/// `cell_loader::unload_cell` removes on every cell unload; the next
/// `apply_worldspace_weather` rebuilt the resource with `[0, 0]`
/// scroll, producing a visible cloud snap-back on every exterior
/// re-entry (~0.5 UV per 30 s of interior time, hard-cap at 1.0 via
/// the `weather_system` `rem_euclid(1.0)` wrap). Lifting the
/// accumulator into its own resource means `unload_cell` leaves it
/// alone, the renderer reads the live values per-frame, and
/// `weather_system` advances them across cell boundaries.
#[derive(Debug, Default)]
pub(crate) struct CloudSimState {
    /// Cloud layer 0 scroll offset (matches the scroll vector that
    /// formerly lived on `SkyParamsRes.cloud_scroll`).
    pub(crate) cloud_scroll: [f32; 2],
    /// Cloud layer 1 scroll offset (WTHR CNAM).
    pub(crate) cloud_scroll_1: [f32; 2],
    /// Cloud layer 2 scroll offset (WTHR ANAM).
    pub(crate) cloud_scroll_2: [f32; 2],
    /// Cloud layer 3 scroll offset (WTHR BNAM).
    pub(crate) cloud_scroll_3: [f32; 2],
}
impl Resource for CloudSimState {}

impl SkyParamsRes {
    /// Bindless texture handles owned by this resource.
    ///
    /// Acquired in `scene.rs` via `texture_registry.load_dds` (sun) and
    /// `acquire_by_path` (cloud layers); each call bumps the registry
    /// refcount once. `cell_loader::unload_cell` consumes this iterator
    /// to issue symmetric `drop_texture` calls so cell-cell transitions
    /// don't leak VRAM (#626). Update this list whenever a new bindless
    /// slot is added to the struct.
    pub(crate) fn texture_indices(&self) -> [u32; 5] {
        [
            self.cloud_texture_index,
            self.cloud_texture_index_1,
            self.cloud_texture_index_2,
            self.cloud_texture_index_3,
            self.sun_texture_index,
        ]
    }
}

/// Game time resource — tracks current hour of day (0.0–24.0).
/// Advances each frame based on real elapsed time × time scale.
pub(crate) struct GameTimeRes {
    /// Current game hour (0.0 = midnight, 6.0 = 6am, 12.0 = noon, etc.)
    pub(crate) hour: f32,
    /// Game-time multiplier: how many game-hours per real-second.
    /// Default 1.0 = 1 game-hour per real-minute (Bethesda default ~30:1).
    pub(crate) time_scale: f32,
}
impl Resource for GameTimeRes {}

impl Default for GameTimeRes {
    fn default() -> Self {
        Self {
            hour: 10.0,       // late morning
            time_scale: 30.0, // 30× = ~2 min per game hour (Bethesda default)
        }
    }
}

/// Full WTHR NAM0 sky color data stored for per-frame time-of-day interpolation.
/// Inserted alongside SkyParamsRes when loading an exterior cell with weather.
pub(crate) struct WeatherDataRes {
    /// 10 color groups × 6 time-of-day slots, raw monitor-space f32 per 0e8efc6.
    /// Indexed by `weather::SKY_*` and `weather::TOD_*` constants.
    pub(crate) sky_colors: [[[f32; 3]; 6]; 10],
    /// Fog distances: [day_near, day_far, night_near, night_far].
    pub(crate) fog: [f32; 4],
    /// Per-climate sunrise/sunset hour breakpoints — `weather_system`
    /// uses these to drive the TOD slot interpolator so Capital
    /// Wasteland and Mojave run on their own schedules (FO3 sunrise
    /// is ~0.3 hr earlier than FNV). Sourced from CLMT TNAM bytes
    /// (10-minute units converted to floating hours: `hour = byte / 6`).
    /// See #463.
    ///
    /// `[sunrise_begin, sunrise_end, sunset_begin, sunset_end]` in hours.
    /// Defaults (6.0, 10.0, 18.0, 22.0) match the pre-#463 hardcoded
    /// values so synthetic test cells and non-climate content keep
    /// their old behaviour.
    pub(crate) tod_hours: [f32; 4],
}
impl Resource for WeatherDataRes {}

/// In-flight cross-fade between two `WeatherDataRes` snapshots (M33.1).
///
/// When a cell load encounters a different weather while one is already
/// active, `scene.rs` keeps the current `WeatherDataRes` in place and
/// inserts this resource carrying the new target plus the WTHR TNAM
/// transition duration. `weather_system` advances `elapsed_secs` each
/// frame, blends the post-TOD-sample colours by `t = elapsed/duration`,
/// and on completion swaps the live `WeatherDataRes` to `target` and
/// removes the transition resource.
///
/// Interpolation happens after each side runs its own TOD-slot pick so
/// the transition stays correct across the midnight wrap (each weather
/// can be on a different slot).
pub(crate) struct WeatherTransitionRes {
    pub(crate) target: WeatherDataRes,
    pub(crate) elapsed_secs: f32,
    pub(crate) duration_secs: f32,
}
impl Resource for WeatherTransitionRes {}

/// Cached name→entity mapping for the animation system.
///
/// Rebuilt only when the count of `Name` components changes. Previously
/// the generation tracked `world.next_entity_id()`, which forced a full
/// rebuild on every entity spawn regardless of whether the spawn
/// involved a `Name` — a 3000-entity cell load with only 500 named
/// entities still triggered one rebuild on the next frame. Using the
/// `Name` storage size as the generation means only spawns/despawns
/// that actually touch `Name` invalidate the cache. See #249.
///
/// Edge case: in-place `Name` replacement (re-inserting `Name` on an
/// existing entity without removing it first) does not change the
/// count and therefore does not invalidate the index. No code in the
/// engine currently renames entities after spawn, so this is not a
/// concern today — add an explicit `invalidate()` call if that
/// changes.
pub(crate) struct NameIndex {
    pub(crate) map: HashMap<FixedString, EntityId>,
    /// Count of `Name` components seen at the last rebuild. `usize::MAX`
    /// on a fresh index so the first comparison always rebuilds.
    pub(crate) generation: usize,
}
impl Resource for NameIndex {}

/// Persisted subtree name maps for animation — maps root entity →
/// (bone name → entity) so the BFS walk isn't repeated every frame.
/// Invalidated alongside `NameIndex` when the Name component count changes. #278.
pub(crate) struct SubtreeCache {
    pub(crate) map: HashMap<EntityId, HashMap<FixedString, EntityId>>,
    /// Name component count at last rebuild — same invalidation signal as NameIndex.
    pub(crate) generation: usize,
}
impl Resource for SubtreeCache {}
impl SubtreeCache {
    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::new(),
            generation: usize::MAX,
        }
    }
}

/// Inverted index of `CellRoot → owned entities`, populated by
/// `cell_loader::stamp_cell_root` and drained by `unload_cell`. Pre-#791
/// the unload path iterated the entire `CellRoot` SparseSet to filter
/// down to victims of a single cell, scanning ~13.5k rows on a
/// radius-3 streaming grid (49 cells × ~1.5k entities) to find the
/// ~1.5k that belong to the unloading cell. With this index the lookup
/// is `HashMap::remove`, independent of the number of resident cells.
///
/// Memory cost is ~8 B per cell-owned entity (one `EntityId` per slot
/// in the inner `Vec`), dwarfed by the entity's component data.
pub(crate) struct CellRootIndex {
    pub(crate) map: HashMap<EntityId, Vec<EntityId>>,
}
impl Resource for CellRootIndex {}
impl CellRootIndex {
    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
}

impl NameIndex {
    pub(crate) fn new() -> Self {
        Self {
            map: HashMap::new(),
            generation: usize::MAX, // Force rebuild on first use.
        }
    }
}

/// Tracks keyboard and mouse input state for the fly camera.
pub(crate) struct InputState {
    pub(crate) keys_held: HashSet<KeyCode>,
    /// Yaw (horizontal) and pitch (vertical) in radians.
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) mouse_captured: bool,
    pub(crate) move_speed: f32,
    pub(crate) look_sensitivity: f32,
}

impl Resource for InputState {}

impl Default for InputState {
    fn default() -> Self {
        Self {
            keys_held: HashSet::new(),
            yaw: 0.0,
            pitch: 0.0,
            mouse_captured: false,
            move_speed: 200.0, // Bethesda units per second
            look_sensitivity: 0.002,
        }
    }
}

// ── M44 Phase 3.5 — footstep gameplay loop ──────────────────────────

/// Marker + accumulator on the entity whose horizontal movement
/// should produce footstep sounds (today the fly-camera entity; an
/// `M28.5` character controller will own this in future).
///
/// `last_position` and `accumulated_stride` are mutated each frame
/// by `footstep_system`; `stride_threshold` is read-only configuration
/// — a stride distance that triggers one footstep. Defaults to 1.5
/// game-units (~1.5m at FNV scale; reasonable walking cadence).
#[derive(Debug, Clone, Copy)]
pub(crate) struct FootstepEmitter {
    /// World position last frame, in renderer Y-up game units. Set
    /// from the entity's `GlobalTransform.translation` on first tick;
    /// updated each frame.
    pub(crate) last_position: Vec3,
    /// Horizontal distance walked since the last footstep fire,
    /// game-units. XZ plane only — vertical motion (jumping, falling)
    /// doesn't count toward stride.
    pub(crate) accumulated_stride: f32,
    /// Stride distance that triggers a footstep dispatch.
    pub(crate) stride_threshold: f32,
    /// Whether `last_position` has been initialised. False on first
    /// tick so the system seeds it without computing a bogus delta
    /// against the default zero pose.
    pub(crate) initialised: bool,
}

impl Component for FootstepEmitter {
    type Storage = SparseSetStorage<Self>;
}

impl FootstepEmitter {
    pub(crate) fn new() -> Self {
        Self {
            last_position: Vec3::ZERO,
            accumulated_stride: 0.0,
            stride_threshold: 1.5,
            initialised: false,
        }
    }
}

/// Resource — engine-wide footstep configuration. Today carries one
/// hardcoded sound (loaded at startup from a vanilla BSA when
/// available). Phase 3.5b replaces the single sound with a per-
/// material lookup (FOOT records).
pub(crate) struct FootstepConfig {
    /// Default footstep sound. `None` when the BSA-load failed
    /// (no archive on disk, decode error, audio inactive).
    pub(crate) default_sound: Option<Arc<Sound>>,
    /// Volume multiplier applied to every footstep play. 0.6 keeps
    /// footsteps mixed below dialogue / weapons / music; tweak per
    /// gameplay feel.
    pub(crate) volume: f32,
}

impl Resource for FootstepConfig {}

impl Default for FootstepConfig {
    fn default() -> Self {
        Self {
            default_sound: None,
            volume: 0.6,
        }
    }
}

/// Per-frame scratch buffer for `footstep_system`'s two-phase pattern
/// (collect trigger positions while walking emitters, then drain to
/// `AudioWorld::play_oneshot`). Pre-#932 the system allocated a fresh
/// `Vec<Vec3>` every frame; with this resource the same backing buffer
/// is `clear()`-ed and refilled, so capacity persists across frames.
///
/// Sized at 32 — typical loaded cell has 5–10 walking NPCs, peak
/// burst ~50 in dense exteriors. 32 covers the common case without
/// re-growing; the Vec doubles past that anyway if a giant crowd ever
/// triggers all at once.
pub(crate) struct FootstepScratch {
    pub(crate) triggers: Vec<Vec3>,
}

impl Resource for FootstepScratch {}

impl Default for FootstepScratch {
    fn default() -> Self {
        Self {
            triggers: Vec::with_capacity(32),
        }
    }
}

