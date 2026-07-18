//! `DIAL` / `INFO` / `MESG` dialogue and message records.

use super::super::common::{read_lstring_or_zstring, read_zstring};
use super::super::condition::{parse_ctda, remap_condition_form_ids, ConditionList};
use crate::esm::reader::SubRecord;
use crate::esm::sub_reader::SubReader;

/// `DIAL` dialogue topic record. Parent of INFO dialogue lines (which
/// live in a nested GRUP tree — tracked as a follow-up; the current
/// `extract_records` walker takes a single record type and can't
/// simultaneously emit DIAL + INFO). This stub captures the topic's
/// quest owners (QSTI refs, 4 bytes each) so NPC / quest systems can
/// enumerate topics without re-parsing.
#[derive(Debug, Clone, Default)]
pub struct DialRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Quest form IDs that own this dialogue topic (one per QSTI
    /// sub-record). FO3/FNV topics often list multiple owners.
    pub quest_refs: Vec<u32>,
    /// `DATA` dialogue-type byte 0 — Topic / Conversation / Combat /
    /// Persuasion / Detection / Service / Miscellaneous (Oblivion enum).
    /// Oblivion's DATA is a single byte; FO3+ widen it (type byte +
    /// flags) but byte 0 is the type in every game, so the byte-0 read is
    /// cross-game safe. 0 (Topic) when DATA is absent. Captured raw;
    /// per-game enum mapping is downstream consumer work.
    pub dial_type: u8,
    /// INFO topic responses parsed from the DIAL's `Topic Children`
    /// sub-GRUP (group_type == 7). Pre-#631 the children were silently
    /// skipped because `extract_records` filters on a single record
    /// type; this field is now populated by the dedicated
    /// `extract_dial_with_info` walker. Each entry is one branch of the
    /// dialogue (a single NPC response + its conditions / triggers).
    pub infos: Vec<InfoRecord>,
}

/// Resolved conversation tree structure — groups INFOs into PNAM chains
/// (reading-order sequences), and surfaces TCLT as inter-topic edges.
/// Built as a pure function over already-parsed DialRecord data.
#[derive(Debug, Clone)]
pub struct ConversationTree {
    /// PNAM chains ordered from head (previous_info==0) to tail.
    /// Each chain is a Vec of INFO form_ids in reading order.
    pub chains: Vec<Vec<u32>>,
    /// Inter-topic edges: source_info_form_id → [destination_topic_form_ids].
    /// Maps each INFO (by form_id) to the topics it routes to via TCLT.
    pub topic_links: std::collections::HashMap<u32, Vec<u32>>,
}

/// Error building a conversation tree (e.g., cycles in PNAM chain).
#[derive(Debug, Clone)]
pub enum ConversationTreeError {
    PnamCycle { info_form_id: u32 },
}

/// `INFO` dialogue topic response. One per branch of an `NPC says X
/// when Y` choice tree, owned by the parent `DIAL` topic via the
/// nested Topic Children GRUP. Stub captures the response text +
/// type byte + sibling links so quest / dialogue systems can
/// enumerate branches without re-parsing. Conditions (CTDA),
/// scripts (SCHR/SCDA), and edits (NAM3) are deferred until the
/// condition runtime lands. See #631.
#[derive(Debug, Clone, Default)]
pub struct InfoRecord {
    pub form_id: u32,
    /// Response text shown / spoken to the player (NAM1).
    pub response_text: String,
    /// Designer notes — usually direction for the voice actor (NAM2).
    pub designer_notes: String,
    /// `TRDT` Emotion Type — the low byte of the `EmotionType` `u32` at
    /// TRDT offset 0: 0=Neutral, 1=Anger, 2=Disgust, 3=Fear, 4=Sad,
    /// 5=Happy, 6=Surprise (Oblivion / FO3 / FNV; Skyrim keeps the
    /// EmotionType-u32 @0 layout). The byte-0 histogram across all
    /// 23,877 `Oblivion.esm` TRDT subrecords is exactly this 0–6
    /// distribution — it is the emotion, NOT a response number (the
    /// real response index is [`Self::response_number`]). 0 when TRDT is
    /// absent. See #1304 (was mislabeled `response_type`).
    pub emotion_type: u8,
    /// `TRDT` Response number — byte 12, after `EmotionType` (u32 @0),
    /// `Emotion Value` (i32 @4), and 4 unused bytes @8. The actual
    /// dialogue-response index within the branch. 0 when TRDT is shorter
    /// than 13 bytes. See #1304.
    pub response_number: u8,
    /// `TCLT` topic-link ref — IDs of other DIAL topics that this
    /// branch routes the conversation to. Multiple TCLTs are
    /// concatenated.
    pub topic_links: Vec<u32>,
    /// `PNAM` previous-info ref — the prior INFO in this branch. 0
    /// means "this is the first response in the chain".
    pub previous_info: u32,
    /// `ANAM` actor form ID — restricts this response to a specific NPC.
    /// 0 means the response works for any actor.
    pub actor_form_id: u32,
    /// Conditions attached to this response (CTDA sub-records).
    pub conditions: ConditionList,
}

