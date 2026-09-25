//! Water rendering + interaction components.
//!
//! Three roles on the entity side:
//!
//! - [`WaterPlane`] — tags a render entity carrying a water surface.
//!   The owning entity has a [`Transform`] (Z plane in Bethesda world
//!   units), a [`MeshHandle`] (flat tessellated quad or per-cell
//!   shoreline-fit mesh), and a [`WaterMaterial`] that drives the
//!   water shader. Rivers and waterfalls also carry [`WaterFlow`].
//!
//! - [`WaterVolume`] — an AABB hung off the [`WaterPlane`] entity
//!   that bounds the "under the surface" region. Submersion queries
//!   point-test cameras / actors against it without walking the
//!   whole world.
//!
//! - [`SubmersionState`] — per-actor / per-camera state recomputed
//!   each frame: how deep we are in water (negative = above), whether
//!   the head is under, which water material to drive underwater FX.
//!   Drives swim animation, underwater fog/tint in composite, and the
//!   audio low-pass send.
//!
//! Design notes:
//!
//! - Water as ECS, not scene graph. A river is one entity per
//!   contiguous flow region; a lake is one entity per cell. The cell
//!   loader spawns these from XCLW (water height) + XCWT (WATR form).
//! - Flat authored mesh with bounded raster-side displacement. Water is not
//!   part of the TLAS, so displacement never triggers per-frame BLAS rebuilds.
//!   Wave detail is normal-map perturbation in the fragment shader.
//!   Reflections through the perturbed normal are RT-traced; refraction
//!   is RT-traced through the inverted normal. See `shaders/water.frag`.
//! - Per-game WATR field layouts differ. The `WaterMaterial` on the
//!   plane component is the engine-normalised view; per-game parsing
//!   lives in `crates/plugin/src/esm/records/misc.rs::parse_watr` and
//!   uses the [`GameKind`] axis already plumbed there.
//!
//! [`Transform`]: super::Transform
//! [`MeshHandle`]: super::MeshHandle
//! [`GameKind`]: ../../../../../plugin/src/esm/reader.rs

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::{Component, EntityId};
use std::sync::Arc;

/// Canonical sentinel values for water records that omit authored wave data.
/// Parser defaults, ECS defaults, and shader normalization all derive from
/// these constants so compatibility water preserves one baseline response.
pub const DEFAULT_WATER_WAVE_AMPLITUDE: f32 = 0.05;
pub const DEFAULT_WATER_WAVE_FREQUENCY: f32 = 0.6;
/// Upper bound authored by vanilla Starfield for each RGB water-column
/// concentration lane. The WATAL translate boundary
/// (`byroredux/src/env_translate.rs`) normalizes authored pigment
/// concentrations against this shared reference into the canonical 0..1
/// pigment fraction; the fourth `oceanness` lane is authored natively in
/// 0..1 and passes through. #4285 moved this normalization out of
/// `water.frag` — per-game unit conventions belong at the
/// parser→canonical boundary, never in shader source.
pub const STARFIELD_WATER_CONCENTRATION_REFERENCE: f32 = 20.0;

/// Canonical half-width of the waterline acceptance/hysteresis band, in
/// Bethesda world units. Camera submersion and rigid-body contact must use
/// this same value so overlapping surfaces and boundary transitions resolve
/// identically across gameplay and physics.
pub const WATERLINE_HYSTERESIS: f32 = 4.0;

/// How a water surface's authored normal/noise texture encodes its
/// perturbation. A property of the shader family that consumes the texture,
/// resolved once at the WATR parse boundary.
///
/// Measured on the shipped textures: Skyrim `DefaultWater.dds` and FO4's
/// `DefaultWater`/`DefaultWaterTile`/`ChurningWaterTile` decode to unit
/// vectors with +Z everywhere; FO3/FNV `WastelandWaterPotomac.dds` and
/// `WaterFlowRippleNoise01.dds` decode to vectors of mean length 0.48 / 0.13
/// with Z below zero on 29 % / 53 % of texels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum WaterNormalEncoding {
    /// A unit tangent-space normal map: `N = normalize(rgb * 2 - 1)`.
    #[default]
    TangentNormal = 0,
    /// FO3/FNV noise map: the decoded `rgb * 2 - 1` is an offset on the
    /// surface up axis, `N = normalize(n + (0, 0, 1))` — the FNV/FO3
    /// `WATER000.pso` (shader package 019) decode at full depth. Read as a
    /// unit normal it points below the surface on a third of the texels.
    OffsetNoise = 1,
}

/// How the surface should move and shade. Drives shader path selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum WaterKind {
    /// Lake / pond / ocean / interior pool. Horizontal plane, two
    /// scrolling normal maps with no preferred direction. Foam only
    /// at shoreline depth contact.
    Calm = 0,
    /// River, canal, slow current. Horizontal plane, normal-map scroll
    /// biased along [`WaterFlow::direction`]. Foam streaks gated on
    /// flow speed but light.
    River = 1,
    /// Rapids — fast-moving horizontal water. Same plane as `River`
    /// but the shader adds heavy flow-aligned foam streaks and a
    /// secondary high-frequency normal layer for whitewater chop.
    Rapids = 2,
    /// Waterfall sheet. Surface is near-vertical; the shader treats
    /// the mesh tangent as the flow axis (downward in world space)
    /// and scrolls the noise sheet along it at high speed. Heavily
    /// opaque, foam at top + bottom of the sheet, no refraction ray.
    Waterfall = 3,
    /// Authored non-water liquid such as Oblivion WATR.MNAM=`lava`.
    /// It remains a contact/damage surface but does not use water refraction,
    /// underwater presentation, directional current, or shoreline foam.
    Lava = 4,
}

