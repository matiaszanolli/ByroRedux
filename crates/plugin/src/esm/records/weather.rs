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
    /// Cloud texture paths (up to 4 layers for FNV).
    pub cloud_textures: [Option<String>; 4],
    /// Cloud layer speeds (0–255 per layer, from DNAM).
    pub cloud_speeds: [u8; 4],
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
            cloud_speeds: [0; 4],
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

            // NAM0: sky colors — 10 groups × 6 slots × 4 bytes = 240 bytes.
            b"NAM0" if sub.data.len() >= SKY_COLOR_GROUPS * SKY_TIME_SLOTS * 4 => {
                let mut offset = 0;
                for group in 0..SKY_COLOR_GROUPS {
                    for slot in 0..SKY_TIME_SLOTS {
                        record.sky_colors[group][slot] = SkyColor {
                            r: sub.data[offset],
                            g: sub.data[offset + 1],
                            b: sub.data[offset + 2],
                            a: sub.data[offset + 3],
                        };
                        offset += 4;
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

            // DNAM: cloud layer speeds (4 bytes — one per layer).
            b"DNAM" if sub.data.len() >= 4 => {
                record.cloud_speeds[0] = sub.data[0];
                record.cloud_speeds[1] = sub.data[1];
                record.cloud_speeds[2] = sub.data[2];
                record.cloud_speeds[3] = sub.data[3];
            }

            // Cloud texture paths: 00TX through 03TX.
            [b'0', b'0', b'T', b'X'] => {
                record.cloud_textures[0] = Some(read_zstring(&sub.data));
            }
            [b'1', b'0', b'T', b'X'] => {
                record.cloud_textures[1] = Some(read_zstring(&sub.data));
            }
            [b'2', b'0', b'T', b'X'] => {
                record.cloud_textures[2] = Some(read_zstring(&sub.data));
            }
            [b'3', b'0', b'T', b'X'] => {
                record.cloud_textures[3] = Some(read_zstring(&sub.data));
            }

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
            make_sub(b"DNAM", vec![10, 20, 30, 40]),
            make_sub(b"00TX", b"sky\\clouds_01.dds\0".to_vec()),
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

        // Clouds
        assert_eq!(w.cloud_speeds, [10, 20, 30, 40]);
        assert_eq!(w.cloud_textures[0].as_deref(), Some("sky\\clouds_01.dds"));
        assert!(w.cloud_textures[1].is_none());
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
