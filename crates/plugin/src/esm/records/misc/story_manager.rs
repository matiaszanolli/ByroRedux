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
//! Decoded since Phase 3 (`#5366` doc §3.3): the `DNAM` policy bits
//! (random / do-all-before-repeating / shares-event — see
//! [`SmNodePolicies`]) and `RNAM` per-quest reset hours (see
//! [`SmQuestLink`]). Deliberately raw (`#5366` alignment pass, doc §5):
//! the open `DNAM` bits (`0x2`, `0x40000`), `XNAM` / `QNAM`, and the
//! FO4+/Starfield `HNAM` (hours-shaped float, node-level-reset
//! candidate) / `MNAM` stay unparsed u32s (observed domains in the
//! doc) rather than being guessed at; unrecognized tail subrecords
//! land in [`SmNodeRecord::extras`] verbatim.

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

/// One `NNAM` quest link in an `SMQN`'s pool, with its `RNAM` companion.
///
/// `RNAM` (346/448 Skyrim `SMQN`s) is the CK's per-quest **Hours until
/// reset** — "the Story Manager will not attempt to start this quest
/// again until the indicated number of Game Hours has passed. If this
/// number is 0.0000, this check is ignored" (SM Event Node, local CK
/// wiki). It follows its `NNAM` in the subrecord stream (corpus:
/// `MQ304SovngardeScenes` alternates `NNAM,RNAM=2.4 / NNAM,RNAM=4.8 /
/// NNAM,RNAM=4.8`; the `WEBountyCollector*` holds carry 1152.0 = 48
/// game days). `0.0` = no reset check authored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmQuestLink {
    pub form_id: u32,
    /// `RNAM` game-hours reset window; `0.0` when no `RNAM` follows the
    /// `NNAM` (the ignored-by-contract default).
    pub reset_hours: f32,
}

/// `DNAM` node-policy bits, decoded (#5366 Phase 3 census, 2026-10-07).
///
/// The CK's node properties (SM Event Node, local wiki) map onto the
/// observed value domain (`{0, 1, 2, 0x10000, 0x10001, …, 0x70001}` — a
/// low byte plus `0x10000/0x20000/0x40000`) as:
///
/// - **`0x1` = Random** (else Stacked): carried by exactly the
///   `CompanionsRadiantNode`-style branches and random quest nodes;
///   `SMEN` roots are always 0.
/// - **`0x20000` = Shares Event**: every `*SHARES*`-named `SMQN` in the
///   corpus carries it (`WIGreetingNodeSHARES` = `0x20000`,
///   `BQ*NodeSHARES` = `0x30001`, …), no branch ever sets it — matching
///   the CK UI, where the checkbox exists on quest nodes only.
///   `WIKillEventsRandomChance`=`0x10000` vs sibling
///   `WIKillEventsNoRandomChanceSHARES`=`0x20000` splits the bits.
/// - **`0x10000` = Do all before repeating**: clusters exactly on the
///   radiant-cycle families (BQ bounty holds, WE/WI wilderness
///   incidents, `WEPriorityQuests`) whose authored behavior is
///   round-robin over the quest pool.
///
/// Still `[open]` (raw `dnam`/`xnam`/`qnam` kept verbatim): `0x2`
/// (`MS04/MS06IncreaseLevelNodeSHARES`=`0x20002`; candidate: Warn if no
/// child quest started), `0x40000` (`FavorChangeLocation*`=`0x60000`;
/// candidates: the Num-quests-to-run / Max-concurrent checkboxes, with
/// `QNAM` 0–68 as one of the numbers), and `XNAM` (0–2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SmNodePolicies {
    /// `DNAM & 0x1` — random child/pool selection instead of stacked
    /// order.
    pub random: bool,
    /// `DNAM & 0x10000` — attempt every pool quest before repeating one.
    pub do_all_before_repeating: bool,
    /// `DNAM & 0x20000` — keep processing the event after this quest
    /// node; clear consumes it.
    pub shares_event: bool,
}

