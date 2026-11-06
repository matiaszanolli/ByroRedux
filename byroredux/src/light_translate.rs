//! ESM LIGH → canonical light translation (#6717: moved verbatim out of `systems::light_anim`).
//!
//! The per-game decode of a LIGH record's flags, falloff exponent and cone geometry. Both the
//! cell loader and the render collector call these, so they sit in the translation tier rather
//! than under the per-frame `systems`.

use byroredux_core::ecs::{
    EmitterKind, LIGHT_FLAG_FLICKER, LIGHT_FLAG_FLICKER_SLOW, LIGHT_FLAG_PULSE,
    LIGHT_FLAG_PULSE_SLOW, LIGHT_FLAG_SHADOW_MASK, LIGHT_FLAG_SHADOW_SPOTLIGHT, LIGHT_FLAG_SPOT,
};
use byroredux_plugin::esm::reader::GameKind;

const SHARED_LIGHT_ANIMATION_MASK: u32 =
    LIGHT_FLAG_FLICKER | LIGHT_FLAG_FLICKER_SLOW | LIGHT_FLAG_PULSE | LIGHT_FLAG_PULSE_SLOW;

/// Decode a game's raw LIGH flags into the shared runtime animation behavior.
///
/// The bits are not portable source data. `0x400` (Shadow Spotlight) is
/// never an animation flag in any game — verified directly against xEdit's
/// `wbDefinitionsTES5.pas` and F4Edit's `wbDefinitionsFO4.pas`, both of
/// which agree `0x400` = Shadow Spotlight, not slow-pulse. Fallout 4's LIGH
/// layout separately leaves the slow-variant bits themselves undefined —
/// `0x40` and `0x100` are reserved/unknown there (F4Edit shows no
/// Flicker-Slow or Pulse-Slow flag for Fallout 4 at all), so it only ever
/// decodes `Flicker`/`Pulse`. Keeping this conversion at the game boundary
/// prevents rendering-only flags from becoming continuous whole-scene
/// light animation.
///
/// #2251 (REN-D22-02) — `GameKind::Fallout76` verified directly against
/// FO76Edit's `wbDefinitionsFO76.pas`: `0x08`/`0x80` (Flicker/Pulse) match
/// Skyrim exactly, and `0x40`/`0x100` are unnamed ("Unknown 6"/"Unknown 8")
/// there too — the identical gap Fallout 4 has, so it shares Fallout 4's
/// arm rather than the Skyrim-shared default.
///
/// `GameKind::Starfield` is its own arm, on the same strict-by-default
/// footing as `Fallout4`/`Fallout76` above: its flags word is real, but none
/// of its bit *meanings* are evidenced.
///
/// #3987 corrected the premise this arm used to rest on. It previously read
/// "the bytes a Flags field would occupy are an undifferentiated `wbUnknown`
/// block", i.e. there is no field at all. That is contradicted by our own
/// `DAT2` decoder (`esm/cell/support.rs`, #1567), whose offset table is
/// introduced as verified against `wbDefinitionsSF1.pas` and whose third row
/// is `{12} UInt16 Flags`. It is also contradicted by the data: across all
/// 1,575 LIGH records in `Starfield.esm`, the u16 at `DAT2+12` takes 16
/// distinct sparse values (union of set bits `0x17F1`) while the u16 at
/// `+14` is zero in every single record — a populated 16-bit bitfield
/// followed by two unused bytes, exactly as the decoder's table describes,
/// and not the u32 Skyrim's `DATA` carries.
///
/// What is still absent is the bit *legend*. The observed bit set does not
/// match Skyrim's either: Skyrim's `0x02`/`0x04`/`0x08` (Can-Be-Carried /
/// Negative / Flicker) never appear in any Starfield record, while an
/// unnamed `0x10` does. So Skyrim's `Flicker`/`Pulse` positions cannot be
/// transferred, and an unnamed bit must not decode into *motion* — a light
/// that visibly pulses when the record never asked for it is an obvious,
/// reported artifact. `0` remains the only defensible value here, now for
/// the accurate reason. Contrast the shadow sibling below, whose opposite
/// default applies to Starfield precisely because the field is real.
pub(crate) fn canonical_light_animation_flags(game: GameKind, source_flags: u32) -> u32 {
    let source_animation_mask = match game {
        GameKind::Fallout4 | GameKind::Fallout76 => LIGHT_FLAG_FLICKER | LIGHT_FLAG_PULSE,
        GameKind::Starfield => 0,
        _ => SHARED_LIGHT_ANIMATION_MASK,
    };
    source_flags & source_animation_mask
}

