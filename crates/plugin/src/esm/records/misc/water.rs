//! Water record (`WATR`) and decoded water parameters.

use super::super::common::{read_f32_at, read_lstring_or_zstring, read_u32_at, read_zstring};
use crate::esm::reader::SubRecord;

/// Water record — referenced by `CELL.XCWT` (water type form ID on a
/// cell). Pre-fix every XCWT reference dangled at cell load.
///
/// Carries the engine-decoded subset of the `DATA` / `DNAM` payload
/// that the water shader actually consumes (colours + fog + Fresnel +
/// scroll). The full per-game byte layout differs across Oblivion /
/// FO3 / FNV / Skyrim+ and isn't fully decoded here: we capture the
/// raw DNAM bytes alongside the structured `params` so a later, more
/// precise per-game parser can keep the storage shape stable while
/// improving accuracy.
///
/// **Confident decode** (cross-checked against UESP CSWiki for
/// Oblivion + FO3 + FNV WATR.DATA, plus the Gamebryo 2.3 water
/// material header):
///
/// - Oblivion DATA: 102 bytes, layout starting with 11 × f32 +
///   3 × u32-packed RGBA8.
/// - FO3 / FNV DATA: 196-byte extension of the Oblivion layout —
///   first 60 bytes preserve the FNV/FO3-compatible prefix.
///
/// **Best-effort decode** for Skyrim+ DNAM (252+ bytes) — the field
/// names are documented but the offsets vary between 1.5 / 1.6
/// patches; we read what we can and leave the rest at default.
#[derive(Debug, Clone, Default)]
pub struct WatrRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Diffuse / noise texture path (`TNAM`). Most FNV water types ship
    /// a `textures\water\*.dds` here.
    pub texture_path: String,
    /// Decoded water shader / shading params. Fields are at their
    /// per-spec defaults when the source record omits a sub-record or
    /// when the byte layout doesn't match the parser's expectations.
    pub params: WaterParams,
    /// GNAM-resolved noise-texture form IDs (Skyrim+ — 3 slots,
    /// `[0; 3]` when the record omits GNAM or only fills a prefix).
    /// References to `NOIS`-style records that the shader samples
    /// for displacement layers when the bindless `TNAM` texture is
    /// unavailable.
    pub noise_textures: [u32; 3],
    /// Raw DNAM bytes — preserved so a future per-game-precise
    /// decoder can re-parse without re-walking the ESM. ~252+ bytes
    /// on Skyrim, ~196 on FNV/FO3, ~102 on Oblivion. Empty when the
    /// record omits DNAM (or pre-FNV DATA is used instead — see
    /// `raw_data`).
    pub raw_dnam: Vec<u8>,
    /// Raw DATA bytes (Oblivion / FO3 / FNV path). Same rationale
    /// as [`Self::raw_dnam`] — preserved for future re-decode.
    pub raw_data: Vec<u8>,
}

