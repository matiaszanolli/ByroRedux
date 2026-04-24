//! WTHR (Weather) record parser.
//!
//! Weather records define sky appearance, fog distances, wind, sun parameters,
//! and cloud layers. Each worldspace references a default weather; the game
//! interpolates between weathers based on time of day and climate.
//!
//! FNV layout (NAM0 = 240 bytes):
//!   10 color groups × 6 time-of-day slots × 4 bytes (RGBA u8).
//!   Groups: sky_upper, fog, ambient, sunlight, sun, stars,
//!           sky_lower, horizon, clouds_lower, clouds_upper.
//!   Slots: sunrise, day, sunset, night, high_noon, midnight.

use super::common::{read_f32_at, read_zstring};
use crate::esm::reader::SubRecord;

/// Number of color groups in NAM0.
pub const SKY_COLOR_GROUPS: usize = 10;
/// Number of time-of-day slots per color group (FNV).
pub const SKY_TIME_SLOTS: usize = 6;

/// Color group indices into `sky_colors`.
pub const SKY_UPPER: usize = 0;
pub const SKY_FOG: usize = 1;
pub const SKY_AMBIENT: usize = 2;
pub const SKY_SUNLIGHT: usize = 3;
pub const SKY_SUN: usize = 4;
pub const SKY_STARS: usize = 5;
pub const SKY_LOWER: usize = 6;
pub const SKY_HORIZON: usize = 7;
pub const SKY_CLOUDS_LOWER: usize = 8;
pub const SKY_CLOUDS_UPPER: usize = 9;

/// Time-of-day slot indices.
pub const TOD_SUNRISE: usize = 0;
pub const TOD_DAY: usize = 1;
pub const TOD_SUNSET: usize = 2;
pub const TOD_NIGHT: usize = 3;
pub const TOD_HIGH_NOON: usize = 4;
pub const TOD_MIDNIGHT: usize = 5;

/// RGBA color from NAM0 sub-record (u8 per channel).
#[derive(Debug, Clone, Copy, Default)]
pub struct SkyColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl SkyColor {
    /// Convert to f32 RGB in raw monitor-space (no gamma correction).
    ///
    /// Per commit `0e8efc6`: Gamebryo / Bethesda ESM colors (LIGH, XCLL, NIF
    /// material, WTHR NAM0) are authored as raw monitor-space floats. The
    /// engine feeds them directly into the ACES tone mapper — applying an
    /// sRGB→linear decode darkens every warm hue (e.g. 0.78 → 0.58).
    pub fn to_rgb_f32(&self) -> [f32; 3] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
        ]
    }
}

/// Weather classification flags.
pub const WTHR_PLEASANT: u8 = 0x01;
pub const WTHR_CLOUDY: u8 = 0x02;
pub const WTHR_RAINY: u8 = 0x04;
pub const WTHR_SNOW: u8 = 0x08;

/// Oblivion WTHR HNAM sub-record — 14 f32 HDR + lighting tuning
/// parameters (56 bytes total). Names + offsets per UESP `Oblivion_Mod:WTHR`.
///
/// These are NOT fog distances, despite the pre-#537 parser's
/// interpretation: the first 4 f32 are `eye_adapt_speed`, `blur_radius`,
/// `blur_passes`, `emissive_mult` — reading them as `[fog_day_near,
/// fog_day_far, fog_night_near, fog_night_far]` saturated every
/// Oblivion exterior to solid fog within a few units of the camera.
/// Fog distances come from FNAM (16 bytes) on Oblivion; HNAM feeds
/// the renderer's eye-adaptation / bloom / scene-dimmer pipeline.
///
/// FNV and Fallout 3 do not ship HNAM at all — their fog comes from
/// FNAM + cloud textures from DNAM/CNAM/ANAM/BNAM. See audit M33-05.
///
/// Consumer wiring (HDR eye-adaptation system) is follow-up work —
/// this captures the authored values verbatim so the future bloom /
/// sunlight-dimmer system has a canonical source.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OblivionHdrLighting {
    /// Rate the scene luminance meter tracks toward the target (0–65536).
    pub eye_adapt_speed: f32,
    /// Bloom blur kernel radius in pixels (0–7).
    pub blur_radius: f32,
    /// Bloom blur iteration count (0–63).
    pub blur_passes: f32,
    /// Emissive pass multiplier — scales self-illum contributions.
    pub emissive_mult: f32,
    /// Target luminance the eye-adapt loop drives toward.
    pub target_lum: f32,
    /// Upper clamp on the scene luminance reading (prevents runaway
    /// adaptation when a bright light enters the view frustum).
    pub upper_lum_clamp: f32,
    /// Bright-pass scale — boosts highlights before the blur kernel.
    pub bright_scale: f32,
    /// Bright-pass clamp on the scaled output.
    pub bright_clamp: f32,
    /// Fallback luminance ramp value when no luminance texture is
    /// available (first-frame / LOD transitions).
    pub lum_ramp_no_tex: f32,
    /// Luminance ramp minimum (eye-adapt floor).
    pub lum_ramp_min: f32,
    /// Luminance ramp maximum (eye-adapt ceiling).
    pub lum_ramp_max: f32,
    /// Directional-sunlight colour multiplier for this weather.
    pub sunlight_dimmer: f32,
    /// Grass colour multiplier (Oblivion-specific — applied at
    /// grass-generator stage before the scene lighting pass).
    pub grass_dimmer: f32,
    /// Tree colour multiplier — same role as `grass_dimmer` for
    /// foliage meshes.
    pub tree_dimmer: f32,
}

