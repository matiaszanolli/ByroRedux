//! Light source component for placed lights (LIGH records).

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;
use crate::lighting::{Emitter, EmitterKind, VisibilityMask};

/// Backward-compatible name for the canonical emitter kind.
pub type LightKind = EmitterKind;

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
    /// Unit-explicit emitter state. Legacy radius, colour, cone, attenuation,
    /// and shadow-map flags are resolved into this value at import time.
    pub emitter: Emitter,
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
    /// Canonicalized legacy projection bits retained only for diagnostics and
    /// round-trip tooling. Rendering consumes `emitter.visibility` and never
    /// interprets this field.
    pub shadow_flags: u32,
}

impl Default for LightSource {
    fn default() -> Self {
        Self {
            emitter: Emitter::default(),
            flags: 0,
            dimmer: 1.0,
            intensity: 1.0,
            shadow_flags: 0,
        }
    }
}

impl LightSource {
    /// Legacy-format constructor. Call this at an ESM/NIF boundary; runtime
    /// systems and the renderer should only inspect [`Self::emitter`].
    #[allow(clippy::too_many_arguments)]
    pub fn from_legacy_world_units(
        radius: f32,
        color: [f32; 3],
        flags: u32,
        falloff_exponent: f32,
        kind: LightKind,
        direction: [f32; 3],
        outer_angle: f32,
        shadow_flags: u32,
    ) -> Self {
        let visibility = VisibilityMask::for_legacy_projection(shadow_flags != 0);
        Self {
            emitter: Emitter::from_legacy_world_units(
                radius,
                color,
                kind,
                direction,
                outer_angle,
                falloff_exponent,
                visibility,
            ),
            flags,
            dimmer: 1.0,
            intensity: 1.0,
            shadow_flags,
        }
    }
}

impl Component for LightSource {
    type Storage = SparseSetStorage<Self>;
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

/// #2439 (NIFAL-D2-01) — the light-SHAPE bit: this LIGH is authored as a
/// cone-emitting spotlight, independent of whether it casts a shadow.
/// Verified directly against xEdit's `Core/wbDefinitionsTES5.pas`
/// (dev-4.1.6): bit 9 is named `'Spot Light'`, bit 10 (0x400,
/// [`LIGHT_FLAG_SHADOW_SPOTLIGHT`] below) is the DISTINCT
/// `'Shadow Spotlight'` bit — a shadow-projection-technique choice, not
/// a light-shape signal. The two are easy to conflate (a shadow-casting
/// spotlight sets both), but treating 0x400 as "is this a spotlight" —
/// the mistake `translate_light`'s predecessor state made by never
/// deriving `LightKind::Spot` at all — would also misclassify any
/// non-cone light some content sets 0x400 on for shadow-technique
/// reasons alone. `byroredux/src/systems/light_anim.rs::translate_light`
/// is the sole consumer.
pub const LIGHT_FLAG_SPOT: u32 = 0x0000_0200;

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
