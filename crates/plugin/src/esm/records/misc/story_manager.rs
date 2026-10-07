//! Story Manager node records — `SMBN` (branch) / `SMEN` (event) /
//! `SMQN` (quest), the Creation-era (Skyrim onward) event-driven quest
//! autostart tree. Design authority: `docs/engine/story-manager.md`
//! (#5366). Oblivion / FO3 / FNV author no SM records at all — their
//! quests autostart from scripts — so these maps are empty there.
//!
//! Corpus-verified decode (2026-10-07 census over `Skyrim.esm`,
//! `Fallout4.esm`, `Starfield.esm`; provenance in the design doc §4):
//!
//! - The tree is encoded by pointers, not nesting: `PNAM` = parent node
//!   FormID, `SNAM` = next-sibling node FormID. The same-parent test
//!   passes 441/443 (Skyrim), 218/219 (FO4), 366/366 (Starfield) on
//!   in-tree `SNAM` targets, and sibling order is the evaluation stack.
//!   Every `SMEN` parents to the in-map root `SMBN` `Root` (`0x5B`,
//!   `PNAM = 0`); FO4+ carry the same node under the `0x01` self-master
//!   byte the standard FormID remap normalizes.
//! - `SMEN` carries the event mnemonic in `ENAM` (4 ASCII bytes — `KILL`,
//!   `CLOC`, `CRFT`, …); each master authors exactly one `SMEN` per
//!   mnemonic (24 Skyrim / 17 FO4 / 20 Starfield).
//! - `SMQN` starts quests through `NNAM`: one FormID per subrecord
//!   (Skyrim: single; FO4+/Starfield author pools — the 16-candidate
//!   `MinutemenRecruitmentPostMin02` node).
//! - Node conditions are the standard `CITC` + `CTDA`/`CIS1`/`CIS2`
//!   shape shared with QUST/PACK/INFO, decoded through
//!   [`push_ctda`](super::super::condition::push_ctda).
//!
//! Deliberately raw (`#5366` alignment pass, doc §5): `DNAM` / `XNAM` /
//! `QNAM` are kept as unparsed u32s (observed value domains are in the
//! doc; bit semantics are NOT settled), and the FO4/Starfield tail
//! subrecords (`HNAM`, `RNAM`, `MNAM`, …) land in [`SmNodeRecord::extras`]
//! untouched rather than being guessed at.

use super::super::common::{read_zstring, remap_fid};
use super::super::condition::{push_ctda, ConditionList};
use crate::esm::reader::{FormIdRemap, SubRecord};

/// Which of the three Story Manager record types a node came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SmNodeKind {
    /// `SMBN` — branch node (pure container; the root `Root` node is one).
    #[default]
    Branch,
    /// `SMEN` — event node: roots one event mnemonic's subtree.
    Event,
    /// `SMQN` — quest node: leaf that starts its `NNAM` quest(s).
    Quest,
}

/// One Story Manager node (`SMBN` / `SMEN` / `SMQN` record).
///
/// Pointer fields are stored **remapped into global load-order space**
/// (same contract as every other cross-record FormID the parsers keep),
/// so a consumer joins `parent` / `next_sibling` / `quest_links` against
/// `EsmIndex.story_manager_nodes` / `EsmIndex.quests` directly.
#[derive(Debug, Clone, Default)]
pub struct SmNodeRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub kind: SmNodeKind,
    /// `PNAM` — parent node FormID; `0` on the root node itself.
    /// Absent `PNAM` subrecords (common on SMEN, which parent to the
    /// root by convention) also store `0`.
    pub parent: u32,
    /// `SNAM` — next-sibling node FormID (`0` = last child).
    pub next_sibling: u32,
    /// `ENAM` — the 4-byte event mnemonic, `SMEN` only.
    pub event_mnemonic: Option<[u8; 4]>,
    /// `CITC`-counted CTDA/CIS1/CIS2 conditions. Empty = fires
    /// unconditionally (the shared evaluator's contract).
    pub conditions: ConditionList,
    /// `NNAM` quest FormIDs, `SMQN` only, in authored order. Zero
    /// entries are dropped (the "no link" sentinel), so an empty vec on
    /// a Quest node means the node genuinely starts nothing.
    pub quest_links: Vec<u32>,
    /// `DNAM` u32, raw. Observed domain: 0x0, 0x1, 0x10001, 0x20000,
    /// 0x20001, 0x30001, 0x50001, 0x60000, 0x70001 across Skyrim/FO4/SF.
    /// Bit semantics pending #5366's alignment pass — do not decode.
    pub dnam: Option<u32>,
    /// `XNAM` u32, raw (observed 0/1/2, meaning pending #5366).
    pub xnam: Option<u32>,
    /// `QNAM` u32, raw, `SMQN` only (observed 0..14, meaning pending
    /// #5366 — candidate repeat/priority, unverified).
    pub qnam: Option<u32>,
    /// Unrecognized / pending-decode tail subrecords (FO4+ `HNAM`,
    /// `RNAM`, Starfield `MNAM`, …), preserved verbatim for the
    /// #5366 Phase-4 dialect pass.
    pub extras: Vec<([u8; 4], Vec<u8>)>,
}