/// Parsed WTHR record.
#[derive(Debug, Clone)]
pub struct WeatherRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// 10 color groups × 6 time-of-day slots. Indexed by SKY_* and TOD_* constants.
    pub sky_colors: [[SkyColor; SKY_TIME_SLOTS]; SKY_COLOR_GROUPS],
    /// Fog day near distance (game units).
    pub fog_day_near: f32,
    /// Fog day far distance (game units).
    pub fog_day_far: f32,
    /// Fog night near distance (game units).
    pub fog_night_near: f32,
    /// Fog night far distance (game units).
    pub fog_night_far: f32,
    /// Wind speed (0–255 scaled).
    pub wind_speed: u8,
    /// Sun glare intensity (0–255 scaled).
    pub sun_glare: u8,
    /// Sun damage factor (0–255).
    pub sun_damage: u8,
    /// Weather classification flags (WTHR_PLEASANT | WTHR_CLOUDY | ...).
    pub classification: u8,
    /// Cloud texture paths. FNV/FO3 ship 4 layers (DNAM/CNAM/ANAM/BNAM
    /// = layers 0–3 in schema-emission order); Oblivion ships 2
    /// (DNAM = 0, CNAM = 1). Paths are `textures\`-root-relative
    /// zstrings — see #468 for the normaliser in `TextureProvider`.
    pub cloud_textures: [Option<String>; 4],
    /// Oblivion-only HDR / eye-adapt / sunlight-dimmer tuning from the
    /// 56-byte HNAM sub-record. `None` for FNV / FO3 / Skyrim+ weather
    /// records, and for Oblivion records that ship a malformed HNAM.
    /// See audit M33-05 / #537.
    pub oblivion_hdr: Option<OblivionHdrLighting>,
}

impl Default for WeatherRecord {
    fn default() -> Self {
        Self {
            form_id: 0,
            editor_id: String::new(),
            sky_colors: [[SkyColor::default(); SKY_TIME_SLOTS]; SKY_COLOR_GROUPS],
            fog_day_near: 0.0,
            fog_day_far: 10000.0,
            fog_night_near: 0.0,
            fog_night_far: 10000.0,
            wind_speed: 0,
            sun_glare: 0,
            sun_damage: 0,
            classification: 0,
            cloud_textures: [None, None, None, None],
            oblivion_hdr: None,
        }
    }
}