impl WaterKind {
    /// Canonical renderer-tuned foam response for this semantic kind.
    ///
    /// These are engine presentation anchors, not claimed WATR-authored
    /// values: calm shoreline contact has the established 0.65 baseline,
    /// rivers keep restrained streaks at 0.20, and rapids/waterfall
    /// whitewater use 0.85. Keeping the profile beside the kind makes ESM
    /// planes and NIF mesh water resolve identically (#3184).
    pub const fn canonical_foam_strength(self) -> f32 {
        match self {
            WaterKind::Calm => 0.65,
            WaterKind::River => 0.20,
            WaterKind::Rapids | WaterKind::Waterfall => 0.85,
            WaterKind::Lava => 0.0,
        }
    }

    /// `true` when the renderer should fire a refraction ray below
    /// the surface. Waterfalls are opaque enough that the refraction
    /// ray is wasted budget.
    #[inline]
    pub fn refracts(self) -> bool {
        !matches!(self, WaterKind::Waterfall | WaterKind::Lava)
    }

    /// Whether this semantic kind implies a directed current.
    pub const fn has_directional_flow(self) -> bool {
        matches!(
            self,
            WaterKind::River | WaterKind::Rapids | WaterKind::Waterfall
        )
    }
}

/// Engine-normalised material parameters for one water surface.
/// Lives on the [`WaterPlane`] component (small enough to inline —
/// no separate registry / handle indirection). Populated from the
/// referenced WATR record at cell-load time; sensible defaults when
/// the cell omits XCWT or the WATR record is missing.
///
/// All colours are linear RGB in the engine's working colour space
/// (matches `Material::diffuse`; see `feedback_color_space.md`).
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct WaterMaterial {
    /// How [`Self::normal_map_index`] and the noise layers are decoded.
    pub normal_encoding: WaterNormalEncoding,
    /// Authored `BSWaterShaderProperty.water_shader_flags` for mesh-bound
    /// water. Zero means the legacy property had no dedicated flag word and
    /// keeps the renderer's compatibility defaults. The nif.xml
    /// `WaterShaderPropertyFlags` vocabulary occupies bits 0..=13;
    /// translation resolves the visual gates once.
    pub shader_flags: u32,
    /// Colour seen looking down through shallow water — blended with
    /// the refraction-ray hit colour via depth-through-water.
    pub shallow_color: [f32; 3],
    /// Colour seen looking down through deep water (refraction ray
    /// distance ≥ [`Self::fog_far`]).
    pub deep_color: [f32; 3],
    /// Authored underwater post-process tint. Older games do not expose a
    /// separate tint and retain the deep-water colour here.
    pub underwater_color: [f32; 3],
    /// NEAR PLANE of the underwater fog ramp (world units): the water
    /// column is clear out to this distance, and absorption starts here.
    ///
    /// #2785 — this said "distance at which the shallow colour reaches 50%
    /// mix", which is not what WATR authors. Vanilla measurements:
    /// Skyrim's `BlackreachWater` 0/290, `MarkarthWater` 0/110,
    /// `HorseTroughWater01` 220/4710; FNV's `NVCleanWaterGS` 7/58;
    /// Oblivion's median `fog_near/fog_far` 0.001. It is `0` for nearly
    /// every water body — a *ramp start*, meaningless as a half-distance
    /// (which would make all that water instantly opaque). Same pair
    /// semantics as the cell-lighting fog range.
    pub fog_near: f32,
    /// FAR PLANE of the same ramp: distance through water at which the
    /// deep colour fully takes over (refraction tint converges to
    /// `deep_color`). Always `> fog_near` — the ESM parser clamps it to
    /// `fog_near + 1` at minimum.
    pub fog_far: f32,
    /// FO4+/Creation-2 WATR `Depth Amount`. Kept distinct from fog ranges:
    /// source schemas author explicit color/underwater ranges separately.
    /// Zero is the absent sentinel for older records and mesh water.
    pub depth_amount: f32,
    /// Underwater fog near/far ramp. A zero far value means reuse the
    /// above-water ramp (legacy records without an underwater tail).
    pub underwater_fog_near: f32,
    pub underwater_fog_far: f32,
    /// Authored underwater fog strength. One is neutral; zero disables the
    /// underwater colour transition while preserving the submersion state.
    pub underwater_fog_amount: f32,
    /// Authored surface opacity from WATR.ANAM. The procedural fallback uses
    /// 0.88 when no WATR record supplies a value.
    pub opacity: f32,
    /// FO4/FO76 depth-dependent surface alpha controls. An all-zero tuple
    /// preserves the legacy constant-opacity path.
    pub alpha_controls: [f32; 4],
    /// Schlick F0 at normal incidence. ~0.02 for clean water; ~0.04
    /// for muddy / chemical / Hubris Comics water. Drives fresnel.
    pub fresnel_f0: f32,
    /// 0..1 — how much of the reflection ray colour is mixed back
    /// (post-fresnel). 1.0 = pure mirror; 0.7 = REDengine-style
    /// "convincing but not chrome".
    pub reflectivity: f32,
    /// Tint applied to the reflected geometry hit colour in
    /// `traceWaterRay`. Sourced from `WATR DATA reflection_color`
    /// (#1069 / F-WAT-09). Allows chemically-tinted, lava, and
    /// ocean water to show distinct reflected-geometry hues.
    /// Default `[0.65, 0.70, 0.75]` matches the pre-fix hard-coded
    /// neutral-grey value in `water.frag`.
    pub reflection_tint: [f32; 3],
    /// Authored legacy reflection HDR multiplier. One is neutral.
    pub reflection_hdr_multiplier: f32,
    /// Daytime surface palette resolved from GNAM slot 0. When the record
    /// has no authored daytime variant this mirrors the base palette.
    pub day_shallow_color: [f32; 3],
    pub day_deep_color: [f32; 3],
    pub day_fog_near: f32,
    pub day_fog_far: f32,
    pub day_reflection_tint: [f32; 3],
    /// Nighttime surface palette resolved from GNAM slot 1. The renderer
    /// blends this with the daytime palette using the live climate TOD
    /// factor; defaults mirror the daytime values for legacy records.
    pub night_shallow_color: [f32; 3],
    pub night_deep_color: [f32; 3],
    pub night_fog_near: f32,
    pub night_fog_far: f32,
    pub night_reflection_tint: [f32; 3],
    /// Canonical normal-map index in the bindless texture array. The render
    /// path uses it for any missing authored wave layers; `u32::MAX` means
    /// procedural fallback.
    pub normal_map_index: u32,
    /// Authored BGSM flow-map index for mesh-bound water. The map stores a
    /// tangent-plane direction in RG; `u32::MAX` means no flow map. Cell WATR
    /// surfaces retain this sentinel because their flow comes from WaterFlow.
    pub flow_map_index: u32,
    /// Skyrim WATR.FNAM bit 0x10. When false, only the primary authored
    /// normal layer contributes; `true` is the compatibility default.
    pub blend_normals: bool,
    /// Bindless indices for Skyrim+/FO4 authored noise layers NAM2–4.
    /// `u32::MAX` means procedural fallback; the render path fills missing
    /// layers from `normal_map_index` when a legacy record has no NAM paths.
    pub noise_map_indices: [u32; 3],
    /// World-space scroll vectors for the three wave layers, in **UV per
    /// second** — the velocity the visible pattern travels at, not a UV
    /// offset rate: the shader advances its sample point at `-scroll·t`, so
    /// a feature moves along `+scroll` (#4728). That sign convention keeps
    /// the surface pattern, the physics current, and the rapids foam
    /// streaks all running the same way.
    ///
    /// Composition lives at the translate boundary (`env_translate.rs`),
    /// not the cell loader: for `River` / `Rapids`, vector 0 is the flow
    /// term `flow.direction * flow.speed * WATER_SCROLL_UV_PER_BU_PER_S`
    /// plus the record's converted layer-0 motion; vector 1 is the
    /// perpendicular shear at half the downstream rate
    /// (`WATER_PERPENDICULAR_SHEAR_SCROLL`) plus the converted layer-1
    /// motion; vector 2 carries the converted layer-3 motion verbatim, or
    /// mirrors vector 0 when the record authors none. `Calm` water keeps
    /// authored vectors verbatim when present. The weather wind term (same
    /// world-space convention) is added on top in `render/water.rs`.
    pub scroll_a: [f32; 2],
    pub scroll_b: [f32; 2],
    pub scroll_c: [f32; 2],
    /// UV scale for each normal-map layer. Detail tile size — small
    /// (~1/200 world units) for choppy water, large (~1/800) for
    /// slow swells.
    pub uv_scale_a: f32,
    pub uv_scale_b: f32,
    /// UV scale for the authored third noise layer (NAM4). Legacy records
    /// use the canonical sentinel and the shader falls back to layer A.
    pub uv_scale_c: f32,
    /// Authored mesh-water UV translation from `WaterShaderProperty` /
    /// `BSWaterShaderProperty`. Cell WATR surfaces use the zero sentinel;
    /// mesh-bound water applies this offset to its world-space normal UVs.
    pub uv_offset: [f32; 2],
    /// Authored normal-amplitude multipliers for NAM2/NAM3/NAM4. A value of
    /// one is the neutral legacy fallback.
    pub noise_amplitude_scales: [f32; 3],
    /// Authored distance at which high-frequency water normals fade out.
    /// Zero preserves the legacy always-on normal response.
    pub noise_falloff: f32,
    /// Authored shallow/deep/surface-effect normal falloff multipliers.
    /// Zero triplet preserves the legacy always-on normal response.
    pub normal_falloff: [f32; 3],
    /// Authored transient-displacement shape: starting size, radial falloff,
    /// and dampener. Zero preserves the legacy ripple profile.
    pub displacement: [f32; 3],
    /// Legacy rain-simulator starting ripple size. Zero uses the shader
    /// default profile.
    pub rain_start_size: f32,
    /// Legacy rain-simulator velocity. Zero uses the shader default rate.
    pub rain_velocity: f32,
    /// Legacy rain-simulator falloff and dampener.
    pub rain_falloff: f32,
    pub rain_dampener: f32,
    /// Authored physical normal magnitude. Applied to the noise amplitudes
    /// before the compact GPU material is uploaded; one is neutral.
    pub normal_magnitude: f32,
    /// Authored above-water fog amount. Applied to the refraction absorption
    /// weight before the compact GPU material is uploaded; one is neutral.
    pub above_water_fog_amount: f32,
    /// Depth-response multipliers for reflections, refraction, normals, and
    /// specular lighting. Legacy records use neutral ones.
    pub depth_weights: [f32; 4],
    /// Authored effect controls: refraction magnitude, local specular power,
    /// reflection magnitude, and sun-specular magnitude.
    pub effect_controls: [f32; 4],
    /// Authored specular magnitude. One is neutral; zero means legacy
    /// records without a separate magnitude field.
    pub specular_magnitude: f32,
    /// Skyrim's authored specular-radius control. Zero is the legacy sentinel.
    pub specular_radius: f32,
    /// Authored flow-map tile scale. One is neutral; zero means legacy
    /// records without a dedicated flow-map field.
    pub flowmap_scale: f32,
    /// Starfield per-channel extinction coefficients. A zero triplet is the
    /// canonical sentinel for pre-Starfield records and keeps their legacy
    /// scalar fog response unchanged.
    pub absorption_coefficients: [f32; 3],
    /// Starfield water-column concentrations: phytoplankton, sediment,
    /// yellow matter, and oceanness, each stored as the **canonical
    /// 0..1 pigment fraction** — the WATAL translate boundary divides the
    /// authored RGB lanes by
    /// [`STARFIELD_WATER_CONCENTRATION_REFERENCE`] (#4285; the fourth
    /// `oceanness` lane is authored natively in 0..1 and passes through).
    /// Zero is the legacy sentinel.
    pub concentration: [f32; 4],
    /// Foam intensity multiplier. Calm water uses a moderate shoreline
    /// baseline; the cell loader raises it for river/rapids whitewater.
    /// `1` is full rapids / waterfall foam.
    pub foam_strength: f32,
    /// Shoreline foam falloff distance (world units). Foam at scene
    /// geometry within this distance below the water surface; fades
    /// to zero past it. ~30 wu matches Skyrim's vanilla shoreline.
    pub shoreline_width: f32,
    /// Refraction IOR. 1.33 = clean water; bumping up to 1.5 for
    /// stylised reads or thick visc fluid. Glass at 1.5.
    pub ior: f32,
    /// Vertex-displacement magnitude (world units) authored in WATR
    /// `DATA`/`DNAM`. The raster water vertex path applies a bounded
    /// two-wave displacement; the water surface remains excluded from the
    /// TLAS, so this does not require per-frame BLAS updates.
    /// Promoted onto the canonical material in WATAL Phase 1 so the
    /// field stops being dropped at the translate boundary.
    pub wave_amplitude: f32,
    /// Wave frequency (Hz) — companion to [`Self::wave_amplitude`].
    pub wave_frequency: f32,
    /// Authored WATR `NAM1` angular velocity around the Gamebryo up axis,
    /// converted to renderer radians per second. Zero is the legacy
    /// sentinel; it rotates authored normal-layer scroll vectors only.
    pub angular_velocity: f32,
    /// Multiplier for live precipitation-driven surface ripples. One is the
    /// neutral fallback; legacy records without a rain simulator retain the
    /// shared weather response.
    pub rain_response: f32,
    /// Direct-sun glint exponent from WATR `Sun Specular Power`.
    /// Larger values produce a smaller, tighter highlight. This is an
    /// exponent, not an intensity multiplier.
    pub sun_specular_power: f32,
    /// Starfield's authored surface roughness. Zero is the legacy sentinel;
    /// the water shader uses it to soften geometry-hit reflections while the
    /// direct-sun exponent remains derived separately from the same source.
    pub roughness: f32,
    /// Source WATR FormID for debug overlays / save-game roundtrip.
    /// `0` when the plane was spawned without an XCWT reference
    /// (default water material).
    pub source_form: u32,
}

