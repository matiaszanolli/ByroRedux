//! Light source component for placed lights (LIGH records).

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// A point/spot/directional light source placed in the world.
///
/// Populated from LIGH record DATA subrecord (radius, color, flags).
/// Per-frame controller animation (NiLight{Color,Dimmer,Intensity,Radius}
/// Controller from the source NIF) mutates the fields below as
/// channels sample; the renderer's light-buffer build reads
/// `final_color = color * dimmer * intensity` and uses `radius`
/// directly. Pre-fix the `dimmer` / `intensity` slots didn't exist
/// and the import dropped every NiLight animation controller on the
/// floor — every torch / lantern / plasma weapon emitted constant
/// light regardless of authored flicker / pulse / dim. See #983.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct LightSource {
    /// Light radius in Bethesda units.
    pub radius: f32,
    /// Light color (RGB, normalized 0..1).
    pub color: [f32; 3],
    /// LIGH flags (dynamic, can be carried, flicker, etc.).
    pub flags: u32,
    /// `NiLightBase.dimmer` — multiplicative scalar applied to
    /// `color` at render time. Default `1.0`. Animated by
    /// `NiLightDimmerController` (FloatTarget::LightDimmer).
    pub dimmer: f32,
    /// Animated intensity multiplier from `NiLightIntensityController`
    /// (FloatTarget::LightIntensity). Default `1.0`. Distinct from
    /// `dimmer`: dimmer is the authored NiLightBase scalar; intensity
    /// is a separate controller-driven channel that some content
    /// authors layer on top (lantern flicker rides on `intensity`
    /// while the steady `dimmer` stays constant). Renderer
    /// composes `color * dimmer * intensity`.
    pub intensity: f32,
    /// Bethesda's per-light attenuation curve exponent from the LIGH
    /// DATA subrecord (bytes 16-19). Shader formula:
    /// `atten = clamp(1 - (d/r)^k, 0, 1)` where `k = falloff_exponent`.
    /// Skyrim authors `k ≈ 1.0` (near-linear); FO3/FNV often author
    /// `k ≈ 2.0` (sharper edge). `0.0` means "use engine default" —
    /// the shader picks a sensible per-game fall-through (1.0). NIF-
    /// direct lights and procedural defaults (interior fill, sun
    /// proxies) leave the field at `0.0`.
    pub falloff_exponent: f32,
    /// #2205 — kind tag driving `GpuLight.color_type.w` (point / spot /
    /// directional). Pre-fix this field didn't exist: every producer
    /// emitted a point light regardless of the source NIF block, so
    /// e.g. Oblivion's `NiDirectionalLight` fixtures (vines, statues,
    /// hair/ear meshes) rendered as full-white omni lights over an
    /// 8,192-unit sphere instead of a directional contribution.
    /// Defaults to [`LightKind::Point`] — every producer that has no
    /// kind signal (ESM LIGH REFRs, procedural fill, sun proxies)
    /// keeps exactly the prior implicit behaviour.
    pub kind: LightKind,
    /// #2205 — unit direction (Y-up), meaningful only for `Directional`
    /// and `Spot`. Zero for `Point`/`Ambient`.
    pub direction: [f32; 3],
    /// #2205 — outer cone half-angle **in radians** for `Spot` (`0.0`
    /// for every other kind). The renderer boundary
    /// (`byroredux/src/render/lights.rs`) converts this to the cosine
    /// `GpuLight.direction_angle.w` expects — the shader never sees a
    /// raw angle, matching the file's existing LIGH-translation
    /// convention for `radius`/`falloff_exponent`.
    pub outer_angle: f32,
    /// #2250 (REN-D22-01) — canonical shadow-projection type, already
    /// decoded through the per-game boundary
    /// (`canonical_light_shadow_flags` in
    /// `byroredux/src/systems/light_anim.rs`) at spawn time. Bit layout
    /// matches [`LIGHT_FLAG_SHADOW_SPOTLIGHT`] /
    /// [`LIGHT_FLAG_SHADOW_HEMISPHERE`] / [`LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL`]
    /// for readability, but unlike [`Self::flags`] (which intentionally
    /// keeps every other bit raw and game-native) this field is never
    /// re-interpreted per game again downstream — `render/lights.rs`
    /// reads it directly (`!= 0` ⇒ casts shadows) instead of decoding
    /// [`Self::flags`] against a fixed TES5 bit layout unconditionally,
    /// which is what #2250 fixed. `0` for producers with no ESM LIGH
    /// DATA at all (NIF-direct lights, procedural fill) that instead
    /// author their shadow behavior explicitly at spawn time.
    pub shadow_flags: u32,
}

impl Default for LightSource {
    fn default() -> Self {
        Self {
            radius: 0.0,
            color: [1.0, 1.0, 1.0],
            flags: 0,
            dimmer: 1.0,
            intensity: 1.0,
            falloff_exponent: 0.0,
            kind: LightKind::Point,
            direction: [0.0, 0.0, 0.0],
            outer_angle: 0.0,
            shadow_flags: 0,
        }
    }
}

impl Component for LightSource {
    type Storage = SparseSetStorage<Self>;
}

/// Kind of a placed [`LightSource`], driving both shadow-projection
/// defaults and the `GpuLight.color_type.w` renderer contract
/// (0=point, 1=spot, 2=directional — see `GpuLight` doc comment;
/// `Ambient` has no radiating representation there and falls back to
/// the point code, a pre-existing gap tracked separately as
/// NIFAL-D3-02, not this field's concern).
///
/// #2205 — moved here (was NIF-import-local `LightKind`) so the
/// canonical ECS component can carry it without `crates/core`
/// depending on `crates/nif`; `byroredux_nif::import::LightKind` is a
/// re-export of this type, so every existing NIF-side call site is
/// source-compatible unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub enum LightKind {
    #[default]
    Point,
    Spot,
    Directional,
    Ambient,
}

