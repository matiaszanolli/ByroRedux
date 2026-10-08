//! `DIAL` / `INFO` / `MESG` dialogue and message records.

use super::super::common::{
    read_lstring_or_zstring, read_zstring, remap_fid, remap_fid_or_sentinel, CommonNamedFields,
};
use super::super::condition::{push_ctda, ComparisonOp, ConditionList, ConditionValue, RunOn};
use super::super::script_instance::{
    parse_info_fragments, InfoScriptFragment, ScriptInstanceData,
};
use crate::esm::reader::{FormIdRemap, GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;

/// `DIAL` dialogue topic record. Parent of INFO dialogue lines (which
/// live in a nested GRUP tree — tracked as a follow-up; the current
/// `extract_records` walker takes a single record type and can't
/// simultaneously emit DIAL + INFO). This stub captures the topic's
/// quest owners (QSTI/QNAM refs, 4 bytes each) so NPC / quest systems
/// can enumerate topics without re-parsing.
#[derive(Debug, Clone, Default)]
pub struct DialRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Quest form IDs that own this dialogue topic (one per QSTI
    /// (Oblivion/FO3/FNV) or QNAM (Skyrim/FO4/FO76/Starfield)
    /// sub-record). Real-data census (#5048): QNAM is the *only*
    /// ownership sub-record on Skyrim (15,037/15,037 DIALs) and FO4
    /// (35,443/35,443); Oblivion authors QSTI on 2,987/3,817 and FNV on
    /// 11,576/18,215. Fallout topics often list multiple owners.
    pub quest_refs: Vec<u32>,
    /// The topic's `DATA` category, translated per game into one canonical
    /// enum. See [`DialogueCategory`] for the per-game layouts (#5045).
    pub category: DialogueCategory,
    /// FO3/FNV `DATA` byte 1 — the topic flags (xEdit `wbDefinitionsFO3`:
    /// `Rumors` 0x01, `Top-level` 0x02). `None` on every other game, and
    /// when the record authored no `DATA` at all; a 1-byte `DATA` (xEdit
    /// marks the flags byte optional) reads as flags `0x00`. #5224 — the
    /// pre-fix decode dropped the byte, so every owned branch-less
    /// Fallout-era topic would have listed as a top-level menu entry.
    pub data_flags: Option<u8>,
    /// Skyrim+ `BNAM` — the [`DlbrRecord`] dialogue branch this topic belongs
    /// to (global space). `None` on Oblivion / FO3 / FNV, which author no
    /// branches.
    pub branch: Option<u32>,
    /// INFO topic responses parsed from the DIAL's `Topic Children`
    /// sub-GRUP (group_type == 7). Pre-#631 the children were silently
    /// skipped because `extract_records` filters on a single record
    /// type; this field is now populated by the dedicated
    /// `extract_dial_with_info` walker. Each entry is one branch of the
    /// dialogue (a single NPC response + its conditions / triggers).
    pub infos: Vec<InfoRecord>,
}

impl DialRecord {
    /// #5224 — the `Top-level` bit (0x02) of [`Self::data_flags`]: only a
    /// Topic DIAL with it set opens the Fallout-era topic menu; the rest
    /// are reached through INFO `TCLT` choices. `None` when the flags are
    /// unknown (any other game, or no `DATA` authored) — consumers keep
    /// the pre-#5224 behaviour then.
    pub fn top_level(&self) -> Option<bool> {
        self.data_flags.map(|flags| flags & 0x02 != 0)
    }
}

/// A `DIAL` topic's category, translated from each game's `DATA` layout
/// (xEdit `wbDefinitions*.pas`, DIAL `DATA`):
///
/// | Games | `DATA` | Category byte | Values |
/// |---|---|---|---|
/// | Oblivion | `Type u8` | byte 0 | 0 Topic, 1 Conversation, 2 Combat, 3 Persuasion, 4 Detection, 5 Service, 6 Miscellaneous |
/// | FO3 / FNV | `Type u8, Flags u8` (flags optional) | byte 0 | as Oblivion, plus 7 Radio |
/// | Skyrim | `Do All Before Repeating u8, Category u8, Subtype u16` | byte 1 | 0 Player, 1 Favor, 2 Scene, 3 Combat, 4 Favors, 5 Detection, 6 Service, 7 Miscellaneous |
/// | FO4 / Starfield | `Topic Flags u8, Category u8, Subtype u16` | byte 1 | 0 Player, 1 Command, 2 Scene, 3 Combat, 4 Favor, 5 Detection, 6 Service, 7 Miscellaneous |
/// | FO76 | as FO4 | byte 1 | 0 Player, 1 Command, 2 Scene, 3 Combat, 4 Detection, 5 Miscellaneous, 6–7 unknown |
///
/// Pre-#5045 byte 0 was read on every game, which on Skyrim / FO4 / FO76 /
/// Starfield is a flags byte (0 on 15,018 of 15,037 Skyrim DIALs), so every
/// Scene and Miscellaneous topic read as a player topic.
///
/// Only the categories shared by name and meaning get a variant. The
/// Skyrim+ index-1 and index-4 labels differ per game (Favor / Command,
/// Favors / Favor) with no source equating them, so they stay [`Self::Other`]
/// with their raw byte, as does anything out of range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DialogueCategory {
    /// A player-selectable topic: classic `Topic` (0), Skyrim+ `Player` (0).
    /// The default when `DATA` is absent or short.
    #[default]
    Topic,
    Conversation,
    Combat,
    Persuasion,
    Detection,
    Service,
    Miscellaneous,
    /// FO3 / FNV `Radio`.
    Radio,
    /// Skyrim+ `Scene`.
    Scene,
    /// A category this engine assigns no meaning to, as its raw byte.
    Other(u8),
}

impl DialogueCategory {
    /// Translate a `DATA` payload for `game`. A payload too short for the
    /// game's category byte reads as [`Self::Topic`], the absent default.
    pub fn from_data(game: GameKind, data: &[u8]) -> Self {
        match game {
            GameKind::Oblivion | GameKind::Fallout3NV => {
                let Some(&raw) = data.first() else {
                    return Self::Topic;
                };
                match raw {
                    0 => Self::Topic,
                    1 => Self::Conversation,
                    2 => Self::Combat,
                    3 => Self::Persuasion,
                    4 => Self::Detection,
                    5 => Self::Service,
                    6 => Self::Miscellaneous,
                    7 if game == GameKind::Fallout3NV => Self::Radio,
                    other => Self::Other(other),
                }
            }
            GameKind::Skyrim | GameKind::Fallout4 | GameKind::Starfield => {
                let Some(&raw) = data.get(1) else {
                    return Self::Topic;
                };
                match raw {
                    0 => Self::Topic,
                    2 => Self::Scene,
                    3 => Self::Combat,
                    5 => Self::Detection,
                    6 => Self::Service,
                    7 => Self::Miscellaneous,
                    other => Self::Other(other),
                }
            }
            GameKind::Fallout76 => {
                let Some(&raw) = data.get(1) else {
                    return Self::Topic;
                };
                match raw {
                    0 => Self::Topic,
                    2 => Self::Scene,
                    3 => Self::Combat,
                    4 => Self::Detection,
                    5 => Self::Miscellaneous,
                    other => Self::Other(other),
                }
            }
        }
    }
}