impl Default for WaterMaterial {
    fn default() -> Self {
        // Sensible defaults — calm freshwater lake, mid-blue cast.
        // Values cross-checked against Skyrim "DefaultWater" WATR
        // and CDPR's `ww_lake_clean` material as documented in the
        // Ultra Plus mod cvar dump.
        Self {
            normal_encoding: WaterNormalEncoding::TangentNormal,
            shader_flags: 0,
            shallow_color: [0.10, 0.32, 0.38],
            deep_color: [0.02, 0.06, 0.10],
            underwater_color: [0.02, 0.06, 0.10],
            fog_near: 80.0,
            fog_far: 600.0,
            depth_amount: 0.0,
            underwater_fog_near: 0.0,
            underwater_fog_far: 0.0,
            underwater_fog_amount: 1.0,
            opacity: 0.88,
            alpha_controls: [0.0; 4],
            fresnel_f0: 0.02,
            reflectivity: 0.85,
            normal_map_index: u32::MAX,
            flow_map_index: u32::MAX,
            blend_normals: true,
            noise_map_indices: [u32::MAX; 3],
            scroll_a: [0.020, 0.011],
            scroll_b: [-0.014, 0.025],
            scroll_c: [0.0, 0.0],
            uv_scale_a: 1.0 / 256.0,
            uv_scale_b: 1.0 / 700.0,
            uv_scale_c: 1.0 / 512.0,
            uv_offset: [0.0, 0.0],
            noise_amplitude_scales: [1.0; 3],
            noise_falloff: 0.0,
            normal_falloff: [0.0; 3],
            displacement: [0.0; 3],
            rain_start_size: 0.0,
            rain_velocity: 0.0,
            rain_falloff: 0.0,
            rain_dampener: 0.0,
            normal_magnitude: 1.0,
            above_water_fog_amount: 1.0,
            depth_weights: [1.0; 4],
            effect_controls: [0.0, 0.0, 1.0, 1.0],
            specular_magnitude: 1.0,
            specular_radius: 0.0,
            flowmap_scale: 1.0,
            absorption_coefficients: [0.0; 3],
            concentration: [0.0; 4],
            // Shoreline foam is authored by the shader's contact ray for
            // every non-waterfall surface. Keep a visible but restrained
            // baseline for calm lakes/oceans; flow kinds override this with
            // their stronger/slower whitewater profiles in EXAL.
            foam_strength: 0.65,
            shoreline_width: 32.0,
            ior: 1.33,
            // SENTINEL (WATAL §4): matches `WaterParams::default` so a
            // record that omits wave data resolves identically across
            // all games.
            wave_amplitude: DEFAULT_WATER_WAVE_AMPLITUDE,
            wave_frequency: DEFAULT_WATER_WAVE_FREQUENCY,
            angular_velocity: 0.0,
            rain_response: 1.0,
            sun_specular_power: 50.0,
            roughness: 0.0,
            source_form: 0,
            reflection_tint: [0.65, 0.70, 0.75],
            reflection_hdr_multiplier: 1.0,
            day_shallow_color: [0.10, 0.32, 0.38],
            day_deep_color: [0.02, 0.06, 0.10],
            day_fog_near: 80.0,
            day_fog_far: 600.0,
            day_reflection_tint: [0.65, 0.70, 0.75],
            night_shallow_color: [0.10, 0.32, 0.38],
            night_deep_color: [0.02, 0.06, 0.10],
            night_fog_near: 80.0,
            night_fog_far: 600.0,
            night_reflection_tint: [0.65, 0.70, 0.75],
        }
    }
}