/// Parse one `SMBN` / `SMEN` / `SMQN` record's subrecords into an
/// [`SmNodeRecord`]. `kind` comes from the group label the dispatch
/// already matched on. Best-effort like the sibling stub parsers:
/// unknown subrecords land in `extras`, short payloads read as
/// absent, and no input can fail the record.
pub fn parse_sm_node(
    kind: SmNodeKind,
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<FormIdRemap>,
) -> SmNodeRecord {
    let mut node = SmNodeRecord {
        form_id,
        kind,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => node.editor_id = read_zstring(&sub.data),
            b"PNAM" if sub.data.len() >= 4 => {
                node.parent = remap_fid(u32::from_le_bytes(sub.data[..4].try_into().unwrap()), remap);
            }
            b"SNAM" if sub.data.len() >= 4 => {
                node.next_sibling =
                    remap_fid(u32::from_le_bytes(sub.data[..4].try_into().unwrap()), remap);
            }
            b"ENAM" if sub.data.len() >= 4 => {
                node.event_mnemonic = Some(sub.data[..4].try_into().unwrap());
            }
            b"NNAM" if sub.data.len() >= 4 => {
                // Zeros are the "no link" sentinel, not a real record.
                let quest = remap_fid(u32::from_le_bytes(sub.data[..4].try_into().unwrap()), remap);
                if quest != 0 {
                    node.quest_links.push(quest);
                }
            }
            b"CTDA" | b"CTDT" | b"CIS1" | b"CIS2" => {
                push_ctda(sub, remap, &mut node.conditions);
            }
            b"DNAM" if sub.data.len() >= 4 => {
                node.dnam = Some(u32::from_le_bytes(sub.data[..4].try_into().unwrap()));
            }
            b"XNAM" if sub.data.len() >= 4 => {
                node.xnam = Some(u32::from_le_bytes(sub.data[..4].try_into().unwrap()));
            }
            b"QNAM" if sub.data.len() >= 4 => {
                node.qnam = Some(u32::from_le_bytes(sub.data[..4].try_into().unwrap()));
            }
            // CITC is the CTDA count; CTDA presence is authoritative, so
            // the counter itself is read past.
            b"CITC" => {}
            other => node.extras.push((*other, sub.data.clone())),
        }
    }
    node
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::sub;

    /// 24-byte Oblivion-layout CTDA (the shortest `parse_ctda` accepts);
    /// 72 = GetIsID. Layout per `condition.rs`'s own fixture builder.
    fn ctda() -> SubRecord {
        let mut data = vec![0u8, 0, 0, 0];
        data.extend_from_slice(&1.0_f32.to_le_bytes());
        data.extend_from_slice(&72u32.to_le_bytes());
        data.extend_from_slice(&0xDEADu32.to_le_bytes());
        data.extend_from_slice(&0xBEEFu32.to_le_bytes());
        data.extend_from_slice(&[0u8; 4]);
        sub(b"CTDA", &data)
    }

    #[test]
    fn smen_decodes_event_mnemonic_without_edid() {
        // Corpus shape: 23 of 24 Skyrim SMENs carry no EDID at all —
        // the record opens directly with PNAM (parent Root 0x5B).
        let subs = vec![
            sub(b"PNAM", 0x0000_005Bu32.to_le_bytes()),
            sub(b"SNAM", 0x0000_0000u32.to_le_bytes()),
            sub(b"CITC", 0u32.to_le_bytes()),
            sub(b"DNAM", 0u32.to_le_bytes()),
            sub(b"XNAM", 0u32.to_le_bytes()),
            sub(b"ENAM", b"CRFT"),
        ];
        let node = parse_sm_node(SmNodeKind::Event, 0x0003_9D86, &subs, &None);
        assert_eq!(node.event_mnemonic, Some(*b"CRFT"));
        assert_eq!(node.parent, 0x5B);
        assert_eq!(node.next_sibling, 0);
        assert_eq!(node.editor_id, "");
        assert!(node.conditions.is_empty());
        assert_eq!(node.dnam, Some(0));
    }

    #[test]
    fn smqn_decodes_quest_links_conditions_and_raw_ints() {
        let subs = vec![
            sub(b"EDID", b"MS05KingOlafsFestivalStarter\0"),
            sub(b"PNAM", 0x0009_0028u32.to_le_bytes()),
            sub(b"SNAM", 0x0001_703Cu32.to_le_bytes()),
            sub(b"CITC", 1u32.to_le_bytes()),
            ctda(),
            sub(b"CIS2", b"::PlayerThievingAndNotPaying_var\0"),
            sub(b"DNAM", 0x0001_0001u32.to_le_bytes()),
            sub(b"XNAM", 0u32.to_le_bytes()),
            sub(b"QNAM", 1u32.to_le_bytes()),
            sub(b"NNAM", 0x000F_0A10u32.to_le_bytes()),
        ];
        let node = parse_sm_node(SmNodeKind::Quest, 0x0001_703E, &subs, &None);
        assert_eq!(node.quest_links, vec![0x000F_0A10]);
        assert_eq!(node.conditions.len(), 1);
        assert_eq!(node.conditions[0].function_index, 72);
        assert!(node.conditions[0].param_2_text.is_some());
        assert_eq!(node.dnam, Some(0x0001_0001));
        assert_eq!(node.qnam, Some(1));
        // DNAM is deliberately not decoded into flags yet (#5366 §5).
        assert_eq!(node.event_mnemonic, None);
    }

    #[test]
    fn smqn_drops_zero_nnam_and_keeps_quest_pool_order() {
        let subs = vec![
            sub(b"NNAM", 0u32.to_le_bytes()),
            sub(b"NNAM", 0x0015_7577u32.to_le_bytes()),
            sub(b"NNAM", 0x0016_4167u32.to_le_bytes()),
        ];
        let node = parse_sm_node(SmNodeKind::Quest, 0x0024_9E50, &subs, &None);
        assert_eq!(node.quest_links, vec![0x0015_7577, 0x0016_4167]);
    }

    #[test]
    fn remap_promotes_pointer_fields_into_load_order_space() {
        // A DLC plugin authors master refs with mod-byte 0; the remap
        // folds them onto the master's global slot, exactly like every
        // other cross-record FormID. Plugin sits at slot 2, its master
        // at slot 1 (the Update.esm shape).
        let remap = FormIdRemap::regular(2, vec![1]);
        let subs = vec![
            sub(b"PNAM", 0x0001_7045u32.to_le_bytes()),
            sub(b"SNAM", 0x0001_7046u32.to_le_bytes()),
            sub(b"NNAM", 0x0001_7047u32.to_le_bytes()),
        ];
        let node = parse_sm_node(SmNodeKind::Quest, 0x0201_4CBA, &subs, &Some(remap));
        assert_eq!(node.parent, 0x0101_7045);
        assert_eq!(node.next_sibling, 0x0101_7046);
        assert_eq!(node.quest_links, vec![0x0101_7047]);
    }

    #[test]
    fn fo4_dialect_tail_lands_in_extras_verbatim() {
        let subs = vec![
            sub(b"HNAM", [0x01, 0x00]),
            sub(b"RNAM", [0xAA, 0xBB, 0xCC]),
            sub(b"MNAM", 0x1234_5678u32.to_le_bytes()),
            sub(b"UNKN", [0xFF]),
        ];
        let node = parse_sm_node(SmNodeKind::Quest, 0x0024_9E50, &subs, &None);
        assert_eq!(node.extras.len(), 4);
        assert_eq!(node.extras[0], (*b"HNAM", vec![0x01, 0x00]));
        assert_eq!(node.extras[3], (*b"UNKN", vec![0xFF]));
    }

    #[test]
    fn short_pointer_payloads_read_as_absent_not_garbage() {
        let subs = vec![sub(b"PNAM", [0x5B]), sub(b"ENAM", b"KI")];
        let node = parse_sm_node(SmNodeKind::Event, 1, &subs, &None);
        assert_eq!(node.parent, 0);
        assert_eq!(node.event_mnemonic, None);
    }
}
