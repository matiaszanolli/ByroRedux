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
//! - Flat plane only. We do **not** displace BLAS geometry per frame.
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
    /// Colour seen looking down through shallow water — blended with
    /// the refraction-ray hit colour via depth-through-water.
    pub shallow_color: [f32; 3],
    /// Colour seen looking down through deep water (refraction ray
    /// distance ≥ [`Self::fog_far`]).
    pub deep_color: [f32; 3],
    /// Distance through water (world units) at which the shallow
    /// colour reaches 50% mix.
    pub fog_near: f32,
    /// Distance through water at which the deep colour fully takes
    /// over (refraction tint converges to `deep_color`).
    pub fog_far: f32,
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
    /// Normal-map index in the bindless texture array. Both wave
    /// layers sample this; the shader applies a different scale +
    /// scroll vector to each. `u32::MAX` = solid-colour water.
    pub normal_map_index: u32,
    /// World-space scroll vectors for the two wave layers (xy = m/s).
    /// For `Calm`, the cell loader picks two non-parallel arbitrary
    /// vectors. For `River` / `Rapids` / `Waterfall`, vector 0 is
    /// `flow.direction * flow.speed`, vector 1 is a perpendicular
    /// shear at half speed.
    pub scroll_a: [f32; 2],
    pub scroll_b: [f32; 2],
    /// UV scale for each normal-map layer. Detail tile size — small
    /// (~1/200 world units) for choppy water, large (~1/800) for
    /// slow swells.
    pub uv_scale_a: f32,
    pub uv_scale_b: f32,
    /// Foam intensity multiplier. 0 = no foam anywhere; 1 = full
    /// rapids / waterfall whitewater. Cell loader sets from
    /// [`WaterKind`].
    pub foam_strength: f32,
    /// Shoreline foam falloff distance (world units). Foam at scene
    /// geometry within this distance below the water surface; fades
    /// to zero past it. ~30 wu matches Skyrim's vanilla shoreline.
    pub shoreline_width: f32,
    /// Refraction IOR. 1.33 = clean water; bumping up to 1.5 for
    /// stylised reads or thick visc fluid. Glass at 1.5.
    pub ior: f32,
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
            shallow_color: [0.10, 0.32, 0.38],
            deep_color: [0.02, 0.06, 0.10],
            fog_near: 80.0,
            fog_far: 600.0,
            fresnel_f0: 0.02,
            reflectivity: 0.85,
            normal_map_index: u32::MAX,
            scroll_a: [0.020, 0.011],
            scroll_b: [-0.014, 0.025],
            uv_scale_a: 1.0 / 256.0,
            uv_scale_b: 1.0 / 700.0,
            foam_strength: 0.0,
            shoreline_width: 32.0,
            ior: 1.33,
            source_form: 0,
            reflection_tint: [0.65, 0.70, 0.75],
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
/// - swim resistance for actors caught in the current (gameplay
///   layer — not wired in this initial cut).
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
    /// World units per second. Typical: 0.5 (calm river) … 8.0
    /// (whitewater rapids) … 25.0 (Tamriel-tall waterfall sheet).
    pub speed: f32,
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