/// Tag component for water-surface entities.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct WaterPlane {
    pub kind: WaterKind,
    pub material: WaterMaterial,
    /// FO3/FNV authored water damage per second. Zero means the surface is
    /// harmless (the default for all non-legacy water generations).
    pub damage_per_second: f32,
}

impl Component for WaterPlane {
    type Storage = SparseSetStorage<Self>;
}

/// Flow vector for rivers / rapids / waterfalls. Drives:
///
/// - shader UV scroll bias (the dominant wave layer travels along
///   [`Self::direction`] at [`Self::speed`]);
/// - foam-streak orientation in `Rapids` mode;
/// - current drag on dynamic bodies and swim resistance for actors.
///
/// `Calm` waters do not carry this component.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct WaterFlow {
    /// Unit vector in **world Y-up space**. Y component is typically
    /// `-1.0` for waterfalls (falls are downward in Y-up); horizontal
    /// currents (rivers) keep Y=0. Synthesized flows use the WATR
    /// `wind_direction` angle after the Z→Y swizzle; records with an
    /// authored NAM0 linear velocity retain that vector's direction instead.
    pub direction: [f32; 3],
    /// World units per second, always inside
    /// [`WaterFlow::SPEED_MIN`]`..=`[`WaterFlow::SPEED_MAX`] when built
    /// through [`WaterFlow::new`] / [`WaterFlow::for_kind`]. Typical:
    /// 0.5 (calm river) … 8.0 (whitewater rapids) … 25.0 (Tamriel-tall
    /// waterfall sheet).
    pub speed: f32,
}

