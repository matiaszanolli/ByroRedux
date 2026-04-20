//! CLMT (Climate) record parser.
//!
//! Climate records define the weather probability table for a worldspace.
//! Each worldspace references one climate via CNAM; the climate lists
//! the possible weathers with relative chances.

use super::common::{read_u32_at, read_zstring};
use crate::esm::reader::SubRecord;

/// A weather entry in the climate's weather list.
#[derive(Debug, Clone)]
pub struct ClimateWeather {
    /// Form ID of the WTHR record.
    pub weather_form_id: u32,
    /// Relative chance (higher = more likely). UESP + in-tree WLST parser
    /// comment both describe this as **i32**. Pre-#476 it was typed u32;
    /// negative-chance entries (used by mods as sentinels or subtractive
    /// weights) wrapped to huge positive and silently won `max_by_key`
    /// during default-weather selection.
    pub chance: i32,
}

/// Parsed CLMT record.
#[derive(Debug, Clone)]
pub struct ClimateRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Ordered weather list with chances. First entry is typically the default.
    pub weathers: Vec<ClimateWeather>,
    /// Sun texture path (from FNAM).
    pub sun_texture: Option<String>,
    /// Sunrise/sunset timing data from TNAM (6 bytes: sunrise begin/end,
    /// sunset begin/end, volatility, moon phases — times in 10-minute units).
    pub sunrise_begin: u8,
    pub sunrise_end: u8,
    pub sunset_begin: u8,
    pub sunset_end: u8,
}

impl Default for ClimateRecord {
    fn default() -> Self {
        Self {
            form_id: 0,
            editor_id: String::new(),
            weathers: Vec::new(),
            sun_texture: None,
            sunrise_begin: 0,
            sunrise_end: 0,
            sunset_begin: 0,
            sunset_end: 0,
        }
    }
}

/// Parse a CLMT record from its sub-records.
pub fn parse_clmt(form_id: u32, subs: &[SubRecord]) -> ClimateRecord {
    let mut record = ClimateRecord {
        form_id,
        ..ClimateRecord::default()
    };

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = read_zstring(&sub.data),

            // WLST: weather list — array of (form_id: u32, chance: i32) pairs.
            // FNV and Skyrim use 12-byte entries: (form_id: u32, chance: i32, global: u32).
            // Oblivion uses 8-byte entries: (form_id: u32, chance: i32).
            // Prefer 12 when divisible; fall back to 8.
            b"WLST" => {
                let entry_size = if sub.data.len() % 12 == 0 { 12 } else { 8 };
                let count = sub.data.len() / entry_size;
                for i in 0..count {
                    let offset = i * entry_size;
                    if let (Some(fid), Some(chance_bits)) = (
                        read_u32_at(&sub.data, offset),
                        read_u32_at(&sub.data, offset + 4),
                    ) {
                        if fid != 0 {
                            record.weathers.push(ClimateWeather {
                                weather_form_id: fid,
                                // Reinterpret the 4-byte little-endian slot as
                                // signed — UESP WLST schema says i32. See #476.
                                chance: chance_bits as i32,
                            });
                        }
                    }
                }
            }

            // FNAM: sun texture path.
            b"FNAM" => record.sun_texture = Some(read_zstring(&sub.data)),

            // TNAM: timing data (6 bytes).
            b"TNAM" if sub.data.len() >= 4 => {
                record.sunrise_begin = sub.data[0];
                record.sunrise_end = sub.data[1];
                record.sunset_begin = sub.data[2];
                record.sunset_end = sub.data[3];
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
    fn parse_clmt_basic() {
        // Build a WLST with 2 weather entries (12-byte format: fid + chance + global).
        let mut wlst_data = Vec::new();
        wlst_data.extend_from_slice(&0x1000u32.to_le_bytes()); // weather 1 form ID
        wlst_data.extend_from_slice(&60i32.to_le_bytes()); // 60% chance
        wlst_data.extend_from_slice(&0u32.to_le_bytes()); // global form ID (unused)
        wlst_data.extend_from_slice(&0x2000u32.to_le_bytes()); // weather 2 form ID
        wlst_data.extend_from_slice(&40i32.to_le_bytes()); // 40% chance
        wlst_data.extend_from_slice(&0u32.to_le_bytes()); // global form ID (unused)

        let subs = vec![
            make_sub(b"EDID", b"TestClimate\0".to_vec()),
            make_sub(b"WLST", wlst_data),
            make_sub(b"FNAM", b"sky\\sun_01.dds\0".to_vec()),
            make_sub(b"TNAM", vec![6, 8, 18, 20, 0, 0]),
        ];

        let c = parse_clmt(0xABCD, &subs);
        assert_eq!(c.form_id, 0xABCD);
        assert_eq!(c.editor_id, "TestClimate");
        assert_eq!(c.weathers.len(), 2);
        assert_eq!(c.weathers[0].weather_form_id, 0x1000);
        assert_eq!(c.weathers[0].chance, 60);
        assert_eq!(c.weathers[1].weather_form_id, 0x2000);
        assert_eq!(c.weathers[1].chance, 40);
        assert_eq!(c.sun_texture.as_deref(), Some("sky\\sun_01.dds"));
        assert_eq!(c.sunrise_begin, 6);
        assert_eq!(c.sunset_end, 20);
    }

    /// Regression: #476 — negative-chance WLST entries must decode as
    /// signed (not wrap to huge u32). Mods use -1 as a sentinel /
    /// subtractive weight; pre-#476 the u32 reinterpretation made -1
    /// win `max_by_key` against legitimate positive chances.
    #[test]
    fn parse_clmt_wlst_decodes_negative_chance() {
        let mut wlst_data = Vec::new();
        wlst_data.extend_from_slice(&0x1000u32.to_le_bytes()); // weather 1
        wlst_data.extend_from_slice(&(-1i32).to_le_bytes());    // negative sentinel
        wlst_data.extend_from_slice(&0u32.to_le_bytes());       // global unused
        wlst_data.extend_from_slice(&0x2000u32.to_le_bytes()); // weather 2
        wlst_data.extend_from_slice(&75i32.to_le_bytes());      // 75% chance
        wlst_data.extend_from_slice(&0u32.to_le_bytes());

        let subs = vec![make_sub(b"WLST", wlst_data)];
        let c = parse_clmt(0xBEEF, &subs);
        assert_eq!(c.weathers.len(), 2);
        assert_eq!(c.weathers[0].chance, -1);
        assert_eq!(c.weathers[1].chance, 75);
        // Consumers that `max_by_key` over chance must filter < 0 first;
        // see cell_loader.rs default-weather selection (#476 consumer fix).
    }
}
