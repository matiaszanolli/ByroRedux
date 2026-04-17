//! Shared sub-record helpers used by every record-type parser.
//!
//! Each parser in `records/` consumes a `&[SubRecord]` slice and walks it
//! to extract the fields it cares about. These helpers cover the patterns
//! that show up in every record: null-terminated strings, full-name lookups,
//! model paths, primitive reads at known offsets.

use crate::esm::reader::SubRecord;

/// Read a null-terminated ASCII string from a sub-record's data buffer.
/// Trailing bytes after the first NUL are ignored. Returns `String::new()`
/// for empty buffers.
pub fn read_zstring(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).to_string()
}

/// Find a sub-record by 4-char type code and return its data slice.
pub fn find_sub<'a>(subs: &'a [SubRecord], code: &[u8; 4]) -> Option<&'a [u8]> {
    subs.iter()
        .find(|s| &s.sub_type == code)
        .map(|s| s.data.as_slice())
}

/// Read a sub-record as a null-terminated string. Returns `None` if absent.
pub fn read_string_sub(subs: &[SubRecord], code: &[u8; 4]) -> Option<String> {
    find_sub(subs, code).map(read_zstring)
}

pub fn read_u32_sub(subs: &[SubRecord], code: &[u8; 4]) -> Option<u32> {
    let data = find_sub(subs, code)?;
    if data.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

pub fn read_f32_sub(subs: &[SubRecord], code: &[u8; 4]) -> Option<f32> {
    let data = find_sub(subs, code)?;
    if data.len() < 4 {
        return None;
    }
    Some(f32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// Read a u32 form ID at a known byte offset within a sub-record's data.
pub fn read_u32_at(data: &[u8], offset: usize) -> Option<u32> {
    if data.len() < offset + 4 {
        return None;
    }
    Some(u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

/// Read a u16 at a known byte offset within a sub-record's data.
pub fn read_u16_at(data: &[u8], offset: usize) -> Option<u16> {
    if data.len() < offset + 2 {
        return None;
    }
    Some(u16::from_le_bytes([data[offset], data[offset + 1]]))
}

/// Read an i16 at a known byte offset.
pub fn read_i16_at(data: &[u8], offset: usize) -> Option<i16> {
    if data.len() < offset + 2 {
        return None;
    }
    Some(i16::from_le_bytes([data[offset], data[offset + 1]]))
}

/// Read an f32 at a known byte offset.
pub fn read_f32_at(data: &[u8], offset: usize) -> Option<f32> {
    if data.len() < offset + 4 {
        return None;
    }
    Some(f32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

/// Common name+model+value+weight bundle that nearly every item record carries.
/// Filled in by walking sub-records once before the type-specific dispatch.
#[derive(Debug, Default, Clone)]
pub struct CommonItemFields {
    pub editor_id: String,
    pub full_name: String,
    pub model_path: String,
    pub icon_path: String,
    /// Legacy attached-script reference (`SCRI`, Oblivion / FO3 / FNV).
    /// Form ID of the SCPT record bound to this item. Skyrim+ records
    /// use `VMAD` (Papyrus VM attached data) instead — see `has_script`.
    pub script_form_id: u32,
    pub value: u32,
    pub weight: f32,
    /// True when the record carries a `VMAD` sub-record — Skyrim+'s
    /// Papyrus VM attached-script blob. Full VMAD decoding (script
    /// names + property bindings) is gated on the scripting-as-ECS
    /// work tracked at M30.2 / M48; for now this flag at least makes
    /// the count of script-bearing records discoverable. See #369.
    pub has_script: bool,
}

impl CommonItemFields {
    /// Walk a sub-record list and pull out the universal item fields. Each
    /// type-specific parser starts from this and then handles its own DNAM /
    /// type-specific blocks.
    pub fn from_subs(subs: &[SubRecord]) -> Self {
        let mut out = Self::default();
        for sub in subs {
            match &sub.sub_type {
                b"EDID" => out.editor_id = read_zstring(&sub.data),
                b"FULL" => out.full_name = read_zstring(&sub.data),
                b"MODL" => out.model_path = read_zstring(&sub.data),
                b"ICON" => out.icon_path = read_zstring(&sub.data),
                b"SCRI" if sub.data.len() >= 4 => {
                    out.script_form_id = read_u32_at(&sub.data, 0).unwrap_or(0);
                }
                // VMAD presence-only flag — see `has_script` field doc.
                b"VMAD" => out.has_script = true,
                _ => {}
            }
        }
        out
    }
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

    /// Regression: #369 — VMAD presence on item records flips
    /// `has_script`. Full Papyrus VM data decoding is deferred.
    #[test]
    fn item_vmad_flips_has_script() {
        let subs = vec![
            sub(b"EDID", b"ScriptedItem\0"),
            sub(b"VMAD", b"\x05\x00\x02\x00\x00\x00"),
        ];
        let c = CommonItemFields::from_subs(&subs);
        assert!(c.has_script);
        assert_eq!(c.editor_id, "ScriptedItem");
    }

    #[test]
    fn item_without_vmad_has_script_false() {
        let subs = vec![sub(b"EDID", b"PlainItem\0")];
        let c = CommonItemFields::from_subs(&subs);
        assert!(!c.has_script);
    }
}