/// Conversion from the shared atmospheric wind magnitude (BU/s) to the
/// water normal-layer UV scroll rate. Renderer upload and CPU crest/contact
/// sampling both use this value so visible water and gameplay stay phase
/// coherent.
pub const WEATHER_SCROLL_PER_BU_PER_S: f32 = 0.0015;

impl WaterFlow {
    /// Slowest current the physics sink will simulate — the "calm river"
    /// anchor of [`Self::speed`]'s documented band. BU/s.
    pub const SPEED_MIN: f32 = 0.5;
    /// Fastest current the physics sink will simulate — the "Tamriel-tall
    /// waterfall sheet" anchor of [`Self::speed`]'s documented band. BU/s.
    ///
    /// This is a hard ceiling, not a hint: `physics::water::current_force`
    /// drives clutter toward `speed` as a terminal velocity, so an
    /// unclamped value is an unbounded velocity target (#2872).
    pub const SPEED_MAX: f32 = 25.0;

    /// Speed for the "whitewater rapids" anchor of the documented band.
    pub const SPEED_RAPIDS: f32 = 8.0;

    /// Build a canonical flow: `direction` normalised, `speed` clamped into
    /// the documented [`Self::SPEED_MIN`]`..=`[`Self::SPEED_MAX`] band.
    ///
    /// Every translate-side producer must come through here. A
    /// degenerate/non-finite direction resolves to `+Z` at `SPEED_MIN`
    /// rather than propagating NaN into the solver.
    pub fn new(direction: [f32; 3], speed: f32) -> Self {
        let [x, y, z] = direction;
        let len = (x * x + y * y + z * z).sqrt();
        let direction = if len.is_finite() && len > 1e-6 {
            [x / len, y / len, z / len]
        } else {
            [0.0, 0.0, 1.0]
        };
        let speed = if speed.is_finite() {
            speed.clamp(Self::SPEED_MIN, Self::SPEED_MAX)
        } else {
            Self::SPEED_MIN
        };
        Self { direction, speed }
    }

