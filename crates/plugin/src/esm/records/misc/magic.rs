//! Magic / perks records.

use super::super::common::{read_lstring_or_zstring, remap_fid, CommonNamedFields};
use super::super::condition::{push_ctda, ConditionList};
use crate::esm::reader::{FormIdRemap, GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;
use anyhow::Result;

/// Trait for typed sub-record schema decoding.
/// Implementers declare the sub-record code and define how to read
/// their data from a SubReader cursor.
pub trait SubRecordSchema: Sized {
    const CODE: [u8; 4];
    fn read(r: &mut SubReader) -> Result<Self>;
}

/// Read a SubRecord using a schema implementer.
pub fn read_sub<T: SubRecordSchema>(sub: &SubRecord) -> Result<T> {
    if sub.sub_type != T::CODE {
        anyhow::bail!(
            "SubRecord type mismatch: expected {:?}, got {:?}",
            std::str::from_utf8(&T::CODE).unwrap_or("invalid UTF-8"),
            std::str::from_utf8(&sub.sub_type).unwrap_or("invalid UTF-8")
        );
    }
    let mut reader = SubReader::new(&sub.data);
    T::read(&mut reader)
}

/// #4415 — a spell's type, canonical across games (xEdit SPIT "Type"
/// enum: Oblivion/FO3/FNV @0, Skyrim/FO4 @8).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SpellType {
    /// A castable spell ("Actor Effect" on FO3/FNV).
    #[default]
    Spell,
    Disease,
    Power,
    LesserPower,
    /// A constant effect the actor always carries: its value modifiers are
    /// permanent while the spell is on the actor.
    Ability,
    Poison,
    Addiction,
    /// Skyrim shouts' voice spells.
    Voice,
    /// Any other value, raw.
    Other(u32),
}

impl SpellType {
    fn from_raw(raw: u32) -> Self {
        match raw {
            0 => Self::Spell,
            1 => Self::Disease,
            2 => Self::Power,
            3 => Self::LesserPower,
            4 => Self::Ability,
            5 => Self::Poison,
            10 => Self::Addiction,
            11 => Self::Voice,
            other => Self::Other(other),
        }
    }
}

/// #4415 — `SPIT` per game (xEdit SPIT): Oblivion/FO3/FNV are Type @0,
/// Cost @4, Level @8, Flags in the LOW BYTE @12 (Oblivion's high three
/// bytes are often `0xCDCDCD` filler); Skyrim/FO4/FO76 are Cost @0, Flags
/// u32 @4, Type @8 (Charge Time f32 @12). Returns `(type, cost, flags)`.
/// The pre-#4415 decoder read cost @0 and flags @12 for every game, so
/// FO3/FNV "cost" was the type and Skyrim "flags" the charge time's bits.
fn decode_spit(data: &[u8], game: GameKind) -> Option<(SpellType, u32, u32)> {
    if data.len() < 16 {
        return None;
    }
    let read = |offset: usize| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    match game {
        GameKind::Oblivion => Some((
            SpellType::from_raw(read(0) & 0xFF),
            read(4),
            read(12) & 0xFF,
        )),
        GameKind::Fallout3NV => Some((SpellType::from_raw(read(0)), read(4), read(12) & 0xFF)),
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 => {
            Some((SpellType::from_raw(read(8)), read(0), read(4)))
        }
        GameKind::Starfield => None,
    }
}

/// ENIT (Enchantment Header) schema — fixed 16 bytes (FO3/FNV/Oblivion)
/// or 20 bytes (Skyrim adds cast_type u32). Phase C schema decoder.
#[derive(Debug, Clone, Copy)]
struct EnchantmentHeader {
    pub enchantment_type: u32, // @0
    pub charge_amount: u32,    // @4
    pub enchant_cost: u32,     // @8
    pub enchant_flags: u32,    // @12
                               // Skyrim adds: cast_type: u32 @16 (not decoded here)
}

impl SubRecordSchema for EnchantmentHeader {
    const CODE: [u8; 4] = *b"ENIT";

    fn read(r: &mut SubReader) -> Result<Self> {
        Ok(EnchantmentHeader {
            enchantment_type: r.u32_or_default(),
            charge_amount: r.u32_or_default(),
            enchant_cost: r.u32_or_default(),
            enchant_flags: r.u32_or_default(),
        })
    }
}

/// DATA (Magic Effect Header) schema.
/// FO3/FNV: 36 bytes
///   @0:  effect_flags u32
///   @4:  base_cost f32
///   @8:  associated_item u32
///   @12: magic_school i32
///   @16: resistance_av i32
///   @20: counter_effect_count u16 + pad u16
///   @24: light_form_id u32
///   @28: projectile_speed f32
///   @32: effect_shader_id u32
/// Skyrim+: Exact layout unknown; decoder is strict about minimum 36-byte FO3/FNV.
/// Phase C schema decoder — fails loudly on short buffers so parse_mgef can
/// log and fall back to defaults rather than silently returning garbage.
#[derive(Debug, Clone)]
struct MagicEffectHeader {
    pub effect_flags: u32,
    pub base_cost: f32,
    pub associated_item: u32,
    pub magic_school: i32,
    pub resistance_av: i32,
    pub light_form_id: u32,
    pub projectile_speed: f32,
    pub effect_shader_id: u32,
}

impl SubRecordSchema for MagicEffectHeader {
    const CODE: [u8; 4] = *b"DATA";

    fn read(r: &mut SubReader) -> Result<Self> {
        // Require minimum 36 bytes (FO3/FNV full layout).
        // If buffer is short, caller will catch the Err and log/default.
        if r.remaining() < 36 {
            anyhow::bail!(
                "MagicEffectHeader DATA too short: need 36 bytes, got {}",
                r.remaining()
            );
        }
        let effect_flags = r.u32_or_default();
        let base_cost = r.f32_or_default();
        let associated_item = r.u32_or_default();
        let magic_school = r.i32_or_default();
        let resistance_av = r.i32_or_default();
        r.skip_or_eof(4); // counter_effect_count u16 + pad u16 @20..24
        let light_form_id = r.u32_or_default();
        let projectile_speed = r.f32_or_default();
        let effect_shader_id = r.u32_or_default();
        Ok(MagicEffectHeader {
            effect_flags,
            base_cost,
            associated_item,
            magic_school,
            resistance_av,
            light_form_id,
            projectile_speed,
            effect_shader_id,
        })
    }
}

/// Typed representation of EPFD (entry-point function data) bytes.
/// The shape depends on the `function_type` byte from EPFT.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PerkFunctionData {
    #[default]
    None,
    Float(f32),
    Range {
        min: f32,
        max: f32,
    },
    FormId(u32),
    LString(u32),
}

/// One body of a `PRKE`/`PRKF` perk entry block. The block opens with
/// a `PRKE` header (entry type + rank + priority) and the following
/// `DATA` carries the per-type payload — three mutually exclusive
/// shapes per [`perk_entry_points.md`](file:./../../../../../../../memory/perk_entry_points.md):
///
/// * **Quest** — start `quest` and advance to `stage` when the
///   condition list (M47.1 follow-up) passes.
/// * **Ability** — add `spell_form_id` to the actor while the perk is
///   held. Lifecycle is automatic (added on perk-grant, removed on
///   perk-revoke).
/// * **EntryPoint** — modify a hardcoded game calculation hook. The
///   `entry_point_index` is the raw u8 from the on-disk schema (~120
///   defined points across the games; the per-game decoder enum lives
///   in `byroredux_scripting`). `function_type` is the raw EPFT byte
///   (Add / Multiply / Set / range / AV-mult etc., 0x00..0x09 on
///   FO3/FNV, extended on Skyrim+/FO4). `function_data` is the raw
///   EPFD payload — typed decode of `f32` / `(f32, f32)` / FormID /
///   lstring per-`function_type` is the follow-up commit.
#[derive(Debug, Clone, PartialEq)]
pub enum PerkEntryBody {
    Quest {
        quest_form_id: u32,
        /// Stage to advance the quest to when the entry fires. Most
        /// vanilla content sets a single stage; the value is u8 on
        /// disk but stored as u16 for forward-compat with future
        /// schema growth.
        stage: u16,
    },
    Ability {
        spell_form_id: u32,
    },
    EntryPoint {
        /// Raw entry-point index (0..~120). Per-game enum dispatch
        /// lives at the consumer side (`byroredux_scripting`).
        entry_point_index: u8,
        /// Raw EPFT byte (Add/Multiply/Set/range/AV-mult/...).
        function_type: u8,
        /// Typed EPFD payload: f32 / range / FormID / lstring per
        /// function_type. Decoded at parse time; consumer reads
        /// the typed variant directly.
        function_data: PerkFunctionData,
        /// EPF2 — FO4+ extended function-data formatter string
        /// (Activate entry-point uses this for the prompt template).
        /// Empty when absent. Captured-on-disk only; consumer-side.
        formatter: Vec<u8>,
        /// EPF3 — FO4+ extended function flags / version byte.
        /// Captured-on-disk only; consumer-side.
        extra_flags: Vec<u8>,
    },
}