pub fn parse_dial(
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<crate::esm::reader::FormIdRemap>,
) -> DialRecord {
    let mut out = DialRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"QSTI" if sub.data.len() >= 4 => {
                if let Ok(q) = SubReader::new(&sub.data).u32() {
                    let remapped = remap.as_ref().map_or(q, |r| r.remap(q));
                    out.quest_refs.push(remapped);
                }
            }
            // DATA byte 0 = dialogue type, cross-game safe (Oblivion: 1 byte;
            // FO3+: wider, byte 0 still the type). #1307 / OBL-D3-...-03.
            b"DATA" if !sub.data.is_empty() => out.dial_type = sub.data[0],
            _ => {}
        }
    }
    out
}

pub fn parse_info(
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<crate::esm::reader::FormIdRemap>,
) -> InfoRecord {
    let mut out = InfoRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"NAM1" => out.response_text = read_lstring_or_zstring(&sub.data),
            b"NAM2" => out.designer_notes = read_zstring(&sub.data),
            b"TRDT" if !sub.data.is_empty() => {
                // TES4 TRDT layout: EmotionType(u32 @0) + EmotionValue
                // (i32 @4) + unused[4] @8 + Response number(u8 @12) +
                // unused[3]. Byte 0 is the emotion (0–6), not a response
                // number; the response index lives at offset 12. #1304.
                out.emotion_type = sub.data[0];
                if sub.data.len() >= 13 {
                    out.response_number = sub.data[12];
                }
            }
            b"TCLT" if sub.data.len() >= 4 => {
                if let Ok(t) = SubReader::new(&sub.data).u32() {
                    let remapped = remap.as_ref().map_or(t, |r| r.remap(t));
                    out.topic_links.push(remapped);
                }
            }
            b"PNAM" if sub.data.len() >= 4 => {
                let raw = SubReader::new(&sub.data).u32_or_default();
                let remapped = remap.as_ref().map_or(raw, |r| r.remap(raw));
                out.previous_info = remapped;
            }
            b"ANAM" if sub.data.len() >= 4 => {
                let raw = u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                let remapped = remap.as_ref().map_or(raw, |r| r.remap(raw));
                out.actor_form_id = remapped;
            }
            b"CTDA" => {
                if let Some(mut cond) = parse_ctda(sub) {
                    remap_condition_form_ids(&mut cond, remap);
                    out.conditions.push(cond);
                }
            }
            _ => {}
        }
    }
    out
}

