//! Container and leveled-list parsers — CONT, LVLI, LVLN.
//!
//! Containers and leveled lists share an inventory-entry sub-record format
//! (`CNTO` for containers, `LVLO` for leveled lists). Each entry references
//! a base item form by ID and gives a count or a level/chance.

use super::common::{read_lstring_or_zstring, read_u32_at, read_zstring};
use crate::esm::reader::SubRecord;

/// One entry in a container's inventory list.
#[derive(Debug, Clone, Copy)]
pub struct InventoryEntry {
    pub item_form_id: u32,
    pub count: i32,
}

/// One entry in a leveled list (LVLI / LVLN).
#[derive(Debug, Clone, Copy)]
pub struct LeveledEntry {
    /// Player level at which this entry can appear.
    pub level: u16,
    /// Form ID of the item or NPC to spawn.
    pub form_id: u32,
    /// How many copies (1 for most NPC entries, more for arrows/ammo).
    pub count: u16,
}

#[derive(Debug, Clone)]
pub struct ContainerRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub model_path: String,
    /// Total weight (used as carry-weight cap for player containers).
    pub weight: f32,
    /// CONT DATA flags byte. FNV layout is `weight (f32) + flags (u8)`
    /// = 5 bytes; pre-#376 the parser gated at `>= 4` and never read
    /// this byte. Bit 0 = respawns (vendor restock + cell-reset loot
    /// refresh), bit 1 = reserved/unknown. `0` on Oblivion (4-byte
    /// DATA) and on any record where the byte was missing — the field
    /// is the absence of the respawn flag, not an unknown state.
    pub flags: u8,
    /// Open/close sound form IDs.
    pub open_sound: u32,
    pub close_sound: u32,
    /// Form ID of an attached script (if any).
    pub script_form_id: u32,
    pub contents: Vec<InventoryEntry>,
}

/// CONT DATA flag bits (FNV / FO3 / Skyrim+; absent on Oblivion).
/// Container respawns on cell reset — vendor restock, looted-chest
/// refresh on the cycle the cell loader applies. Pre-#376 invisible
/// to gameplay systems.
pub const CONT_FLAG_RESPAWNS: u8 = 0x01;

#[derive(Debug, Clone)]
pub struct LeveledList {
    pub form_id: u32,
    pub editor_id: String,
    /// 0–100 chance the entire list rolls "nothing".
    pub chance_none: u8,
    /// Flags (bit 0: calculate from all levels, bit 1: calculate for each item).
    pub flags: u8,
    pub entries: Vec<LeveledEntry>,
}

pub fn parse_cont(form_id: u32, subs: &[SubRecord]) -> ContainerRecord {
    let mut record = ContainerRecord {
        form_id,
        editor_id: String::new(),
        full_name: String::new(),
        model_path: String::new(),
        weight: 0.0,
        flags: 0,
        open_sound: 0,
        close_sound: 0,
        script_form_id: 0,
        contents: Vec::new(),
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = read_zstring(&sub.data),
            b"FULL" => record.full_name = read_lstring_or_zstring(&sub.data),
            b"MODL" => record.model_path = read_zstring(&sub.data),
            b"SCRI" if sub.data.len() >= 4 => {
                record.script_form_id = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            // CNTO: item form ID (u32) + count (i32)
            b"CNTO" if sub.data.len() >= 8 => {
                let item_form_id = read_u32_at(&sub.data, 0).unwrap_or(0);
                let count =
                    i32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]);
                record.contents.push(InventoryEntry {
                    item_form_id,
                    count,
                });
            }
            // DATA: weight(f32) + flags(u8). FNV / FO3 / Skyrim+ ship
            // 5 bytes; Oblivion is 4 bytes (no flags trailer). Read
            // weight from the canonical 4-byte prefix and surface the
            // optional flags byte when present. Pre-#376 (F2-02) the
            // gate read only the weight half — the respawn bit on
            // every vanilla FNV trash bag / loot crate was invisible
            // to gameplay systems.
            b"DATA" if sub.data.len() >= 4 => {
                record.weight =
                    f32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                if sub.data.len() >= 5 {
                    record.flags = sub.data[4];
                }
            }
            // SNAM: open sound form ID
            b"SNAM" if sub.data.len() >= 4 => {
                record.open_sound = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            // QNAM: close sound form ID
            b"QNAM" if sub.data.len() >= 4 => {
                record.close_sound = read_u32_at(&sub.data, 0).unwrap_or(0);
            }
            _ => {}
        }
    }
    record
}