/// One `PRKE`/`PRKF` perk entry — the entry header + body together.
#[derive(Debug, Clone, PartialEq)]
pub struct PerkEntry {
    /// Rank within the perk that this entry applies at. Multi-rank
    /// perks (e.g. Skyrim's One-handed tree) define one entry per
    /// rank, each with stronger function data.
    pub rank: u8,
    /// Priority order — higher value runs first when multiple entries
    /// on different perks target the same Entry Point. Mod the actor's
    /// perks by priority, then evaluate in descending order. Per
    /// `perk_system.md`.
    pub priority: u8,
    /// The body — Quest / Ability / EntryPoint variant.
    pub body: PerkEntryBody,
    /// Conditions attached to this entry (CTDA sub-records).
    pub conditions: ConditionList,
}

/// `PERK` perk / trait record. Holds the condition list + entry-point
/// tree that drives the `perk_system.md` / `perk_entry_points.md`
/// memos' ~120 catalog. Identity + DATA header + PRKE entries are
/// decoded. Per-entry CTDA conditions (gate whether each entry fires,
/// via `push_ctda`) and the per-`function_type` EPFD semantic decode
/// (#4225: `function_type` 1-5 — None/Float/Range/FormId/LString —
/// are all typed and decoded; an unrecognized value falls back to
/// `PerkFunctionData::None` rather than being dropped) are both
/// implemented, not follow-ups.
#[derive(Debug, Clone, Default)]
pub struct PerkRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// First byte of DATA. On FO3/FNV this is the `trait` flag (0 or
    /// 1; trait perks are non-removable). On Skyrim+ the layout is
    /// the same first byte. Kept for backwards-compat with the prior
    /// stub; new code should prefer [`Self::is_trait`].
    pub perk_flags: u8,
    /// True when the DATA `trait` byte is set — perk is a permanent
    /// trait, can't be removed by the perk pool.
    pub is_trait: bool,
    /// DATA num_ranks (count of multi-rank steps). 1 for most perks;
    /// 3–5 for Skyrim skill-tree perks with progressive ranks. 0 when
    /// the DATA payload is too short to read this field.
    pub num_ranks: u8,
    /// DATA playable flag — true when the perk shows up in the
    /// level-up perk selection UI.
    pub playable: bool,
    /// DATA hidden flag — true for engine-only perks (NPC-only
    /// abilities, debug perks).
    pub hidden: bool,
    /// All `PRKE`/`PRKF` entry blocks in authoring order. Each entry
    /// has its own rank/priority and Quest/Ability/EntryPoint body.
    pub entries: Vec<PerkEntry>,
}

/// Block-state for the PRKE walker — mirrors the QUST INDX/QOBJ
/// pattern. `Open` carries the partially-decoded entry header until
/// either the per-type DATA fills the body OR the closing PRKF
/// flushes whatever's been collected.
enum PerkBlock {
    None,
    Open {
        entry_type: u8,
        rank: u8,
        priority: u8,
        /// Per-type body, populated by the first DATA inside the
        /// block. Stays `None` until the body shows up — a malformed
        /// PRKE/PRKF pair with no DATA in between is dropped silently
        /// at PRKF rather than panicking.
        body: Option<PerkEntryBody>,
        /// Conditions accumulated for this entry (CTDA sub-records).
        conditions: ConditionList,
    },
}

pub fn parse_perk(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> PerkRecord {
    let mut out = PerkRecord {
        form_id,
        ..Default::default()
    };
    let mut block = PerkBlock::None;

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
            // PERK-level DATA: trait + (level OR num_ranks per-game) +
            // playable + hidden + level/trailing. The leading byte is
            // game-shared; trailing bytes are read defensively when
            // present.
            b"DATA" if matches!(block, PerkBlock::None) && !sub.data.is_empty() => {
                let mut r = SubReader::new(&sub.data);
                out.perk_flags = r.u8_or_default();
                out.is_trait = out.perk_flags != 0;
                // FO3/FNV layout: trait + level + num_ranks + playable + hidden.
                // Skyrim layout:   trait + num_ranks + playable + hidden + level.
                // The schemas overlap on `trait` and disagree past that.
                // Without a GameKind dispatch wired through here, capture
                // num_ranks / playable / hidden positionally per FO3/FNV
                // (the more common shape across the catalogues sampled);
                // Skyrim consumers that need the strict layout can rev
                // this to a per-game arm when the per-game ESM dispatch
                // lands. The first-byte `trait` reads correctly either
                // way, which is the load-bearing field.
                let _level = r.u8_or_default();
                out.num_ranks = r.u8_or_default();
                out.playable = r.u8_or_default() != 0;
                out.hidden = r.u8_or_default() != 0;
            }
            // PRKE opens an entry block. Anything still open is
            // dropped silently — a stray PRKE with no closing PRKF on
            // the prior block is content-corruption rather than a
            // parser bug we should panic on.
            b"PRKE" if sub.data.len() >= 3 => {
                let mut r = SubReader::new(&sub.data);
                let entry_type = r.u8_or_default();
                let rank = r.u8_or_default();
                let priority = r.u8_or_default();
                block = PerkBlock::Open {
                    entry_type,
                    rank,
                    priority,
                    body: None,
                    conditions: vec![],
                };
            }
            // Per-entry DATA inside an Open block. Shape depends on
            // entry_type captured at PRKE: 0=Quest, 1=Ability,
            // 2=EntryPoint.
            b"DATA" => {
                if let PerkBlock::Open {
                    entry_type, body, ..
                } = &mut block
                {
                    let mut r = SubReader::new(&sub.data);
                    *body = match *entry_type {
                        // #3715 — quest_form_id / spell_form_id are
                        // embedded FormIDs; both need the same load-order
                        // remap `push_ctda` already applies to this
                        // record's condition list.
                        0 if sub.data.len() >= 5 => {
                            let quest_form_id = remap_fid(r.u32_or_default(), remap);
                            let stage = r.u8_or_default() as u16;
                            Some(PerkEntryBody::Quest {
                                quest_form_id,
                                stage,
                            })
                        }
                        1 if sub.data.len() >= 4 => {
                            let spell_form_id = remap_fid(r.u32_or_default(), remap);
                            Some(PerkEntryBody::Ability { spell_form_id })
                        }
                        2 if sub.data.len() >= 2 => {
                            let entry_point_index = r.u8_or_default();
                            let function_type = r.u8_or_default();
                            Some(PerkEntryBody::EntryPoint {
                                entry_point_index,
                                function_type,
                                function_data: PerkFunctionData::None,
                                formatter: Vec::new(),
                                extra_flags: Vec::new(),
                            })
                        }
                        _ => None,
                    };
                }
            }
            // EPFT may appear inside an EntryPoint body when the
            // PRKE-internal DATA carried only the entry-point index
            // (some versions emit function_type via EPFT instead).
            // Overwrite the body's function_type when present.
            b"EPFT" if !sub.data.is_empty() => {
                if let PerkBlock::Open {
                    body: Some(PerkEntryBody::EntryPoint { function_type, .. }),
                    ..
                } = &mut block
                {
                    *function_type = sub.data[0];
                }
            }
            // EPFD carries the typed function-data bytes. Decode based
            // on function_type, which was set by either the per-type
            // DATA (old authoring) or EPFT (new authoring).
            b"EPFD" => {
                if let PerkBlock::Open {
                    body:
                        Some(PerkEntryBody::EntryPoint {
                            function_type,
                            function_data,
                            ..
                        }),
                    ..
                } = &mut block
                {
                    let d = &sub.data;
                    *function_data = match function_type {
                        1 => PerkFunctionData::None,
                        2 if d.len() >= 4 => {
                            PerkFunctionData::Float(f32::from_le_bytes([d[0], d[1], d[2], d[3]]))
                        }
                        3 if d.len() >= 8 => PerkFunctionData::Range {
                            min: f32::from_le_bytes([d[0], d[1], d[2], d[3]]),
                            max: f32::from_le_bytes([d[4], d[5], d[6], d[7]]),
                        },
                        // #4069 — function_type 4 is a genuine cross-record
                        // FormID and needs the load-order remap. Type 5
                        // below is an lstring *index*, not a FormID, and
                        // must NOT be remapped.
                        4 if d.len() >= 4 => PerkFunctionData::FormId(remap_fid(
                            u32::from_le_bytes([d[0], d[1], d[2], d[3]]),
                            remap,
                        )),
                        5 if d.len() >= 4 => {
                            PerkFunctionData::LString(u32::from_le_bytes([d[0], d[1], d[2], d[3]]))
                        }
                        _ => PerkFunctionData::None,
                    };
                }
            }
            b"EPF2" => {
                if let PerkBlock::Open {
                    body: Some(PerkEntryBody::EntryPoint { formatter, .. }),
                    ..
                } = &mut block
                {
                    *formatter = sub.data.clone();
                }
            }
            b"EPF3" => {
                if let PerkBlock::Open {
                    body: Some(PerkEntryBody::EntryPoint { extra_flags, .. }),
                    ..
                } = &mut block
                {
                    *extra_flags = sub.data.clone();
                }
            }
            b"CTDA" | b"CIS1" | b"CIS2" => {
                if let PerkBlock::Open {
                    ref mut conditions, ..
                } = &mut block
                {
                    push_ctda(sub, remap, conditions);
                }
            }
            // PRKF closes the entry. Push it onto `entries` only if
            // both PRKE and per-type DATA were captured — incomplete
            // blocks are dropped silently rather than emitted with
            // sentinel values.
            b"PRKF" => {
                let prev = std::mem::replace(&mut block, PerkBlock::None);
                if let PerkBlock::Open {
                    rank,
                    priority,
                    body: Some(body),
                    conditions,
                    ..
                } = prev
                {
                    out.entries.push(PerkEntry {
                        rank,
                        priority,
                        body,
                        conditions,
                    });
                }
            }
            _ => {}
        }
    }
    // A trailing PRKE with no closing PRKF is content-corruption —
    // drop silently rather than emit a half-populated entry.
    out
}