/// Engine-side water shader parameter view. The renderer's
/// `WaterMaterial` is derived from this by the cell loader (the
/// loader applies the `WaterKind` heuristic + scroll-vector synthesis
/// from `wind_speed` / `wind_direction`).
///
/// Colours are stored as **linear-RGB f32** (per
/// [`feedback_color_space`] — Gamebryo colour bytes are raw monitor-
/// space floats with no sRGB curve to invert).
///
/// [`feedback_color_space`]: ../../../../../home/matias/.claude/projects/-mnt-data-src-gamebyro-redux/memory/feedback_color_space.md
#[derive(Debug, Clone, Copy)]
pub struct WaterParams {
    /// Linear RGB of the shallow-water tint (DATA / DNAM RGBA bytes
    /// → f32; alpha is dropped — the renderer drives opacity from
    /// `WaterKind` + grazing angle).
    pub shallow_color: [f32; 3],
    /// Linear RGB of the deep-water tint.
    pub deep_color: [f32; 3],
    /// Linear RGB of the reflection tint — multiplied into the RT
    /// reflection ray hit colour by the water shader.
    pub reflection_color: [f32; 3],
    /// Fog distance (world units) at which the shallow tint reaches
    /// 50% mix. Default `80.0` — UESP-documented FNV vanilla median.
    pub fog_near: f32,
    /// Fog distance at which the deep tint fully takes over.
    /// Default `600.0`.
    pub fog_far: f32,
    /// 0..1 reflectivity multiplier (`reflectivity_amount`).
    pub reflectivity: f32,
    /// 0..1 Fresnel amount — drives the surface's edge fresnel
    /// intensity. Default `0.02` (~clean water F0).
    pub fresnel: f32,
    /// Wind speed driving normal-map scroll, world units per second.
    pub wind_speed: f32,
    /// Wind direction in radians (DATA `wind_direction`).
    pub wind_direction: f32,
    /// Wave amplitude — vertex displacement magnitude. Not used by
    /// the flat-mesh shader (we perturb shading normals instead) but
    /// carried for future displacement work / underwater systems.
    pub wave_amplitude: f32,
    /// Wave frequency, Hz.
    pub wave_frequency: f32,
}

impl Default for WaterParams {
    fn default() -> Self {
        Self {
            shallow_color: [0.10, 0.32, 0.38],
            deep_color: [0.02, 0.06, 0.10],
            reflection_color: [0.85, 0.88, 0.92],
            fog_near: 80.0,
            fog_far: 600.0,
            reflectivity: 0.85,
            fresnel: 0.02,
            wind_speed: 1.0,
            wind_direction: 0.0,
            wave_amplitude: 0.05,
            wave_frequency: 0.6,
        }
    }
}

/// Decode an 8-bit unsigned colour component into the engine's
/// linear-RGB working space. Gamebryo colours are raw monitor-space
/// floats — no sRGB curve to invert (see [`feedback_color_space`]).
///
/// [`feedback_color_space`]: ../../../../../home/matias/.claude/projects/-mnt-data-src-gamebyro-redux/memory/feedback_color_space.md
#[inline]
fn u8_to_linear(byte: u8) -> f32 {
    byte as f32 / 255.0
}

/// Parse Oblivion / FO3 / FNV WATR.DATA. The 60-byte prefix layout
/// is identical across these three games (per UESP +
/// CK / GECK wiki cross-check):
///
/// ```text
/// offset  size  field
/// ------  ----  --------------------------------
///  0      4     wind_velocity      (f32)
///  4      4     wind_direction     (f32)
///  8      4     wave_amplitude     (f32)
/// 12      4     wave_frequency     (f32)
/// 16      4     sun_power          (f32) — unused
/// 20      4     reflectivity_amt   (f32)
/// 24      4     fresnel_amount     (f32)
/// 28      4     fog_distance_near  (f32) — FNV/FO3
/// 32      4     fog_distance_far   (f32)
/// 36      4     shallow_color      (RGBA u8x4)
/// 40      4     deep_color         (RGBA u8x4)
/// 44      4     reflection_color   (RGBA u8x4)
/// ```
///
/// Oblivion DATA omits the explicit fog distances (offsets 28..36) —
/// `decode_data` falls back to defaults for any field whose source
/// offset is past the buffer end.
fn decode_data(data: &[u8]) -> WaterParams {
    let mut p = WaterParams::default();
    if data.len() >= 4 {
        p.wind_speed = read_f32_at(data, 0).unwrap_or(p.wind_speed);
    }
    if data.len() >= 8 {
        p.wind_direction = read_f32_at(data, 4).unwrap_or(p.wind_direction);
    }
    if data.len() >= 12 {
        p.wave_amplitude = read_f32_at(data, 8).unwrap_or(p.wave_amplitude);
    }
    if data.len() >= 16 {
        p.wave_frequency = read_f32_at(data, 12).unwrap_or(p.wave_frequency);
    }
    // skip sun_power at 16..20 — unused by the flat-mesh shader.
    if data.len() >= 24 {
        p.reflectivity = read_f32_at(data, 20)
            .unwrap_or(p.reflectivity)
            .clamp(0.0, 1.0);
    }
    if data.len() >= 28 {
        p.fresnel = read_f32_at(data, 24).unwrap_or(p.fresnel).clamp(0.0, 1.0);
    }
    if data.len() >= 32 {
        p.fog_near = read_f32_at(data, 28).unwrap_or(p.fog_near).max(0.0);
    }
    if data.len() >= 36 {
        p.fog_far = read_f32_at(data, 32).unwrap_or(p.fog_far).max(p.fog_near + 1.0);
    }
    if data.len() >= 40 {
        p.shallow_color = [
            u8_to_linear(data[36]),
            u8_to_linear(data[37]),
            u8_to_linear(data[38]),
        ];
    }
    if data.len() >= 44 {
        p.deep_color = [
            u8_to_linear(data[40]),
            u8_to_linear(data[41]),
            u8_to_linear(data[42]),
        ];
    }
    if data.len() >= 48 {
        p.reflection_color = [
            u8_to_linear(data[44]),
            u8_to_linear(data[45]),
            u8_to_linear(data[46]),
        ];
    }
    p
}