/// Skyrim+ `DLBR` dialogue branch (xEdit `wbDefinitionsTES5/FO4/FO76/SF1`
/// `DLBR`: `EDID`, `QNAM` Quest, `TNAM` Category u32, `DNAM` Flags u32,
/// `SNAM` Starting Topic). A branch groups topics (`DIAL.BNAM`); only its
/// starting topic is reachable from outside, and the rest are reached
/// through `INFO` `TCLT` links.
///
/// Skyrim ships these as a top-level `DLBR` group (3,061 in `Skyrim.esm`);
/// FO4 nests them under `QUST` (132 in `Fallout4.esm`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DlbrRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Owning quest (`QNAM`, global space).
    pub quest: u32,
    /// `DNAM` flag word; see [`Self::top_level`] / [`Self::blocking`].
    pub flags: u32,
    /// The branch's entry topic (`SNAM`, global space).
    pub starting_topic: u32,
}

impl DlbrRecord {
    /// `DNAM` bit 0 — `Top-Level`.
    pub const FLAG_TOP_LEVEL: u32 = 0x1;
    /// `DNAM` bit 1 — `Blocking`.
    pub const FLAG_BLOCKING: u32 = 0x2;
    /// `DNAM` bit 2 — `Exclusive` (decoded, not consumed).
    pub const FLAG_EXCLUSIVE: u32 = 0x4;

    /// The starting topic is offered in the actor's initial topic list when
    /// valid (Creation Kit, "Bethesda Tutorial Advanced Dialogue").
    pub fn top_level(&self) -> bool {
        self.flags & Self::FLAG_TOP_LEVEL != 0
    }

    /// When the starting topic qualifies, it is the only thing the actor
    /// talks about: it becomes their Hello, and the topic list is replaced by
    /// whatever links from it (same source).
    pub fn blocking(&self) -> bool {
        self.flags & Self::FLAG_BLOCKING != 0
    }
}