/// One magic effect in a spell or enchantment chain.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MagicEffectItem {
    /// Form ID of the MGEF (magic effect) from EFID.
    pub effect_form_id: u32,
    /// Magnitude from EFIT offset 0 (f32).
    pub magnitude: f32,
    /// Area of effect from EFIT offset 4 (u32).
    pub area: u32,
    /// Duration in game seconds from EFIT offset 8 (u32).
    pub duration: u32,
}

/// Accumulator for the EFID → EFIT pair chain shared by `parse_spel` and
/// `parse_ench` (TD2-110 / #2069), which decoded it verbatim twice.
///
/// The wire format is a stream of alternating sub-records, not a single
/// packed array: `EFID` names the MGEF and `EFIT` supplies that effect's
/// magnitude / area / duration. Hence the latch — `EFID` stores the pending
/// form ID and the following `EFIT` consumes it, clearing the latch so a
/// stray second `EFIT` cannot re-bind the same effect. An `EFIT` with no
/// pending `EFID` is dropped rather than pushed with a null effect ID.
#[derive(Debug)]
struct MagicEffectAccumulator {
    game: GameKind,
    pending_efid: u32,
    items: Vec<MagicEffectItem>,
}

impl MagicEffectAccumulator {
    fn for_game(game: GameKind) -> Self {
        Self {
            game,
            pending_efid: 0,
            items: Vec::new(),
        }
    }

    /// `EFIT` per game (xEdit): FO3/FNV 20 bytes with an integer magnitude
    /// @0, then area, duration; Oblivion 24 bytes led by the 4-char effect
    /// code, integer magnitude @4; Skyrim/FO4 12 bytes, float magnitude @0.
    /// Pre-#4415 every game's magnitude was read as an `f32`, so an FO3/FNV
    /// spell magnitude of 5 decoded as ~7e-45. Returns (magnitude, area,
    /// duration).
    fn decode_efit(&self, data: &[u8]) -> Option<(f32, u32, u32)> {
        let read = |offset: usize| {
            data.get(offset..offset + 4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        };
        match self.game {
            GameKind::Fallout3NV => Some((read(0)? as f32, read(4)?, read(8)?)),
            GameKind::Oblivion => Some((read(4)? as f32, read(8)?, read(12)?)),
            _ => Some((f32::from_bits(read(0)?), read(4)?, read(8)?)),
        }
    }

    /// Feed one `EFID` or `EFIT` sub-record. Any other sub-type is ignored,
    /// so callers can delegate a combined `b"EFID" | b"EFIT"` match arm here.
    ///
    /// Short sub-records are skipped without disturbing the latch, matching
    /// the length guards the two hand-rolled copies carried.
    fn feed(&mut self, sub: &SubRecord, remap: &Option<FormIdRemap>) {
        match &sub.sub_type {
            b"EFID" if sub.data.len() >= 4 => {
                // #4071 — EFID is an MGEF cross-reference. Remapped at the
                // latch so every caller of this accumulator inherits it
                // rather than each re-deriving the rule.
                self.pending_efid = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
            }
            b"EFIT" if sub.data.len() >= 12 && self.pending_efid != 0 => {
                let Some((magnitude, area, duration)) = self.decode_efit(&sub.data) else {
                    return;
                };
                self.items.push(MagicEffectItem {
                    effect_form_id: self.pending_efid,
                    magnitude,
                    area,
                    duration,
                });
                self.pending_efid = 0;
            }
            _ => {}
        }
    }
}

/// `SPEL` spell / ability / power record. FO3/FNV also covers passive
/// abilities and radiation-poisoning style auto-cast effects. SPIT
/// carries cost + level requirement + flags; effect list (EFID/EFIT)
/// is decoded as MagicEffectItem chains.
#[derive(Debug, Clone, Default)]
pub struct SpelRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// #4415 — the spell's type (ability, power, disease, …); see
    /// `decode_spit` for the per-game offsets.
    pub spell_type: SpellType,
    /// `SPIT` flags — the low byte on Oblivion/FO3/FNV (bit 0 Manual Cost,
    /// bit 2 PC Start Spell, bit 7 Touch Explodes), a u32 on Skyrim/FO4.
    pub spell_flags: u32,
    /// `SPIT` cost (@4 on Oblivion/FO3/FNV, where it is unused; @0 on
    /// Skyrim/FO4).
    pub cost: u32,
    /// Magic effects applied by this spell. Built from EFID/EFIT pairs.
    pub effects: Vec<MagicEffectItem>,
}

pub fn parse_spel(
    form_id: u32,
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> SpelRecord {
    let mut out = SpelRecord {
        form_id,
        ..Default::default()
    };
    let mut effects = MagicEffectAccumulator::for_game(game);
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    out.editor_id = common.editor_id;
    out.full_name = common.full_name;
    for sub in subs {
        match &sub.sub_type {
            b"SPIT" => {
                if let Some((spell_type, cost, flags)) = decode_spit(&sub.data, game) {
                    out.spell_type = spell_type;
                    out.cost = cost;
                    out.spell_flags = flags;
                }
            }
            b"EFID" | b"EFIT" => effects.feed(sub, remap),
            _ => {}
        }
    }
    out.effects = effects.items;
    out
}

/// `MGEF` magic effect record. Universal bridge for Actor Value
/// modifications — every perk entry point, spell effect, and
/// ingredient effect routes through here. Full DATA structure
/// decoded to enable effect application across games.
#[derive(Debug, Clone, PartialEq)]
pub struct MgefRecord {
    /// Game-local vital AV index eligible for immediate restoration, not a FormID.
    pub instant_restoration_av: Option<u32>,
    /// Same vital mapping, additionally safe for a constant-rate duration.
    pub timed_restoration_av: Option<u32>,
    /// FO3/FNV Value and Parts also restores the body-condition actor values.
    pub restores_body_parts: bool,
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// Flags from DATA offset 0 (hostile / recover / detrimental / ...).
    pub effect_flags: u32,
    /// Base magicka cost from DATA offset 4 (f32).
    pub base_cost: f32,
    /// Associated item (e.g. ingredient for poisoning) from DATA offset 8 (u32).
    /// 0xFFFF_FFFF means none.
    pub associated_item: u32,
    /// Magic school category from DATA offset 12 (i32).
    /// -1 means none.
    pub magic_school: i32,
    /// Actor Value resistance/counter from DATA offset 16 (i32).
    /// -1 means none.
    pub resistance_av: i32,
    /// Light effect form ID from DATA offset 24 (u32).
    pub light_form_id: u32,
    /// Projectile speed from DATA offset 28 (f32).
    pub projectile_speed: f32,
    /// Effect shader form ID from DATA offset 32 (u32).
    pub effect_shader_id: u32,
    /// #4415 — the effect's archetype, canonical across games. `None`
    /// where the record has none (Oblivion, which keys effects by 4-char
    /// code) or its `DATA` is not a known layout.
    pub archetype: Option<MagicArchetype>,
    /// #4415 — the actor value the archetype acts on (`DATA` @68).
    pub primary_actor_value: Option<ActorValueRef>,
    /// #4415 — Skyrim/FO4 Dual Value Modifier's second actor value (@88),
    /// scaled by [`Self::second_actor_value_weight`] (@60).
    pub secondary_actor_value: Option<ActorValueRef>,
    pub second_actor_value_weight: f32,
}

/// #4415 — a magic effect's archetype, the canonical subset the runtime
/// applies (xEdit `wbDefinitions{FO3,FNV,TES5,FO4}.pas` MGEF DATA
/// archetype enum; FO3/FNV have only Value Modifier of these four).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MagicArchetype {
    /// Modifies one actor value by the effect magnitude.
    ValueModifier,
    /// Skyrim/FO4 — the same, holding the value at its peak.
    PeakValueModifier,
    /// Skyrim/FO4 — primary actor value, plus the secondary one scaled by
    /// the second-AV weight.
    DualValueModifier,
    /// Skyrim/FO4 — damages the target's value and restores the caster's.
    Absorb,
    /// Any other archetype, by its raw per-game value.
    Other(u32),
}

/// #4415 — how a magic effect names an actor value, per game: a game-local
/// actor-value index (FO3/FNV/Skyrim) or an AVIF FormID (FO4+). Resolved to
/// the canonical AVIF FormID by `EsmIndex::resolve_actor_value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorValueRef {
    Index(u32),
    Form(u32),
}

/// `MGEF` flag bits shared by every game with an archetype (xEdit; same
/// positions on Oblivion too).
///
/// "Recover": the change is undone when the effect ends (a buff/debuff on
/// the value's maximum) rather than applied as ongoing damage/restoration.
pub const MGEF_FLAG_RECOVER: u32 = 0x2;
/// "Detrimental": the magnitude is applied as a negative change.
pub const MGEF_FLAG_DETRIMENTAL: u32 = 0x4;

impl Default for MgefRecord {
    fn default() -> Self {
        Self {
            instant_restoration_av: None,
            timed_restoration_av: None,
            restores_body_parts: false,
            form_id: 0,
            editor_id: String::new(),
            full_name: String::new(),
            description: String::new(),
            effect_flags: 0,
            base_cost: 0.0,
            associated_item: 0xFFFF_FFFF,
            magic_school: -1,
            resistance_av: -1,
            light_form_id: 0,
            projectile_speed: 0.0,
            effect_shader_id: 0,
            archetype: None,
            primary_actor_value: None,
            secondary_actor_value: None,
            second_actor_value_weight: 0.0,
        }
    }
}