impl SmNodePolicies {
    pub fn from_dnam(dnam: u32) -> Self {
        Self {
            random: dnam & 0x1 != 0,
            do_all_before_repeating: dnam & 0x1_0000 != 0,
            shares_event: dnam & 0x2_0000 != 0,
        }
    }
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
    /// `NNAM` quest pool with each link's `RNAM` reset window, `SMQN`
    /// only, in authored order. Zero entries are dropped (the "no link"
    /// sentinel), so an empty vec on a Quest node means the node
    /// genuinely starts nothing.
    pub quests: Vec<SmQuestLink>,
    /// `DNAM` policy bits, decoded (see [`SmNodePolicies`]). The raw
    /// u32 is kept alongside for the still-open bits.
    pub policies: SmNodePolicies,
    /// `DNAM` u32, raw. Decoded bits: 0x1 random / 0x10000
    /// do-all-before-repeating / 0x20000 shares-event. Open: 0x2, 0x40000
    /// (see [`SmNodePolicies`]'s doc).
    pub dnam: Option<u32>,
    /// `HNAM` u32, FO4+/Starfield only, raw. Read as f32 LE the corpus
    /// values are hours-magnitude floats — 72.0 on
    /// `MinutemenRecruitmentPostMin02`, 24.0/12.0/1.0 on FO4 encounter
    /// nodes, 0.3 on Starfield conversations nodes — the shape of a
    /// node-level reset window. **[open]**: semantics unverified; the
    /// runtime does not gate on it.
    pub hnam: Option<u32>,
    /// `MNAM` u32, FO4 (rare) / Starfield (common, sits between `XNAM`
    /// and `QNAM`), raw. Small ints (observed 1–22). **[open]**.
    pub mnam: Option<u32>,
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
                    node.quests.push(SmQuestLink {
                        form_id: quest,
                        reset_hours: 0.0,
                    });
                }
            }
            b"RNAM" if sub.data.len() >= 4 => {
                // Per-quest "Hours until reset" for the NNAM it follows
                // (see `SmQuestLink`). A stray RNAM with no preceding
                // link is malformed authoring — skip it loudly.
                let hours = f32::from_le_bytes(sub.data[..4].try_into().unwrap());
                match node.quests.last_mut() {
                    Some(link) => link.reset_hours = hours,
                    None => log::warn!(
                        "#5366: RNAM reset window {:?}h with no preceding NNAM — dropped",
                        hours
                    ),
                }
            }
            b"CTDA" | b"CTDT" | b"CIS1" | b"CIS2" => {
                push_ctda(sub, remap, &mut node.conditions);
            }
            b"DNAM" if sub.data.len() >= 4 => {
                let dnam = u32::from_le_bytes(sub.data[..4].try_into().unwrap());
                node.policies = SmNodePolicies::from_dnam(dnam);
                node.dnam = Some(dnam);
            }
            b"XNAM" if sub.data.len() >= 4 => {
                node.xnam = Some(u32::from_le_bytes(sub.data[..4].try_into().unwrap()));
            }
            b"QNAM" if sub.data.len() >= 4 => {
                node.qnam = Some(u32::from_le_bytes(sub.data[..4].try_into().unwrap()));
            }
            b"HNAM" if sub.data.len() >= 4 => {
                node.hnam = Some(u32::from_le_bytes(sub.data[..4].try_into().unwrap()));
            }
            b"MNAM" if sub.data.len() >= 4 => {
                node.mnam = Some(u32::from_le_bytes(sub.data[..4].try_into().unwrap()));
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
        assert_eq!(
            node.quests,
            vec![SmQuestLink {
                form_id: 0x000F_0A10,
                reset_hours: 0.0
            }]
        );
        assert_eq!(node.conditions.len(), 1);
        assert_eq!(node.conditions[0].function_index, 72);
        assert!(node.conditions[0].param_2_text.is_some());
        assert_eq!(node.dnam, Some(0x0001_0001));
        // 0x10001 = random | do-all-before-repeating, not sharing.
        assert_eq!(
            node.policies,
            SmNodePolicies {
                random: true,
                do_all_before_repeating: true,
                shares_event: false
            }
        );
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
        assert_eq!(
            node.quests.iter().map(|link| link.form_id).collect::<Vec<_>>(),
            vec![0x0015_7577, 0x0016_4167]
        );
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
        assert_eq!(
            node.quests.iter().map(|link| link.form_id).collect::<Vec<_>>(),
            vec![0x0101_7047]
        );
    }

    #[test]
    fn fo4_dialect_tail_lands_in_extras_verbatim() {
        // HNAM/MNAM decode as typed-raw fields since Phase 4 (a full
        // 4-byte payload); sub-4-byte tails are short authoring and ride
        // in extras verbatim alongside the genuinely unrecognized.
        let subs = vec![
            sub(b"HNAM", [0x01, 0x00]),
            sub(b"RNAM", [0xAA, 0xBB, 0xCC]),
            sub(b"MNAM", 0x1234_5678u32.to_le_bytes()),
            sub(b"UNKN", [0xFF]),
        ];
        let node = parse_sm_node(SmNodeKind::Quest, 0x0024_9E50, &subs, &None);
        assert_eq!(node.extras.len(), 3);
        assert_eq!(node.extras[0], (*b"HNAM", vec![0x01, 0x00]));
        assert_eq!(node.extras[2], (*b"UNKN", vec![0xFF]));
        // Short HNAM reads as absent; the full MNAM decoded.
        assert_eq!(node.hnam, None);
        assert_eq!(node.mnam, Some(0x1234_5678));
    }

    #[test]
    fn short_pointer_payloads_read_as_absent_not_garbage() {
        let subs = vec![sub(b"PNAM", [0x5B]), sub(b"ENAM", b"KI")];
        let node = parse_sm_node(SmNodeKind::Event, 1, &subs, &None);
        assert_eq!(node.parent, 0);
        assert_eq!(node.event_mnemonic, None);
    }
    /// `RNAM` reset hours pair with their preceding `NNAM` (the
    /// MQ304SovngardeScenes corpus shape) and a stray RNAM is dropped
    /// loudly.
    #[test]
    fn rnam_reset_hours_pair_with_their_nnam() {
        let subs = vec![
            // Stray: arrives before any NNAM.
            sub(b"RNAM", 48.0_f32.to_le_bytes()),
            sub(b"NNAM", 0x000E_DF6Au32.to_le_bytes()),
            sub(b"RNAM", 2.4_f32.to_le_bytes()),
            sub(b"NNAM", 0x000F_1A41u32.to_le_bytes()),
            sub(b"RNAM", 4.8_f32.to_le_bytes()),
            // No RNAM follows this link — the ignored-by-contract 0.0.
            sub(b"NNAM", 0x000F_1A44u32.to_le_bytes()),
        ];
        let node = parse_sm_node(SmNodeKind::Quest, 1, &subs, &None);
        assert_eq!(
            node.quests,
            vec![
                SmQuestLink { form_id: 0x000E_DF6A, reset_hours: 2.4 },
                SmQuestLink { form_id: 0x000F_1A41, reset_hours: 4.8 },
                SmQuestLink { form_id: 0x000F_1A44, reset_hours: 0.0 },
            ]
        );
    }
    /// FO4/SF tail: `HNAM` (hours-shaped float, raw) and `MNAM` decode
    /// as typed-raw u32s instead of landing in `extras`.
    #[test]
    fn fo4_tail_hnam_mnam_decode_typed_raw() {
        let subs = vec![
            sub(b"EDID", b"MinutemenRecruitmentPostMin02\0"),
            sub(b"DNAM", 0x0001_0001u32.to_le_bytes()),
            sub(b"XNAM", 0u32.to_le_bytes()),
            sub(b"HNAM", 72.0_f32.to_le_bytes()),
            sub(b"QNAM", 0u32.to_le_bytes()),
            sub(b"NNAM", 0x0015_7577u32.to_le_bytes()),
        ];
        let node = parse_sm_node(SmNodeKind::Quest, 0x0024_9E50, &subs, &None);
        assert_eq!(node.hnam, Some(0x4290_0000)); // 72.0f as raw LE u32
        assert_eq!(node.mnam, None);
        assert!(
            node.extras.is_empty(),
            "HNAM no longer rides in extras: {:?}",
            node.extras
        );
        // Starfield shape: MNAM between XNAM and QNAM.
        let subs = vec![
            sub(b"DNAM", 0u32.to_le_bytes()),
            sub(b"XNAM", 0u32.to_le_bytes()),
            sub(b"MNAM", 0x0000_0016u32.to_le_bytes()),
            sub(b"QNAM", 0u32.to_le_bytes()),
            sub(b"NNAM", 0x0016_4167u32.to_le_bytes()),
        ];
        let node = parse_sm_node(SmNodeKind::Quest, 1, &subs, &None);
        assert_eq!(node.mnam, Some(22));
    }
}
