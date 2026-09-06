//! `DIAL` / `INFO` / `MESG` dialogue and message records.

use super::super::common::{read_lstring_or_zstring, read_zstring, remap_fid, CommonNamedFields};
use super::super::condition::{push_ctda, ComparisonOp, ConditionList, ConditionValue, RunOn};
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
/// enumerate branches without re-parsing. Conditions (CTDA/CTDT),
/// scripts (SCHR/SCDA), and edits (NAM3) are deferred until the
/// condition runtime lands. See #631.
#[derive(Debug, Clone, Default)]
pub struct InfoRecord {
    pub form_id: u32,
    /// Response text shown / spoken to the player: every authored
    /// [`Self::responses`] segment's `text`, joined in order with `"\n"`.
    /// #3616 — pre-fix this was `NAM1`'s bare assignment, so a
    /// multi-segment INFO (19.3% of Oblivion's, per the per-record vs.
    /// per-occurrence census below) silently kept only its last segment.
    /// A consumer that wants the segments unjoined (per-clip playback,
    /// per-segment emotion) should read `responses` directly.
    pub response_text: String,
    /// Designer notes: every authored [`Self::responses`] segment's
    /// `designer_notes`, joined in order with `"\n"`. Same #3616 fix —
    /// `NAM2` is authored per-response-segment (`wbRStruct('Response',
    /// [TRDT, NAM1, NAM2])` in xEdit's TES4 definitions), not once per
    /// INFO, and shares NAM1/TRDT's exact 23,877-occurrence /
    /// 19,260-record count on `Oblivion.esm` — the identical
    /// assign-not-push shape the SIBLING check asked for.
    pub designer_notes: String,
    /// `TRDT` Emotion Type of the *first* authored response segment —
    /// the low byte of the `EmotionType` `u32` at TRDT offset 0:
    /// 0=Neutral, 1=Anger, 2=Disgust, 3=Fear, 4=Sad, 5=Happy, 6=Surprise
    /// (Oblivion / FO3 / FNV; Skyrim keeps the EmotionType-u32 @0
    /// layout). 0 when there are no responses. See #1304 (was mislabeled
    /// `response_type`) and #3616 (was the *last* segment's value, an
    /// accident of assign-not-push rather than a deliberate choice).
    pub emotion_type: u8,
    /// `TRDT` Response number of the *first* authored response segment —
    /// byte 12, after `EmotionType` (u32 @0), `Emotion Value` (i32 @4),
    /// and 4 unused bytes @8. 0 when there are no responses, or when
    /// that segment's TRDT is shorter than 13 bytes. See #1304 / #3616.
    pub response_number: u8,
    /// Every authored TRDT+NAM1+NAM2 response segment, in authored
    /// order (#3616). `Oblivion.esm` authors up to 8 segments on one
    /// INFO; [`Self::response_text`] / [`Self::designer_notes`] /
    /// [`Self::emotion_type`] / [`Self::response_number`] above are
    /// derived from this for callers that don't need per-segment detail.
    pub responses: Vec<ResponseSegment>,
    /// `TCLT` topic-link ref — IDs of other DIAL topics that this
    /// branch routes the conversation to. Multiple TCLTs are
    /// concatenated.
    pub topic_links: Vec<u32>,
    /// `NAME` "Add topics" ref (#3614) — DIAL topics this response
    /// unlocks, distinct from [`Self::topic_links`]'s immediate choices:
    /// per xEdit's TES4 definitions (`wbRArray('Add topics', ...)`) NAME
    /// sits before the response array, TCLT after it, and UESP's field
    /// table separately calls TCLT "choice" vs. NAME "add topic". Was
    /// dropped entirely pre-fix — 1,044 `Oblivion.esm` INFOs (5.4%)
    /// author at least one and could not unlock the topic they intended.
    pub added_topics: Vec<u32>,
    /// `TCLF` "Link From" ref (#3614) — DIAL topics that this INFO's own
    /// topic is reached from, the inverse direction of
    /// [`Self::topic_links`]. Per xEdit's TES4 definitions:
    /// `wbRArray('Link From', wbFormIDCk(TCLF, 'Topic', [DIAL]))`, the
    /// same target class (`DIAL`) as `TCLT`'s "Choices", just the other
    /// edge direction — so it is kept as its own field rather than
    /// merged into `topic_links`, which would silently invert its
    /// meaning. Was dropped entirely pre-fix — 3,792 `Oblivion.esm`
    /// INFOs (19.7%), the other half of the title's topic-graph edges.
    pub linked_from_topics: Vec<u32>,
    /// `PNAM` previous-info ref — the prior INFO in this branch. 0
    /// means "this is the first response in the chain".
    pub previous_info: u32,
    /// `ANAM` actor form ID — restricts this response to a specific NPC.
    /// 0 means the response works for any actor.
    pub actor_form_id: u32,
    /// Conditions attached to this response (`CTDA`/`CTDT` sub-records,
    /// #3614 — see [`push_ctda`]'s doc for why `CTDT` decodes through the
    /// same path).
    pub conditions: ConditionList,
}