/// Parse a `DLBR` record. FormID fields go through the load-order remap.
pub fn parse_dlbr(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> DlbrRecord {
    let mut out = DlbrRecord {
        form_id,
        ..Default::default()
    };
    out.editor_id = CommonNamedFields::from_subs_with_remap(subs, remap).editor_id;
    for sub in subs {
        match &sub.sub_type {
            b"QNAM" if sub.data.len() >= 4 => {
                out.quest = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
            }
            b"DNAM" if sub.data.len() >= 4 => {
                out.flags = SubReader::new(&sub.data).u32_or_default();
            }
            b"SNAM" if sub.data.len() >= 4 => {
                out.starting_topic = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
            }
            _ => {}
        }
    }
    out
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
/// enumerate branches without re-parsing. Scripts (SCHR/SCDA) and
/// edits (NAM3) are deferred; conditions (CTDA/CTDT) landed via
/// #3614 and the `DATA` header via #4469. See #631.
#[derive(Debug, Clone, Default)]
pub struct InfoRecord {
    pub form_id: u32,
    /// The INFO's own `EDID`. Not used for matching (conditions and
    /// speaker filters are), but Bethesda's voice-file naming embeds it:
    /// `sound\\voice\\<plugin>\\<voice type>\\<EDID>_<formid>_<n>`
    /// (#5367 Phase V), so the runtime needs it verbatim.
    pub editor_id: String,
    /// The INFO's own `DATA` header, typed (#4469 decoded it — pre-fix the
    /// whole sub-record was silently discarded, on 22,327 / 22,327 measured
    /// FO3 INFOs and 23,247 / 23,247 FNV; #5295 typed it — the old raw-u16
    /// tail spanned two xEdit fields and documented Goodbye on the wrong
    /// bit). `None` when the record authored no `DATA` (all of FO4, and
    /// 30,541 / 31,465 Skyrim).
    pub data: Option<InfoDataHeader>,
    /// Skyrim's unrelated 8-byte `DATA` (`Quest Dialogue Tab u16, Response
    /// Flags u16, Reset Days f32` — xEdit TES5 INFO), carried raw (924
    /// Skyrim INFOs author one) so [`Self::data`]'s typed layout is never
    /// fed bytes it does not describe: its byte 0 is the tab's low byte,
    /// not a dialogue `Type`. No consumer yet.
    pub skyrim_data: Option<[u8; 8]>,
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
    /// #5271 — `QSTI`, the INFO's **own** owning quest. FO3/FNV/Oblivion
    /// author it on every INFO (census: 23,247/23,247 measured FNV INFOs);
    /// the DIAL-level [`DialRecord::quest_refs`] list every quest that owns
    /// the TOPIC, and one topic can list several — in the GECK an INFO
    /// counts only while THIS quest is running, its priority orders the
    /// topic's INFOs, and its dialogue conditions gate them. Skyrim+/FO4
    /// author no per-INFO QSTI (ownership is DIAL-side `QNAM`); `0` there
    /// means "no per-INFO quest — use the topic's ownership".
    pub quest: u32,
    /// `ANAM` actor form ID — restricts this response to a specific NPC.
    /// 0 means the response works for any actor.
    pub actor_form_id: u32,
    /// Conditions attached to this response (`CTDA`/`CTDT` sub-records,
    /// #3614 — see [`push_ctda`]'s doc for why `CTDT` decodes through the
    /// same path).
    pub conditions: ConditionList,
    /// Skyrim+ `VMAD` scripts section — the compiled `TIF_` topic-info
    /// script's own attached-script + property bindings (e.g. a
    /// `MiscObject Property akItem` a fragment hands to a chest). `None`
    /// on pre-Papyrus games, on INFOs without a VMAD, or when the VMAD
    /// carries no scripts section. This is the property table a
    /// fragment's `Property`-targeted effect resolves through at
    /// dispatch time (#5152, the INFO twin of `QustRecord::script_instance`).
    pub script_instance: Option<ScriptInstanceData>,
    /// The `VMAD` fragment section's OnBegin/OnEnd bindings
    /// ([`parse_info_fragments`], per xEdit `wbVMADFragmentedINFO`).
    /// Empty on pre-Papyrus games and FO4+ (whose section shape is a
    /// separate derivation). Vanilla Skyrim: 5 257 of 31 465 INFOs carry
    /// bindings — see the decoder's doc for the census.
    pub script_fragments: Vec<InfoScriptFragment>,
}

/// The TES4 / FO3 / FNV `INFO` `DATA` header, typed per xEdit's layout
/// (#5295): `Type u8, Next Speaker u8 (wbNextSpeaker, Common:8537),
/// Flags 1 u8, Flags 2 u8`, `SetOptionalFrom(3)` — a 3-byte
/// tail-dropped form is legal (634 FO3 INFOs and 19,276 Oblivion INFOs
/// ship one; Oblivion never authors byte 3). Skyrim's 8-byte `DATA` is a
/// different struct and lives in [`InfoRecord::skyrim_data`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InfoDataHeader {
    /// Byte 0: dialogue `Type` — the same byte-0-is-type convention
    /// `DialRecord::category` decodes per game.
    pub info_type: u8,
    /// Byte 1: `Next Speaker` — 0 Target / 1 Self / 2 Either. Vanilla
    /// census never exceeds 2 (Oblivion byte 1 ∈ {0, 1, 2}; FO3
    /// {0: 22,128, 1: 199}).
    pub next_speaker: u8,
    /// Byte 2: `Flags 1`. FO3 / FNV: bit 0 (`0x01`) = **Goodbye**
    /// (4,716 FO3 + 8,275 FNV goodbye lines); bit 7 (`0x80`) =
    /// Speech Challenge — the bit `wbINFOAfterLoad` (`FO3.pas:2206`)
    /// actually tests (`DATA\Flags 1 and $80`) to decide `DNAM`
    /// retention. Oblivion: its own 8-bit flags — `0x01` Goodbye,
    /// `0x02` Random, `0x04` Say Once, …
    pub flags1: u8,
    /// Byte 3: `Flags 2` (FO3 / FNV only) — Say Once a Day, Always
    /// Darken. `None` on the 3-byte form and on Oblivion.
    pub flags2: Option<u8>,
}

/// The `Flags 1` bits the dialogue *runtime* consumes (#5367 Phase L).
/// Oblivion documents all three directly (xEdit TES4: `0x01` Goodbye,
/// `0x02` Random, `0x04` Say Once); FO3/FNV share the low-bit layout —
/// bit 0 Goodbye is parser-verified (see `flags1`), and bits 1/2 match
/// the 2026-10-07 corpus distribution (FO3: 5 596 Random-shape /
/// 1 149 SayOnce-shape INFOs; FNV: 1 731 / 2 918 — values 2/4 in
/// isolation, never mixed with bit 7's Speech Challenge domain).
/// Skyrim authors `DATA` on 924 of 31 465 INFOs; the rest carry no
/// flags and simply qualify for none of these behaviors.
impl InfoDataHeader {
    /// The conversation ends when this line's presentation finishes.
    pub fn goodbye(&self) -> bool {
        self.flags1 & 0x01 != 0
    }
    /// Among *passing* candidates, this INFO joins the uniform random
    /// pool instead of file/priority order (the greeting mainstay).
    pub fn random(&self) -> bool {
        self.flags1 & 0x02 != 0
    }
    /// Spoken once per save: a said line stops qualifying.
    pub fn say_once(&self) -> bool {
        self.flags1 & 0x04 != 0
    }
}

impl InfoRecord {
    /// [`InfoDataHeader::goodbye`] on this record's typed `DATA`;
    /// `false` when it authored none.
    pub fn goodbye(&self) -> bool {
        self.data.as_ref().is_some_and(InfoDataHeader::goodbye)
    }
    /// [`InfoDataHeader::random`]; `false` when it authored no `DATA`.
    pub fn random(&self) -> bool {
        self.data.as_ref().is_some_and(InfoDataHeader::random)
    }
    /// [`InfoDataHeader::say_once`]; `false` when it authored no `DATA`.
    pub fn say_once(&self) -> bool {
        self.data.as_ref().is_some_and(InfoDataHeader::say_once)
    }
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
    /// #4645 — `TRDA` (FO4 / FO76 / Starfield) emotion keyword FormID,
    /// remapped into global space. From FO4 on the emotion is a KYWD
    /// reference, not TES5's u32 enum, so it cannot land in
    /// [`Self::emotion_type`]; 0 on TRDT-era games.
    pub emotion_keyword: u32,
    /// #4645 — `TRDA` (FO4 / FO76) sound FormID at payload offset 5,
    /// remapped. 0 on TRDT-era games and Starfield (whose 12-byte TRDA
    /// carries no sound field).
    pub sound_form_id: u32,
    /// #4645 — `TRDA` (Starfield) WEM file id at payload offset 4. 0 on
    /// every other game.
    pub wem_file: u32,
}

pub fn parse_dial(
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<crate::esm::reader::FormIdRemap>,
    game: GameKind,
) -> DialRecord {
    let mut out = DialRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    out.editor_id = common.editor_id;
    out.full_name = common.full_name;
    for sub in subs {
        match &sub.sub_type {
            // QSTI (Oblivion/FO3/FNV DIAL "Quest") and QNAM (Skyrim/FO4/
            // FO76/Starfield DIAL "Quest") are the same authored edge per
            // game family — xEdit declares `wbFormIDCkNoReach(QNAM,
            // 'Quest', [QUST])` on TES5/FO4/FO76/SF1, and the raw
            // `Skyrim.esm` census (#5048) puts QNAM on 100% of DIALs with
            // no QSTI on either Skyrim or FO4. Verified against raw
            // `Skyrim.esm` bytes 2026-09-29: MS01's topics
            // (`MS01EltrysNotAtShrineNoteTopic` et al.) carry
            // `QNAM = 0x00018B4B` and no QSTI, which left every Skyrim
            // DIAL with an empty `quest_refs` and starved the
            // activation→topic selection of its ownership edge
            // (`docs/engine/p4-quest-fixture.md` blocker 1). FO4 DIAL
            // ownership was fixed by this same QNAM arm as a side effect.
            b"QSTI" | b"QNAM" if sub.data.len() >= 4 => {
                if let Ok(q) = SubReader::new(&sub.data).u32() {
                    let remapped = remap.as_ref().map_or(q, |r| r.remap(q));
                    out.quest_refs.push(remapped);
                }
            }
            // #5045 — the category byte and its enum are per game.
            // #5224 — FO3/FNV author the topic flags in DATA byte 1
            // (Rumors 0x01, Top-level 0x02, xEdit `wbDefinitionsFO3`).
            // Stored for `DialRecord::top_level`; no other game reads
            // byte 1 as flags (Skyrim+ carry the category there).
            b"DATA" => {
                out.category = DialogueCategory::from_data(game, &sub.data);
                if game == GameKind::Fallout3NV && !sub.data.is_empty() {
                    out.data_flags = Some(sub.data.get(1).copied().unwrap_or(0));
                }
            }
            // Skyrim+ dialogue branch; Oblivion–FNV author no BNAM on DIAL.
            b"BNAM" if sub.data.len() >= 4 => {
                let branch = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
                out.branch = (branch != 0).then_some(branch);
            }
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
            // #5367 Phase V — kept verbatim for the voice-file naming.
            b"EDID" => out.editor_id = read_zstring(&sub.data),
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
            // #4068 (ESM-2026-09-09-D4-01) — FO4 and Starfield rename the
            // per-segment opener from `TRDT` to `TRDA`, and #4645
            // (ESM-2026-09-21-D4-01) supplies the payload layouts the
            // earlier "no cited source" comment deferred to. The two
            // shapes are distinguished by length:
            //
            // * FO4 / FO76, 20 bytes (`wbDefinitionsFO4.pas:9732-9740`,
            //   `wbDefinitionsFO76.pas:12045`): Emotion FormID [KYWD]
            //   u32 @0, Response number u8 @4, Sound FormID u32 @5,
            //   unknown u8 @9, Interrupt u16 @10, two alias s32 @12/@16.
            // * Starfield, 12 bytes (`wbDefinitionsSF1.pas:12815`):
            //   Emotion KYWD u32 @0, WEM file u32 @4, Emotion Out f32
            //   @8. SF1 drops the response number entirely.
            //
            // From FO4 on the emotion is a keyword FormID that must ride
            // `remap_fid`, not a TES5-style u32 enum — `emotion_type`
            // (the TRDT enum byte) stays 0 for TRDA-opened segments and
            // the remapped FormID lands in `emotion_keyword` instead.
            // Any other length keeps the #4068 split-only contract.
            b"TRDA" => {
                if let Some(finished) = current_response.take() {
                    out.responses.push(finished);
                }
                let mut segment = ResponseSegment::default();
                match sub.data.len() {
                    len if len >= 20 => {
                        // #5075 — the emotion KYWD is xEdit
                        // `wbFormIDCk('Emotion', [KYWD, FFFF])`; the none
                        // sentinel (45% of vanilla FO4's rows) must
                        // bypass the remap, not warn out-of-range.
                        segment.emotion_keyword = remap_fid_or_sentinel(
                            SubReader::new(&sub.data[0..4]).u32_or_default(),
                            remap,
                        );
                        segment.response_number = sub.data[4];
                        segment.sound_form_id =
                            remap_fid(SubReader::new(&sub.data[5..9]).u32_or_default(), remap);
                    }
                    len if len >= 12 => {
                        segment.emotion_keyword = remap_fid_or_sentinel(
                            SubReader::new(&sub.data[0..4]).u32_or_default(),
                            remap,
                        );
                        segment.wem_file = SubReader::new(&sub.data[4..8]).u32_or_default();
                    }
                    _ => {}
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
            // #5271 — the INFO's own owning quest. Distinct from the
            // DIAL-level QSTI/QNAM arm in `parse_dial`: that one lists
            // every quest owning the TOPIC, while each FO3/FNV/Oblivion
            // INFO names the one whose running state gates it. Last
            // sub-record wins, matching the PNAM/ANAM assign convention.
            b"QSTI" if sub.data.len() >= 4 => {
                let raw = SubReader::new(&sub.data).u32_or_default();
                let remapped = remap.as_ref().map_or(raw, |r| r.remap(raw));
                out.quest = remapped;
            }
            b"ANAM" if sub.data.len() >= 4 => {
                let raw = u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]);
                let remapped = remap.as_ref().map_or(raw, |r| r.remap(raw));
                out.actor_form_id = remapped;
            }
            // #4469 — the INFO's own `DATA` header, previously dropped on
            // 100% of measured FO3/FNV INFOs. #5295 — typed per xEdit's
            // `Type u8, Next Speaker u8, Flags 1 u8, Flags 2 u8`
            // (`SetOptionalFrom(3)`); Skyrim's 8-byte DATA is a different
            // struct (`Quest Dialogue Tab u16, Response Flags u16, Reset
            // Days f32`) and is carried raw so the typed layout above is
            // never fed bytes it does not describe.
            b"DATA" if !sub.data.is_empty() => {
                if sub.data.len() == 8 {
                    out.skyrim_data = Some(sub.data[..8].try_into().unwrap());
                } else {
                    let mut header = InfoDataHeader {
                        info_type: sub.data[0],
                        ..Default::default()
                    };
                    if sub.data.len() >= 2 {
                        header.next_speaker = sub.data[1];
                    }
                    if sub.data.len() >= 3 {
                        header.flags1 = sub.data[2];
                    }
                    if sub.data.len() >= 4 {
                        header.flags2 = Some(sub.data[3]);
                    }
                    out.data = Some(header);
                }
            }
            // #3614 — `CTDT` is the legacy fixed-layout encoding of the
            // same condition; see `push_ctda`'s doc.
            b"CTDA" | b"CTDT" | b"CIS1" | b"CIS2" => push_ctda(sub, remap, &mut out.conditions),
            // #5152 — Skyrim+ `VMAD`: the TIF_ topic-info script's property
            // table plus the OnBegin/OnEnd fragment bindings. The INFO twin
            // of the QUST arm; FO4+ fragment sections are a separate
            // derivation and decode to an empty binding list (the scripts
            // section itself still decodes — same shape across the family).
            b"VMAD" if !sub.data.is_empty() => {
                out.script_instance = Some(ScriptInstanceData::parse_with_remap(&sub.data, remap));
                out.script_fragments = parse_info_fragments(&sub.data);
            }
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

pub fn parse_mesg(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> MesgRecord {
    let mut out = MesgRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    out.editor_id = common.editor_id;
    out.full_name = common.full_name;
    for sub in subs {
        match &sub.sub_type {
            b"DESC" => out.description = read_lstring_or_zstring(&sub.data),
            // #4071 — QNAM is a QUST cross-reference and needs the
            // load-order remap.
            b"QNAM" if sub.data.len() >= 4 => {
                out.owner_quest = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::sub;

    #[test]
    fn parse_dial_accumulates_multiple_quest_refs() {
        let subs = vec![
            sub(b"EDID", b"GREETING\0"),
            sub(b"FULL", b"Greeting\0"),
            sub(b"QSTI", 0x0100_0001u32.to_le_bytes()),
            sub(b"QSTI", 0x0100_0002u32.to_le_bytes()),
            sub(b"QSTI", 0x0100_0003u32.to_le_bytes()),
        ];
        let d = parse_dial(0xC3C3, &subs, &None, GameKind::Fallout3NV);
        assert_eq!(d.quest_refs.len(), 3);
        assert_eq!(d.quest_refs[1], 0x0100_0002);
        // DATA absent → the Topic default, and no branch.
        assert_eq!(d.category, DialogueCategory::Topic);
        assert_eq!(d.branch, None);
    }

    /// #1307 — Oblivion / FO3 / FNV author the category in DATA byte 0.
    #[test]
    fn classic_games_read_the_type_byte() {
        let d = parse_dial(0xDEAD, &[sub(b"DATA", [3u8])], &None, GameKind::Oblivion);
        assert_eq!(d.category, DialogueCategory::Persuasion);
        // FO3/FNV: type + flags; 7 is Radio there, out of range on Oblivion.
        let fnv = parse_dial(
            0xBEEF,
            &[sub(b"DATA", [7u8, 0x02])],
            &None,
            GameKind::Fallout3NV,
        );
        assert_eq!(fnv.category, DialogueCategory::Radio);
        let obl = parse_dial(0xBEEF, &[sub(b"DATA", [7u8])], &None, GameKind::Oblivion);
        assert_eq!(obl.category, DialogueCategory::Other(7));
        // Empty DATA must not panic and leaves the default.
        let empty = parse_dial(0xF00D, &[sub(b"DATA", [])], &None, GameKind::Oblivion);
        assert_eq!(empty.category, DialogueCategory::Topic);
    }

    /// #5045 — Skyrim+ author the category in DATA byte 1; byte 0 is a
    /// flags byte (Skyrim `Do All Before Repeating`, FO4+ `Topic Flags`).
    /// The pre-fix byte-0 read made every Skyrim Scene topic a player topic.
    #[test]
    fn skyrim_plus_read_the_category_byte_not_the_flags_byte() {
        let scene = [0x01u8, 2, 0, 0]; // flags 1, category Scene
        for game in [GameKind::Skyrim, GameKind::Fallout4, GameKind::Starfield] {
            let d = parse_dial(0x1, &[sub(b"DATA", scene)], &None, game);
            assert_eq!(d.category, DialogueCategory::Scene, "{game:?}");
            let misc = parse_dial(0x1, &[sub(b"DATA", [0, 7, 0, 0])], &None, game);
            assert_eq!(misc.category, DialogueCategory::Miscellaneous, "{game:?}");
            let player = parse_dial(0x1, &[sub(b"DATA", [0x01, 0, 0, 0])], &None, game);
            assert_eq!(player.category, DialogueCategory::Topic, "{game:?}");
        }
        // FO76 renumbers after Combat: 4 Detection, 5 Miscellaneous.
        let fo76 = parse_dial(
            0x1,
            &[sub(b"DATA", [0, 5, 0, 0])],
            &None,
            GameKind::Fallout76,
        );
        assert_eq!(fo76.category, DialogueCategory::Miscellaneous);
        let fo4 = parse_dial(
            0x1,
            &[sub(b"DATA", [0, 5, 0, 0])],
            &None,
            GameKind::Fallout4,
        );
        assert_eq!(fo4.category, DialogueCategory::Detection);
        // Index 1 / 4 labels differ per game — kept raw.
        let favor = parse_dial(0x1, &[sub(b"DATA", [0, 1, 0, 0])], &None, GameKind::Skyrim);
        assert_eq!(favor.category, DialogueCategory::Other(1));
    }

    /// #5224 — FO3/FNV author the topic flags in DATA byte 1 (`Rumors`
    /// 0x01, `Top-level` 0x02); the pre-fix decode dropped the byte. Every
    /// other game keeps `data_flags` at `None` — on Skyrim+ byte 1 is the
    /// category byte, not a flags byte.
    #[test]
    fn fallout_era_dial_flags_decode_and_other_games_stay_none() {
        let top = parse_dial(
            0x1,
            &[sub(b"DATA", [0u8, 0x02])],
            &None,
            GameKind::Fallout3NV,
        );
        assert_eq!(top.category, DialogueCategory::Topic);
        assert_eq!(top.top_level(), Some(true));

        let rumors = parse_dial(
            0x2,
            &[sub(b"DATA", [0u8, 0x01])],
            &None,
            GameKind::Fallout3NV,
        );
        assert_eq!(
            rumors.top_level(),
            Some(false),
            "Rumors alone is not Top-level"
        );

        // xEdit marks the flags byte optional; a 1-byte DATA reads as
        // flags 0x00, i.e. not top-level.
        let short = parse_dial(0x3, &[sub(b"DATA", [0u8])], &None, GameKind::Fallout3NV);
        assert_eq!(short.top_level(), Some(false));

        // No DATA at all stays unknown (`None`), like every other game.
        let none = parse_dial(0x4, &[sub(b"EDID", b"X\0")], &None, GameKind::Fallout3NV);
        assert_eq!(none.top_level(), None);

        // Skyrim+'s byte 1 is the category — stored as such, never as flags.
        let skyrim = parse_dial(0x5, &[sub(b"DATA", [0u8, 2u8, 0, 0])], &None, GameKind::Skyrim);
        assert_eq!(skyrim.category, DialogueCategory::Scene);
        assert_eq!(skyrim.top_level(), None);
        let oblivion = parse_dial(0x6, &[sub(b"DATA", [0u8])], &None, GameKind::Oblivion);
        assert_eq!(oblivion.top_level(), None);
    }

    #[test]
    fn parse_dial_reads_the_branch() {
        let d = parse_dial(
            0x0008_06B8,
            &[sub(b"BNAM", 0x0001_8A96u32.to_le_bytes())],
            &None,
            GameKind::Skyrim,
        );
        assert_eq!(d.branch, Some(0x0001_8A96));
    }

    /// Skyrim's `MS01EltrysBlockingShrineBranch01` (raw `Skyrim.esm` bytes).
    #[test]
    fn parse_dlbr_decodes_quest_flags_and_starting_topic() {
        let subs = vec![
            sub(b"EDID", b"MS01EltrysBlockingShrineBranch01\0"),
            sub(b"QNAM", 0x0001_8B4Bu32.to_le_bytes()),
            sub(b"TNAM", 0u32.to_le_bytes()),
            sub(b"DNAM", 2u32.to_le_bytes()),
            sub(b"SNAM", 0x0008_06B8u32.to_le_bytes()),
        ];
        let b = parse_dlbr(0x0001_8A96, &subs, &None);
        assert_eq!(b.editor_id, "MS01EltrysBlockingShrineBranch01");
        assert_eq!(b.quest, 0x0001_8B4B);
        assert_eq!(b.starting_topic, 0x0008_06B8);
        assert!(b.blocking());
        assert!(!b.top_level());
    }

    #[test]
    fn parse_mesg_picks_desc_and_owner_quest() {
        let subs = vec![
            sub(b"EDID", b"FastTravelMessage\0"),
            sub(b"FULL", b"Fast Travel\0"),
            sub(b"DESC", b"You cannot fast travel right now.\0"),
            sub(b"QNAM", 0x0002_1234u32.to_le_bytes()),
        ];
        let m = parse_mesg(0xD4D4, &subs, &None);
        assert_eq!(m.description, "You cannot fast travel right now.");
        assert_eq!(m.owner_quest, 0x0002_1234);
    }

    /// #4131 — `parse_mesg`'s QNAM (#4071) was only ever exercised under
    /// `&None` (identity) remap, so a future regression reverting the
    /// `remap_fid` call would compile clean and pass every existing test.
    #[test]
    fn parse_mesg_qnam_is_remapped() {
        let remap = Some(FormIdRemap::regular(2, vec![0]));
        let subs = vec![
            sub(b"EDID", b"FastTravelMessage\0"),
            sub(b"QNAM", 0x0100_9999u32.to_le_bytes()), // self-ref
        ];
        let m = parse_mesg(0x0200_0001, &subs, &remap);
        assert_eq!(
            m.owner_quest, 0x0200_9999,
            "a self-referencing QNAM must land on the plugin's own global \
             slot, not keep its plugin-local mod index"
        );
    }

    #[test]
    fn parse_info_picks_anam_actor() {
        let anam = 0xDEAD_BEEFu32.to_le_bytes();
        let subs = vec![sub(b"NAM1", b"hello\0"), sub(b"ANAM", anam)];
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

    /// #5152 — the INFO `VMAD` fragment section decodes per xEdit's
    /// `wbVMADFragmentedINFO`. The fixture bytes are a real vanilla
    /// `Skyrim.esm` sample (INFO `0x0D66E5`, topic `0x0228A4`, probed
    /// 2026-09-30): scripts section naming `TIF__000D66E5` with no
    /// properties, then a version-2 section whose flags byte `0x01`
    /// binds one OnBegin fragment, `Fragment_1`, on the same script.
    #[test]
    fn parse_info_vmad_fragments_decode() {
        let mut vmad: Vec<u8> = vec![
            // Scripts section: version 5, object format 2, 1 script,
            // status 0, "TIF__000D66E5", 0 properties (flags u32 + u16).
            0x05, 0x00, 0x02, 0x00, 0x01, 0x00,
        ];
        vmad.extend(0x0du16.to_le_bytes());
        vmad.extend(b"TIF__000D66E5");
        // status u8 (version >= 4) + property count u16 = 0.
        vmad.extend([0x00, 0x00, 0x00]);
        // Fragment section: version 2, flags 0x01 (OnBegin), FileName,
        // one entry {unknown 1, ScriptName, FragmentName "Fragment_1"}.
        vmad.extend([0x02, 0x01]);
        vmad.extend(0x0du16.to_le_bytes());
        vmad.extend(b"TIF__000D66E5");
        vmad.push(0x01);
        vmad.extend(0x0du16.to_le_bytes());
        vmad.extend(b"TIF__000D66E5");
        vmad.extend(0x0au16.to_le_bytes());
        vmad.extend(b"Fragment_1");

        let info = parse_info(0x0D66E5, &[sub(b"VMAD", &vmad)], &None);
        let script = info.script_instance.expect("scripts section decodes");
        assert_eq!(script.scripts.len(), 1);
        assert_eq!(script.scripts[0].name, "TIF__000D66E5");
        assert_eq!(info.script_fragments.len(), 1);
        let fragment = &info.script_fragments[0];
        assert!(fragment.on_begin, "flags 0x01 is OnBegin");
        assert_eq!(fragment.script_name, "TIF__000D66E5");
        assert_eq!(fragment.fragment_name, "Fragment_1");

        // Flags 0x02 = OnEnd only; 0x03 = both, OnBegin first (xEdit:
        // "Do NOT sort, ordered OnBegin, OnEnd").
        // The FileName is the FIRST string after the flags byte.
        // the flags byte.
        let mut on_end = [
            0x05u8, 0x00, 0x02, 0x00, 0x00, 0x00, // scripts: 0 scripts
            0x02, 0x02, // fragment version 2, flags OnEnd
        ]
        .to_vec();
        on_end.extend(0x0du16.to_le_bytes());
        on_end.extend(b"TIF__000D66E5"); // FileName
        on_end.push(0x01); // unknown
        on_end.extend(0x0du16.to_le_bytes());
        on_end.extend(b"TIF__000D66E5"); // ScriptName
        on_end.extend(0x0au16.to_le_bytes());
        on_end.extend(b"Fragment_0"); // FragmentName
        let info = parse_info(0x1, &[sub(b"VMAD", &on_end)], &None);
        assert_eq!(info.script_fragments.len(), 1);
        assert!(!info.script_fragments[0].on_begin, "flags 0x02 is OnEnd");

        let mut both = [
            0x05u8, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x03,
        ]
        .to_vec();
        both.extend(0x0du16.to_le_bytes());
        both.extend(b"TIF__000D66E5"); // FileName
        for name in ["Fragment_7", "Fragment_9"] {
            both.push(0x01);
            both.extend(0x0du16.to_le_bytes());
            both.extend(b"TIF__000D66E5");
            both.extend((name.len() as u16).to_le_bytes());
            both.extend(name.as_bytes());
        }
        let info = parse_info(0x2, &[sub(b"VMAD", &both)], &None);
        assert_eq!(info.script_fragments.len(), 2, "both flags = two entries");
        assert!(info.script_fragments[0].on_begin, "OnBegin first");
        assert!(!info.script_fragments[1].on_begin, "OnEnd second");
        assert_eq!(info.script_fragments[0].fragment_name, "Fragment_7");
        assert_eq!(info.script_fragments[1].fragment_name, "Fragment_9");

        // A pre-Papyrus game ships no VMAD at all; an empty one decodes to
        // nothing rather than panicking.
        let info = parse_info(0x3, &[], &None);
        assert!(info.script_instance.is_none());
        assert!(info.script_fragments.is_empty());
        let info = parse_info(0x4, &[sub(b"VMAD", [])], &None);
        assert!(info.script_fragments.is_empty());
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
        let subs = vec![sub(b"NAM1", b"hi\0"), sub(b"CTDT", ctdt)];
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
            sub(b"TCLF", 0x0001_1111u32.to_le_bytes()),
            sub(b"TCLF", 0x0001_2222u32.to_le_bytes()),
            sub(b"NAME", 0x0001_3333u32.to_le_bytes()),
        ];
        let info = parse_info(0x9999, &subs, &None);
        assert_eq!(info.linked_from_topics, vec![0x0001_1111, 0x0001_2222]);
        assert_eq!(info.added_topics, vec![0x0001_3333]);
    }

    /// #4469 — the INFO's own `DATA` header (dialogue `Type` byte + the
    /// tail) was silently discarded on 22,327 / 22,327 measured FO3 INFOs
    /// and 23,247 / 23,247 FNV. #5295 — the tail is typed per xEdit's
    /// `Type u8, Next Speaker u8, Flags 1 u8, Flags 2 u8`: Goodbye is
    /// `Flags 1` bit **0** (the old decode put it on bit 7 of byte 1,
    /// which is Next Speaker and never set — 4,716 FO3 + 8,275 FNV
    /// goodbye lines read as plain), and bit 7 of `Flags 1` is Speech
    /// Challenge, the bit `wbINFOAfterLoad` actually tests.
    #[test]
    fn parse_info_data_header_decodes_type_and_flags() {
        // FO3/FNV shape: Type u8 + Next Speaker u8 + Flags 1 u8 [+ Flags 2].
        // A goodbye line: Flags 1 bit 0.
        let subs = vec![sub(b"DATA", [0u8, 0x00, 0x01, 0x00])];
        let info = parse_info(0x5678, &subs, &None);
        let header = info.data.unwrap();
        assert_eq!(header.info_type, 0, "a plain Topic line");
        assert_eq!(header.next_speaker, 0, "Target");
        assert_eq!(
            header.flags1 & 0x01,
            0x01,
            "the goodbye bit must survive the parse (#4469)"
        );
        assert_eq!(header.flags2, Some(0));

        // A speech-challenge line: Flags 1 bit 7 — the bit xEdit's
        // `wbINFOAfterLoad` (`DATA\Flags 1 and $80`) reads for DNAM
        // retention, which #4469's decode mislabeled as Goodbye.
        let subs = vec![sub(b"DATA", [0u8, 0x01, 0x80, 0x00])];
        let header = parse_info(0x5678, &subs, &None).data.unwrap();
        assert_eq!(header.next_speaker, 1, "Self");
        assert_eq!(header.flags1 & 0x80, 0x80, "Speech Challenge");
        assert_eq!(header.flags1 & 0x01, 0, "not Goodbye");

        // The 3-byte tail-dropped form (634 FO3 + all Oblivion INFOs):
        // `Flags 2` is absent, not zero.
        let subs = vec![sub(b"DATA", [3u8, 0x02, 0x04])];
        let header = parse_info(0x5678, &subs, &None).data.unwrap();
        assert_eq!(header.info_type, 3, "Combat");
        assert_eq!(header.next_speaker, 2, "Either");
        assert_eq!(header.flags1, 0x04, "Oblivion bit 2 = Say Once");
        assert_eq!(header.flags2, None);

        // Skyrim's 8-byte DATA is a different struct and must not leak
        // into the typed header (its byte 0 is a Quest Dialogue Tab low
        // byte, not a dialogue Type).
        let subs = vec![sub(b"DATA", [9u8, 0, 1, 0, 0, 0, 0x30, 0x42])];
        let info = parse_info(0x5678, &subs, &None);
        assert_eq!(info.data, None);
        assert_eq!(info.skyrim_data, Some([9, 0, 1, 0, 0, 0, 0x30, 0x42]));

        // No DATA at all (a typical Skyrim / all FO4 INFO) keeps both
        // fields at None.
        let info = parse_info(0x5678, &[sub(b"NAM1", b"hi\0")], &None);
        assert_eq!(info.data, None);
        assert_eq!(info.skyrim_data, None);
    }

    /// #3614 — TCLF/NAME FormIDs are plugin-local like TCLT/PNAM/ANAM and
    /// must be remapped the same way.
    #[test]
    fn parse_info_remaps_tclf_and_name_with_remap() {
        use crate::esm::reader::FormIdRemap;
        let remap = FormIdRemap::regular(1, vec![0]);
        let subs = vec![
            sub(b"TCLF", 0x01_040000u32.to_le_bytes()),
            sub(b"NAME", 0x01_050000u32.to_le_bytes()),
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
            sub(b"TRDT", trdt(5, 0)), // Happy
            sub(b"NAM1", b"First line.\0"),
            sub(b"NAM2", b"cheerfully\0"),
            sub(b"TRDT", trdt(1, 1)), // Anger
            sub(b"NAM1", b"Second line.\0"),
            sub(b"NAM2", b"then annoyed\0"),
            sub(b"TRDT", trdt(4, 2)), // Sad
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

    /// #4068 (ESM-2026-09-09-D4-01) — FO4/Starfield's shape: `TRDA` opens
    /// each segment instead of `TRDT`. Pre-fix, `TRDA` had no arm at all, so
    /// every `NAM1` assigned into the same lazily-created segment for the
    /// whole record — exactly the pre-#3616 collapse, on the two newest
    /// titles this time. Payloads are zeroed here so this pins only the
    /// segment-splitting contract; the #4645 decode tests below pin the
    /// field extraction.
    #[test]
    fn parse_info_trda_opens_a_new_segment_like_trdt() {
        let subs = vec![
            sub(b"TRDA", [0u8; 20]), // FO4 shape — zero payload
            sub(b"NAM1", b"First line.\0"),
            sub(b"NAM2", b"cheerfully\0"),
            sub(b"TRDA", [0u8; 12]), // Starfield shape — different width
            sub(b"NAM1", b"Second line.\0"),
        ];
        let info = parse_info(0x1234, &subs, &None);
        assert_eq!(
            info.responses.len(),
            2,
            "each TRDA must start a fresh segment, not collapse into one"
        );
        assert_eq!(info.responses[0].text, "First line.");
        assert_eq!(info.responses[0].designer_notes, "cheerfully");
        assert_eq!(info.responses[1].text, "Second line.");
        assert_eq!(
            info.responses[1].designer_notes, "",
            "no leakage across a TRDA boundary"
        );
        assert_eq!(
            info.response_text, "First line.\nSecond line.",
            "flat convenience field must carry every segment, not just the last"
        );
    }

    /// #4645 — the FO4 20-byte TRDA payload decodes: emotion KYWD @0
    /// (remapped), response number u8 @4 (the `<INFO>_<n>` voice-file
    /// key), sound FormID @5 (remapped). Layout cited from xEdit
    /// `wbDefinitionsFO4.pas:9732-9740`; FO76 shares it.
    #[test]
    fn fo4_trda_payload_decodes_response_number_and_emotion_keyword() {
        use crate::esm::reader::FormIdRemap;
        // Plugin slot 1, master slot 0 — a DLC-authored INFO referencing
        // a base-game emotion keyword.
        let remap = FormIdRemap::regular(1, vec![0]);
        let mut fo4 = Vec::with_capacity(20);
        fo4.extend_from_slice(&0x0007_1122u32.to_le_bytes()); // emotion KYWD (master)
        fo4.push(3); // response number
        fo4.extend_from_slice(&0x0100_3344u32.to_le_bytes()); // sound (self)
        fo4.push(0); // unknown
        fo4.extend_from_slice(&0u16.to_le_bytes()); // interrupt
        fo4.extend_from_slice(&0i32.to_le_bytes()); // alias 1
        fo4.extend_from_slice(&0i32.to_le_bytes()); // alias 2
        assert_eq!(fo4.len(), 20);

        let subs = vec![
            sub(b"TRDA", &fo4),
            sub(b"NAM1", b"One.\0"),
            // A second, zeroed FO4 segment: defaults must not leak from
            // the first.
            sub(b"TRDA", [0u8; 20]),
        ];
        let info = parse_info(0x99, &subs, &Some(remap));
        assert_eq!(info.responses.len(), 2);
        let seg = &info.responses[0];
        assert_eq!(seg.emotion_keyword, 0x0007_1122, "master KYWD remaps through the load order");
        assert_eq!(seg.response_number, 3);
        assert_eq!(seg.sound_form_id, 0x0100_3344, "self sound ref remaps to the plugin slot");
        assert_eq!(seg.emotion_type, 0, "TRDA segments keep the TRDT enum field at 0");
        assert_eq!(
            info.response_number, 3,
            "flat convenience field carries the first segment's number"
        );
        assert_eq!(info.responses[1].response_number, 0);
        assert_eq!(info.responses[1].emotion_keyword, 0);
    }

    /// #5075 — the TRDA emotion field is xEdit
    /// `wbFormIDCk('Emotion', [KYWD, FFFF])`: the `FFFF` none sentinel is
    /// authored on ~45% of vanilla FO4's rows (and 40–52% of the DLC
    /// plugins'), and its 0xFF mod index must bypass the remap rather than
    /// warn out-of-range once per row on every plugin with masters — the
    /// same class as #4172's MGEF `associated_item` fix.
    #[test]
    fn trda_emotion_none_sentinel_bypasses_the_remap() {
        use crate::esm::reader::FormIdRemap;
        let remap = FormIdRemap::regular(1, vec![0]);
        let mut fo4 = vec![0u8; 20];
        fo4[0..4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // none sentinel
        fo4[4] = 7; // response number
        fo4[5..9].copy_from_slice(&0x0100_3344u32.to_le_bytes()); // sound (self)
        let subs = vec![sub(b"TRDA", &fo4), sub(b"NAM1", b"Line.\0")];
        let info = parse_info(0x99, &subs, &Some(remap));
        let seg = &info.responses[0];
        assert_eq!(
            seg.emotion_keyword, 0xFFFF_FFFF,
            "the none sentinel must round-trip verbatim, never enter the \
             remap's out-of-range warn arm"
        );
        assert_eq!(seg.response_number, 7);
        assert_eq!(seg.sound_form_id, 0x0100_3344, "a regular ref still rides the remap");
    }

    /// #4645 — the Starfield 12-byte TRDA payload decodes: emotion KYWD
    /// @0 (remapped) and WEM file u32 @4. SF1 has no response number and
    /// no sound field — both must stay 0. Layout cited from xEdit
    /// `wbDefinitionsSF1.pas:12815`.
    #[test]
    fn starfield_trda_payload_decodes_emotion_keyword_and_wem() {
        use crate::esm::reader::FormIdRemap;
        let remap = FormIdRemap::regular(0, Vec::new());
        let mut sf = Vec::with_capacity(12);
        sf.extend_from_slice(&0x000A_BCDEu32.to_le_bytes()); // emotion KYWD
        sf.extend_from_slice(&0x0001_2345u32.to_le_bytes()); // WEM file
        sf.extend_from_slice(&0.5f32.to_le_bytes()); // emotion out
        assert_eq!(sf.len(), 12);

        let subs = vec![sub(b"TRDA", &sf), sub(b"NAM1", b"Hello.\0")];
        let info = parse_info(0x77, &subs, &Some(remap));
        let seg = &info.responses[0];
        assert_eq!(seg.emotion_keyword, 0x000A_BCDE);
        assert_eq!(seg.wem_file, 0x0001_2345);
        assert_eq!(seg.response_number, 0, "SF1 TRDA carries no response number");
        assert_eq!(seg.sound_form_id, 0, "SF1 TRDA carries no sound FormID");
    }

    /// #4645 — an unrecognized TRDA width (neither >= 20 nor >= 12 with
    /// the two known shapes) keeps the #4068 split-only contract instead
    /// of guessing a decode.
    #[test]
    fn unknown_width_trda_still_splits_without_decoding() {
        let subs = vec![
            sub(b"TRDA", [0xEEu8; 7]),
            sub(b"NAM1", b"Split me.\0"),
        ];
        let info = parse_info(0x55, &subs, &None);
        assert_eq!(info.responses.len(), 1);
        assert_eq!(info.responses[0].text, "Split me.");
        assert_eq!(info.responses[0].emotion_keyword, 0);
    }

    #[test]
    fn parse_info_remaps_formids_with_remap() {
        use crate::esm::reader::FormIdRemap;
        // PNAM (previous_info) and TCLT (topic_links) and ANAM (actor)
        // should be remapped when a remap is provided.
        // This plugin at index 1, master at index 0 (all regular, no ESL).
        let remap = FormIdRemap::regular(1, vec![0]);
        let subs = vec![
            sub(b"PNAM", 0x00_050000u32.to_le_bytes()), // plugin 0 (master), form 0x050000
            sub(b"TCLT", 0x01_030000u32.to_le_bytes()), // plugin 1 (this), form 0x030000
            sub(b"ANAM", 0x00_020000u32.to_le_bytes()), // plugin 0 (master), form 0x020000
            // #5271 — the INFO's own owning quest, remapped like its
            // sibling reference subs.
            sub(b"QSTI", 0x00_040000u32.to_le_bytes()), // plugin 0 (master), form 0x040000
        ];
        // With remap: plugin 0 stays 0 (master), plugin 1 stays 1 (this)
        let info = parse_info(0x5678, &subs, &Some(remap));
        assert_eq!(info.previous_info, 0x00_050000);
        assert_eq!(info.topic_links[0], 0x01_030000);
        assert_eq!(info.actor_form_id, 0x00_020000);
        assert_eq!(info.quest, 0x00_040000, "#5271 — QSTI remaps with the rest");
        // Verify that without remap, values are identical (no remap = identity)
        let info_no_remap = parse_info(0x5678, &subs, &None);
        assert_eq!(info_no_remap.previous_info, info.previous_info);
        assert_eq!(info_no_remap.quest, info.quest);
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