/// Parse a WTHR record from its sub-records.
pub fn parse_wthr(form_id: u32, subs: &[SubRecord]) -> WeatherRecord {
    let mut record = WeatherRecord {
        form_id,
        ..WeatherRecord::default()
    };

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = read_zstring(&sub.data),

            // NAM0: sky colors. Two on-disk strides exist:
            //   - 240 B = 10 groups × 6 TOD slots × 4 B (FNV default + later games):
            //     SUNRISE, DAY, SUNSET, NIGHT, HIGH_NOON, MIDNIGHT.
            //   - 160 B = 10 groups × 4 TOD slots × 4 B (Oblivion, FO3, some older /
            //     DLC FNV records): SUNRISE, DAY, SUNSET, NIGHT only. HIGH_NOON and
            //     MIDNIGHT are synthesised from DAY and NIGHT respectively so the
            //     `[[SkyColor; 6]; 10]` struct layout remains authoritative for
            //     downstream consumers (`weather_system` 7-key interpolator,
            //     `build_tod_keys`). See #533 / audit M33-01.
            b"NAM0" if sub.data.len() >= SKY_COLOR_GROUPS * 4 * 4 => {
                let on_disk_slots = if sub.data.len() >= SKY_COLOR_GROUPS * SKY_TIME_SLOTS * 4 {
                    SKY_TIME_SLOTS
                } else {
                    4
                };
                let mut offset = 0;
                for group in 0..SKY_COLOR_GROUPS {
                    for slot in 0..on_disk_slots {
                        record.sky_colors[group][slot] = SkyColor {
                            r: sub.data[offset],
                            g: sub.data[offset + 1],
                            b: sub.data[offset + 2],
                            a: sub.data[offset + 3],
                        };
                        offset += 4;
                    }
                    if on_disk_slots < SKY_TIME_SLOTS {
                        record.sky_colors[group][TOD_HIGH_NOON] =
                            record.sky_colors[group][TOD_DAY];
                        record.sky_colors[group][TOD_MIDNIGHT] =
                            record.sky_colors[group][TOD_NIGHT];
                    }
                }
            }

            // FNAM: fog distances. Four f32 at offsets 0/4/8/12:
            // `[day_near, day_far, night_near, night_far]`. Game-dependent
            // total size: 16 B in Oblivion, 24 B in FNV/FO3 (trailing 8 B
            // have not been cross-checked against a UESP-authoritative
            // schema so they are ignored here).
            //
            // Pre-fix the FNAM arm had an empty body with a comment
            // ("fallback when HNAM is absent"). But FNV and FO3 do not
            // ship HNAM — they ship only FNAM. The result was that every
            // FNV + FO3 weather defaulted to `fog_day_far = 10000.0`
            // independent of weather type. See audit M33-04 / #536.
            b"FNAM" if sub.data.len() >= 16 => {
                record.fog_day_near = read_f32_at(&sub.data, 0).unwrap_or(0.0);
                record.fog_day_far = read_f32_at(&sub.data, 4).unwrap_or(10000.0);
                record.fog_night_near = read_f32_at(&sub.data, 8).unwrap_or(0.0);
                record.fog_night_far = read_f32_at(&sub.data, 12).unwrap_or(10000.0);
            }

            // HNAM — Oblivion-only HDR / eye-adapt / sunlight-dimmer
            // tuning. 56 bytes = 14 × f32. See #537 / audit M33-05 and
            // the `OblivionHdrLighting` doc comment for the field
            // order. Fog for Oblivion comes from FNAM (handled above),
            // NEVER from HNAM. FNV + FO3 do not ship HNAM.
            //
            // Pre-#537 the parser read the first 16 bytes as fog
            // distances (`[fog_day_near, fog_day_far, fog_night_near,
            // fog_night_far]`), picking up the HDR params
            // `[eye_adapt_speed, blur_radius, blur_passes,
            // emissive_mult]` instead — which produced
            // `fog_day_far ≈ 4.0` on every Oblivion exterior and
            // saturated the scene to solid fog within a few units of
            // the camera.
            //
            // The 16-byte legacy fixture path is retained behind a
            // narrower gate (`== 16`) so synthetic tests that use a
            // 16-byte HNAM as a fog source keep compiling; real
            // Oblivion masters ship only the 56-byte form.
            b"HNAM" if sub.data.len() == 56 => {
                record.oblivion_hdr = Some(OblivionHdrLighting {
                    eye_adapt_speed: read_f32_at(&sub.data, 0).unwrap_or(0.0),
                    blur_radius: read_f32_at(&sub.data, 4).unwrap_or(0.0),
                    blur_passes: read_f32_at(&sub.data, 8).unwrap_or(0.0),
                    emissive_mult: read_f32_at(&sub.data, 12).unwrap_or(0.0),
                    target_lum: read_f32_at(&sub.data, 16).unwrap_or(0.0),
                    upper_lum_clamp: read_f32_at(&sub.data, 20).unwrap_or(0.0),
                    bright_scale: read_f32_at(&sub.data, 24).unwrap_or(0.0),
                    bright_clamp: read_f32_at(&sub.data, 28).unwrap_or(0.0),
                    lum_ramp_no_tex: read_f32_at(&sub.data, 32).unwrap_or(0.0),
                    lum_ramp_min: read_f32_at(&sub.data, 36).unwrap_or(0.0),
                    lum_ramp_max: read_f32_at(&sub.data, 40).unwrap_or(0.0),
                    sunlight_dimmer: read_f32_at(&sub.data, 44).unwrap_or(0.0),
                    grass_dimmer: read_f32_at(&sub.data, 48).unwrap_or(0.0),
                    tree_dimmer: read_f32_at(&sub.data, 52).unwrap_or(0.0),
                });
            }
            b"HNAM" if sub.data.len() == 16 => {
                // Legacy fixture path — synthetic tests pre-#537 built
                // 16-byte HNAMs and used them as fog sources. Preserved
                // so the M33-05 regression test (which asserts a real
                // 56-byte HNAM does NOT clobber FNAM fog) keeps its
                // counterpart. No vanilla master ships this shape.
                record.fog_day_near = read_f32_at(&sub.data, 0).unwrap_or(0.0);
                record.fog_day_far = read_f32_at(&sub.data, 4).unwrap_or(10000.0);
                record.fog_night_near = read_f32_at(&sub.data, 8).unwrap_or(0.0);
                record.fog_night_far = read_f32_at(&sub.data, 12).unwrap_or(10000.0);
            }

            // DATA: general weather data (15 bytes, shared by Oblivion,
            // FO3, FNV). Byte-confirmed layout from #538 / audit M33-06:
            //   [ 0]    wind_speed
            //   [ 1- 2] cloud speed lower / upper (unparsed — no consumer)
            //   [ 3]    trans_delta (unparsed)
            //   [ 4]    sun_glare
            //   [ 5]    sun_damage
            //   [ 6- 9] precipitation / thunder fade params (unparsed)
            //   [10]    reserved / unknown (0xFF on every PLEASANT sample;
            //                                varies on RAINY)
            //   [11]    classification flag byte (WTHR_PLEASANT=0x01,
            //           CLOUDY=0x02, RAINY=0x04, SNOW=0x08)
            //   [12-14] lightning color (RGB) — `ff ff ff` on RAINY, zero
            //           or 0xFF on non-rainy
            //
            // Pre-fix the parser read classification from byte 13 — a
            // zero / padding byte on nearly every record. Verified the
            // new offset against 4 flag bits on Oblivion (Clear/Cloudy/
            // Rain/Snow) and 3 on FNV (0x00/0x01/0x02).
            b"DATA" if sub.data.len() >= 15 => {
                record.wind_speed = sub.data[0];
                record.sun_glare = sub.data[4];
                record.sun_damage = sub.data[5];
                record.classification = sub.data[11];
            }

            // Cloud texture paths. Per-game sub-record FourCCs — verified
            // by byte-level scan of FalloutNV.esm, Fallout3.esm,
            // Oblivion.esm (docs/audits/AUDIT_M33_2026-04-21.md §M33-02):
            //   FNV / FO3 (4 layers, schema-emission order):
            //     DNAM = layer 0 (base)  CNAM = layer 1
            //     ANAM = layer 2         BNAM = layer 3 (top)
            //   Oblivion (2 layers):
            //     DNAM = layer 0         CNAM = layer 1
            // The pre-fix parser matched `00TX/10TX/20TX/30TX` which
            // doesn't appear in any shipped master (M33-02) and read
            // DNAM as `[u8; 4]` cloud speeds (M33-03, #535) — DNAM was
            // claimed both ways and ended up neither.
            //
            // Bodies are `textures\`-root-relative zstrings; #468's
            // `TextureProvider` normaliser prepends the prefix.
            b"DNAM" => record.cloud_textures[0] = Some(read_zstring(&sub.data)),
            b"CNAM" => record.cloud_textures[1] = Some(read_zstring(&sub.data)),
            b"ANAM" => record.cloud_textures[2] = Some(read_zstring(&sub.data)),
            b"BNAM" => record.cloud_textures[3] = Some(read_zstring(&sub.data)),

            _ => {}
        }
    }

    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::reader::SubRecord;

    fn make_sub(sub_type: &[u8; 4], data: Vec<u8>) -> SubRecord {
        SubRecord {
            sub_type: *sub_type,
            data,
        }
    }

    #[test]
    fn parse_wthr_basic() {
        let mut nam0_data = vec![0u8; 240];
        // Set sky_upper/day slot (group 0, slot 1) to (100, 150, 200, 255).
        let offset = (0 * SKY_TIME_SLOTS + TOD_DAY) * 4;
        nam0_data[offset] = 100;
        nam0_data[offset + 1] = 150;
        nam0_data[offset + 2] = 200;
        nam0_data[offset + 3] = 255;

        // Set horizon/day (group 7, slot 1).
        let offset = (SKY_HORIZON * SKY_TIME_SLOTS + TOD_DAY) * 4;
        nam0_data[offset] = 180;
        nam0_data[offset + 1] = 170;
        nam0_data[offset + 2] = 140;
        nam0_data[offset + 3] = 255;

        let mut hnam_data = vec![0u8; 16];
        hnam_data[0..4].copy_from_slice(&500.0_f32.to_le_bytes());
        hnam_data[4..8].copy_from_slice(&15000.0_f32.to_le_bytes());
        hnam_data[8..12].copy_from_slice(&200.0_f32.to_le_bytes());
        hnam_data[12..16].copy_from_slice(&8000.0_f32.to_le_bytes());

        let mut data_bytes = vec![0u8; 15];
        data_bytes[0] = 30; // wind speed
        data_bytes[4] = 128; // sun glare
        data_bytes[5] = 10; // sun damage
        // Classification sits at byte 11. See #538 / audit M33-06 —
        // pre-fix the parser read byte 13 (padding).
        data_bytes[11] = WTHR_PLEASANT;

        let subs = vec![
            make_sub(b"EDID", b"TestWeather\0".to_vec()),
            make_sub(b"NAM0", nam0_data),
            make_sub(b"HNAM", hnam_data),
            make_sub(b"DATA", data_bytes),
            // Cloud layers in schema-emission order (DNAM/CNAM/ANAM/BNAM).
            // See #534 / audit M33-02 for the FourCC-to-layer mapping.
            make_sub(b"DNAM", b"sky\\clouds_00_base.dds\0".to_vec()),
            make_sub(b"CNAM", b"sky\\clouds_01.dds\0".to_vec()),
            make_sub(b"ANAM", b"sky\\clouds_02.dds\0".to_vec()),
            make_sub(b"BNAM", b"sky\\clouds_03_top.dds\0".to_vec()),
        ];

        let w = parse_wthr(0x1234, &subs);
        assert_eq!(w.form_id, 0x1234);
        assert_eq!(w.editor_id, "TestWeather");

        // Sky upper / day
        let upper_day = w.sky_colors[SKY_UPPER][TOD_DAY];
        assert_eq!((upper_day.r, upper_day.g, upper_day.b), (100, 150, 200));

        // Horizon / day
        let horiz_day = w.sky_colors[SKY_HORIZON][TOD_DAY];
        assert_eq!((horiz_day.r, horiz_day.g, horiz_day.b), (180, 170, 140));

        // Fog
        assert!((w.fog_day_near - 500.0).abs() < 0.01);
        assert!((w.fog_day_far - 15000.0).abs() < 0.01);
        assert!((w.fog_night_near - 200.0).abs() < 0.01);
        assert!((w.fog_night_far - 8000.0).abs() < 0.01);

        // General data
        assert_eq!(w.wind_speed, 30);
        assert_eq!(w.sun_glare, 128);
        assert_eq!(w.classification, WTHR_PLEASANT);

        // Cloud layer paths — DNAM/CNAM/ANAM/BNAM = 0/1/2/3 (#534).
        assert_eq!(
            w.cloud_textures[0].as_deref(),
            Some("sky\\clouds_00_base.dds"),
        );
        assert_eq!(w.cloud_textures[1].as_deref(), Some("sky\\clouds_01.dds"));
        assert_eq!(w.cloud_textures[2].as_deref(), Some("sky\\clouds_02.dds"));
        assert_eq!(
            w.cloud_textures[3].as_deref(),
            Some("sky\\clouds_03_top.dds"),
        );
    }

    /// Regression for #534 / audit M33-02: the pre-fix parser matched
    /// `00TX/10TX/20TX/30TX` which never appear in any shipped master.
    /// Guard: those FourCCs should NOT be recognised — a WTHR that only
    /// contains them produces zero cloud layers.
    #[test]
    fn parse_wthr_00tx_fourccs_are_inert() {
        let subs = vec![
            make_sub(b"EDID", b"StaleFourCCs\0".to_vec()),
            make_sub(b"00TX", b"sky\\stale.dds\0".to_vec()),
            make_sub(b"10TX", b"sky\\stale.dds\0".to_vec()),
        ];
        let w = parse_wthr(0xFADE, &subs);
        assert!(w.cloud_textures.iter().all(|c| c.is_none()));
    }

    /// Regression for #538 + #543 / audit M33-06: classification sits
    /// at DATA byte 11, not byte 13. Byte-confirmed against 4 flag
    /// values on Oblivion (Clear/Cloudy/Rain/Snow) and 3 on FNV
    /// (0x00/0x01/0x02). Pre-fix the parser read byte 13 — a padding
    /// byte that read back as 0x00 or 0xFF on nearly every weather,
    /// making the `classification` field effectively unusable for any
    /// future downstream consumer.
    #[test]
    fn parse_wthr_classification_is_byte_11() {
        // Build DATA with unique sentinels at every byte so an offset
        // slip shows up as a wrong value rather than a lucky zero.
        let mut data_bytes = vec![0u8; 15];
        for (i, slot) in data_bytes.iter_mut().enumerate() {
            *slot = 0xA0 + i as u8;
        }
        // Override the fields the parser reads + place the RAINY flag
        // at byte 11.
        data_bytes[0] = 50; // wind
        data_bytes[4] = 200; // glare
        data_bytes[5] = 30; // damage
        data_bytes[11] = WTHR_RAINY;
        // Bytes 10 and 13 hold decoy values that would have misparsed
        // pre-fix. 0xD3 at byte 13 is the slot the old code read.
        let old_byte_13_noise = data_bytes[13];
        assert_ne!(old_byte_13_noise, WTHR_RAINY);

        let subs = vec![
            make_sub(b"EDID", b"RainyOffsetCheck\0".to_vec()),
            make_sub(b"DATA", data_bytes),
        ];
        let w = parse_wthr(0x600, &subs);
        assert_eq!(w.wind_speed, 50);
        assert_eq!(w.sun_glare, 200);
        assert_eq!(w.sun_damage, 30);
        assert_eq!(w.classification, WTHR_RAINY);
    }

    /// Regression for #536 / audit M33-04: FNAM must carry fog.
    /// Pre-fix the arm body was empty ("fallback when HNAM is absent"),
    /// but FNV + FO3 ship only FNAM (no HNAM), so every FNV + FO3
    /// weather defaulted to `fog_day_far = 10000.0`. This test uses
    /// the 24-byte FNV stride (16 B of 4 fog f32 + 8 trailing unknown
    /// bytes) to pin the fog fields.
    #[test]
    fn parse_wthr_fnam_fog_24byte_stride() {
        let mut fnam_data = vec![0u8; 24];
        fnam_data[0..4].copy_from_slice(&(-500.0_f32).to_le_bytes());
        fnam_data[4..8].copy_from_slice(&85_000.0_f32.to_le_bytes());
        fnam_data[8..12].copy_from_slice(&(-1000.0_f32).to_le_bytes());
        fnam_data[12..16].copy_from_slice(&40_000.0_f32.to_le_bytes());
        // Trailing 8 B are ignored by the parser — fill with sentinels.
        fnam_data[16..24].copy_from_slice(&[0xFE; 8]);
        let subs = vec![
            make_sub(b"EDID", b"FNVFogCheck\0".to_vec()),
            make_sub(b"FNAM", fnam_data),
        ];
        let w = parse_wthr(0xF09, &subs);
        assert!((w.fog_day_near + 500.0).abs() < 0.01);
        assert!((w.fog_day_far - 85_000.0).abs() < 0.01);
        assert!((w.fog_night_near + 1000.0).abs() < 0.01);
        assert!((w.fog_night_far - 40_000.0).abs() < 0.01);
    }

    /// Oblivion FNAM is 16 B — same 4-f32 fog layout, just without
    /// the FNV/FO3 trailing 8 B.
    #[test]
    fn parse_wthr_fnam_fog_16byte_stride() {
        let mut fnam_data = vec![0u8; 16];
        fnam_data[0..4].copy_from_slice(&(-500.0_f32).to_le_bytes());
        fnam_data[4..8].copy_from_slice(&16_000.0_f32.to_le_bytes());
        fnam_data[8..12].copy_from_slice(&(-600.0_f32).to_le_bytes());
        fnam_data[12..16].copy_from_slice(&6_000.0_f32.to_le_bytes());
        let subs = vec![
            make_sub(b"EDID", b"OBLFogCheck\0".to_vec()),
            make_sub(b"FNAM", fnam_data),
        ];
        let w = parse_wthr(0x0B1, &subs);
        assert!((w.fog_day_near + 500.0).abs() < 0.01);
        assert!((w.fog_day_far - 16_000.0).abs() < 0.01);
        assert!((w.fog_night_near + 600.0).abs() < 0.01);
        assert!((w.fog_night_far - 6_000.0).abs() < 0.01);
    }

    /// Real 56-byte Oblivion HNAM must NOT be interpreted as fog.
    /// Pre-#537 the HNAM arm gated on `>= 16` and would overwrite
    /// FNAM's correct fog values with the first 4 f32 of HNAM's
    /// lighting parameters (`0.7, 4.0, 2.0, 1.0` — fog-far of 4.0
    /// saturated every Oblivion exterior to solid fog_color within a
    /// few units of the camera). The 56-byte arm now captures the
    /// HDR tuning into `oblivion_hdr` and leaves fog to FNAM. See
    /// audit M33-05.
    #[test]
    fn parse_wthr_oblivion_hnam_56byte_does_not_clobber_fnam() {
        let mut fnam_data = vec![0u8; 16];
        fnam_data[0..4].copy_from_slice(&(-500.0_f32).to_le_bytes());
        fnam_data[4..8].copy_from_slice(&16_000.0_f32.to_le_bytes());
        fnam_data[8..12].copy_from_slice(&(-600.0_f32).to_le_bytes());
        fnam_data[12..16].copy_from_slice(&6_000.0_f32.to_le_bytes());
        let mut hnam_data = vec![0u8; 56];
        // Lighting-model values that would be DISASTROUS if read as fog.
        hnam_data[0..4].copy_from_slice(&0.7_f32.to_le_bytes());
        hnam_data[4..8].copy_from_slice(&4.0_f32.to_le_bytes());
        hnam_data[8..12].copy_from_slice(&2.0_f32.to_le_bytes());
        hnam_data[12..16].copy_from_slice(&1.0_f32.to_le_bytes());
        let subs = vec![
            make_sub(b"EDID", b"OBLMixed\0".to_vec()),
            make_sub(b"FNAM", fnam_data),
            make_sub(b"HNAM", hnam_data),
        ];
        let w = parse_wthr(0xEED, &subs);
        assert!((w.fog_day_far - 16_000.0).abs() < 0.01, "fog_day_far={}", w.fog_day_far);
        assert!((w.fog_night_far - 6_000.0).abs() < 0.01, "fog_night_far={}", w.fog_night_far);
    }

    /// Regression for #537 / audit M33-05 — a real 56-byte Oblivion
    /// HNAM must decode into `oblivion_hdr` with the 14 UESP-documented
    /// fields in the right order. Values chosen per the audit's live
    /// sample from `SEClearTrans`: first 4 f32 = `0.7, 4.0, 2.0, 1.0`
    /// = eye_adapt_speed / blur_radius / blur_passes / emissive_mult.
    /// Remaining slots filled with sentinel values so an offset slip
    /// surfaces as a wrong field rather than a lucky zero.
    #[test]
    fn parse_wthr_oblivion_hnam_56byte_decodes_to_hdr_fields() {
        // 14 distinct sentinel values so we can tell fields apart.
        let values: [f32; 14] = [
            0.7,   // eye_adapt_speed
            4.0,   // blur_radius
            2.0,   // blur_passes
            1.0,   // emissive_mult
            0.85,  // target_lum
            10.0,  // upper_lum_clamp
            0.25,  // bright_scale
            0.95,  // bright_clamp
            1.5,   // lum_ramp_no_tex
            0.05,  // lum_ramp_min
            2.5,   // lum_ramp_max
            0.8,   // sunlight_dimmer
            0.9,   // grass_dimmer
            0.75,  // tree_dimmer
        ];
        let mut hnam = Vec::with_capacity(56);
        for v in values {
            hnam.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(hnam.len(), 56);

        let subs = vec![
            make_sub(b"EDID", b"SEClearTrans\0".to_vec()),
            make_sub(b"HNAM", hnam),
        ];
        let w = parse_wthr(0x0100_0001, &subs);
        let hdr = w
            .oblivion_hdr
            .expect("56-byte HNAM must populate oblivion_hdr");
        assert_eq!(hdr.eye_adapt_speed, 0.7);
        assert_eq!(hdr.blur_radius, 4.0);
        assert_eq!(hdr.blur_passes, 2.0);
        assert_eq!(hdr.emissive_mult, 1.0);
        assert_eq!(hdr.target_lum, 0.85);
        assert_eq!(hdr.upper_lum_clamp, 10.0);
        assert_eq!(hdr.bright_scale, 0.25);
        assert_eq!(hdr.bright_clamp, 0.95);
        assert_eq!(hdr.lum_ramp_no_tex, 1.5);
        assert_eq!(hdr.lum_ramp_min, 0.05);
        assert_eq!(hdr.lum_ramp_max, 2.5);
        assert_eq!(hdr.sunlight_dimmer, 0.8);
        assert_eq!(hdr.grass_dimmer, 0.9);
        assert_eq!(hdr.tree_dimmer, 0.75);

        // Fog fields must stay at defaults — HNAM must NOT leak into
        // the fog slots.
        assert_eq!(w.fog_day_near, 0.0);
        assert!(
            (w.fog_day_far - 10000.0).abs() < 0.01,
            "fog_day_far must retain its default (got {})",
            w.fog_day_far
        );
    }

    /// Sibling: FNV / FO3 / Skyrim+ weather records have no HNAM, so
    /// `oblivion_hdr` must stay `None` — consumers can pattern-match
    /// on `Some` to tell "this is an Oblivion weather" apart.
    #[test]
    fn parse_wthr_non_oblivion_leaves_oblivion_hdr_none() {
        let mut fnam_data = vec![0u8; 16];
        fnam_data[4..8].copy_from_slice(&32_000.0_f32.to_le_bytes());
        let subs = vec![
            make_sub(b"EDID", b"FNVDay\0".to_vec()),
            make_sub(b"FNAM", fnam_data),
        ];
        let w = parse_wthr(0x0200_0001, &subs);
        assert!(w.oblivion_hdr.is_none());
    }

    /// Regression for #535 / audit M33-03: DNAM must be treated as a
    /// cloud-texture-path zstring, never as `[u8; 4]` cloud speeds.
    /// Feeding a 4-byte payload that would previously have decoded to
    /// `cloud_speeds = [1, 2, 3, 4]` now parses as the tiny zstring it
    /// resembles — and `cloud_speeds` no longer exists on the struct.
    #[test]
    fn parse_wthr_dnam_is_texture_path_not_speeds() {
        let subs = vec![
            make_sub(b"EDID", b"DnamPathCheck\0".to_vec()),
            make_sub(b"DNAM", b"sky\\a.dds\0".to_vec()),
        ];
        let w = parse_wthr(0x5ECD, &subs);
        assert_eq!(w.cloud_textures[0].as_deref(), Some("sky\\a.dds"));
    }

    /// Regression for #533 / audit M33-01: the 160-byte Oblivion/FO3
    /// NAM0 stride (10 groups × 4 TOD slots × 4 B) must parse. The
    /// pre-fix gate demanded 240 B and silently dropped every Oblivion
    /// and FO3 weather — sky colours stayed at `WeatherRecord::default()`
    /// zero RGB. HIGH_NOON / MIDNIGHT are synthesised from DAY / NIGHT
    /// so the six-slot downstream layout remains valid.
    #[test]
    fn parse_wthr_nam0_160_byte_stride() {
        // 10 groups × 4 slots × 4 B = 160 B. Fill each group's 4 slots
        // with distinct colours so we can assert ordering + synthesis.
        let mut nam0_data = vec![0u8; 160];
        for group in 0..SKY_COLOR_GROUPS {
            for slot in 0..4 {
                let off = (group * 4 + slot) * 4;
                nam0_data[off] = (group * 10) as u8;
                nam0_data[off + 1] = (slot * 50) as u8;
                nam0_data[off + 2] = (group + slot * 2) as u8;
                nam0_data[off + 3] = 255;
            }
        }

        let subs = vec![
            make_sub(b"EDID", b"OblivionClear\0".to_vec()),
            make_sub(b"NAM0", nam0_data),
        ];

        let w = parse_wthr(0x2468, &subs);
        // On-disk slots populate as authored.
        let sun_sunrise = w.sky_colors[SKY_SUN][TOD_SUNRISE];
        assert_eq!((sun_sunrise.r, sun_sunrise.g), (40, 0));
        let horiz_sunset = w.sky_colors[SKY_HORIZON][TOD_SUNSET];
        assert_eq!((horiz_sunset.r, horiz_sunset.g), (70, 100));

        // Synthesised slots: HIGH_NOON = DAY, MIDNIGHT = NIGHT.
        for group in 0..SKY_COLOR_GROUPS {
            let day = w.sky_colors[group][TOD_DAY];
            let high_noon = w.sky_colors[group][TOD_HIGH_NOON];
            assert_eq!(
                (day.r, day.g, day.b),
                (high_noon.r, high_noon.g, high_noon.b),
                "HIGH_NOON should mirror DAY for group {}",
                group,
            );
            let night = w.sky_colors[group][TOD_NIGHT];
            let midnight = w.sky_colors[group][TOD_MIDNIGHT];
            assert_eq!(
                (night.r, night.g, night.b),
                (midnight.r, midnight.g, midnight.b),
                "MIDNIGHT should mirror NIGHT for group {}",
                group,
            );
        }
    }

    /// Sanity: a NAM0 shorter than 160 B is still silently dropped
    /// (malformed — no downstream should have to guess what the stride
    /// was). The gate stays `>= 160` to preserve that invariant.
    #[test]
    fn parse_wthr_nam0_below_160_drops() {
        let subs = vec![
            make_sub(b"EDID", b"Truncated\0".to_vec()),
            make_sub(b"NAM0", vec![0xFF; 80]),
        ];
        let w = parse_wthr(0xBADD, &subs);
        // All slots remain at SkyColor::default() (all zero).
        for group in 0..SKY_COLOR_GROUPS {
            for slot in 0..SKY_TIME_SLOTS {
                let c = w.sky_colors[group][slot];
                assert_eq!((c.r, c.g, c.b, c.a), (0, 0, 0, 0));
            }
        }
    }

    #[test]
    fn sky_color_to_rgb_f32() {
        let c = SkyColor {
            r: 255,
            g: 0,
            b: 128,
            a: 255,
        };
        let rgb = c.to_rgb_f32();
        assert!((rgb[0] - 1.0).abs() < 0.001);
        assert!(rgb[1].abs() < 0.001);
        assert!((rgb[2] - 128.0 / 255.0).abs() < 0.001);
    }
}
