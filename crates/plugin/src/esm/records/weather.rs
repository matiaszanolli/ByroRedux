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

            // HNAM: fog distances — 4 × f32 (day near, day far, night near, night far).
            b"HNAM" if sub.data.len() >= 16 => {
                record.fog_day_near = read_f32_at(&sub.data, 0).unwrap_or(0.0);
                record.fog_day_far = read_f32_at(&sub.data, 4).unwrap_or(10000.0);
                record.fog_night_near = read_f32_at(&sub.data, 8).unwrap_or(0.0);
                record.fog_night_far = read_f32_at(&sub.data, 12).unwrap_or(10000.0);
            }

            // FNAM: short fog far distances (older format, 4 bytes).
            // Some records use FNAM instead of HNAM for a simpler fog definition.
            b"FNAM" if sub.data.len() >= 4 && record.fog_day_far == 10000.0 => {
                // FNAM packs day_far and night_far as u8 * 100 or similar.
                // Only used as fallback when HNAM is absent.
            }

            // DATA: general weather data (15 bytes for FNV).
            b"DATA" if sub.data.len() >= 15 => {
                record.wind_speed = sub.data[0];
                // bytes 1-2: cloud speed lower/upper
                // byte 3: trans delta
                record.sun_glare = sub.data[4];
                record.sun_damage = sub.data[5];
                // bytes 6-11: precipitation/thunder fade params
                // byte 12: thunder frequency
                record.classification = sub.data[13];
                // bytes 13-14: lightning color (partial)
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
        data_bytes[13] = WTHR_PLEASANT;

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
