//! Cell lighting types, extracted from ../mod.rs (#5805 type-hub split).
//!
//! Types: `CellLighting` (the XCLL family shared across games),
//! `StarfieldLighting` + `decode_starfield_lighting_tail` (Starfield's
//! distinct 108-byte XCLL tail, shared with LGTM).

/// Interior cell lighting from XCLL subrecord.
///
/// The base fields (ambient through fog_far) are shared across all games
/// (Oblivion, FO3, FNV, Skyrim). Skyrim extends XCLL to 92 bytes with
/// directional ambient, fog far color, light fade distances, etc. — these
/// are stored in the `Option` fields below, which are `None` for pre-Skyrim
/// games.
#[derive(Debug, Clone)]
pub struct CellLighting {
    /// Ambient light color (RGB 0–1).
    pub ambient: [f32; 3],
    /// Directional light color (RGB 0–1).
    pub directional_color: [f32; 3],
    /// XCLL Directional Rotation XY: horizontal azimuth, in radians.
    pub directional_azimuth: f32,
    /// XCLL Directional Rotation Z: cyclic elevation, in radians. Vanilla
    /// commonly authors 270 degrees for a light source overhead.
    pub directional_elevation: f32,
    /// Fog color (RGB 0–1), from XCLL bytes 8-10.
    pub fog_color: [f32; 3],
    /// Fog near distance (game units), from XCLL bytes 12-15.
    pub fog_near: f32,
    /// Fog far distance (game units), from XCLL bytes 16-19.
    pub fog_far: f32,
    // ── Skyrim+ extended fields (92-byte XCLL) ──────────────────────
    /// Directional light fade multiplier (bytes 28-31).
    pub directional_fade: Option<f32>,
    /// Fog clip distance (bytes 32-35).
    pub fog_clip: Option<f32>,
    /// Fog power exponent (bytes 36-39).
    pub fog_power: Option<f32>,
    /// Fog far color (RGB 0–1, bytes 72-74). Separate from near fog color.
    pub fog_far_color: Option<[f32; 3]>,
    /// Maximum fog opacity (bytes 76-79).
    pub fog_max: Option<f32>,
    /// Light fade begin distance (bytes 80-83).
    pub light_fade_begin: Option<f32>,
    /// Light fade end distance (bytes 84-87).
    pub light_fade_end: Option<f32>,
    /// Directional ambient cube — one RGB triplet per cardinal axis
    /// (bytes 40-63 as 6×RGBA). The face order follows Creation Kit /
    /// niflib convention: `[+X, -X, +Y, -Y, +Z, -Z]`. Alpha bytes are
    /// discarded — vanilla Skyrim stores zero there anyway. Drives the
    /// per-cell ambient probe: ±Z asymmetry is what makes cave floors
    /// read warm while ceilings read cool without a dedicated IBL pass.
    /// Pre-#367 these 24 bytes were parsed-past-but-dropped, so every
    /// Skyrim interior used a single `ambient` color. `None` on pre-
    /// Skyrim XCLL (36 / 40 bytes).
    pub directional_ambient: Option<[[f32; 3]; 6]>,
    /// Specular color (bytes 64-67 as RGBA → RGB triplet). `None` on
    /// pre-Skyrim XCLL.
    pub specular_color: Option<[f32; 3]>,
    /// Specular alpha (byte 67). Stored separately so consumers can
    /// decide whether the RGBA packing was intentional or padding.
    pub specular_alpha: Option<f32>,
    /// Fresnel power exponent (bytes 68-71). `None` on pre-Skyrim XCLL.
    /// Skyrim-only — `None` on Starfield (its XCLL has no fresnel field).
    pub fresnel_power: Option<f32>,
    /// Skyrim+ per-field lighting-template inheritance mask (bytes 88-91).
    /// `None` on pre-Skyrim and Starfield XCLL layouts. Each set bit means
    /// the matching local field group must be replaced from the cell's
    /// `LTMP`/`LGTM` record before consumers see the lighting values.
    pub inheritance_flags: Option<u32>,
    /// Starfield-specific XCLL fields. `Some` only for Starfield's
    /// 108-byte XCLL, whose bytes 40-107 are a volumetric height-fog
    /// model distinct from the Skyrim ambient-cube layout (#1293). When
    /// `Some`, `directional_ambient` / `specular_*` / `fresnel_power` are
    /// `None` (Starfield doesn't author them) and `directional_fade` is
    /// `None` (Starfield reuses byte 28 as [`StarfieldLighting::gravity_scale`]).
    pub starfield: Option<StarfieldLighting>,
}

