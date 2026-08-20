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
use crate::ecs::storage::Component;

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
}

impl WaterKind {
    /// `true` when the renderer should fire a refraction ray below
    /// the surface. Waterfalls are opaque enough that the refraction
    /// ray is wasted budget.
    #[inline]
    pub fn refracts(self) -> bool {
        !matches!(self, WaterKind::Waterfall)
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
    /// Authored `BSWaterShaderProperty.water_shader_flags` for mesh-bound
    /// water. Zero means the legacy property had no dedicated flag word and
    /// keeps the renderer's compatibility defaults.
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
    /// Normal-map index in the bindless texture array. Both wave
    /// layers sample this; the shader applies a different scale +
    /// scroll vector to each. `u32::MAX` = solid-colour water.
    pub normal_map_index: u32,
    /// Bindless indices for Skyrim+/FO4 authored noise layers NAM2–4.
    /// `u32::MAX` means procedural fallback; the loader fills missing
    /// layers from `normal_map_index` when a legacy record has no NAM paths.
    pub noise_map_indices: [u32; 3],
    /// World-space scroll vectors for the three wave layers (xy = m/s).
    /// For `Calm`, the cell loader preserves authored vectors when present.
    /// For `River` / `Rapids` / `Waterfall`, vector 0 is
    /// `flow.direction * flow.speed`, vector 1 is a perpendicular
    /// shear at half speed, and vector 2 carries the authored third layer.
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
    /// Authored normal-amplitude multipliers for NAM2/NAM3/NAM4. A value of
    /// one is the neutral legacy fallback.
    pub noise_amplitude_scales: [f32; 3],
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
    /// Authored flow-map tile scale. One is neutral; zero means legacy
    /// records without a dedicated flow-map field.
    pub flowmap_scale: f32,
    /// Starfield per-channel color-absorption ranges in world units. A zero
    /// triplet is the canonical sentinel for pre-Starfield records and keeps
    /// their legacy scalar fog response unchanged.
    pub absorption_ranges: [f32; 3],
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
    /// Direct-sun glint exponent from WATR `Sun Specular Power`.
    /// Larger values produce a smaller, tighter highlight. This is an
    /// exponent, not an intensity multiplier.
    pub sun_specular_power: f32,
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
            shader_flags: 0,
            shallow_color: [0.10, 0.32, 0.38],
            deep_color: [0.02, 0.06, 0.10],
            underwater_color: [0.02, 0.06, 0.10],
            fog_near: 80.0,
            fog_far: 600.0,
            underwater_fog_near: 0.0,
            underwater_fog_far: 0.0,
            underwater_fog_amount: 1.0,
            opacity: 0.88,
            fresnel_f0: 0.02,
            reflectivity: 0.85,
            normal_map_index: u32::MAX,
            noise_map_indices: [u32::MAX; 3],
            scroll_a: [0.020, 0.011],
            scroll_b: [-0.014, 0.025],
            scroll_c: [0.0, 0.0],
            uv_scale_a: 1.0 / 256.0,
            uv_scale_b: 1.0 / 700.0,
            uv_scale_c: 1.0 / 512.0,
            noise_amplitude_scales: [1.0; 3],
            normal_magnitude: 1.0,
            above_water_fog_amount: 1.0,
            depth_weights: [1.0; 4],
            effect_controls: [0.0, 0.0, 1.0, 1.0],
            specular_magnitude: 1.0,
            flowmap_scale: 1.0,
            absorption_ranges: [0.0; 3],
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
            wave_amplitude: 0.05,
            wave_frequency: 0.6,
            sun_specular_power: 50.0,
            source_form: 0,
            reflection_tint: [0.65, 0.70, 0.75],
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
    /// currents (rivers) keep Y=0. Set from the WATR `wind_direction`
    /// angle after the Z→Y swizzle in `cell_loader/water.rs`.
    pub direction: [f32; 3],
    /// World units per second, always inside
    /// [`WaterFlow::SPEED_MIN`]`..=`[`WaterFlow::SPEED_MAX`] when built
    /// through [`WaterFlow::new`] / [`WaterFlow::for_kind`]. Typical:
    /// 0.5 (calm river) … 8.0 (whitewater rapids) … 25.0 (Tamriel-tall
    /// waterfall sheet).
    pub speed: f32,
}

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
            WaterKind::Calm | WaterKind::River => Self::SPEED_MIN,
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
    /// The water material driving underwater FX. `None` when the
    /// entity is above any water plane or out of all volumes.
    pub material: Option<WaterMaterial>,
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
/// (`submerged_fraction == 0`, `material == None`) for one transition so
/// the buoyancy phase can restore the body's authored damping exactly
/// once; bodies that have never touched water carry no component at all.
#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct WaterContact {
    /// Surface Y minus the body's centre Y. Positive = centre below the
    /// surface. Mirrors [`SubmersionState::depth`].
    pub depth: f32,
    /// Fraction of the body's vertical span below the surface, `0.0`
    /// (dry) … `1.0` (fully under), from the collider AABB vs the water
    /// column. The displaced-volume proxy buoyancy integrates against.
    pub submerged_fraction: f32,
    /// `true` once the whole body (AABB top) is below the surface — the
    /// drowning / fully-submerged-FX gate. (Drowning is not yet wired.)
    pub head_submerged: bool,
    /// The current acting on this body, when it sits in flowing water
    /// (river / rapids / waterfall). `None` for calm water. Carried here
    /// so the physics current force and gameplay can consume it without
    /// re-querying the plane.
    pub flow: Option<WaterFlow>,
    /// The water material driving this body's underwater FX / surface
    /// interaction. `None` when the body is out of all water volumes.
    pub material: Option<WaterMaterial>,
}

impl Component for WaterContact {
    type Storage = SparseSetStorage<Self>;
}