// ── Shared flicker / pulse behavior bits ─────────────────────────────
//
// The values retain Skyrim's LIGH layout, but runtime animation reads them
// from `LightFlicker.animation_flags`, not directly from the raw
// `LightSource.flags`. Loaders must translate each game's source layout;
// unrelated flags (Dynamic, CanCarry, Spot/SpotShadow, etc.) remain raw.

/// `0x08` — Skyrim candle/torch flicker. Random per-frame intensity
/// noise + position jitter at the LIGH's authored period. Most vanilla
/// candles ship this bit alongside a 0.5 s period.
pub const LIGHT_FLAG_FLICKER: u32 = 0x0000_0008;
/// `0x40` — same as `FLICKER` but with the noise sampled at ~half
/// speed (slower, more "tired" flame). Used on dying torches +
/// lanterns running low on oil.
pub const LIGHT_FLAG_FLICKER_SLOW: u32 = 0x0000_0040;
/// `0x80` — smooth sinusoidal modulation. Used by glowing crystals,
/// some mage-light spells, dragon shouts.
pub const LIGHT_FLAG_PULSE: u32 = 0x0000_0080;
/// `0x100` — same as `PULSE` but at half angular velocity; ambience-style
/// lights. (Was previously mis-defined as `0x400`, which is actually the
/// unrelated Shadow-Spotlight flag in both Skyrim's and Fallout 4's LIGH
/// layout, per direct verification against xEdit/F4Edit's own record
/// definitions — see `LightFlicker::animation_flags` below.)
pub const LIGHT_FLAG_PULSE_SLOW: u32 = 0x0000_0100;

// ── Skyrim/Fallout shadow behavior bits ─────────────────────────────
//
// These are raw LIGH DATA flags, not animation behaviors. xEdit's TES5
// definitions identify the three mutually-compatible shadow projections
// at 0x400/0x800/0x1000; a light with none of them is intentionally
// unshadowed (often paired with Portal-strict at 0x2000).

/// Authored cone/spotlight shadow projection.
pub const LIGHT_FLAG_SHADOW_SPOTLIGHT: u32 = 0x0000_0400;
/// Authored hemispherical shadow projection.
pub const LIGHT_FLAG_SHADOW_HEMISPHERE: u32 = 0x0000_0800;
/// Authored omnidirectional shadow projection.
pub const LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL: u32 = 0x0000_1000;
/// Any authored shadow projection accepted by the renderer.
pub const LIGHT_FLAG_SHADOW_MASK: u32 =
    LIGHT_FLAG_SHADOW_SPOTLIGHT | LIGHT_FLAG_SHADOW_HEMISPHERE | LIGHT_FLAG_SHADOW_OMNIDIRECTIONAL;

/// Procedural light-animation behavior plus the parameters sourced from
/// the LIGH DATA subrecord (`period_secs`, `intensity_amplitude`,
/// `movement_amplitude`). Attached at spawn time to every light whose
/// game-specific source flags decode to a shared Flicker / Pulse behavior;
/// other lights skip the attachment so the
/// [`animate_lights_system`](../../../byroredux/src/systems/light_anim.rs)
/// can use the component presence as the iteration filter.
///
/// Sparse storage — most static placed lights (interior fill, sun
/// proxies) carry no flicker and don't pay the slot. Phase 17b.
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct LightFlicker {
    /// Shared animation behavior flags decoded from the source game's
    /// LIGH flag layout. This is deliberately separate from
    /// [`LightSource::flags`]: `0x400` is Shadow Spotlight in both Skyrim's
    /// and Fallout 4's LIGH layout (verified against xEdit's
    /// `wbDefinitionsTES5.pas` and F4Edit's `wbDefinitionsFO4.pas`), never
    /// slow-pulse — the two games instead diverge on whether the
    /// slow-variant bits (`0x40` Flicker-Slow, `0x100` Pulse-Slow) exist at
    /// all: Fallout 4 leaves both reserved/unused, so it only ever decodes
    /// `Flicker`/`Pulse` (see `canonical_light_animation_flags`).
    pub animation_flags: u32,
    /// LIGH FNAM `period` field — flicker / pulse cycle time in
    /// seconds. Defaults to `0.5` when the LIGH record was
    /// truncated (pre-Skyrim layouts that ship only the 16-byte
    /// DATA header).
    pub period_secs: f32,
    /// LIGH FNAM `intensity_amplitude` — percent variation around
    /// the authored intensity (`0.25` = ±25%). Multiplied into the
    /// noise / sine output before adding to the base intensity.
    pub intensity_amplitude: f32,
    /// LIGH FNAM `movement_amplitude` — position jitter amplitude
    /// in Bethesda units. The animator offsets the light's local
    /// translation by a noise vector scaled by this value, then
    /// restores to `base_translation` on the next tick before
    /// re-jittering — jitter never accumulates.
    pub movement_amplitude: f32,
    /// Cached un-jittered local translation captured at spawn.
    /// The animator computes `transform.translation =
    /// base_translation + noise * movement_amplitude` each frame.
    pub base_translation: [f32; 3],
    /// Per-entity phase offset in seconds so a roomful of identical
    /// candles don't flicker in lockstep. Seeded from the light's
    /// EntityId at spawn (cheap, deterministic per session).
    pub phase_offset_secs: f32,
}

impl Component for LightFlicker {
    type Storage = SparseSetStorage<Self>;
}