/// Starfield-only XCLL fields (offsets within the 108-byte XCLL). The
/// shared 0-19 colours/fog and the 32/36 fog-clip/power decode onto the
/// base [`CellLighting`] fields; the 40-55 fog-far-colour / fog-max /
/// light-fade map onto `fog_far_color` / `fog_max` / `light_fade_*`.
/// Everything below is Starfield's distinct volumetric height-fog model
/// with no Skyrim equivalent. Layout per xEdit SF1 `wbDefinitionsSF1.pas`
/// (`wbStruct(XCLL, 'Lighting', ...)`), confirmed byte-exact against
/// vanilla `Starfield.esm` (the `interior_type` enum 0-4 and the 3-byte
/// trailing `Unused` pad both match the on-disk distribution). #1293.
#[derive(Debug, Clone)]
pub struct StarfieldLighting {
    /// Gravity scale (byte 28). Skyrim stores `Directional Fade` here.
    pub gravity_scale: f32,
    /// "Unknown" RGB colour (bytes 56-58; 4th byte unused). xEdit labels
    /// this field `Unknown`.
    pub unknown_color: [f32; 3],
    /// Near-fog height midpoint (bytes 60-63).
    pub near_height_mid: f32,
    /// Near-fog height range (bytes 64-67).
    pub near_height_range: f32,
    /// High-altitude near-fog colour (bytes 68-70).
    pub fog_color_high_near: [f32; 3],
    /// High-altitude far-fog colour (bytes 72-74).
    pub fog_color_high_far: [f32; 3],
    /// High-density fog scale (bytes 76-79).
    pub high_density_scale: f32,
    /// Fog near scale (bytes 80-83).
    pub fog_near_scale: f32,
    /// Fog far scale (bytes 84-87).
    pub fog_far_scale: f32,
    /// High-altitude fog near scale (bytes 88-91).
    pub fog_high_near_scale: f32,
    /// High-altitude fog far scale (bytes 92-95).
    pub fog_high_far_scale: f32,
    /// Far-fog height midpoint (bytes 96-99).
    pub far_height_mid: f32,
    /// Far-fog height range (bytes 100-103).
    pub far_height_range: f32,
    /// Interior type (byte 104). Raw enum — per xEdit SF1:
    /// 0=Interior, 1=Ship Cell, 2=Space Cell, 3=PackIn,
    /// 4=Instanceable Interior. Captured raw; downstream mapping is
    /// consumer work (mirrors `response_type`). The 3
    /// trailing bytes (105-107) are uninitialized CK pad — discarded.
    pub interior_type: u8,
}

/// The Starfield tail fields that land on a carrier's *base* lighting
/// `Option`s (see [`CellLighting`]'s fog/lights group and `LgtmRecord`'s
/// mirrors) — returned by [`decode_starfield_lighting_tail`] beside the
/// height-fog model.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StarfieldTailFields {
    pub fog_clip: f32,
    pub fog_power: f32,
    pub fog_far_color: [f32; 3],
    pub fog_max: f32,
    pub light_fade_begin: f32,
    pub light_fade_end: f32,
}

/// #5346 — decode Starfield's shared lighting tail (bytes 28..108 of an
/// `XCLL` body or an `LGTM` `DATA`) in ONE place. The two carriers are
/// byte-identical from 28 on (#5002 fixed LGTM by hand-copying the XCLL
/// offset table, leaving two 20-field decoders to drift — and the
/// byte-28/56-59 labels are already disputed between xEdit's SF1 LGTM and
/// XCLL definitions, so any future correction must land once). Callers
/// guarantee `tail.len() >= 80` (the ≥108 record gate); short input
/// degrades to zeros exactly like the previous per-site `or_default`
/// reads. Layout per xEdit SF1 `wbDefinitionsSF1.pas` (#1293 / #5002).
pub(crate) fn decode_starfield_lighting_tail(
    tail: &[u8],
) -> (StarfieldTailFields, StarfieldLighting) {
    let mut r = crate::esm::sub_reader::SubReader::new(tail);
    let gravity_scale = r.f32_or_default(); // 28
    let fields = StarfieldTailFields {
        fog_clip: r.f32_or_default(),                     // 32 Fog Clip Distance
        fog_power: r.f32_or_default(),                    // 36 Fog Power
        fog_far_color: r.rgb_color().unwrap_or([0.0; 3]), // 40 Fog Color Far
        fog_max: r.f32_or_default(),                      // 44 Fog Max
        light_fade_begin: r.f32_or_default(),             // 48 Light Fade Begin
        light_fade_end: r.f32_or_default(),               // 52 Light Fade End
    };
    let starfield = StarfieldLighting {
        gravity_scale,
        unknown_color: r.rgb_color().unwrap_or([0.0; 3]), // 56 Unknown
        near_height_mid: r.f32_or_default(),              // 60
        near_height_range: r.f32_or_default(),            // 64
        fog_color_high_near: r.rgb_color().unwrap_or([0.0; 3]), // 68
        fog_color_high_far: r.rgb_color().unwrap_or([0.0; 3]), // 72
        high_density_scale: r.f32_or_default(),           // 76
        fog_near_scale: r.f32_or_default(),               // 80
        fog_far_scale: r.f32_or_default(),                // 84
        fog_high_near_scale: r.f32_or_default(),          // 88
        fog_high_far_scale: r.f32_or_default(),           // 92
        far_height_mid: r.f32_or_default(),               // 96
        far_height_range: r.f32_or_default(),             // 100
        interior_type: r.u8_or_default(),                 // 104 (105-107 pad)
    };
    (fields, starfield)
}