/// One `TRDT`+`NAM1`+`NAM2` response segment (#3616). xEdit's TES4
/// definitions author these as a repeated struct
/// (`wbRArray('Responses', wbRStruct('Response', [TRDT, NAM1, NAM2]))`),
/// so a fresh `TRDT` sub-record starts a new segment and the `NAM1`/`NAM2`
/// immediately following it belong to that segment — never assigned onto
/// a shared field the way the pre-fix parser did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResponseSegment {
    /// See [`InfoRecord::emotion_type`]'s doc for the enum values.
    pub emotion_type: u8,
    pub response_number: u8,
    /// `NAM1` — response text shown / spoken to the player.
    pub text: String,
    /// `NAM2` — designer notes / voice-actor direction.
    pub designer_notes: String,
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
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;
    out.full_name = common.full_name;
    for sub in subs {
        match &sub.sub_type {
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
    // #3616 — the response segment currently being built. A `TRDT`
    // starts a new one (xEdit's TES4 layout repeats the whole
    // TRDT+NAM1+NAM2 struct per response); `NAM1`/`NAM2` fill in
    // whichever segment is open, lazily starting one if a malformed
    // record's text arrives before its TRDT.
    let mut current_response: Option<ResponseSegment> = None;
    for sub in subs {
        match &sub.sub_type {
            b"NAM1" => {
                current_response.get_or_insert_with(Default::default).text =
                    read_lstring_or_zstring(&sub.data);
            }
            b"NAM2" => {
                current_response
                    .get_or_insert_with(Default::default)
                    .designer_notes = read_zstring(&sub.data);
            }
            b"TRDT" if !sub.data.is_empty() => {
                // TES4 TRDT layout: EmotionType(u32 @0) + EmotionValue
                // (i32 @4) + unused[4] @8 + Response number(u8 @12) +
                // unused[3]. Byte 0 is the emotion (0–6), not a response
                // number; the response index lives at offset 12. #1304.
                //
                // #3616 — finalize whatever segment is open before
                // starting this one: a bare NAM1/NAM2 with no TRDT at
                // all (malformed data) still gets pushed rather than
                // silently merged into the next real segment.
                if let Some(finished) = current_response.take() {
                    out.responses.push(finished);
                }
                let mut segment = ResponseSegment {
                    emotion_type: sub.data[0],
                    ..Default::default()
                };
                if sub.data.len() >= 13 {
                    segment.response_number = sub.data[12];
                }
                current_response = Some(segment);
            }
            b"TCLT" if sub.data.len() >= 4 => {
                if let Ok(t) = SubReader::new(&sub.data).u32() {
                    let remapped = remap.as_ref().map_or(t, |r| r.remap(t));
                    out.topic_links.push(remapped);
                }
            }
            // #3614 — "Add topics": DIAL topics this response unlocks.
            // See `InfoRecord::added_topics`'s doc for why this is a
            // separate field from `topic_links`.
            b"NAME" if sub.data.len() >= 4 => {
                if let Ok(t) = SubReader::new(&sub.data).u32() {
                    out.added_topics.push(remap_fid(t, remap));
                }
            }
            // #3614 — "Link From": the inverse edge direction of TCLT.
            // See `InfoRecord::linked_from_topics`'s doc.
            b"TCLF" if sub.data.len() >= 4 => {
                if let Ok(t) = SubReader::new(&sub.data).u32() {
                    out.linked_from_topics.push(remap_fid(t, remap));
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
            // #3614 — `CTDT` is the legacy fixed-layout encoding of the
            // same condition; see `push_ctda`'s doc.
            b"CTDA" | b"CTDT" | b"CIS1" | b"CIS2" => push_ctda(sub, remap, &mut out.conditions),
            _ => {}
        }
    }
    if let Some(finished) = current_response.take() {
        out.responses.push(finished);
    }
    // #3616 — derive the flat convenience fields from the full sequence
    // rather than dropping everything but the last segment. `join("\n")`
    // rather than concatenation so a multi-segment response reads as
    // separate lines, not one run-on sentence; a consumer that wants the
    // segments unmerged reads `responses` directly.
    out.response_text = out
        .responses
        .iter()
        .map(|r| r.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    out.designer_notes = out
        .responses
        .iter()
        .map(|r| r.designer_notes.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if let Some(first) = out.responses.first() {
        out.emotion_type = first.emotion_type;
        out.response_number = first.response_number;
    }
    if out.actor_form_id == 0 {
        out.actor_form_id = speaker_from_conditions(&out.conditions);
    }
    out
}

/// `GetIsID` — the Oblivion-era speaker signal. See
/// [`speaker_from_conditions`].
const CONDITION_GET_IS_ID: u32 = 72;

/// Derive an INFO's speaker from its `CTDA` conditions when `ANAM` is
/// absent (#3600).
///
/// `ANAM` was introduced after Oblivion: a sub-record census over all
/// 19,278 `Oblivion.esm` INFO records finds **zero** `ANAM` and **zero**
/// `PNAM`, so `actor_form_id` was 0 on every record of the entire title.
/// Oblivion identifies the speaker through conditions instead, and the
/// signal is both present and unambiguous — 19,345 `GetIsID` conditions
/// across 15,736 of the 19,278 records.
///
/// The rule, measured rather than assumed, over `Oblivion.esm`:
///
/// * `run_on` is `Subject` on **19,345 of 19,345** — which for dialogue is
///   the speaker by definition (Oblivion's 24-byte CTDA has no run-on field
///   at all, so this is structural, not a coincidence of authoring).
/// * `param_1` resolves to an `NPC_` on **19,344 of 19,345**.
/// * Only the **positive** form identifies a speaker. 2,432 of the 19,345
///   are not `== 1` — those are exclusions ("this line is not for X") and
///   reading one as the speaker would invert its meaning.
/// * Exactly one positive `GetIsID` on **12,940** records — an unambiguous
///   speaker. **1,626** carry several (an OR list of alternate speakers, so
///   there is no single one) and **4,712** carry none (generic topics).
///   Both of those keep `actor_form_id == 0`, which is already the
///   documented "works for any actor" value, so the ambiguous and absent
///   cases degrade to exactly the prior behaviour rather than to a guess.
///
/// Only consulted when `ANAM` is absent, so FO3+ is untouched.
fn speaker_from_conditions(conditions: &ConditionList) -> u32 {
    let mut speaker = 0u32;
    let mut count = 0usize;
    for condition in conditions {
        if condition.function_index != CONDITION_GET_IS_ID
            || !matches!(condition.run_on, RunOn::Subject)
            || condition.comparator != ComparisonOp::Eq
        {
            continue;
        }
        let ConditionValue::Literal(value) = condition.comparand else {
            continue;
        };
        if (value - 1.0).abs() > f32::EPSILON || condition.param_1 == 0 {
            continue;
        }
        speaker = condition.param_1;
        count += 1;
    }
    if count == 1 {
        speaker
    } else {
        0
    }
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

    // #3600 — Oblivion authors NO `PNAM`: a census over all 19,278
    // `Oblivion.esm` INFO records finds zero. Every record therefore looks
    // like a chain head, the walk below degenerates into 19,278
    // single-element chains, and the whole title's dialogue comes out
    // unordered — silently, with no parse error.
    //
    // `PNAM` was introduced after Oblivion; that generation orders INFOs by
    // their record order within the DIAL group's Topic Children sub-GRUP,
    // which `extract_dial_with_info` already preserves (it pushes in walk
    // order). So when the group carries no `PNAM` at all, slice order IS the
    // authored order and the group is one chain.
    //
    // Gated on "not one single record in this group has a PNAM" rather than
    // on a game enum: a genuine FO3+ group always has at least one
    // non-head, and a hand-built single-INFO group is one chain either way.
    // That keeps the FO3+ path bit-identical and needs no game plumbed in
    // here.
    let record_order_is_authoritative =
        !infos.is_empty() && infos.iter().all(|info| info.previous_info == 0);

    let mut visited = std::collections::HashSet::new();
    let mut chains: Vec<Vec<u32>> = Vec::new();

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

    // #3600 — collapse to the single record-order chain when the group
    // authored no `PNAM` at all. Done here rather than as an early return so
    // the `topic_links` map below is built identically on both paths.
    if record_order_is_authoritative {
        chains = vec![infos.iter().map(|info| info.form_id).collect()];
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
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;
    out.full_name = common.full_name;
    for sub in subs {
        match &sub.sub_type {
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

    /// #3614 — `CTDT` is the legacy fixed-layout encoding of the same
    /// condition `CTDA` carries; this exact 20-byte payload is a real
    /// `Oblivion.esm` INFO CTDT (`probe_substring`-style extraction,
    /// 2026-09-06): `type_byte=0x60, comparand=1.0, function=0x003A (58,
    /// GetStage), param_1=0x00027815` (a quest form id). Pre-fix, none of
    /// the 45 Oblivion INFOs whose only conditions are CTDT-encoded
    /// reached `push_ctda` at all, so they parsed as unconditional.
    #[test]
    fn parse_info_ctdt_condition_decodes_as_conditional() {
        let ctdt: [u8; 20] = [
            0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f, 0x3a, 0x00, 0x00, 0x00, 0x15, 0x78,
            0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let subs = vec![sub(b"NAM1", b"hi\0"), sub(b"CTDT", &ctdt)];
        let info = parse_info(0x5678, &subs, &None);
        assert_eq!(
            info.conditions.len(),
            1,
            "a CTDT-only INFO must not parse as unconditional (#3614)"
        );
        assert_eq!(info.conditions[0].function_index, 58, "GetStage");
        assert_eq!(info.conditions[0].comparand, ConditionValue::Literal(1.0));
        assert_eq!(info.conditions[0].param_1, 0x0002_7815);
    }

    /// #3614 — TCLF ("Link From") and NAME ("Add topics") were both
    /// dropped entirely pre-fix (3,792 and 1,044 `Oblivion.esm` INFOs
    /// respectively). Both are FormID-array sub-records like TCLT, so
    /// the parse+push shape mirrors `parse_info_remaps_formids_with_remap`
    /// below, but pins the two new fields specifically rather than
    /// TCLT/PNAM/ANAM again.
    #[test]
    fn parse_info_tclf_and_name_are_not_dropped() {
        let subs = vec![
            sub(b"TCLF", &0x0001_1111u32.to_le_bytes()),
            sub(b"TCLF", &0x0001_2222u32.to_le_bytes()),
            sub(b"NAME", &0x0001_3333u32.to_le_bytes()),
        ];
        let info = parse_info(0x9999, &subs, &None);
        assert_eq!(info.linked_from_topics, vec![0x0001_1111, 0x0001_2222]);
        assert_eq!(info.added_topics, vec![0x0001_3333]);
    }

    /// #3614 — TCLF/NAME FormIDs are plugin-local like TCLT/PNAM/ANAM and
    /// must be remapped the same way.
    #[test]
    fn parse_info_remaps_tclf_and_name_with_remap() {
        use crate::esm::reader::FormIdRemap;
        let remap = FormIdRemap::regular(1, vec![0]);
        let subs = vec![
            sub(b"TCLF", &0x01_040000u32.to_le_bytes()),
            sub(b"NAME", &0x01_050000u32.to_le_bytes()),
        ];
        let info = parse_info(0x5678, &subs, &Some(remap));
        assert_eq!(info.linked_from_topics, vec![0x01_040000]);
        assert_eq!(info.added_topics, vec![0x01_050000]);
    }

    /// #3616 — a real multi-response Oblivion.esm shape: TRDT+NAM1+NAM2
    /// repeated per segment (xEdit's TES4 `wbRStruct('Response', [TRDT,
    /// NAM1, NAM2])`). Pre-fix, NAM1/TRDT/NAM2 each assigned rather than
    /// pushed, so only the third segment's text/emotion/notes survived —
    /// exactly the shape that dropped 4,617 response segments title-wide.
    #[test]
    fn parse_info_multi_response_preserves_every_segment_in_order() {
        fn trdt(emotion: u32, response_number: u8) -> Vec<u8> {
            let mut d = emotion.to_le_bytes().to_vec(); // EmotionType @0
            d.extend_from_slice(&0i32.to_le_bytes()); // EmotionValue @4
            d.extend_from_slice(&[0u8; 4]); // unused @8
            d.push(response_number); // @12
            d.extend_from_slice(&[0u8; 3]); // unused @13
            d
        }
        let subs = vec![
            sub(b"TRDT", &trdt(5, 0)), // Happy
            sub(b"NAM1", b"First line.\0"),
            sub(b"NAM2", b"cheerfully\0"),
            sub(b"TRDT", &trdt(1, 1)), // Anger
            sub(b"NAM1", b"Second line.\0"),
            sub(b"NAM2", b"then annoyed\0"),
            sub(b"TRDT", &trdt(4, 2)), // Sad
            sub(b"NAM1", b"Third line.\0"),
            // No NAM2 on the last segment — must not leak the prior one.
        ];
        let info = parse_info(0x1234, &subs, &None);
        assert_eq!(info.responses.len(), 3, "all three segments must survive");
        assert_eq!(info.responses[0].text, "First line.");
        assert_eq!(info.responses[1].text, "Second line.");
        assert_eq!(info.responses[2].text, "Third line.");
        assert_eq!(
            info.responses[2].designer_notes, "",
            "no leakage across segments"
        );
        // Flat convenience fields: full join, first-segment emotion/number
        // (pre-fix these silently held only the LAST segment's values).
        assert_eq!(info.response_text, "First line.\nSecond line.\nThird line.");
        assert_eq!(info.designer_notes, "cheerfully\nthen annoyed\n");
        assert_eq!(
            info.emotion_type, 5,
            "first segment's emotion, not the last"
        );
        assert_eq!(
            info.response_number, 0,
            "first segment's number, not the last"
        );
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

#[cfg(test)]
mod oblivion_generation_tests {
    use super::*;
    use crate::esm::records::condition::{ComparisonOp, Condition, ConditionValue, RunOn};

    fn get_is_id(form_id: u32, positive: bool) -> Condition {
        Condition {
            function_index: super::CONDITION_GET_IS_ID,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(if positive { 1.0 } else { 0.0 }),
            param_1: form_id,
            param_2: 0,
            param_1_text: None,
            param_2_text: None,
            run_on: RunOn::Subject,
            reference_form_id: 0,
            extra_data_id: 0,
            or_next: false,
        }
    }

    fn info(form_id: u32, conditions: Vec<Condition>) -> InfoRecord {
        InfoRecord {
            form_id,
            conditions,
            ..Default::default()
        }
    }

    /// #3600 — Oblivion authors zero `ANAM` on all 19,278 INFO records, so
    /// `actor_form_id` was 0 for the entire title. It identifies the speaker
    /// through `GetIsID` conditions instead: 19,345 of them across 15,736
    /// records, `run_on == Subject` on 19,345 of 19,345 (structural —
    /// Oblivion's 24-byte CTDA has no run-on field), `param_1` resolving to
    /// an `NPC_` on 19,344 of 19,345.
    #[test]
    fn a_single_positive_get_is_id_is_the_speaker() {
        let subs = vec![
            sub_of(b"NAM1", b"Greetings.\0"),
            ctda_of(&get_is_id(0x0002_1234, true)),
        ];
        let parsed = parse_info(0xAAAA, &subs, &None);
        assert_eq!(
            parsed.actor_form_id, 0x0002_1234,
            "an unambiguous GetIsID must supply the speaker ANAM never carried"
        );
    }

    /// Only the POSITIVE form. 2,432 of the 19,345 vanilla `GetIsID`
    /// conditions are not `== 1` — those are exclusions ("this line is NOT
    /// for X"), and reading one as the speaker inverts its meaning.
    #[test]
    fn a_negated_get_is_id_is_an_exclusion_not_a_speaker() {
        let subs = vec![ctda_of(&get_is_id(0x0002_1234, false))];
        assert_eq!(parse_info(0xAAAA, &subs, &None).actor_form_id, 0);
    }

    /// Several positive `GetIsID`s are an OR list of alternate speakers
    /// (1,626 vanilla records), so there is no single one. Falling back to 0
    /// is not a loss: 0 is already the documented "works for any actor"
    /// value, so the ambiguous case degrades to the prior behaviour rather
    /// than to a guess.
    #[test]
    fn several_positive_get_is_ids_leave_the_speaker_unset() {
        let subs = vec![
            ctda_of(&get_is_id(0x0002_1234, true)),
            ctda_of(&get_is_id(0x0002_5678, true)),
        ];
        assert_eq!(parse_info(0xAAAA, &subs, &None).actor_form_id, 0);
    }

    /// An authored `ANAM` always wins — FO3+ must be bit-identical.
    #[test]
    fn an_authored_anam_is_never_overridden_by_conditions() {
        let anam = 0xDEAD_BEEFu32.to_le_bytes();
        let subs = vec![
            sub_of(b"ANAM", &anam),
            ctda_of(&get_is_id(0x0002_1234, true)),
        ];
        assert_eq!(parse_info(0xAAAA, &subs, &None).actor_form_id, 0xDEAD_BEEF);
    }

    /// #3600 — with zero `PNAM` in the group (every vanilla Oblivion DIAL),
    /// the PNAM walk gave 19,278 single-element chains and the title's
    /// dialogue came out unordered. Record order within the DIAL group's
    /// Topic Children sub-GRUP is the authored order for that generation,
    /// and `extract_dial_with_info` preserves it.
    #[test]
    fn a_group_with_no_pnam_orders_by_record_order() {
        let infos = vec![
            info(0x111, vec![]),
            info(0x222, vec![]),
            info(0x333, vec![]),
        ];
        let tree = build_conversation_tree(&infos).expect("no PNAM is not an error");
        assert_eq!(
            tree.chains,
            vec![vec![0x111, 0x222, 0x333]],
            "one chain, in record order — not three single-element chains"
        );
    }

    /// The FO3+ path must be untouched: a group with even one `PNAM` still
    /// walks the chain. Gating on "not one record has a PNAM" rather than a
    /// game enum is what keeps that true without plumbing the game in here.
    #[test]
    fn a_group_with_any_pnam_still_walks_the_chain() {
        let mut b = info(0x222, vec![]);
        b.previous_info = 0x111;
        let mut c = info(0x333, vec![]);
        c.previous_info = 0x222;
        // Scrambled input order — the PNAM walk must reorder it.
        let infos = vec![c, info(0x111, vec![]), b];
        let tree = build_conversation_tree(&infos).expect("valid chain");
        assert_eq!(tree.chains, vec![vec![0x111, 0x222, 0x333]]);
    }

    /// An empty group must stay empty rather than becoming an empty chain.
    #[test]
    fn an_empty_group_produces_no_chains() {
        let tree = build_conversation_tree(&[]).expect("empty is not an error");
        assert!(tree.chains.is_empty());
    }

    fn sub_of(code: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *code,
            data: data.to_vec(),
        }
    }

    /// A 24-byte Oblivion CTDA payload for `condition`.
    fn ctda_of(condition: &Condition) -> SubRecord {
        let mut data = Vec::with_capacity(24);
        // type byte: comparator in the high 3 bits (Eq == 0), no flags.
        data.push(0u8);
        data.extend_from_slice(&[0, 0, 0]); // unused
        let ConditionValue::Literal(value) = condition.comparand else {
            unreachable!("fixture only builds literal comparands")
        };
        data.extend_from_slice(&value.to_le_bytes());
        data.extend_from_slice(&condition.function_index.to_le_bytes());
        data.extend_from_slice(&condition.param_1.to_le_bytes());
        data.extend_from_slice(&condition.param_2.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes()); // run-on absent pre-FO3
        sub_of(b"CTDA", &data)
    }
}