/// Best-effort Skyrim+ DNAM decode. The 1.5 / 1.6 layouts differ in
/// the trailing fields but the leading prefix that we consume is
/// stable. When the buffer is shorter than expected, each field
/// falls back to its default rather than emitting partial reads.
fn decode_dnam_skyrim(data: &[u8]) -> WaterParams {
    let mut p = WaterParams::default();
    // Skyrim DNAM starts with a 4-byte unknown / version tag at
    // offset 0; the wind/wave/fog prefix matches the FNV layout
    // starting at offset 4.
    if data.len() < 52 {
        return p;
    }
    if data.len() >= 8 {
        p.wind_speed = read_f32_at(data, 4).unwrap_or(p.wind_speed);
    }
    if data.len() >= 12 {
        p.wind_direction = read_f32_at(data, 8).unwrap_or(p.wind_direction);
    }
    if data.len() >= 16 {
        p.wave_amplitude = read_f32_at(data, 12).unwrap_or(p.wave_amplitude);
    }
    if data.len() >= 20 {
        p.wave_frequency = read_f32_at(data, 16).unwrap_or(p.wave_frequency);
    }
    if data.len() >= 28 {
        p.reflectivity = read_f32_at(data, 24)
            .unwrap_or(p.reflectivity)
            .clamp(0.0, 1.0);
    }
    if data.len() >= 32 {
        p.fresnel = read_f32_at(data, 28).unwrap_or(p.fresnel).clamp(0.0, 1.0);
    }
    if data.len() >= 36 {
        p.fog_near = read_f32_at(data, 32).unwrap_or(p.fog_near).max(0.0);
    }
    if data.len() >= 40 {
        p.fog_far = read_f32_at(data, 36).unwrap_or(p.fog_far).max(p.fog_near + 1.0);
    }
    if data.len() >= 44 {
        p.shallow_color = [
            u8_to_linear(data[40]),
            u8_to_linear(data[41]),
            u8_to_linear(data[42]),
        ];
    }
    if data.len() >= 48 {
        p.deep_color = [
            u8_to_linear(data[44]),
            u8_to_linear(data[45]),
            u8_to_linear(data[46]),
        ];
    }
    if data.len() >= 52 {
        p.reflection_color = [
            u8_to_linear(data[48]),
            u8_to_linear(data[49]),
            u8_to_linear(data[50]),
        ];
    }
    p
}