/// Decode legacy projection flags for diagnostics. Runtime visibility is
/// always material-aware and full-scene, independent of this word; the
/// shadow-policy discussion below records the historical decoding rationale.
///
/// `LIGHT_FLAG_SHADOW_MASK` (`0x400`/`0x800`/`0x1000`) is directly verified
/// against TES5's LIGH layout (`light.rs`'s doc comment), and against
/// FO76Edit's `wbDefinitionsFO76.pas` too (#2251) — both name all three
/// bits identically to Skyrim at the same positions, high confidence. No
/// divergence has been identified for Oblivion / FO3 / FNV's own LIGH
/// layouts either, so those stay on the shared mask rather than silently
/// decoding zero, matching `canonical_light_animation_flags`'s own
/// default-to-shared-mask treatment of every game it hasn't specifically
/// gated. A verified per-game divergence gets its own `match` arm here,
/// exactly like `GameKind::Fallout4` does above.
///
/// Preserve the verified source shadow bits for diagnostics. Since #4557,
/// legacy projection flags no longer select the renderer's visibility mask;
/// that policy is shared by all legacy lights. Starfield's Light Type enum
/// is the one explicit translation into a canonical shadow-technique bit.
pub(crate) fn canonical_light_shadow_flags(
    game: GameKind,
    source_flags: u32,
    starfield_light_type: u8,
) -> u32 {
    let authored = source_flags & LIGHT_FLAG_SHADOW_MASK;
    // #4424's evidence pass — Starfield's u16 flags carry NO
    // shadow-projection bits under the now-published xEdit legend; the
    // Shadow-vs-NonShadow choice lives in the DAT2+56 Light Type enum
    // (`1 = Shadow Spotlight`, `2 = NonShadow Spotlight`). Map that enum
    // onto the canonical technique bits: an authored Shadow Spotlight
    // traces spot-cone shadows; a NonShadow Spotlight keeps the cone but
    // carries no spotlight-shadow technique.
    if game == GameKind::Starfield && starfield_light_type == 1 {
        return LIGHT_FLAG_SHADOW_SPOTLIGHT;
    }
    // Projection flags are retained as authored diagnostics. Visibility
    // policy is resolved separately and does not synthesize legacy bits.
    authored
}

/// The geometry half of a canonical [`LightSource`] derived from an ESM
/// LIGH `DATA`/`DAT2` record: [`LightSource::kind`], [`LightSource::
/// direction`], [`LightSource::outer_angle`]. Sibling boundary to
/// [`canonical_light_shadow_flags`] / [`canonical_light_animation_flags`]
/// above — same "translate once at the game boundary, not per-producer"
/// shape, closing the gap #2205 (NIFAL-D3-01) left for every ESM-sourced
/// light: only the direct-`NiPointLight`/`NiSpotLight` NIF-import path
/// populated these fields; every ESM-LIGH producer hand-copied
/// radius/color/flags/falloff and hard-defaulted the rest, so no
/// ESM-placed spotlight could ever render as anything but a full
/// omnidirectional point light. #2439 / NIFAL-D2-01.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LightGeometry {
    pub kind: EmitterKind,
    pub direction: [f32; 3],
    pub outer_angle: f32,
}

