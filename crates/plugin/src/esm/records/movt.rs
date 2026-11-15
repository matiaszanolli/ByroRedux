//! `MOVT` movement types (Skyrim, Fallout 4, Starfield).
//!
//! Decoded for M65 (SNK-STN-005): the sneak stance's speed on Skyrim and
//! Fallout 4 is the ratio of the sneaking movement type's `SPED` to the
//! default one's, because the playable races author no `SNMV`
//! (`docs/engine/sneak-detection.md` §2, V2). Only the EditorID, the `MNAM`
//! name and the raw `SPED` floats are kept. The float layout differs per game
//! (xEdit `wbDefinitionsTES5`: walk / run per direction; `wbDefinitionsFO4`:
//! four floats per direction, the first and last unused), so the indices are
//! the consumer's row (`byroredux_core::character::StanceModel`), not decoded
//! here.

use crate::esm::reader::SubRecord;

/// One `MOVT` record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MovementType {
    pub form_id: u32,
    pub editor_id: String,
    /// `MNAM`, the name the behaviour graph selects the type by.
    pub name: String,
    /// `SPED`, every little-endian `f32` in order.
    pub speeds: Vec<f32>,
}

/// Parse a `MOVT` record's sub-records. A truncated trailing float is dropped.
pub fn parse_movt(form_id: u32, subs: &[SubRecord]) -> MovementType {
    let mut record = MovementType { form_id, ..MovementType::default() };
    let text = |data: &[u8]| String::from_utf8_lossy(data).trim_end_matches('\0').to_owned();
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = text(&sub.data),
            b"MNAM" => record.name = text(&sub.data),
            b"SPED" => {
                record.speeds = sub
                    .data
                    .chunks_exact(4)
                    .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                    .collect();
            }
            _ => {}
        }
    }
    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::sub;

    #[test]
    fn decodes_the_editor_id_name_and_every_speed() {
        let speeds: Vec<u8> = [41.44f32, 200.0, 41.44, 200.0, 47.2, 222.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .chain([0xAB, 0xCD]) // a truncated tail is dropped
            .collect();
        let record = parse_movt(
            0x0003580B,
            &[
                sub(b"EDID", b"NPC_Sneaking_MT\0"),
                sub(b"MNAM", b"NPCSneaking\0"),
                sub(b"SPED", speeds),
            ],
        );
        assert_eq!(record.editor_id, "NPC_Sneaking_MT");
        assert_eq!(record.name, "NPCSneaking");
        assert_eq!(record.speeds, vec![41.44, 200.0, 41.44, 200.0, 47.2, 222.0]);
    }
}
