//! Global variable and game setting parsers — GLOB, GMST.
//!
//! Both records are essentially `(editor_id, value)` pairs. Globals are
//! script-mutable runtime values; game settings are engine-tunable
//! constants. The data sub-record carries a single value whose type is
//! determined by either an FNAM byte (GLOB) or by the editor_id prefix
//! convention (GMST: `s`/`f`/`i` for string/float/int).

use super::common::{read_f32_at, read_zstring};
use crate::esm::reader::SubRecord;

/// A typed value stored in a GLOB or GMST record.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    /// 32-bit signed integer (the most common type for GLOB).
    Int(i32),
    /// 32-bit float (e.g. fJumpHeightMin).
    Float(f32),
    /// String (e.g. sTalkAlt) — only used by GMST.
    String(String),
    /// Short integer (16-bit, occasionally used by older games).
    Short(i16),
}

#[derive(Debug, Clone)]
pub struct GlobalRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub value: SettingValue,
}

#[derive(Debug, Clone)]
pub struct GameSetting {
    pub form_id: u32,
    pub editor_id: String,
    pub value: SettingValue,
}

pub fn parse_glob(form_id: u32, subs: &[SubRecord]) -> GlobalRecord {
    let mut editor_id = String::new();
    let mut fnam_type = b'f';
    let mut value = SettingValue::Float(0.0);

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => editor_id = read_zstring(&sub.data),
            b"FNAM" if !sub.data.is_empty() => {
                fnam_type = sub.data[0];
            }
            // FLTV: the typed value as a 4-byte little-endian. Type is dictated
            // by FNAM ('s' = short, 'l' = long, 'f' = float).
            b"FLTV" if sub.data.len() >= 4 => {
                value = match fnam_type {
                    b's' => SettingValue::Short(i16::from_le_bytes([sub.data[0], sub.data[1]])),
                    b'l' => SettingValue::Int(i32::from_le_bytes([
                        sub.data[0],
                        sub.data[1],
                        sub.data[2],
                        sub.data[3],
                    ])),
                    _ => SettingValue::Float(read_f32_at(&sub.data, 0).unwrap_or(0.0)),
                };
            }
            _ => {}
        }
    }

    GlobalRecord {
        form_id,
        editor_id,
        value,
    }
}

pub fn parse_gmst(form_id: u32, subs: &[SubRecord]) -> GameSetting {
    let mut editor_id = String::new();
    let mut data_bytes: Vec<u8> = Vec::new();

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => editor_id = read_zstring(&sub.data),
            b"DATA" => data_bytes = sub.data.clone(),
            _ => {}
        }
    }

    // Type is encoded in the first character of the editor_id by Bethesda
    // convention: i… = int, f… = float, s… = string, b… = bool/short.
    let value = match editor_id.as_bytes().first().copied() {
        Some(b'f') if data_bytes.len() >= 4 => {
            SettingValue::Float(read_f32_at(&data_bytes, 0).unwrap_or(0.0))
        }
        Some(b'i') if data_bytes.len() >= 4 => SettingValue::Int(i32::from_le_bytes([
            data_bytes[0],
            data_bytes[1],
            data_bytes[2],
            data_bytes[3],
        ])),
        Some(b'b') if data_bytes.len() >= 4 => SettingValue::Int(i32::from_le_bytes([
            data_bytes[0],
            data_bytes[1],
            data_bytes[2],
            data_bytes[3],
        ])),
        Some(b's') => SettingValue::String(read_zstring(&data_bytes)),
        // Unknown prefix — treat as raw int if 4 bytes, otherwise empty string.
        _ if data_bytes.len() >= 4 => SettingValue::Int(i32::from_le_bytes([
            data_bytes[0],
            data_bytes[1],
            data_bytes[2],
            data_bytes[3],
        ])),
        _ => SettingValue::String(String::new()),
    };

    GameSetting {
        form_id,
        editor_id,
        value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::reader::SubRecord;

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    #[test]
    fn glob_long_value() {
        let subs = vec![
            sub(b"EDID", b"GameDay\0"),
            sub(b"FNAM", &[b'l']),
            sub(b"FLTV", &7i32.to_le_bytes()),
        ];
        let g = parse_glob(0x10, &subs);
        assert_eq!(g.editor_id, "GameDay");
        assert_eq!(g.value, SettingValue::Int(7));
    }

    #[test]
    fn glob_float_value() {
        let subs = vec![
            sub(b"EDID", b"GameYear\0"),
            sub(b"FNAM", &[b'f']),
            sub(b"FLTV", &2281.5f32.to_le_bytes()),
        ];
        let g = parse_glob(0x11, &subs);
        match g.value {
            SettingValue::Float(v) => assert!((v - 2281.5).abs() < 1e-6),
            _ => panic!("expected float"),
        }
    }

    #[test]
    fn gmst_int_via_prefix() {
        let subs = vec![
            sub(b"EDID", b"iMaxLevel\0"),
            sub(b"DATA", &50i32.to_le_bytes()),
        ];
        let g = parse_gmst(0x20, &subs);
        assert_eq!(g.value, SettingValue::Int(50));
    }

    #[test]
    fn gmst_float_via_prefix() {
        let subs = vec![
            sub(b"EDID", b"fJumpHeightMin\0"),
            sub(b"DATA", &76.0f32.to_le_bytes()),
        ];
        let g = parse_gmst(0x21, &subs);
        match g.value {
            SettingValue::Float(v) => assert!((v - 76.0).abs() < 1e-6),
            _ => panic!("expected float"),
        }
    }

    #[test]
    fn gmst_string_via_prefix() {
        let subs = vec![sub(b"EDID", b"sTalkAlt\0"), sub(b"DATA", b"Talk\0")];
        let g = parse_gmst(0x22, &subs);
        assert_eq!(g.value, SettingValue::String("Talk".into()));
    }
}