    /// Canonical current speed implied by a [`WaterKind`], in BU/s.
    ///
    /// #2872 — the physics current is synthesized from the *kind* rather
    /// than from the WATR wind field, which carries no usable per-record
    /// speed: across vanilla Fallout 3, Fallout: New Vegas and Skyrim SE,
    /// the float at the head of `WATR.DATA` / `DNAM` is `90.0` on 146 of
    /// 165 records (every 196- and 228-byte record, i.e. all of Skyrim and
    /// ~85% of the Fallouts) — a constant, and exactly the value the
    /// shorter legacy layouts carry in the *direction* slot. A field with
    /// no variance across three games cannot be an authored per-water
    /// velocity, and 90 BU/s is 3.6× this band's ceiling. Resolving which
    /// offset actually holds the wind velocity in the newer layouts is a
    /// decode-side question owned by the ESM parser, not something the
    /// physics sink should guess at; these three values are the anchors
    /// [`Self::speed`] already documents.
    pub const fn speed_for_kind(kind: WaterKind) -> f32 {
        match kind {
            // Calm water carries no `WaterFlow` at all; the arm exists so
            // the match stays total if a caller asks anyway.
            WaterKind::Calm | WaterKind::River | WaterKind::Lava => Self::SPEED_MIN,
            WaterKind::Rapids => Self::SPEED_RAPIDS,
            WaterKind::Waterfall => Self::SPEED_MAX,
        }
    }

    /// Canonical flow for a `kind` travelling along `direction`.
    pub fn for_kind(kind: WaterKind, direction: [f32; 3]) -> Self {
        Self::new(direction, Self::speed_for_kind(kind))
    }
}

impl Component for WaterFlow {
    type Storage = SparseSetStorage<Self>;
}

/// Axis-aligned bounding volume for the underwater region of a
/// water plane. `min.y` is the cell floor (or the lowest world
/// vertex within the planar extent); `max.y` equals the plane height.
/// Used by `submersion_system` to short-circuit the per-actor depth
/// query against every plane in the world.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct WaterVolume {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Component for WaterVolume {
    type Storage = SparseSetStorage<Self>;
}

impl WaterVolume {
    /// Static (wave-free) surface height over the `(x, z)` column, or `None`
    /// when the column lies outside this water's footprint.
    ///
    /// Without a [`WaterSurfaceMesh`] the surface is the planar `max.y`. With
    /// one, the surface is the authored triangles themselves; `reference_y`
    /// picks the nearest layer where the mesh overlaps itself in XZ. Every
    /// submersion / swim / buoyancy consumer resolves the surface through
    /// this one function so the three cannot disagree about where water is.
    pub fn surface_y_at(
        &self,
        surface: Option<&WaterSurfaceMesh>,
        x: f32,
        z: f32,
        reference_y: f32,
    ) -> Option<f32> {
        if x < self.min[0] || x > self.max[0] || z < self.min[2] || z > self.max[2] {
            return None;
        }
        match surface {
            None => Some(self.max[1]),
            Some(mesh) => mesh.surface_y_at(x, z, reference_y),
        }
    }
}

/// World-space surface triangles of a mesh-bound water body that authors no
/// phantom volume.
///
/// [`WaterVolume`] alone models a flat surface at `max.y`, which a water mesh
/// need not be: Skyrim's `markarthwatersystemstream.nif` descends ~1800 BU
/// through the city, so a single plane at its placement origin put the
/// Markarth gate — 237 BU from the nearest stream vertex — 393 BU
/// "underwater". The rendered triangles are the only authored description of
/// where that surface is, so they are the surface
/// ([`WaterVolume::surface_y_at`]); the volume stays the coarse AABB reject.
#[derive(Debug, Clone)]
pub struct WaterSurfaceMesh {
    // Keep geometry immutable so the construction-time index cannot become stale.
    triangles: Arc<[[[f32; 3]; 3]]>,
    grid: Option<Arc<surface_grid::SurfaceGrid>>,
}

mod surface_grid;

// Shared with the grid bounds: the broad phase must include the same slack
// as the exact barycentric test, including columns just outside an edge.
const WATER_SURFACE_EDGE_EPSILON: f32 = 1.0e-4;

impl Component for WaterSurfaceMesh {
    type Storage = SparseSetStorage<Self>;
}

impl WaterSurfaceMesh {
    /// Build the immutable XZ lookup once when the placed surface is created.
    /// Cloning the component shares both geometry and index with physics.
    pub fn new(triangles: Vec<[[f32; 3]; 3]>) -> Self {
        let grid = (triangles.len() > 8)
            .then(|| surface_grid::SurfaceGrid::new(&triangles))
            .flatten()
            .map(Arc::new);
        Self { triangles: triangles.into(), grid }
    }

    /// Height of the triangle surface over `(x, z)` nearest to
    /// `reference_y`, or `None` when no triangle covers the column.
    pub fn surface_y_at(&self, x: f32, z: f32, reference_y: f32) -> Option<f32> {
        if !x.is_finite() || !z.is_finite() || !reference_y.is_finite() {
            return None;
        }
        // Barycentric slack so a column exactly on a shared edge or vertex
        // is not lost to rounding between the two triangles that own it.
        let mut best: Option<(f32, usize)> = None;
        let mut visit = |index: usize| {
            let [a, b, c] = &self.triangles[index];
            let det = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
            // Vertical (edge-on in XZ) triangles cover no column.
            if det.abs() <= f32::EPSILON {
                return;
            }
            let l1 = ((b[2] - c[2]) * (x - c[0]) + (c[0] - b[0]) * (z - c[2])) / det;
            let l2 = ((c[2] - a[2]) * (x - c[0]) + (a[0] - c[0]) * (z - c[2])) / det;
            let l3 = 1.0 - l1 - l2;
            if l1 < -WATER_SURFACE_EDGE_EPSILON
                || l2 < -WATER_SURFACE_EDGE_EPSILON
                || l3 < -WATER_SURFACE_EDGE_EPSILON
            {
                return;
            }
            let y = l1 * a[1] + l2 * b[1] + l3 * c[1];
            // Cell and large-triangle lists are visited separately. Preserve
            // the original source-order tie break when two layers are equidistant.
            if y.is_finite() && best.is_none_or(|(prev, previous_index)| {
                let distance = (y - reference_y).abs();
                let previous_distance = (prev - reference_y).abs();
                distance < previous_distance || (distance == previous_distance && index < previous_index)
            }) {
                best = Some((y, index));
            }
        };
        if let Some(grid) = &self.grid {
            for &index in grid.candidates(x, z)?.iter().chain(grid.large_triangles()) {
                visit(index);
            }
        } else {
            // Tiny surfaces have a bounded scan without an index allocation.
            for index in 0..self.triangles.len() {
                visit(index);
            }
        }
        best.map(|(height, _)| height)
    }
}