/// Build a conversation tree from flat INFO list.
/// Orders INFOs by PNAM chains (head = previous_info == 0).
/// Detects cycles to ensure chain termination.
pub fn build_conversation_tree(
    infos: &[InfoRecord],
) -> Result<ConversationTree, ConversationTreeError> {
    use std::collections::HashMap;

    // Index by form_id for fast lookup and cycle detection.
    let mut info_map: HashMap<u32, &InfoRecord> = HashMap::new();
    for info in infos {
        info_map.insert(info.form_id, info);
    }

    let mut visited = std::collections::HashSet::new();
    let mut chains = Vec::new();

    // Find all chain heads (previous_info == 0) and follow each to its tail.
    for info in infos {
        if info.previous_info == 0 && !visited.contains(&info.form_id) {
            let mut chain = Vec::new();
            let mut current = info.form_id;

            loop {
                chain.push(current);
                visited.insert(current);

                // Follow the chain: look up the next INFO by its own form_id
                // in the infos list (the NEXT INFO points back to this one
                // via previous_info).
                let next_info = infos.iter().find(|i| i.previous_info == current);
                match next_info {
                    Some(nxt) => {
                        // Cycle detection: if the next form_id is already in this chain, bail.
                        if chain.contains(&nxt.form_id) {
                            return Err(ConversationTreeError::PnamCycle {
                                info_form_id: nxt.form_id,
                            });
                        }
                        current = nxt.form_id;
                    }
                    None => break, // End of chain.
                }
            }

            chains.push(chain);
        }
    }

    // Orphans: infos not in any chain. Check for cycles in orphaned sub-chains.
    for info in infos {
        if !visited.contains(&info.form_id) {
            // This INFO is not a head and not yet visited.
            // Start from it and walk backward via previous_info to find the chain head.
            let mut walk_back = Vec::new();
            let mut current = info.form_id;

            loop {
                if walk_back.contains(&current) {
                    // Cycle detected (no head exists for this chain).
                    return Err(ConversationTreeError::PnamCycle {
                        info_form_id: current,
                    });
                }
                walk_back.push(current);

                // If current has previous_info == 0, it's the head.
                if let Some(curr_info) = info_map.get(&current) {
                    if curr_info.previous_info == 0 {
                        break; // Found the head; this chain should already be visited.
                    }
                    current = curr_info.previous_info;
                } else {
                    // current form_id not in infos — dangling reference.
                    // The last valid INFO we saw is the actual head.
                    if !walk_back.is_empty() {
                        walk_back.pop(); // Remove the invalid form_id
                    }
                    break;
                }
            }

            // walk_back is now [starting_info, ..., head]. Reverse to get proper order.
            walk_back.reverse();
            if let Some(&head_fid) = walk_back.first() {
                let mut chain = vec![head_fid];
                visited.insert(head_fid);
                let mut current = head_fid;

                loop {
                    let next_info = infos.iter().find(|i| i.previous_info == current);
                    match next_info {
                        Some(nxt) => {
                            if chain.contains(&nxt.form_id) {
                                return Err(ConversationTreeError::PnamCycle {
                                    info_form_id: nxt.form_id,
                                });
                            }
                            chain.push(nxt.form_id);
                            visited.insert(nxt.form_id);
                            current = nxt.form_id;
                        }
                        None => break,
                    }
                }

                chains.push(chain);
            }
        }
    }

    // Build topic_links map: info_form_id → destination topics.
    let mut topic_links = HashMap::new();
    for info in infos {
        if !info.topic_links.is_empty() {
            topic_links.insert(info.form_id, info.topic_links.clone());
        }
    }

    Ok(ConversationTree {
        chains,
        topic_links,
    })
}

/// `MESG` message / popup record. Quest-tutorial banners and
/// interaction prompts. `DESC` carries the text; `QNAM` optionally
/// ties the message to a quest for clean-up on quest completion.
#[derive(Debug, Clone, Default)]
pub struct MesgRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// Owning quest form ID (optional) — message clears when quest
    /// completes.
    pub owner_quest: u32,
}