/// #4415 — decode the archetype and actor value(s) from `DATA` (xEdit
/// MGEF DATA; our offsets for FO3/FNV and TES5 were confirmed against it):
/// FO3/FNV 72 bytes, archetype u32 @64, actor value index s32 @68;
/// Skyrim/FO4/FO76 152 bytes, archetype @64, primary AV @68, second-AV
/// weight f32 @60, second AV @88 — an index on Skyrim, an AVIF FormID on
/// FO4+ (remapped). `-1` / NULL means none.
fn decode_mgef_archetype(
    out: &mut MgefRecord,
    data: &[u8],
    game: crate::esm::reader::GameKind,
    remap: &Option<FormIdRemap>,
) {
    use crate::esm::reader::GameKind;
    let read = |offset: usize| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    let index_ref = |raw: u32| (raw as i32 >= 0).then_some(ActorValueRef::Index(raw));
    let form_ref = |raw: u32| (raw != 0).then(|| ActorValueRef::Form(remap_fid(raw, remap)));
    match game {
        GameKind::Fallout3NV if data.len() >= 72 => {
            out.archetype = Some(match read(64) {
                0 => MagicArchetype::ValueModifier,
                other => MagicArchetype::Other(other),
            });
            out.primary_actor_value = index_ref(read(68));
        }
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 if data.len() >= 152 => {
            out.archetype = Some(match read(64) {
                0 => MagicArchetype::ValueModifier,
                4 => MagicArchetype::Absorb,
                5 => MagicArchetype::DualValueModifier,
                34 => MagicArchetype::PeakValueModifier,
                other => MagicArchetype::Other(other),
            });
            let skyrim = game == GameKind::Skyrim;
            let reference = |raw| {
                if skyrim {
                    index_ref(raw)
                } else {
                    form_ref(raw)
                }
            };
            out.primary_actor_value = reference(read(68));
            out.secondary_actor_value = reference(read(88));
            out.second_actor_value_weight = f32::from_bits(read(60));
        }
        _ => {}
    }
}

pub fn parse_mgef_for_game(
    form_id: u32,
    subs: &[SubRecord],
    game: crate::esm::reader::GameKind,
    remap: &Option<FormIdRemap>,
) -> MgefRecord {
    let mut out = parse_mgef(form_id, subs, remap);
    use crate::esm::reader::GameKind;
    if let Some(data) = subs.iter().rev().find(|s| &s.sub_type == b"DATA") {
        decode_mgef_archetype(&mut out, &data.data, game, remap);
    }
    if !matches!(game, GameKind::Skyrim | GameKind::Fallout3NV)
        || subs
            .iter()
            .any(|s| matches!(&s.sub_type, b"VMAD" | b"CTDA" | b"ESCE"))
    {
        return out;
    }
    if let Some(data) = subs
        .iter()
        .rev()
        .find(|s| &s.sub_type == b"DATA")
        .map(|s| &s.data)
    {
        // FO3/FNV share archetype/AV offsets with TES5 but have a 72-byte
        // DATA payload and different flags. Skill/attribute-scaled effects
        // require runtime magnitude evaluation, not a constant restoration.
        if game == GameKind::Fallout3NV {
            if data.len() == 72 {
                let read =
                    |offset| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
                let av = read(68);
                if read(0) & (1 | 2 | 4 | 0x100 | 0x80000 | 0x100000) == 0
                    && read(0) & 0x10 != 0
                    && (read(64) == 0 || (read(64) == 34 && av == 16))
                    && read(20) & 0xffff == 0
                    && matches!(av, 12 | 16)
                {
                    out.instant_restoration_av = Some(av);
                    out.restores_body_parts = read(64) == 34;
                    // FO3/FNV No Duration is bit 7 (TES5 uses bit 9).
                    // No Death Dispel needs a different lifecycle policy.
                    if read(0) & (0x80 | 0x10000000) == 0 {
                        out.timed_restoration_av = Some(av);
                    }
                }
            }
            return out;
        }
        // TES5 xEdit MGEF DATA: archetype@64, primary AV@68,
        // casting type@80, delivery@84. Exclude hostile, recover,
        // detrimental and no-magnitude flags (temporary fortify isn't heal).
        if data.len() >= 152 {
            let read = |offset| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
            let av = read(68);
            if read(0) & (1 | 2 | 4 | 0x400) == 0
                && read(64) == 0
                && read(80) == 1
                && read(84) == 0
                && read(20) & 0xffff == 0
                // Linked abilities, image-space effects, and perks require
                // additional runtime consumers; don't silently omit them.
                && read(128) == 0
                && read(132) == 0
                && read(136) == 0
                && (24..=26).contains(&av)
            {
                out.instant_restoration_av = Some(av);
                // TES5 taper duration is DATA+56. No-recast, keyword dispel,
                // and no-death-dispel require lifecycle policies not yet
                // implemented by the ordinary restorative tick.
                if read(0) & (0x200 | 0x20000 | 0x100 | 0x10000000) == 0
                    && f32::from_bits(read(56)) == 0.0
                {
                    out.timed_restoration_av = Some(av);
                }
            }
        }
    }
    out
}

pub fn parse_mgef(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> MgefRecord {
    let mut out = MgefRecord {
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
            b"DATA" => {
                if let Ok(header) = read_sub::<MagicEffectHeader>(sub) {
                    out.effect_flags = header.effect_flags;
                    out.base_cost = header.base_cost;
                    // #4070 — DATA @8. An ITEM/WEAP cross-reference, the
                    // same kind of embedded FormID as light_form_id @24
                    // below; #3715 remapped only that one.
                    // #4172 — the documented "no item" sentinel
                    // (0xFFFFFFFF) must bypass the remap: its mod_index
                    // byte is 255, which on any multi-master load falls
                    // into FormIdRemap::remap's suspicious-out-of-range
                    // warn arm, logging one false warning per no-item MGEF
                    // (common, well-documented authored data) and drowning
                    // the genuinely-malformed case that branch exists to
                    // catch. The value round-trips either way; the guard
                    // keeps the log honest.
                    out.associated_item = if header.associated_item == 0xFFFF_FFFF {
                        0xFFFF_FFFF
                    } else {
                        remap_fid(header.associated_item, remap)
                    };
                    out.magic_school = header.magic_school;
                    out.resistance_av = header.resistance_av;
                    // #3715 — embedded light-effect FormID.
                    out.light_form_id = remap_fid(header.light_form_id, remap);
                    out.projectile_speed = header.projectile_speed;
                    // #4070 — DATA @32, an EFSH cross-reference.
                    out.effect_shader_id = remap_fid(header.effect_shader_id, remap);
                }
            }
            _ => {}
        }
    }
    out
}

/// `ENCH` enchantment record (Oblivion / FO3 / FNV / Skyrim). Carries
/// the effect chain a `WEAP.eitm` / `AMMO.eitm` / `ARMO.eitm` reference
/// resolves to: Pulse Gun's "Pulse" enchantment, This Machine's charge
/// effect, Holorifle's energy splash, and the entire vanilla-Skyrim
/// weapon-enchantment table. ENIT carries type/charge/cost/flags;
/// EFID/EFIT effect blocks carry the effect chain decoded as
/// MagicEffectItem sequences. See #629 / FNV-D2-01.
#[derive(Debug, Clone, Default)]
pub struct EnchRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// ENIT offset 0 (u32). Spell-school target: `0` = spell, `1` =
    /// staff, `2` = weapon, `3` = apparel. Vanilla weapon-EITM refs
    /// resolve to `2` exclusively.
    pub enchantment_type: u32,
    /// ENIT offset 4 (u32). Magicka / charge pool — interpreted per
    /// `enchantment_type`. Weapon enchantments use this as the per-hit
    /// charge cost.
    pub charge_amount: u32,
    /// ENIT offset 8 (u32). Pre-calculated enchant cost (auto-generated
    /// at compile time from the EFIT chain). Used by the auto-calc UI
    /// in the editor; runtime consumers re-derive from the effect chain
    /// if `flags & NoAutoCalculate` is set.
    pub enchant_cost: u32,
    /// ENIT offset 12 (u32). Bit 0 = `NoAutoCalculate` (manual
    /// override of `enchant_cost`); other bits unused on FO3/FNV.
    pub enchant_flags: u32,
    /// Magic effects applied by this enchantment. Built from EFID/EFIT pairs.
    pub effects: Vec<MagicEffectItem>,
}