/// Non-rendering current volume authored by a placed water-current marker
/// (`REFR.XWCU` + spatial bounds). Unlike [`WaterPlane`], this component
/// never participates in submersion or buoyancy surface selection; it only
/// contributes bounded current drag to dynamic bodies inside its volume.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct WaterCurrentVolume {
    pub volume: WaterVolume,
    pub flow: WaterFlow,
}

impl Component for WaterCurrentVolume {
    type Storage = SparseSetStorage<Self>;
}

/// Per-frame submersion state for actors and cameras.
///
/// Recomputed every frame by `submersion_system` from current world
/// position + the set of active `WaterPlane` / `WaterVolume` entities.
/// Drives downstream consumers:
///
/// - Underwater composite tint / fog (camera path).
/// - Swim animation state switch + slower locomotion (actor path).
/// - Audio submix: head-under triggers low-pass on the master bus
///   (audio system reads this on the player's camera entity).
#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct SubmersionState {
    /// Distance from the entity origin to the nearest water plane
    /// above it, along world Y (up). Positive = under water by this
    /// many world units. Negative or zero = above the surface.
    pub depth: f32,
    /// `true` once `depth >= head_offset`. Actors set `head_offset`
    /// implicitly via their collider height; cameras set it to 0.
    pub head_submerged: bool,
    /// Water-plane entity that supplied this state. Underwater consumers
    /// resolve its canonical [`WaterPlane::material`] on demand; `None` means
    /// the entity is above every surface or outside all water volumes.
    pub surface_entity: Option<EntityId>,
}

impl Component for SubmersionState {
    type Storage = SparseSetStorage<Self>;
}

/// Per-**physics-body** water contact — the generalisation of
/// [`SubmersionState`] from the camera to every dynamic body (WATAL §5.4).
///
/// Written each tick by the physics buoyancy phase (`byroredux_physics`)
/// for any body whose collider overlaps a [`WaterVolume`]. Where
/// `SubmersionState` carries only a scalar `depth` (enough for camera
/// underwater FX), `WaterContact` adds [`Self::submerged_fraction`] — the
/// displaced-volume estimate Archimedes buoyancy needs and the scalar
/// `depth` cannot provide.
///
/// A body that has left every water volume keeps a **dry** `WaterContact`
/// (`submerged_fraction == 0`, `surface_entity == None`) for one transition so
/// the buoyancy phase can restore the body's authored damping exactly
/// once; bodies that have never touched water carry no component at all.
#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct WaterContact {
    /// Water-plane entity that supplied this contact. Presentation bridges
    /// use it to place a body ripple on the actual rendered surface; `None`
    /// is the dry sentinel.
    pub surface_entity: Option<EntityId>,
    /// Surface Y minus the body's centre Y. Positive = centre below the
    /// surface. Mirrors [`SubmersionState::depth`].
    pub depth: f32,
    /// Fraction of the body's vertical span below the surface, `0.0`
    /// (dry) … `1.0` (fully under), from the collider AABB vs the water
    /// column. The displaced-volume proxy buoyancy integrates against.
    pub submerged_fraction: f32,
    /// `true` once the whole body (AABB top) is below the surface — the
    /// drowning / fully-submerged-FX gate.
    pub head_submerged: bool,
    /// The current acting on this body, when it sits in flowing water
    /// (river / rapids / waterfall). `None` for calm water. Carried here
    /// so the physics current force and gameplay can consume it without
    /// re-querying the plane.
    pub flow: Option<WaterFlow>,
    /// FO3/FNV authored water damage per second, carried through contact
    /// resolution for actor/gameplay consumers. Zero is harmless water.
    pub damage_per_second: f32,
}

impl Component for WaterContact {
    type Storage = SparseSetStorage<Self>;
}

#[cfg(test)]
mod surface_mesh_tests {
    use super::{WaterSurfaceMesh, WaterVolume};

    /// Two triangles forming a strip over x∈[0,100], z∈[0,10] that descends
    /// from y=50 at x=0 to y=0 at x=100 — a stream segment.
    fn sloped_strip() -> WaterSurfaceMesh {
        let (a, b) = ([0.0, 50.0, 0.0], [100.0, 0.0, 0.0]);
        let (c, d) = ([100.0, 0.0, 10.0], [0.0, 50.0, 10.0]);
        WaterSurfaceMesh::new(vec![[a, b, c], [a, c, d]])
    }

    #[test]
    fn sloped_surface_is_sampled_not_taken_from_the_highest_point() {
        let mesh = sloped_strip();
        let y = mesh
            .surface_y_at(75.0, 5.0, 0.0)
            .expect("column over strip");
        assert!((y - 12.5).abs() < 1e-4, "{y}");
        // Shared diagonal edge still resolves.
        assert!(mesh.surface_y_at(50.0, 5.0, 0.0).is_some());
    }