pub fn parse_mesg(form_id: u32, subs: &[SubRecord]) -> MesgRecord {
    let mut out = MesgRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"DESC" => out.description = read_lstring_or_zstring(&sub.data),
            b"QNAM" if sub.data.len() >= 4 => {
                out.owner_quest = SubReader::new(&sub.data).u32_or_default();
            }
            _ => {}
        }
    }
    out
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

    #[test]
    fn parse_dial_accumulates_multiple_quest_refs() {
        let subs = vec![
            sub(b"EDID", b"GREETING\0"),
            sub(b"FULL", b"Greeting\0"),
            sub(b"QSTI", &0x0100_0001u32.to_le_bytes()),
            sub(b"QSTI", &0x0100_0002u32.to_le_bytes()),
            sub(b"QSTI", &0x0100_0003u32.to_le_bytes()),
        ];
        let d = parse_dial(0xC3C3, &subs, &None);
        assert_eq!(d.quest_refs.len(), 3);
        assert_eq!(d.quest_refs[1], 0x0100_0002);
        // DATA absent → dial_type defaults to 0 (Topic).
        assert_eq!(d.dial_type, 0);
    }

    /// #1307 / OBL-D3-...-03 — DIAL DATA byte 0 is the dialogue type.
    /// Captured for all games (Oblivion single-byte DATA here; FO3+ widen
    /// it but byte 0 is still the type). Pre-fix this byte was dropped for
    /// all 3817 Oblivion DIAL records.
    #[test]
    fn parse_dial_captures_dialogue_type_byte() {
        // Oblivion DATA: a single type byte. 3 = Persuasion in the TES4 enum.
        let subs = vec![sub(b"EDID", b"PersuasionTopic\0"), sub(b"DATA", &[3u8])];
        let d = parse_dial(0xDEAD, &subs, &None);
        assert_eq!(d.dial_type, 3);

        // FO3+ widen DATA (type byte + flags); byte 0 still the type.
        let subs_fo3 = vec![sub(b"DATA", &[5u8, 0x01, 0x00, 0x00])];
        assert_eq!(parse_dial(0xBEEF, &subs_fo3, &None).dial_type, 5);

        // Empty DATA must not panic and leaves the default.
        let subs_empty = vec![sub(b"DATA", &[])];
        assert_eq!(parse_dial(0xF00D, &subs_empty, &None).dial_type, 0);
    }

    #[test]
    fn parse_mesg_picks_desc_and_owner_quest() {
        let subs = vec![
            sub(b"EDID", b"FastTravelMessage\0"),
            sub(b"FULL", b"Fast Travel\0"),
            sub(b"DESC", b"You cannot fast travel right now.\0"),
            sub(b"QNAM", &0x0002_1234u32.to_le_bytes()),
        ];
        let m = parse_mesg(0xD4D4, &subs);
        assert_eq!(m.description, "You cannot fast travel right now.");
        assert_eq!(m.owner_quest, 0x0002_1234);
    }

    #[test]
    fn parse_info_picks_anam_actor() {
        let anam = 0xDEAD_BEEFu32.to_le_bytes();
        let subs = vec![sub(b"NAM1", b"hello\0"), sub(b"ANAM", &anam)];
        let info = parse_info(0x1234, &subs, &None);
        assert_eq!(info.actor_form_id, 0xDEAD_BEEF);
    }

    #[test]
    fn parse_info_ctda_conditions_stored() {
        let mut ctda = Vec::new();
        ctda.push(0x00u8); // type_byte (offset 0)
        ctda.extend_from_slice(&[0u8; 3]); // pad (offsets 1-3)
        ctda.extend_from_slice(&1.0f32.to_le_bytes()); // comparand (offsets 4-7)
        ctda.extend_from_slice(&36u32.to_le_bytes()); // function_index (offsets 8-11, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // param_1 (offsets 12-15, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // param_2 (offsets 16-19, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // run_on (offsets 20-23, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // ref_fid (offsets 24-27, u32)

        let subs = vec![sub(b"NAM1", b"hi\0"), sub(b"CTDA", &ctda)];
        let info = parse_info(0x5678, &subs, &None);
        assert_eq!(info.conditions.len(), 1);
        assert_eq!(info.conditions[0].function_index, 36);
    }

    #[test]
    fn parse_info_remaps_formids_with_remap() {
        use crate::esm::reader::FormIdRemap;
        // PNAM (previous_info) and TCLT (topic_links) and ANAM (actor)
        // should be remapped when a remap is provided.
        // This plugin at index 1, master at index 0 (all regular, no ESL).
        let remap = FormIdRemap::regular(1, vec![0]);
        let subs = vec![
            sub(b"PNAM", &0x00_050000u32.to_le_bytes()), // plugin 0 (master), form 0x050000
            sub(b"TCLT", &0x01_030000u32.to_le_bytes()), // plugin 1 (this), form 0x030000
            sub(b"ANAM", &0x00_020000u32.to_le_bytes()), // plugin 0 (master), form 0x020000
        ];
        // With remap: plugin 0 stays 0 (master), plugin 1 stays 1 (this)
        let info = parse_info(0x5678, &subs, &Some(remap));
        assert_eq!(info.previous_info, 0x00_050000);
        assert_eq!(info.topic_links[0], 0x01_030000);
        assert_eq!(info.actor_form_id, 0x00_020000);
        // Verify that without remap, values are identical (no remap = identity)
        let info_no_remap = parse_info(0x5678, &subs, &None);
        assert_eq!(info_no_remap.previous_info, info.previous_info);
    }

    #[test]
    fn build_conversation_tree_orders_pnam_chain() {
        // Three INFOs: A (head), B, C.
        // PNAM chain: A (previous_info=0) <- B <- C (C.previous_info=B.form_id)
        // Insert them in scrambled order to test ordering.
        let infos = vec![
            InfoRecord {
                form_id: 0xBBBB,
                response_text: "B response".to_string(),
                previous_info: 0xAAAA, // Points back to A
                ..Default::default()
            },
            InfoRecord {
                form_id: 0xAAAA,
                response_text: "A response".to_string(),
                previous_info: 0, // Head
                ..Default::default()
            },
            InfoRecord {
                form_id: 0xCCCC,
                response_text: "C response".to_string(),
                previous_info: 0xBBBB, // Points back to B
                ..Default::default()
            },
        ];

        let tree = build_conversation_tree(&infos).expect("should build tree");
        assert_eq!(tree.chains.len(), 1, "should have 1 chain");
        assert_eq!(
            tree.chains[0],
            vec![0xAAAA, 0xBBBB, 0xCCCC],
            "chain should be ordered A→B→C"
        );
    }

    #[test]
    fn build_conversation_tree_detects_pnam_cycle() {
        // Cycle: A <- B <- C <- A (C.previous_info=A)
        let infos = vec![
            InfoRecord {
                form_id: 0xAAAA,
                response_text: "A response".to_string(),
                previous_info: 0xCCCC, // Points back to C (cycle!)
                ..Default::default()
            },
            InfoRecord {
                form_id: 0xBBBB,
                response_text: "B response".to_string(),
                previous_info: 0xAAAA,
                ..Default::default()
            },
            InfoRecord {
                form_id: 0xCCCC,
                response_text: "C response".to_string(),
                previous_info: 0xBBBB,
                ..Default::default()
            },
        ];

        let result = build_conversation_tree(&infos);
        assert!(result.is_err(), "should detect cycle");
        match result.unwrap_err() {
            ConversationTreeError::PnamCycle { info_form_id } => {
                assert_eq!(
                    info_form_id, 0xAAAA,
                    "cycle detection should report the repeating form_id"
                );
            }
        }
    }

    #[test]
    fn build_conversation_tree_surfaces_tclt_edges() {
        // Two separate PNAM chains; first INFO of first chain has TCLT edges.
        let infos = vec![
            InfoRecord {
                form_id: 0xAAAA,
                response_text: "Chain1 head".to_string(),
                previous_info: 0,
                topic_links: vec![0x1111, 0x2222], // Routes to two topics
                ..Default::default()
            },
            InfoRecord {
                form_id: 0xBBBB,
                response_text: "Chain2 head".to_string(),
                previous_info: 0,
                topic_links: vec![],
                ..Default::default()
            },
        ];

        let tree = build_conversation_tree(&infos).expect("should build tree");
        assert_eq!(
            tree.topic_links.len(),
            1,
            "should have 1 INFO with topic_links"
        );
        assert_eq!(
            tree.topic_links.get(&0xAAAA),
            Some(&vec![0x1111, 0x2222]),
            "should surface TCLT edges for chain1 head"
        );
        assert!(
            !tree.topic_links.contains_key(&0xBBBB),
            "chain2 head has no TCLT"
        );
    }

    #[test]
    fn build_conversation_tree_handles_orphaned_infos() {
        // An INFO with previous_info pointing to a non-existent INFO becomes a 1-element chain.
        let infos = vec![InfoRecord {
            form_id: 0xAAAA,
            response_text: "Orphan".to_string(),
            previous_info: 0x9999, // Points to non-existent INFO
            ..Default::default()
        }];

        let tree = build_conversation_tree(&infos).expect("should build tree");
        assert_eq!(
            tree.chains.len(),
            1,
            "orphan should become a 1-element chain"
        );
        assert_eq!(tree.chains[0], vec![0xAAAA]);
    }
}