pub fn parse_watr(form_id: u32, subs: &[SubRecord]) -> WatrRecord {
    let mut out = WatrRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"TNAM" => out.texture_path = read_zstring(&sub.data),
            b"DATA" => {
                // Oblivion / FO3 / FNV path. The two byte layouts
                // are compatible on the 60-byte prefix we consume.
                out.params = decode_data(&sub.data);
                out.raw_data = sub.data.clone();
            }
            b"DNAM" => {
                // Skyrim+ path — best-effort decode (see
                // `decode_dnam_skyrim`).
                out.params = decode_dnam_skyrim(&sub.data);
                out.raw_dnam = sub.data.clone();
            }
            b"GNAM" => {
                // 12 bytes = three u32 FormIDs (noise layer 0/1/2).
                // Fewer bytes → unfilled slots stay at zero.
                let count = (sub.data.len() / 4).min(3);
                for i in 0..count {
                    if let Some(fid) = read_u32_at(&sub.data, i * 4) {
                        out.noise_textures[i] = fid;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Adapter from a parsed `WatrRecord` onto a `WaterParams` view.
/// The per-game decode happens inside `parse_watr` (DATA vs DNAM
/// sub-records); this helper just returns the structured view.
/// Re-introduce a `GameKind` parameter when a divergent per-game
/// projection actually ships (TD8-017 / #1120 — the placeholder was
/// dropped per CLAUDE.md's "no `_var` hypothetical-future" rule).
pub fn watr_to_params(record: &WatrRecord) -> WaterParams {
    record.params
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    #[test]
    fn parse_watr_picks_edid_full_tnam() {
        let subs = vec![
            sub(b"EDID", b"WaterFreshDefault\0"),
            sub(b"FULL", b"Fresh Water\0"),
            sub(b"TNAM", b"textures\\water\\fresh.dds\0"),
        ];
        let w = parse_watr(0x1234, &subs);
        assert_eq!(w.form_id, 0x1234);
        assert_eq!(w.editor_id, "WaterFreshDefault");
        assert_eq!(w.full_name, "Fresh Water");
        assert_eq!(w.texture_path, "textures\\water\\fresh.dds");
    }

    #[test]
    fn parse_watr_decodes_data_fields() {
        // Construct a FO3/FNV-shaped DATA payload — 48 bytes covers
        // every field `decode_data` consumes.
        let mut data = Vec::with_capacity(48);
        data.extend_from_slice(&1.5f32.to_le_bytes()); // wind_speed
        data.extend_from_slice(&0.25f32.to_le_bytes()); // wind_direction
        data.extend_from_slice(&0.10f32.to_le_bytes()); // wave_amplitude
        data.extend_from_slice(&0.80f32.to_le_bytes()); // wave_frequency
        data.extend_from_slice(&0.00f32.to_le_bytes()); // sun_power (unused)
        data.extend_from_slice(&0.65f32.to_le_bytes()); // reflectivity
        data.extend_from_slice(&0.04f32.to_le_bytes()); // fresnel
        data.extend_from_slice(&50.0f32.to_le_bytes()); // fog_near
        data.extend_from_slice(&400.0f32.to_le_bytes()); // fog_far
        data.extend_from_slice(&[0x20, 0x60, 0x80, 0xFF]); // shallow RGBA
        data.extend_from_slice(&[0x05, 0x0F, 0x18, 0xFF]); // deep RGBA
        data.extend_from_slice(&[0xC0, 0xD0, 0xE0, 0xFF]); // reflection RGBA

        let subs = vec![sub(b"DATA", &data)];
        let w = parse_watr(0xAAAA, &subs);
        assert!((w.params.wind_speed - 1.5).abs() < 1e-6);
        assert!((w.params.wave_frequency - 0.80).abs() < 1e-6);
        assert!((w.params.reflectivity - 0.65).abs() < 1e-6);
        assert!((w.params.fresnel - 0.04).abs() < 1e-6);
        assert!((w.params.fog_near - 50.0).abs() < 1e-3);
        assert!((w.params.fog_far - 400.0).abs() < 1e-3);
        // 0x20 = 32 → 32/255 ≈ 0.1255 — within tolerance.
        assert!((w.params.shallow_color[0] - (0x20 as f32 / 255.0)).abs() < 1e-6);
        assert!((w.params.deep_color[2] - (0x18 as f32 / 255.0)).abs() < 1e-6);
        assert_eq!(w.raw_data.len(), 48);
        assert!(w.raw_dnam.is_empty());
    }

    #[test]
    fn parse_watr_short_data_keeps_defaults_past_buffer_end() {
        // 12 bytes — only wind_speed, wind_direction, wave_amplitude
        // get decoded; everything else stays at default.
        let mut data = Vec::with_capacity(12);
        data.extend_from_slice(&3.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.5f32.to_le_bytes());
        let subs = vec![sub(b"DATA", &data)];
        let w = parse_watr(0xBBBB, &subs);
        assert!((w.params.wind_speed - 3.0).abs() < 1e-6);
        assert!((w.params.wave_amplitude - 0.5).abs() < 1e-6);
        // Defaults preserved past offset 12.
        assert!((w.params.fog_near - 80.0).abs() < 1e-3);
        assert!((w.params.fog_far - 600.0).abs() < 1e-3);
        assert!((w.params.fresnel - 0.02).abs() < 1e-6);
    }

    #[test]
    fn parse_watr_decodes_dnam_skyrim_prefix() {
        // Skyrim DNAM with 52-byte prefix (the shortest that fills
        // every decoded field). Leading 4 bytes are the unknown /
        // version tag.
        let mut data = Vec::with_capacity(52);
        data.extend_from_slice(&0u32.to_le_bytes()); // unknown
        data.extend_from_slice(&2.0f32.to_le_bytes()); // wind_speed @ offset 4
        data.extend_from_slice(&1.2f32.to_le_bytes()); // wind_direction
        data.extend_from_slice(&0.20f32.to_le_bytes()); // wave_amplitude
        data.extend_from_slice(&0.55f32.to_le_bytes()); // wave_frequency
        data.extend_from_slice(&1.0f32.to_le_bytes()); // sun_power (offset 20, unused)
        data.extend_from_slice(&0.75f32.to_le_bytes()); // reflectivity @ 24
        data.extend_from_slice(&0.03f32.to_le_bytes()); // fresnel
        data.extend_from_slice(&60.0f32.to_le_bytes()); // fog_near
        data.extend_from_slice(&500.0f32.to_le_bytes()); // fog_far
        data.extend_from_slice(&[0x10, 0x40, 0x70, 0xFF]); // shallow
        data.extend_from_slice(&[0x02, 0x08, 0x10, 0xFF]); // deep
        data.extend_from_slice(&[0xA0, 0xB0, 0xC0, 0xFF]); // reflection

        let subs = vec![sub(b"DNAM", &data)];
        let w = parse_watr(0xCCCC, &subs);
        assert!((w.params.wind_speed - 2.0).abs() < 1e-6);
        assert!((w.params.reflectivity - 0.75).abs() < 1e-6);
        assert!((w.params.fog_far - 500.0).abs() < 1e-3);
        assert_eq!(w.raw_dnam.len(), 52);
        assert!(w.raw_data.is_empty());
    }

    #[test]
    fn parse_watr_decodes_gnam_noise_textures() {
        let mut gnam = Vec::with_capacity(12);
        gnam.extend_from_slice(&0x11111111u32.to_le_bytes());
        gnam.extend_from_slice(&0x22222222u32.to_le_bytes());
        gnam.extend_from_slice(&0x33333333u32.to_le_bytes());
        let subs = vec![sub(b"GNAM", &gnam)];
        let w = parse_watr(0xDDDD, &subs);
        assert_eq!(
            w.noise_textures,
            [0x11111111, 0x22222222, 0x33333333]
        );
    }
}