    #[test]
    fn column_beside_the_triangles_is_outside_the_water() {
        let mesh = sloped_strip();
        assert_eq!(mesh.surface_y_at(50.0, 40.0, 0.0), None);
        let volume = WaterVolume {
            min: [0.0, -200.0, 0.0],
            max: [100.0, 50.0, 60.0],
        };
        // Inside the AABB, but no triangle covers the column.
        assert_eq!(volume.surface_y_at(Some(&mesh), 50.0, 40.0, 0.0), None);
        // Planar volumes keep the flat `max.y` surface.
        assert_eq!(volume.surface_y_at(None, 50.0, 40.0, 0.0), Some(50.0));
        assert_eq!(volume.surface_y_at(None, 150.0, 40.0, 0.0), None);
    }

    #[test]
    fn overlapping_layers_resolve_to_the_nearest_surface() {
        let low = [[0.0, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 0.0, 10.0]];
        let high = [[0.0, 30.0, 0.0], [10.0, 30.0, 0.0], [0.0, 30.0, 10.0]];
        // Vertical sheet: edge-on in XZ, covers no column.
        let sheet = [[0.0, 0.0, 1.0], [10.0, 0.0, 1.0], [10.0, 30.0, 1.0]];
        let mesh = WaterSurfaceMesh::new(vec![low, high, sheet]);
        assert_eq!(mesh.surface_y_at(2.0, 2.0, 5.0), Some(0.0));
        assert_eq!(mesh.surface_y_at(2.0, 2.0, 25.0), Some(30.0));
    }

    fn tiled_surface() -> Vec<[[f32; 3]; 3]> {
        let mut triangles = Vec::new();
        for z in 0..24 {
            for x in 0..24 {
                // Leave holes: the index must not turn an AABB into water.
                if x % 7 == 3 && z % 5 == 1 {
                    continue;
                }
                for layer in [0.0, 30.0] {
                    let point = |dx, dz| {
                        let x = x as f32 + dx;
                        [x, layer - x * 0.5, z as f32 + dz]
                    };
                    let (a, b, c, d) = (
                        point(0.0, 0.0), point(1.0, 0.0),
                        point(1.0, 1.0), point(0.0, 1.0),
                    );
                    triangles.extend([[a, b, c], [a, c, d]]);
                }
            }
        }
        triangles
    }

    #[test]
    fn indexed_water_matches_full_scan_at_edges_holes_and_overlapping_layers() {
        let indexed = WaterSurfaceMesh::new(tiled_surface());
        let reference = WaterSurfaceMesh { triangles: indexed.triangles.clone(), grid: None };
        let grid = indexed.grid.as_ref().expect("large surface has an index");
        let visited = grid.candidates(10.25, 10.25).unwrap().len()
            + grid.large_triangles().len();
        assert!(visited < indexed.triangles.len() / 8, "visited {visited} triangles");
        // Includes the outer edge, cell boundaries, shared diagonals, and holes.
        for z in -1..=49 {
            for x in -1..=49 {
                for reference_y in [-10.0, 5.0, 25.0] {
                    let (x, z) = (x as f32 * 0.5, z as f32 * 0.5);
                    assert_eq!(indexed.surface_y_at(x, z, reference_y),
                        reference.surface_y_at(x, z, reference_y), "at ({x}, {z})");
                }
            }
        }
        for x in [-0.00005, 0.99995, 1.00005, 23.99995, 24.00005] {
            assert_eq!(indexed.surface_y_at(x, 0.5, 0.0),
                reference.surface_y_at(x, 0.5, 0.0), "edge slack at {x}");
        }
    }

    #[test]
    fn large_triangles_keep_source_order_ties_without_grid_storage_explosion() {
        let mut triangles = vec![
            [[0.0, 30.0, 0.0], [24.0, 30.0, 0.0], [0.0, 30.0, 24.0]],
        ];
        triangles.extend(tiled_surface());
        let indexed = WaterSurfaceMesh::new(triangles);
        let grid = indexed.grid.as_ref().unwrap();
        assert!(grid.large_triangles().contains(&0));
        let reference = WaterSurfaceMesh { triangles: indexed.triangles.clone(), grid: None };
        // The large triangle precedes the local cell triangles in source order,
        // but is visited after them by the index. Equal distances keep that order.
        assert_eq!(indexed.surface_y_at(0.0, 0.0, 15.0), Some(30.0));
        for reference_y in [-30.0, 0.0, 15.0, 30.0] {
            assert_eq!(indexed.surface_y_at(1.25, 1.25, reference_y),
                reference.surface_y_at(1.25, 1.25, reference_y));
        }
    }

    #[test]
    fn indexed_water_handles_empty_degenerate_and_nonfinite_inputs() {
        assert_eq!(WaterSurfaceMesh::new(Vec::new()).surface_y_at(0.0, 0.0, 0.0), None);
        let mesh = WaterSurfaceMesh::new(vec![[[1.0, 2.0, 3.0]; 3]; 20]);
        assert_eq!(mesh.surface_y_at(1.0, 3.0, 2.0), None);
        assert_eq!(mesh.surface_y_at(f32::NAN, 3.0, 2.0), None);
        let invalid = WaterSurfaceMesh::new(vec![[[f32::NAN; 3]; 3]; 20]);
        assert_eq!(invalid.surface_y_at(0.0, 0.0, 0.0), None);
    }
}