/// Shared parser for both LVLI and LVLN — they have the same sub-record layout.
pub fn parse_leveled_list(form_id: u32, subs: &[SubRecord]) -> LeveledList {
    let mut record = LeveledList {
        form_id,
        editor_id: String::new(),
        chance_none: 0,
        flags: 0,
        entries: Vec::new(),
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = read_zstring(&sub.data),
            b"LVLD" if !sub.data.is_empty() => record.chance_none = sub.data[0],
            b"LVLF" if !sub.data.is_empty() => record.flags = sub.data[0],
            // LVLO: level(u16) + pad(u16) + form_id(u32) + count(u16) + pad(u16)
            b"LVLO" if sub.data.len() >= 12 => {
                let level = u16::from_le_bytes([sub.data[0], sub.data[1]]);
                let entry_form = read_u32_at(&sub.data, 4).unwrap_or(0);
                let count = u16::from_le_bytes([sub.data[8], sub.data[9]]);
                record.entries.push(LeveledEntry {
                    level,
                    form_id: entry_form,
                    count,
                });
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

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    fn cnto_bytes(form_id: u32, count: i32) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&form_id.to_le_bytes());
        d.extend_from_slice(&count.to_le_bytes());
        d
    }

    fn lvlo_bytes(level: u16, form_id: u32, count: u16) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&level.to_le_bytes());
        d.extend_from_slice(&0u16.to_le_bytes()); // pad
        d.extend_from_slice(&form_id.to_le_bytes());
        d.extend_from_slice(&count.to_le_bytes());
        d.extend_from_slice(&0u16.to_le_bytes()); // pad
        d
    }

    #[test]
    fn cont_extracts_inventory_and_weight() {
        let mut data = Vec::new();
        data.extend_from_slice(&50.0f32.to_le_bytes());
        data.push(0); // flags

        let subs = vec![
            sub(b"EDID", b"TestChest\0"),
            sub(b"FULL", b"Big Chest\0"),
            sub(b"MODL", b"meshes\\furn\\chest.nif\0"),
            sub(b"DATA", &data),
            sub(b"CNTO", &cnto_bytes(0x100, 5)),
            sub(b"CNTO", &cnto_bytes(0x200, -1)),
        ];
        let r = parse_cont(0xABCD, &subs);
        assert_eq!(r.editor_id, "TestChest");
        assert_eq!(r.full_name, "Big Chest");
        assert_eq!(r.model_path, "meshes\\furn\\chest.nif");
        assert!((r.weight - 50.0).abs() < 1e-6);
        assert_eq!(r.contents.len(), 2);
        assert_eq!(r.contents[0].item_form_id, 0x100);
        assert_eq!(r.contents[0].count, 5);
        assert_eq!(r.contents[1].count, -1);
        // Synthetic flags=0 above; the dedicated #376 / F2-02
        // regression tests below cover non-zero respawn.
        assert_eq!(r.flags, 0);
    }

    /// Regression for #376 / F2-02: the FNV CONT DATA `flags` byte
    /// (5th byte of the 5-byte payload) must surface as
    /// `ContainerRecord.flags`. Pre-fix the gate at `>= 4` ignored
    /// it. `GenericTrashbag01`-class containers carry bit 0 to mark
    /// "respawns on cell reset", driving vendor restock and looted-
    /// chest refresh — invisible to gameplay systems pre-#376.
    #[test]
    fn cont_data_extracts_fnv_5byte_flags_byte() {
        let mut data = Vec::new();
        data.extend_from_slice(&12.5f32.to_le_bytes());
        data.push(CONT_FLAG_RESPAWNS); // 0x01 — respawns on cell reset

        let subs = vec![
            sub(b"EDID", b"GenericTrashbag01\0"),
            sub(b"DATA", &data),
        ];
        let r = parse_cont(0xCAFE, &subs);
        assert!((r.weight - 12.5).abs() < 1e-6);
        assert_eq!(
            r.flags & CONT_FLAG_RESPAWNS,
            CONT_FLAG_RESPAWNS,
            "FNV trash-bag respawn flag must round-trip through CONT DATA",
        );
    }

    /// Oblivion CONT DATA is 4 bytes (no flags trailer). The parser
    /// must accept the shorter shape, populate weight, and leave
    /// `flags = 0` rather than reading past end-of-data. The default
    /// is the *absence* of any flag, not an unknown state.
    #[test]
    fn cont_data_handles_oblivion_4byte_payload_without_overrun() {
        let mut data = Vec::new();
        data.extend_from_slice(&5.0f32.to_le_bytes()); // 4 bytes, no flags

        let subs = vec![sub(b"EDID", b"OblivionChest\0"), sub(b"DATA", &data)];
        let r = parse_cont(0x1234, &subs);
        assert!((r.weight - 5.0).abs() < 1e-6);
        assert_eq!(
            r.flags, 0,
            "Oblivion 4-byte DATA must leave flags = 0, not panic on length",
        );
    }

    /// Bit 1 (reserved/unknown per UESP) — defensive round-trip so a
    /// future "what does bit 1 mean" investigation doesn't have to
    /// re-instrument the parser. Validates that the byte goes
    /// through verbatim, not just bit 0.
    #[test]
    fn cont_data_preserves_full_flags_byte_not_just_bit_zero() {
        let mut data = Vec::new();
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.push(0b0000_0011); // bit 0 + bit 1

        let subs = vec![sub(b"DATA", &data)];
        let r = parse_cont(0, &subs);
        assert_eq!(r.flags, 0b0000_0011);
    }

    #[test]
    fn lvli_extracts_entries_and_chance() {
        let subs = vec![
            sub(b"EDID", b"LL_Test\0"),
            sub(b"LVLD", &[25u8]),
            sub(b"LVLF", &[0x01u8]),
            sub(b"LVLO", &lvlo_bytes(1, 0x100, 1)),
            sub(b"LVLO", &lvlo_bytes(10, 0x200, 3)),
            sub(b"LVLO", &lvlo_bytes(20, 0x300, 1)),
        ];
        let r = parse_leveled_list(0x9999, &subs);
        assert_eq!(r.chance_none, 25);
        assert_eq!(r.flags, 0x01);
        assert_eq!(r.entries.len(), 3);
        assert_eq!(r.entries[1].level, 10);
        assert_eq!(r.entries[1].form_id, 0x200);
        assert_eq!(r.entries[1].count, 3);
    }
}