pub fn parse_ench(
    form_id: u32,
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> EnchRecord {
    let mut out = EnchRecord {
        form_id,
        ..Default::default()
    };
    let mut effects = MagicEffectAccumulator::for_game(game);
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    out.editor_id = common.editor_id;
    out.full_name = common.full_name;
    for sub in subs {
        match &sub.sub_type {
            // ENIT decoded via schema decoder (Phase C).
            b"ENIT" if sub.data.len() >= 16 => {
                if let Ok(header) = read_sub::<EnchantmentHeader>(sub) {
                    out.enchantment_type = header.enchantment_type;
                    out.charge_amount = header.charge_amount;
                    out.enchant_cost = header.enchant_cost;
                    out.enchant_flags = header.enchant_flags;
                }
            }
            b"EFID" | b"EFIT" => effects.feed(sub, remap),
            _ => {}
        }
    }
    out.effects = effects.items;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::sub;

    #[test]
    fn parse_perk_picks_data_flags() {
        let subs = vec![
            sub(b"EDID", b"IntenseTraining\0"),
            sub(b"FULL", b"Intense Training\0"),
            sub(b"DESC", b"Increase any one S.P.E.C.I.A.L. by 1.\0"),
            sub(b"DATA", &[0x01]), // playable
        ];
        let p = parse_perk(0xE5E5, &subs, &None);
        assert_eq!(p.editor_id, "IntenseTraining");
        assert_eq!(p.perk_flags, 0x01);
        assert!(p.is_trait, "byte 0 = 0x01 → trait set");
    }

    #[test]
    fn parse_perk_decodes_full_data_header() {
        // Full 5-byte DATA header (FO3/FNV layout: trait + level +
        // num_ranks + playable + hidden).
        let subs = vec![
            sub(b"EDID", b"Bloody Mess\0"),
            sub(b"DATA", &[0x00, 6, 1, 0x01, 0x00]),
        ];
        let p = parse_perk(0xBADAu32, &subs, &None);
        assert!(!p.is_trait);
        assert_eq!(p.num_ranks, 1);
        assert!(p.playable);
        assert!(!p.hidden);
    }

    #[test]
    fn parse_perk_decodes_quest_entry() {
        // Quest entry: type=0, rank=1, priority=10, quest_form_id=0x000FED11,
        // stage=20. Closes with PRKF.
        let mut data = Vec::new();
        data.extend_from_slice(&0x000F_ED11u32.to_le_bytes()); // quest_form_id
        data.extend_from_slice(&[20u8, 0, 0, 0]); // stage + 3 bytes pad
        let subs = vec![
            sub(b"EDID", b"PerkQuestEntry\0"),
            sub(b"DATA", &[0x00, 0, 0, 0x01, 0x00]),
            sub(b"PRKE", &[0u8, 1, 10]), // type=Quest, rank, priority
            sub(b"DATA", &data),
            sub(b"PRKF", &[]),
        ];
        let p = parse_perk(0xAAAAu32, &subs, &None);
        assert_eq!(p.entries.len(), 1);
        let entry = &p.entries[0];
        assert_eq!(entry.rank, 1);
        assert_eq!(entry.priority, 10);
        match &entry.body {
            PerkEntryBody::Quest {
                quest_form_id,
                stage,
            } => {
                assert_eq!(*quest_form_id, 0x000F_ED11);
                assert_eq!(*stage, 20);
            }
            other => panic!("expected Quest, got {other:?}"),
        }
    }

    #[test]
    fn parse_perk_decodes_ability_entry() {
        // Ability entry: type=1, single u32 spell ref.
        let subs = vec![
            sub(b"EDID", b"PowerAttack\0"),
            sub(b"PRKE", &[1u8, 0, 5]),
            sub(b"DATA", &0x000A_BC01u32.to_le_bytes()),
            sub(b"PRKF", &[]),
        ];
        let p = parse_perk(0xBBBBu32, &subs, &None);
        assert_eq!(p.entries.len(), 1);
        match &p.entries[0].body {
            PerkEntryBody::Ability { spell_form_id } => {
                assert_eq!(*spell_form_id, 0x000A_BC01);
            }
            other => panic!("expected Ability, got {other:?}"),
        }
    }

    /// #3715 — `parse_perk` already took `remap` (applied it to `push_ctda`
    /// condition lists) but never applied it to `quest_form_id` /
    /// `spell_form_id`.
    #[test]
    fn parse_perk_quest_and_ability_entries_are_remapped() {
        let remap = Some(FormIdRemap::regular(2, vec![0]));

        let mut quest_data = Vec::new();
        quest_data.extend_from_slice(&0x0100_7777u32.to_le_bytes()); // self-ref
        quest_data.extend_from_slice(&[20u8, 0, 0, 0]);
        let quest_subs = vec![
            sub(b"PRKE", &[0u8, 1, 10]),
            sub(b"DATA", &quest_data),
            sub(b"PRKF", &[]),
        ];
        let quest = parse_perk(0xAAAA, &quest_subs, &remap);
        match &quest.entries[0].body {
            PerkEntryBody::Quest { quest_form_id, .. } => {
                assert_eq!(*quest_form_id, 0x0200_7777);
            }
            other => panic!("expected Quest, got {other:?}"),
        }

        let ability_subs = vec![
            sub(b"PRKE", &[1u8, 0, 5]),
            sub(b"DATA", &0x0100_8888u32.to_le_bytes()), // self-ref
            sub(b"PRKF", &[]),
        ];
        let ability = parse_perk(0xBBBB, &ability_subs, &remap);
        match &ability.entries[0].body {
            PerkEntryBody::Ability { spell_form_id } => {
                assert_eq!(*spell_form_id, 0x0200_8888);
            }
            other => panic!("expected Ability, got {other:?}"),
        }
    }

    #[test]
    fn parse_perk_decodes_entry_point_with_epft_epfd() {
        // EntryPoint entry: type=2. PRKE-internal DATA carries the
        // entry_point_index byte + function_type byte; EPFT can rewrite
        // function_type (some game versions emit it via EPFT instead);
        // EPFD carries typed function-data (f32 for function_type=2).
        let epfd = 1.5f32.to_le_bytes();
        let subs = vec![
            sub(b"EDID", b"ModAttackDamage\0"),
            sub(b"PRKE", &[2u8, 0, 99]),
            sub(b"DATA", &[0x07, 0x01, 0x00, 0x00]), // entry_point=7 (Mod Attack Dmg), function=1 (Add)
            sub(b"EPFT", &[0x02]),                   // function overwrite to 2 (Multiply = Float)
            sub(b"EPFD", &epfd),
            sub(b"PRKF", &[]),
        ];
        let p = parse_perk(0xCCCCu32, &subs, &None);
        assert_eq!(p.entries.len(), 1);
        match &p.entries[0].body {
            PerkEntryBody::EntryPoint {
                entry_point_index,
                function_type,
                function_data,
                ..
            } => {
                assert_eq!(*entry_point_index, 0x07);
                assert_eq!(*function_type, 0x02, "EPFT overrode DATA's function_type");
                assert_eq!(*function_data, PerkFunctionData::Float(1.5));
            }
            other => panic!("expected EntryPoint, got {other:?}"),
        }
    }

    #[test]
    fn parse_perk_multi_entry_authoring_order_preserved() {
        // Three entries — one of each type — emitted in PRKE order.
        let subs = vec![
            sub(b"PRKE", &[0u8, 1, 1]),
            sub(b"DATA", &[1u8, 0, 0, 0, 5, 0, 0, 0]), // Quest: quest=1, stage=5
            sub(b"PRKF", &[]),
            sub(b"PRKE", &[1u8, 1, 2]),
            sub(b"DATA", &0x0000_BEEFu32.to_le_bytes()),
            sub(b"PRKF", &[]),
            sub(b"PRKE", &[2u8, 1, 3]),
            sub(b"DATA", &[0x10, 0x00, 0, 0]),
            sub(b"PRKF", &[]),
        ];
        let p = parse_perk(0xDDDDu32, &subs, &None);
        assert_eq!(p.entries.len(), 3);
        assert_eq!(p.entries[0].priority, 1);
        assert_eq!(p.entries[1].priority, 2);
        assert_eq!(p.entries[2].priority, 3);
        assert!(matches!(p.entries[0].body, PerkEntryBody::Quest { .. }));
        assert!(matches!(p.entries[1].body, PerkEntryBody::Ability { .. }));
        assert!(matches!(
            p.entries[2].body,
            PerkEntryBody::EntryPoint { .. }
        ));
    }

    #[test]
    fn parse_perk_unclosed_block_dropped_silently() {
        // PRKE with no closing PRKF — entry never lands. Defensive
        // against content corruption.
        let subs = vec![
            sub(b"PRKE", &[1u8, 1, 1]),
            sub(b"DATA", &0x0000_BEEFu32.to_le_bytes()),
            // No PRKF, no PRKE-after either.
        ];
        let p = parse_perk(0xEEEEu32, &subs, &None);
        assert!(p.entries.is_empty());
    }

    /// #4415 — `SPIT` per game: FO3/FNV (and Oblivion) lead with the spell
    /// type, cost @4, flags in the low byte @12; Skyrim/FO4 lead with cost,
    /// flags @4, type @8. The pre-#4415 decoder read cost @0 / flags @12
    /// everywhere.
    #[test]
    fn parse_spel_decodes_spit_per_game() {
        let words = |w: [u32; 4]| w.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>();
        let fo3 = vec![sub(b"SPIT", &words([4, 42, 0, 0xCDCD_CD04]))];
        let s = parse_spel(0xF6F6, &fo3, GameKind::Fallout3NV, &None);
        assert_eq!(
            (s.spell_type, s.cost, s.spell_flags),
            (SpellType::Ability, 42, 0x04)
        );

        let mut skyrim = words([60, 0x20000, 0, 0]);
        skyrim[12..16].copy_from_slice(&0.5f32.to_le_bytes()); // charge time
        skyrim.extend_from_slice(&[0u8; 20]);
        let subs = vec![sub(b"EDID", b"Flames\0"), sub(b"SPIT", &skyrim)];
        let s = parse_spel(0xF6F7, &subs, GameKind::Skyrim, &None);
        assert_eq!(
            (s.spell_type, s.cost, s.spell_flags),
            (SpellType::Spell, 60, 0x20000)
        );

        let oblivion = vec![sub(b"SPIT", &words([0xCDCD_CD02, 5, 0, 0x81]))];
        let s = parse_spel(0xF6F8, &oblivion, GameKind::Oblivion, &None);
        assert_eq!((s.spell_type, s.spell_flags), (SpellType::Power, 0x81));
    }

    /// #4415 — FO3/FNV `EFIT` carries an integer magnitude; reading it as a
    /// float turned 25 into ~3.5e-44.
    #[test]
    fn fo3_efit_magnitude_is_an_integer() {
        let mut efit = Vec::new();
        for value in [25u32, 0, 30, 0, 16] {
            efit.extend_from_slice(&value.to_le_bytes());
        }
        let subs = vec![sub(b"EFID", &0xAAAAu32.to_le_bytes()), sub(b"EFIT", &efit)];
        let s = parse_spel(0x1, &subs, GameKind::Fallout3NV, &None);
        assert_eq!(s.effects[0].magnitude, 25.0);
        assert_eq!(s.effects[0].duration, 30);
    }

    #[test]
    fn parse_ench_picks_enit_scalars() {
        // Synthesize ENIT for FNV's Pulse Gun-style weapon enchant:
        //   type    = 2 (weapon)
        //   charge  = 25 (per-hit charge cost)
        //   cost    = 100 (auto-calc cost)
        //   flags   = 0x01 (NoAutoCalculate)
        let mut enit = Vec::new();
        enit.extend_from_slice(&2u32.to_le_bytes());
        enit.extend_from_slice(&25u32.to_le_bytes());
        enit.extend_from_slice(&100u32.to_le_bytes());
        enit.extend_from_slice(&0x0000_0001u32.to_le_bytes());
        let subs = vec![
            sub(b"EDID", b"PulseEnchant\0"),
            sub(b"FULL", b"Pulse\0"),
            sub(b"ENIT", &enit),
        ];
        let e = parse_ench(0x000E_5C77, &subs, GameKind::Skyrim, &None);
        assert_eq!(e.editor_id, "PulseEnchant");
        assert_eq!(e.full_name, "Pulse");
        assert_eq!(e.enchantment_type, 2);
        assert_eq!(e.charge_amount, 25);
        assert_eq!(e.enchant_cost, 100);
        assert_eq!(e.enchant_flags, 0x01);
    }

    #[test]
    fn parse_ench_tolerates_skyrim_20_byte_enit() {
        // Skyrim appended a `cast_type` u32 to ENIT (20 bytes total).
        // The shared 16-byte prefix must still decode safely; the
        // trailing field is ignored. #629 / FNV-D2-01 must not regress
        // future Skyrim parses that route through the same arm.
        let mut enit = Vec::new();
        enit.extend_from_slice(&2u32.to_le_bytes());
        enit.extend_from_slice(&50u32.to_le_bytes());
        enit.extend_from_slice(&200u32.to_le_bytes());
        enit.extend_from_slice(&0x0000_0000u32.to_le_bytes());
        enit.extend_from_slice(&3u32.to_le_bytes()); // Skyrim cast_type
        assert_eq!(enit.len(), 20);
        let subs = vec![sub(b"EDID", b"FireDmg\0"), sub(b"ENIT", &enit)];
        let e = parse_ench(0x0001_F25D, &subs, GameKind::Skyrim, &None);
        assert_eq!(e.charge_amount, 50);
        assert_eq!(e.enchant_cost, 200);
    }

    #[test]
    fn parse_ench_short_enit_keeps_defaults() {
        // Author-malformed ENIT (< 16 bytes) must not panic and must
        // leave scalars at their defaults so the surrounding records
        // still load.
        let subs = vec![sub(b"EDID", b"BrokenEnchant\0"), sub(b"ENIT", &[0u8; 8])];
        let e = parse_ench(0xDEAD_BEEF, &subs, GameKind::Skyrim, &None);
        assert_eq!(e.editor_id, "BrokenEnchant");
        assert_eq!(e.enchantment_type, 0);
        assert_eq!(e.charge_amount, 0);
        assert_eq!(e.enchant_cost, 0);
        assert_eq!(e.enchant_flags, 0);
    }

    #[test]
    fn parse_mgef_rejects_short_data_buffer() {
        // Author-malformed DATA (only 4 bytes instead of 36) is rejected by
        // the strict schema decoder. parse_mgef falls back to all defaults.
        let subs = vec![
            sub(b"EDID", b"RadiationPoisoning\0"),
            sub(b"FULL", b"Radiation Poisoning\0"),
            sub(b"DESC", b"Contaminated by radiation.\0"),
            sub(b"DATA", &0x0000_0009u32.to_le_bytes()),
        ];
        let e = parse_mgef(0xA7A7, &subs, &None);
        assert_eq!(e.effect_flags, 0, "short DATA rejected, defaults apply");
        assert_eq!(e.base_cost, 0.0);
        assert_eq!(e.magic_school, -1);
    }

    #[test]
    fn parse_perk_entry_ctda_stored() {
        let mut ctda = Vec::new();
        ctda.push(0x00u8); // type_byte (offset 0)
        ctda.extend_from_slice(&[0u8; 3]); // pad (offsets 1-3)
        ctda.extend_from_slice(&1.0f32.to_le_bytes()); // comparand (offsets 4-7)
        ctda.extend_from_slice(&5u32.to_le_bytes()); // function_index (offsets 8-11, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // param_1 (offsets 12-15, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // param_2 (offsets 16-19, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // run_on (offsets 20-23, u32)
        ctda.extend_from_slice(&0u32.to_le_bytes()); // ref_fid (offsets 24-27, u32)

        let mut data = Vec::new();
        data.extend_from_slice(&0x1234u32.to_le_bytes()); // quest_form_id
        data.push(0u8); // stage
        let subs = vec![
            sub(b"EDID", b"TestPerk\0"),
            sub(b"DATA", &[0x00, 0, 1, 0, 0]),
            sub(b"PRKE", &[0u8, 1, 50]), // type=Quest
            sub(b"DATA", &data),         // quest_form_id=0x1234, stage=0
            sub(b"CTDA", &ctda),
            sub(b"PRKF", &[]),
        ];
        let p = parse_perk(0xFFFF, &subs, &None);
        assert_eq!(p.entries.len(), 1);
        assert_eq!(p.entries[0].conditions.len(), 1);
        assert_eq!(p.entries[0].conditions[0].function_index, 5);
    }

    #[test]
    fn parse_perk_epfd_float() {
        let epfd = 2.5f32.to_le_bytes();
        let subs = vec![
            sub(b"PRKE", &[2u8, 0, 1]),
            sub(b"DATA", &[42u8, 2, 0, 0]), // entry_point=42, function=2 (Float)
            sub(b"EPFD", &epfd),
            sub(b"PRKF", &[]),
        ];
        let p = parse_perk(0x1111, &subs, &None);
        assert_eq!(p.entries.len(), 1);
        match &p.entries[0].body {
            PerkEntryBody::EntryPoint { function_data, .. } => {
                assert_eq!(*function_data, PerkFunctionData::Float(2.5));
            }
            _ => panic!("expected EntryPoint"),
        }
    }

    #[test]
    fn parse_perk_epfd_range() {
        let mut epfd = Vec::new();
        epfd.extend_from_slice(&1.0f32.to_le_bytes());
        epfd.extend_from_slice(&5.0f32.to_le_bytes());
        let subs = vec![
            sub(b"PRKE", &[2u8, 0, 1]),
            sub(b"DATA", &[43u8, 3, 0, 0]), // entry_point=43, function=3 (Range)
            sub(b"EPFD", &epfd),
            sub(b"PRKF", &[]),
        ];
        let p = parse_perk(0x2222, &subs, &None);
        assert_eq!(p.entries.len(), 1);
        match &p.entries[0].body {
            PerkEntryBody::EntryPoint { function_data, .. } => {
                assert_eq!(
                    *function_data,
                    PerkFunctionData::Range { min: 1.0, max: 5.0 }
                );
            }
            _ => panic!("expected EntryPoint"),
        }
    }

    #[test]
    fn parse_perk_epfd_form_id() {
        let epfd = 0xBEEF_1234u32.to_le_bytes();
        let subs = vec![
            sub(b"PRKE", &[2u8, 0, 1]),
            sub(b"DATA", &[44u8, 4, 0, 0]), // entry_point=44, function=4 (FormId)
            sub(b"EPFD", &epfd),
            sub(b"PRKF", &[]),
        ];
        let p = parse_perk(0x3333, &subs, &None);
        assert_eq!(p.entries.len(), 1);
        match &p.entries[0].body {
            PerkEntryBody::EntryPoint { function_data, .. } => {
                assert_eq!(*function_data, PerkFunctionData::FormId(0xBEEF_1234));
            }
            _ => panic!("expected EntryPoint"),
        }
    }

    /// #4131 — the prior EPFD FormId test only ever exercised `&None`
    /// (identity) remap, so a future regression that un-wrapped this arm's
    /// `remap_fid` call would compile clean and pass every existing test.
    /// Pin the non-identity case the way #4069's sibling fixes do elsewhere.
    #[test]
    fn parse_perk_epfd_form_id_is_remapped() {
        // mod_index 1 == this plugin's own slot (self-reference).
        let epfd = 0x0100_0ABCu32.to_le_bytes();
        let subs = vec![
            sub(b"PRKE", &[2u8, 0, 1]),
            sub(b"DATA", &[44u8, 4, 0, 0]), // entry_point=44, function=4 (FormId)
            sub(b"EPFD", &epfd),
            sub(b"PRKF", &[]),
        ];
        let remap = FormIdRemap::regular(2, vec![0]);
        let p = parse_perk(0x0200_0001, &subs, &Some(remap));
        assert_eq!(p.entries.len(), 1);
        match &p.entries[0].body {
            PerkEntryBody::EntryPoint { function_data, .. } => {
                assert_eq!(
                    *function_data,
                    PerkFunctionData::FormId(0x0200_0ABC),
                    "a self-referencing EPFD FormId must land on the plugin's \
                     own global slot, not keep its plugin-local mod index"
                );
            }
            _ => panic!("expected EntryPoint"),
        }
    }

    #[test]
    fn parse_perk_epfd_lstring() {
        let epfd = 0x0042u32.to_le_bytes();
        let subs = vec![
            sub(b"PRKE", &[2u8, 0, 1]),
            sub(b"DATA", &[45u8, 5, 0, 0]), // entry_point=45, function=5 (LString)
            sub(b"EPFD", &epfd),
            sub(b"PRKF", &[]),
        ];
        let p = parse_perk(0x4444, &subs, &None);
        assert_eq!(p.entries.len(), 1);
        match &p.entries[0].body {
            PerkEntryBody::EntryPoint { function_data, .. } => {
                assert_eq!(*function_data, PerkFunctionData::LString(0x0042));
            }
            _ => panic!("expected EntryPoint"),
        }
    }

    #[test]
    fn parse_mgef_full_data_fnv_layout() {
        let mut data = Vec::new();
        data.extend_from_slice(&0x0000_0001u32.to_le_bytes()); // effect_flags
        data.extend_from_slice(&10.0f32.to_le_bytes()); // base_cost
        data.extend_from_slice(&0x0001_ABCD_u32.to_le_bytes()); // associated_item
        data.extend_from_slice(&2i32.to_le_bytes()); // magic_school
        data.extend_from_slice(&5i32.to_le_bytes()); // resistance_av
        data.extend_from_slice(&0u16.to_le_bytes()); // counter_count
        data.extend_from_slice(&0u16.to_le_bytes()); // pad
        data.extend_from_slice(&0x1111u32.to_le_bytes()); // light_form_id
        data.extend_from_slice(&3000.0f32.to_le_bytes()); // projectile_speed
        data.extend_from_slice(&0x2222u32.to_le_bytes()); // effect_shader_id
        assert_eq!(data.len(), 36);

        let subs = vec![
            sub(b"EDID", b"TestEffect\0"),
            sub(b"FULL", b"Test\0"),
            sub(b"DESC", b"Test effect\0"),
            sub(b"DATA", &data),
        ];
        let m = parse_mgef(0x5555, &subs, &None);
        assert_eq!(m.effect_flags, 0x0000_0001);
        assert_eq!(m.base_cost, 10.0);
        assert_eq!(m.associated_item, 0x0001_ABCD);
        assert_eq!(m.magic_school, 2);
        assert_eq!(m.resistance_av, 5);
        assert_eq!(m.light_form_id, 0x1111);
        assert_eq!(m.projectile_speed, 3000.0);
        assert_eq!(m.effect_shader_id, 0x2222);
    }

    /// #3715 — `parse_mgef` never took a remap at all; `light_form_id` is
    /// an embedded reference into a LIGH record.
    #[test]
    fn parse_mgef_light_form_id_is_remapped() {
        let remap = Some(FormIdRemap::regular(2, vec![0]));
        let mut data = Vec::new();
        data.extend_from_slice(&0u32.to_le_bytes()); // effect_flags
        data.extend_from_slice(&0.0f32.to_le_bytes()); // base_cost
        data.extend_from_slice(&0u32.to_le_bytes()); // associated_item
        data.extend_from_slice(&0i32.to_le_bytes()); // magic_school
        data.extend_from_slice(&(-1i32).to_le_bytes()); // resistance_av
        data.extend_from_slice(&0u16.to_le_bytes()); // counter_count
        data.extend_from_slice(&0u16.to_le_bytes()); // pad
        data.extend_from_slice(&0x0100_6666u32.to_le_bytes()); // light_form_id, self-ref
        data.extend_from_slice(&0.0f32.to_le_bytes()); // projectile_speed
        data.extend_from_slice(&0u32.to_le_bytes()); // effect_shader_id
        assert_eq!(data.len(), 36);
        let subs = vec![sub(b"DATA", &data)];
        let m = parse_mgef(0x6000, &subs, &remap);
        assert_eq!(m.light_form_id, 0x0200_6666);
    }

    /// #4131 — `associated_item` and `effect_shader_id` (#4070) were only
    /// ever exercised under `&None` (identity) remap in
    /// `parse_mgef_full_data_fnv_layout`, so a future regression reverting
    /// either call to a raw read would compile clean and pass every
    /// existing test. Pin both the way `parse_mgef_light_form_id_is_remapped`
    /// pins the sibling `light_form_id` field.
    #[test]
    fn parse_mgef_associated_item_and_effect_shader_id_are_remapped() {
        let remap = Some(FormIdRemap::regular(2, vec![0]));
        let mut data = Vec::new();
        data.extend_from_slice(&0u32.to_le_bytes()); // effect_flags
        data.extend_from_slice(&0.0f32.to_le_bytes()); // base_cost
        data.extend_from_slice(&0x0100_1111u32.to_le_bytes()); // associated_item, self-ref
        data.extend_from_slice(&0i32.to_le_bytes()); // magic_school
        data.extend_from_slice(&(-1i32).to_le_bytes()); // resistance_av
        data.extend_from_slice(&0u16.to_le_bytes()); // counter_count
        data.extend_from_slice(&0u16.to_le_bytes()); // pad
        data.extend_from_slice(&0u32.to_le_bytes()); // light_form_id
        data.extend_from_slice(&0.0f32.to_le_bytes()); // projectile_speed
        data.extend_from_slice(&0x0000_2222u32.to_le_bytes()); // effect_shader_id, master-ref
        assert_eq!(data.len(), 36);
        let subs = vec![sub(b"DATA", &data)];
        let m = parse_mgef(0x6001, &subs, &remap);
        assert_eq!(
            m.associated_item, 0x0200_1111,
            "a self-referencing associated_item must land on the plugin's \
             own global slot, not keep its plugin-local mod index"
        );
        assert_eq!(
            m.effect_shader_id, 0x0000_2222,
            "a master-slot effect_shader_id (mod_index 0) already sits at slot 0"
        );
    }

    /// #4172 — the documented "no associated item" sentinel (0xFFFFFFFF)
    /// must bypass the remap: its mod_index byte is 255, which on a
    /// multi-master remap lands in the suspicious-out-of-range warn arm.
    /// The old behavior round-tripped the value correctly but logged one
    /// false warning per no-item MGEF. Pin the sentinel round-trip under
    /// the same non-identity remap the remap pin above uses.
    #[test]
    fn parse_mgef_associated_item_sentinel_bypasses_the_remap() {
        let remap = Some(FormIdRemap::regular(2, vec![0]));
        let mut data = Vec::new();
        data.extend_from_slice(&0u32.to_le_bytes()); // effect_flags
        data.extend_from_slice(&0.0f32.to_le_bytes()); // base_cost
        data.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // associated_item sentinel
        data.extend_from_slice(&0i32.to_le_bytes()); // magic_school
        data.extend_from_slice(&(-1i32).to_le_bytes()); // resistance_av
        data.extend_from_slice(&0u16.to_le_bytes()); // counter_count
        data.extend_from_slice(&0u16.to_le_bytes()); // pad
        data.extend_from_slice(&0u32.to_le_bytes()); // light_form_id
        data.extend_from_slice(&0.0f32.to_le_bytes()); // projectile_speed
        data.extend_from_slice(&0u32.to_le_bytes()); // effect_shader_id
        assert_eq!(data.len(), 36);
        let subs = vec![sub(b"DATA", &data)];
        let m = parse_mgef(0x6002, &subs, &remap);
        assert_eq!(
            m.associated_item, 0xFFFF_FFFF,
            "the no-item sentinel must round-trip verbatim, never enter the \
             remap's out-of-range warn arm"
        );
    }

    #[test]
    fn parse_spel_with_two_effects() {
        let mut spit = Vec::new();
        spit.extend_from_slice(&100u32.to_le_bytes()); // cost
        spit.extend_from_slice(&[0u8; 8]); // pad
        spit.extend_from_slice(&0u32.to_le_bytes()); // flags

        let mut efit1 = Vec::new();
        efit1.extend_from_slice(&5.0f32.to_le_bytes()); // mag
        efit1.extend_from_slice(&0u32.to_le_bytes()); // area
        efit1.extend_from_slice(&3u32.to_le_bytes()); // dur

        let mut efit2 = Vec::new();
        efit2.extend_from_slice(&10.0f32.to_le_bytes()); // mag
        efit2.extend_from_slice(&2u32.to_le_bytes()); // area
        efit2.extend_from_slice(&0u32.to_le_bytes()); // dur

        let subs = vec![
            sub(b"EDID", b"TestSpell\0"),
            sub(b"FULL", b"Test Spell\0"),
            sub(b"SPIT", &spit),
            sub(b"EFID", &0xAAAAu32.to_le_bytes()),
            sub(b"EFIT", &efit1),
            sub(b"EFID", &0xBBBBu32.to_le_bytes()),
            sub(b"EFIT", &efit2),
        ];
        let s = parse_spel(0x6666, &subs, GameKind::Skyrim, &None);
        assert_eq!(s.effects.len(), 2);
        assert_eq!(s.effects[0].effect_form_id, 0xAAAA);
        assert_eq!(s.effects[0].magnitude, 5.0);
        assert_eq!(s.effects[1].effect_form_id, 0xBBBB);
        assert_eq!(s.effects[1].duration, 0);
    }

    /// #4131 — `MagicEffectAccumulator::feed`'s EFID remap (#4071) was only
    /// ever exercised under `&None` (identity) remap, so a future regression
    /// reverting the latch's `remap_fid` call would compile clean and pass
    /// every existing test. Pin the non-identity case here, shared by every
    /// `parse_spel`/`parse_ench` caller of the accumulator.
    #[test]
    fn parse_spel_efid_is_remapped() {
        let remap = Some(FormIdRemap::regular(2, vec![0]));
        let mut efit = Vec::new();
        efit.extend_from_slice(&1.0f32.to_le_bytes()); // mag
        efit.extend_from_slice(&0u32.to_le_bytes()); // area
        efit.extend_from_slice(&0u32.to_le_bytes()); // dur

        let subs = vec![
            sub(b"EFID", &0x0100_7777u32.to_le_bytes()), // self-ref
            sub(b"EFIT", &efit),
        ];
        let s = parse_spel(0x6667, &subs, GameKind::Skyrim, &remap);
        assert_eq!(s.effects.len(), 1);
        assert_eq!(
            s.effects[0].effect_form_id, 0x0200_7777,
            "a self-referencing EFID must land on the plugin's own global \
             slot, not keep its plugin-local mod index"
        );
    }

    #[test]
    fn parse_ench_with_one_effect() {
        let mut enit = Vec::new();
        enit.extend_from_slice(&2u32.to_le_bytes()); // type
        enit.extend_from_slice(&25u32.to_le_bytes()); // charge
        enit.extend_from_slice(&100u32.to_le_bytes()); // cost
        enit.extend_from_slice(&0u32.to_le_bytes()); // flags

        let mut efit = Vec::new();
        efit.extend_from_slice(&1.5f32.to_le_bytes()); // mag
        efit.extend_from_slice(&0u32.to_le_bytes()); // area
        efit.extend_from_slice(&10u32.to_le_bytes()); // dur

        let subs = vec![
            sub(b"EDID", b"TestEnch\0"),
            sub(b"FULL", b"Test\0"),
            sub(b"ENIT", &enit),
            sub(b"EFID", &0x1234u32.to_le_bytes()),
            sub(b"EFIT", &efit),
        ];
        let e = parse_ench(0x7777, &subs, GameKind::Skyrim, &None);
        assert_eq!(e.effects.len(), 1);
        assert_eq!(e.effects[0].effect_form_id, 0x1234);
        assert_eq!(e.effects[0].magnitude, 1.5);
        assert_eq!(e.effects[0].duration, 10);
    }

    #[test]
    fn parse_spel_efit_without_efid_is_skipped() {
        let mut spit = Vec::new();
        spit.extend_from_slice(&100u32.to_le_bytes());
        spit.extend_from_slice(&[0u8; 8]);
        spit.extend_from_slice(&0u32.to_le_bytes());

        let mut efit = Vec::new();
        efit.extend_from_slice(&1.0f32.to_le_bytes());
        efit.extend_from_slice(&0u32.to_le_bytes());
        efit.extend_from_slice(&0u32.to_le_bytes());

        let subs = vec![
            sub(b"EDID", b"NoEfidSpell\0"),
            sub(b"SPIT", &spit),
            sub(b"EFIT", &efit), // EFIT without prior EFID
        ];
        let s = parse_spel(0x8888, &subs, GameKind::Skyrim, &None);
        assert!(s.effects.is_empty());
    }

    /// TD2-110 / #2069 — the EFID latch must clear when an EFIT consumes it,
    /// so a stray second EFIT cannot re-bind the same MGEF and emit a
    /// phantom duplicate effect. `parse_spel_efit_without_efid_is_skipped`
    /// covers the never-latched case; this covers the already-consumed one,
    /// which is the invariant `MagicEffectAccumulator` now single-sources
    /// for both `parse_spel` and `parse_ench`.
    #[test]
    fn efit_after_a_consumed_efid_does_not_duplicate_the_effect() {
        let mut spit = Vec::new();
        spit.extend_from_slice(&100u32.to_le_bytes());
        spit.extend_from_slice(&[0u8; 8]);
        spit.extend_from_slice(&0u32.to_le_bytes());

        let mut efit = Vec::new();
        efit.extend_from_slice(&4.0f32.to_le_bytes());
        efit.extend_from_slice(&1u32.to_le_bytes());
        efit.extend_from_slice(&7u32.to_le_bytes());

        let subs = vec![
            sub(b"EDID", b"DoubleEfitSpell\0"),
            sub(b"SPIT", &spit),
            sub(b"EFID", &0xCAFEu32.to_le_bytes()),
            sub(b"EFIT", &efit),
            sub(b"EFIT", &efit), // no intervening EFID — must be dropped
        ];
        let s = parse_spel(0x9999, &subs, GameKind::Skyrim, &None);
        assert_eq!(s.effects.len(), 1, "the consumed EFID must not re-bind");
        assert_eq!(s.effects[0].effect_form_id, 0xCAFE);

        // Same accumulator, same rule, on the enchantment side.
        let mut enit = Vec::new();
        enit.extend_from_slice(&[0u8; 16]);
        let subs = vec![
            sub(b"EDID", b"DoubleEfitEnch\0"),
            sub(b"ENIT", &enit),
            sub(b"EFID", &0xBEEFu32.to_le_bytes()),
            sub(b"EFIT", &efit),
            sub(b"EFIT", &efit),
        ];
        let e = parse_ench(0x9998, &subs, GameKind::Skyrim, &None);
        assert_eq!(e.effects.len(), 1);
        assert_eq!(e.effects[0].effect_form_id, 0xBEEF);
    }

    #[test]
    fn enchantment_header_schema_reads_correctly() {
        // Test the Phase C schema decoder for ENIT.
        let mut enit = Vec::new();
        enit.extend_from_slice(&3u32.to_le_bytes()); // enchantment_type
        enit.extend_from_slice(&50u32.to_le_bytes()); // charge_amount
        enit.extend_from_slice(&200u32.to_le_bytes()); // enchant_cost
        enit.extend_from_slice(&0x0000_0005u32.to_le_bytes()); // enchant_flags

        let sub = SubRecord {
            sub_type: *b"ENIT",
            data: enit,
        };

        let header = read_sub::<EnchantmentHeader>(&sub).expect("read schema");
        assert_eq!(header.enchantment_type, 3);
        assert_eq!(header.charge_amount, 50);
        assert_eq!(header.enchant_cost, 200);
        assert_eq!(header.enchant_flags, 0x0000_0005);
    }

    #[test]
    fn enchantment_header_schema_rejects_wrong_type() {
        // Schema should reject if sub_type doesn't match CODE.
        let wrong_sub = SubRecord {
            sub_type: *b"XXXX",
            data: vec![0u8; 16],
        };

        let result = read_sub::<EnchantmentHeader>(&wrong_sub);
        assert!(result.is_err(), "should reject mismatched sub_type");
    }

    #[test]
    fn magic_effect_header_schema_reads_correctly() {
        // Test the Phase C schema decoder for DATA (FO3/FNV 36-byte layout).
        let mut data = Vec::new();
        data.extend_from_slice(&0x0000_0003u32.to_le_bytes()); // effect_flags
        data.extend_from_slice(&15.5f32.to_le_bytes()); // base_cost
        data.extend_from_slice(&0x0002_3456_u32.to_le_bytes()); // associated_item
        data.extend_from_slice(&3i32.to_le_bytes()); // magic_school
        data.extend_from_slice(&7i32.to_le_bytes()); // resistance_av
        data.extend_from_slice(&5u16.to_le_bytes()); // counter_effect_count
        data.extend_from_slice(&0u16.to_le_bytes()); // pad
        data.extend_from_slice(&0x3333u32.to_le_bytes()); // light_form_id
        data.extend_from_slice(&2500.0f32.to_le_bytes()); // projectile_speed
        data.extend_from_slice(&0x4444u32.to_le_bytes()); // effect_shader_id

        let sub = SubRecord {
            sub_type: *b"DATA",
            data,
        };

        let header = read_sub::<MagicEffectHeader>(&sub).expect("read schema");
        assert_eq!(header.effect_flags, 0x0000_0003);
        assert_eq!(header.base_cost, 15.5);
        assert_eq!(header.associated_item, 0x0002_3456);
        assert_eq!(header.magic_school, 3);
        assert_eq!(header.resistance_av, 7);
        assert_eq!(header.light_form_id, 0x3333);
        assert_eq!(header.projectile_speed, 2500.0);
        assert_eq!(header.effect_shader_id, 0x4444);
    }

    #[test]
    fn magic_effect_header_schema_rejects_wrong_type() {
        // Schema should reject if sub_type doesn't match CODE.
        let wrong_sub = SubRecord {
            sub_type: *b"XXXX",
            data: vec![0u8; 36],
        };

        let result = read_sub::<MagicEffectHeader>(&wrong_sub);
        assert!(result.is_err(), "should reject mismatched sub_type");
    }
}