/// See [`LightGeometry`]. `ref_rot` is the placing REFR's own rotation
/// (already converted to the engine's Y-up quaternion convention via
/// `euler_zup_to_quat_yup_refr`) — these ESM-sourced lights carry no
/// per-light NIF node of their own, so the REFR's placement rotation is
/// the only orientation signal available for the emitted cone.
pub(crate) fn translate_light(
    ld: &byroredux_plugin::esm::cell::LightData,
    game: GameKind,
    ref_rot: byroredux_core::math::Quat,
) -> LightGeometry {
    // xEdit dev-4.1.6 Core/wbDefinitions{TES4,FNV,TES5,FO4,FO76,SF1}.pas:
    // "Shadow Spotlight" (0x400) is itself a cone; vanilla Skyrim/FO4 do
    // NOT also set 0x200. FO4/FO76 call 0x200 "Unknown 9" and move the
    // non-shadow cone to 0x4000. Starfield uses DAT2+56's explicit enum;
    // its 0x200 means "Focus Spotlight Beam", not the emitter's shape.
    // Keep these raw-format differences here. "NonShadow" describes the
    // source engine's shadow allocation, not our material-aware visibility.
    let is_spot = match game {
        GameKind::Starfield => matches!(ld.starfield_light_type, 1 | 2),
        GameKind::Fallout4 | GameKind::Fallout76 => {
            const NON_SHADOW_SPOTLIGHT: u32 = 0x4000;
            ld.flags & (LIGHT_FLAG_SHADOW_SPOTLIGHT | NON_SHADOW_SPOTLIGHT) != 0
        }
        GameKind::Oblivion | GameKind::Fallout3NV | GameKind::Skyrim => {
            ld.flags & (LIGHT_FLAG_SPOT | LIGHT_FLAG_SHADOW_SPOTLIGHT) != 0
        }
    };
    if !is_spot {
        return LightGeometry {
            kind: EmitterKind::Point,
            direction: [0.0, 0.0, 0.0],
            outer_angle: 0.0,
        };
    }

    // Gamebryo's `NiSpotLight` model direction is `(1, 0, 0)` — the
    // FIRST column of the world rotation matrix, NOT local -Z (verified
    // directly against `gamebryo-v32/Include/NiSpotLight.h`: "The model
    // location of the light is (0,0,0)... The model direction of the
    // light is (1,0,0). The world direction is the first column of the
    // world rotation matrix."). Identical convention to
    // `NiDirectionalLight`, already validated for XCLL directional
    // lighting (`euler_zup_to_quat_yup_tests.rs`,
    // `matches_refr_placement_rotation_of_model_direction`) — `ref_rot`
    // is the same Z-up-Euler→Y-up-quaternion conversion, confirmed there
    // to reproduce the correct engine-space direction when applied to
    // the `(1, 0, 0)` model direction.
    let direction = (ref_rot * byroredux_core::math::Vec3::new(1.0, 0.0, 0.0)).to_array();

    // xEdit's FOV field (`Core/wbDefinitionsTES5.pas`, dev-4.1.6:
    // `wbFloat('FOV', ..., wbNormalizeToRange(0.001, 160), 90)`) is the
    // FULL cone angle in degrees, default 90°. The legacy constructor
    // accepts the outer cone HALF-angle in radians — halve
    // before converting. `0.0` means the DATA subrecord was too short to
    // carry a FOV at all (see `LightData::fov_degrees`), not an authored
    // zero-degree cone, so it falls back to xEdit's own default rather
    // than producing a degenerate zero-width cone.
    let fov_degrees = if ld.fov_degrees > 0.0 {
        ld.fov_degrees
    } else {
        90.0
    };
    let outer_angle = (fov_degrees * 0.5).to_radians();

    LightGeometry {
        kind: EmitterKind::Spot,
        direction,
        outer_angle,
    }
}

/// Resolve the LIGH `falloff_exponent` sentinel (`0.0`, the "field absent"
/// encoding documented on [`byroredux_plugin::esm::cell::LightData::
/// falloff_exponent`]) to the per-layout default the doc contract names:
/// `1.0` on the Skyrim+ 48-byte layout, `2.0` on the pre-Skyrim 32-byte one
/// (FO3/FNV's common quadratic `k ≈ 2`; Oblivion's LIGH shares that
/// generation). An authored positive value passes through verbatim. Lives
/// beside its `canonical_light_*` siblings so the per-game knowledge enters
/// once, at the ESM-light boundary, instead of one shared fallback reaching
/// every game (`lighting.rs`'s own `1.0` remains as the inert last-resort
/// net for non-ESM producers).
pub(crate) fn canonical_light_falloff_exponent(game: GameKind, source: f32) -> f32 {
    if source.is_finite() && source > 0.0 {
        return source;
    }
    match game {
        GameKind::Oblivion | GameKind::Fallout3NV => 2.0,
        _ => 1.0,
    }
}
