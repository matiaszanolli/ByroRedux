//! Magic / perks records.

use super::super::common::{read_lstring_or_zstring, read_zstring};
use super::super::condition::{parse_ctda, ConditionList};
use crate::esm::sub_reader::SubReader;
use crate::esm::reader::SubRecord;
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
    if &sub.sub_type != &T::CODE {
        anyhow::bail!(
            "SubRecord type mismatch: expected {:?}, got {:?}",
            std::str::from_utf8(&T::CODE).unwrap_or("invalid UTF-8"),
            std::str::from_utf8(&sub.sub_type).unwrap_or("invalid UTF-8")
        );
    }
    let mut reader = SubReader::new(&sub.data);
    T::read(&mut reader)
}

/// SPIT (Spell Header) schema.
/// FO3/FNV: 16 bytes (magicka_cost u32, spell_type u32, level u32, spell_flags u32)
/// Skyrim+: 20+ bytes (same as above + cast_type u32 @16, …)
/// Phase C schema decoder with per-game size branching.
#[derive(Debug, Clone, Copy)]
struct SpellHeader {
    pub cost: u32,         // @0
    pub spell_flags: u32,  // @12
    // Skyrim adds: cast_type: u32 @16, … (not decoded here)
}

impl SubRecordSchema for SpellHeader {
    const CODE: [u8; 4] = *b"SPIT";

    fn read(r: &mut SubReader) -> Result<Self> {
        let cost = r.u32_or_default();
        r.skip_or_eof(8);  // spell_type (u32) + level (u32) at offsets 4..12
        let spell_flags = r.u32_or_default();
        Ok(SpellHeader { cost, spell_flags })
    }
}

/// ENIT (Enchantment Header) schema — fixed 16 bytes (FO3/FNV/Oblivion)
/// or 20 bytes (Skyrim adds cast_type u32). Phase C schema decoder.
#[derive(Debug, Clone, Copy)]
struct EnchantmentHeader {
    pub enchantment_type: u32,    // @0
    pub charge_amount: u32,       // @4
    pub enchant_cost: u32,        // @8
    pub enchant_flags: u32,       // @12
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
        r.skip_or_eof(4);  // counter_effect_count u16 + pad u16 @20..24
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
    Range { min: f32, max: f32 },
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
/// decoded. Per-entry CTDA conditions (gate whether each entry fires)
/// and the per-`function_type` EPFD semantic decode are follow-ups —
/// M47.1's `ConditionList` already covers the consumer side of CTDA;
/// the per-entry list just needs threading through the block walker.
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

pub fn parse_perk(form_id: u32, subs: &[SubRecord]) -> PerkRecord {
    let mut out = PerkRecord {
        form_id,
        ..Default::default()
    };
    let mut block = PerkBlock::None;

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
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
                        0 if sub.data.len() >= 5 => {
                            let quest_form_id = r.u32_or_default();
                            let stage = r.u8_or_default() as u16;
                            Some(PerkEntryBody::Quest {
                                quest_form_id,
                                stage,
                            })
                        }
                        1 if sub.data.len() >= 4 => {
                            let spell_form_id = r.u32_or_default();
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
                    body:
                        Some(PerkEntryBody::EntryPoint { function_type, .. }),
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
                    body: Some(PerkEntryBody::EntryPoint {
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
                        2 if d.len() >= 4 => PerkFunctionData::Float(
                            f32::from_le_bytes([d[0], d[1], d[2], d[3]])
                        ),
                        3 if d.len() >= 8 => PerkFunctionData::Range {
                            min: f32::from_le_bytes([d[0], d[1], d[2], d[3]]),
                            max: f32::from_le_bytes([d[4], d[5], d[6], d[7]]),
                        },
                        4 if d.len() >= 4 => PerkFunctionData::FormId(
                            u32::from_le_bytes([d[0], d[1], d[2], d[3]])
                        ),
                        5 if d.len() >= 4 => PerkFunctionData::LString(
                            u32::from_le_bytes([d[0], d[1], d[2], d[3]])
                        ),
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
            b"CTDA" => {
                if let PerkBlock::Open { ref mut conditions, .. } = &mut block {
                    if let Some(cond) = parse_ctda(sub) {
                        conditions.push(cond);
                    }
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

/// `SPEL` spell / ability / power record. FO3/FNV also covers passive
/// abilities and radiation-poisoning style auto-cast effects. SPIT
/// carries cost + level requirement + flags; effect list (EFID/EFIT)
/// is decoded as MagicEffectItem chains.
#[derive(Debug, Clone, Default)]
pub struct SpelRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Flags from SPIT offset 12 (or 8 on some pre-FNV variants).
    /// Bit 0 = `Manual Cost`, bit 2 = `Touch Explodes`.
    pub spell_flags: u32,
    /// Magicka cost from SPIT offset 0.
    pub cost: u32,
    /// Magic effects applied by this spell. Built from EFID/EFIT pairs.
    pub effects: Vec<MagicEffectItem>,
}

pub fn parse_spel(form_id: u32, subs: &[SubRecord]) -> SpelRecord {
    let mut out = SpelRecord {
        form_id,
        ..Default::default()
    };
    let mut current_efid: u32 = 0;
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"SPIT" => {
                if let Ok(header) = read_sub::<SpellHeader>(sub) {
                    out.cost = header.cost;
                    out.spell_flags = header.spell_flags;
                }
            }
            b"EFID" if sub.data.len() >= 4 => {
                current_efid = u32::from_le_bytes([
                    sub.data[0], sub.data[1], sub.data[2], sub.data[3],
                ]);
            }
            b"EFIT" if sub.data.len() >= 12 => {
                if current_efid != 0 {
                    out.effects.push(MagicEffectItem {
                        effect_form_id: current_efid,
                        magnitude: f32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]),
                        area: u32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]),
                        duration: u32::from_le_bytes([sub.data[8], sub.data[9], sub.data[10], sub.data[11]]),
                    });
                    current_efid = 0;
                }
            }
            _ => {}
        }
    }
    out
}

/// `MGEF` magic effect record. Universal bridge for Actor Value
/// modifications — every perk entry point, spell effect, and
/// ingredient effect routes through here. Full DATA structure
/// decoded to enable effect application across games.
#[derive(Debug, Clone, PartialEq)]
pub struct MgefRecord {
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
}

impl Default for MgefRecord {
    fn default() -> Self {
        Self {
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
        }
    }
}

pub fn parse_mgef(form_id: u32, subs: &[SubRecord]) -> MgefRecord {
    let mut out = MgefRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            b"DESC" => out.description = read_lstring_or_zstring(&sub.data),
            b"DATA" => {
                if let Ok(header) = read_sub::<MagicEffectHeader>(sub) {
                    out.effect_flags = header.effect_flags;
                    out.base_cost = header.base_cost;
                    out.associated_item = header.associated_item;
                    out.magic_school = header.magic_school;
                    out.resistance_av = header.resistance_av;
                    out.light_form_id = header.light_form_id;
                    out.projectile_speed = header.projectile_speed;
                    out.effect_shader_id = header.effect_shader_id;
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

pub fn parse_ench(form_id: u32, subs: &[SubRecord]) -> EnchRecord {
    let mut out = EnchRecord {
        form_id,
        ..Default::default()
    };
    let mut current_efid: u32 = 0;
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
            // ENIT decoded via schema decoder (Phase C).
            b"ENIT" if sub.data.len() >= 16 => {
                if let Ok(header) = read_sub::<EnchantmentHeader>(sub) {
                    out.enchantment_type = header.enchantment_type;
                    out.charge_amount = header.charge_amount;
                    out.enchant_cost = header.enchant_cost;
                    out.enchant_flags = header.enchant_flags;
                }
            }
            b"EFID" if sub.data.len() >= 4 => {
                current_efid = u32::from_le_bytes([
                    sub.data[0], sub.data[1], sub.data[2], sub.data[3],
                ]);
            }
            b"EFIT" if sub.data.len() >= 12 => {
                if current_efid != 0 {
                    out.effects.push(MagicEffectItem {
                        effect_form_id: current_efid,
                        magnitude: f32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]),
                        area: u32::from_le_bytes([sub.data[4], sub.data[5], sub.data[6], sub.data[7]]),
                        duration: u32::from_le_bytes([sub.data[8], sub.data[9], sub.data[10], sub.data[11]]),
                    });
                    current_efid = 0;
                }
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
    fn parse_perk_picks_data_flags() {
        let subs = vec![
            sub(b"EDID", b"IntenseTraining\0"),
            sub(b"FULL", b"Intense Training\0"),
            sub(b"DESC", b"Increase any one S.P.E.C.I.A.L. by 1.\0"),
            sub(b"DATA", &[0x01]), // playable
        ];
        let p = parse_perk(0xE5E5, &subs);
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
        let p = parse_perk(0xBADAu32, &subs);
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
        let p = parse_perk(0xAAAAu32, &subs);
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
        let p = parse_perk(0xBBBBu32, &subs);
        assert_eq!(p.entries.len(), 1);
        match &p.entries[0].body {
            PerkEntryBody::Ability { spell_form_id } => {
                assert_eq!(*spell_form_id, 0x000A_BC01);
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
            sub(b"EPFT", &[0x02]),                    // function overwrite to 2 (Multiply = Float)
            sub(b"EPFD", &epfd),
            sub(b"PRKF", &[]),
        ];
        let p = parse_perk(0xCCCCu32, &subs);
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
        let p = parse_perk(0xDDDDu32, &subs);
        assert_eq!(p.entries.len(), 3);
        assert_eq!(p.entries[0].priority, 1);
        assert_eq!(p.entries[1].priority, 2);
        assert_eq!(p.entries[2].priority, 3);
        assert!(matches!(p.entries[0].body, PerkEntryBody::Quest { .. }));
        assert!(matches!(p.entries[1].body, PerkEntryBody::Ability { .. }));
        assert!(matches!(p.entries[2].body, PerkEntryBody::EntryPoint { .. }));
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
        let p = parse_perk(0xEEEEu32, &subs);
        assert!(p.entries.is_empty());
    }

    #[test]
    fn parse_spel_picks_spit_cost_and_flags() {
        let mut spit = Vec::new();
        spit.extend_from_slice(&42u32.to_le_bytes()); // cost
        spit.extend_from_slice(&[0u8; 8]); // padding to flags offset
        spit.extend_from_slice(&0x0000_0004u32.to_le_bytes()); // flags
        let subs = vec![sub(b"EDID", b"Fireball\0"), sub(b"SPIT", &spit)];
        let s = parse_spel(0xF6F6, &subs);
        assert_eq!(s.cost, 42);
        assert_eq!(s.spell_flags, 0x0000_0004);
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
        let e = parse_ench(0x000E_5C77, &subs);
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
        let e = parse_ench(0x0001_F25D, &subs);
        assert_eq!(e.charge_amount, 50);
        assert_eq!(e.enchant_cost, 200);
    }

    #[test]
    fn parse_ench_short_enit_keeps_defaults() {
        // Author-malformed ENIT (< 16 bytes) must not panic and must
        // leave scalars at their defaults so the surrounding records
        // still load.
        let subs = vec![sub(b"EDID", b"BrokenEnchant\0"), sub(b"ENIT", &[0u8; 8])];
        let e = parse_ench(0xDEAD_BEEF, &subs);
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
        let e = parse_mgef(0xA7A7, &subs);
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
            sub(b"DATA", &data), // quest_form_id=0x1234, stage=0
            sub(b"CTDA", &ctda),
            sub(b"PRKF", &[]),
        ];
        let p = parse_perk(0xFFFF, &subs);
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
        let p = parse_perk(0x1111, &subs);
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
        let p = parse_perk(0x2222, &subs);
        assert_eq!(p.entries.len(), 1);
        match &p.entries[0].body {
            PerkEntryBody::EntryPoint { function_data, .. } => {
                assert_eq!(
                    *function_data,
                    PerkFunctionData::Range {
                        min: 1.0,
                        max: 5.0
                    }
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
        let p = parse_perk(0x3333, &subs);
        assert_eq!(p.entries.len(), 1);
        match &p.entries[0].body {
            PerkEntryBody::EntryPoint { function_data, .. } => {
                assert_eq!(*function_data, PerkFunctionData::FormId(0xBEEF_1234));
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
        let p = parse_perk(0x4444, &subs);
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
        data.extend_from_slice(&0x0001_AB_CDu32.to_le_bytes()); // associated_item
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
        let m = parse_mgef(0x5555, &subs);
        assert_eq!(m.effect_flags, 0x0000_0001);
        assert_eq!(m.base_cost, 10.0);
        assert_eq!(m.associated_item, 0x0001_AB_CD);
        assert_eq!(m.magic_school, 2);
        assert_eq!(m.resistance_av, 5);
        assert_eq!(m.light_form_id, 0x1111);
        assert_eq!(m.projectile_speed, 3000.0);
        assert_eq!(m.effect_shader_id, 0x2222);
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
        let s = parse_spel(0x6666, &subs);
        assert_eq!(s.effects.len(), 2);
        assert_eq!(s.effects[0].effect_form_id, 0xAAAA);
        assert_eq!(s.effects[0].magnitude, 5.0);
        assert_eq!(s.effects[1].effect_form_id, 0xBBBB);
        assert_eq!(s.effects[1].duration, 0);
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
        let e = parse_ench(0x7777, &subs);
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
        let s = parse_spel(0x8888, &subs);
        assert!(s.effects.is_empty());
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
    fn spell_header_schema_reads_correctly() {
        // Test the Phase C schema decoder for SPIT (FO3/FNV 16-byte layout).
        let mut spit = Vec::new();
        spit.extend_from_slice(&75u32.to_le_bytes()); // cost
        spit.extend_from_slice(&2u32.to_le_bytes()); // spell_type
        spit.extend_from_slice(&15u32.to_le_bytes()); // level
        spit.extend_from_slice(&0x0000_0001u32.to_le_bytes()); // spell_flags

        let sub = SubRecord {
            sub_type: *b"SPIT",
            data: spit,
        };

        let header = read_sub::<SpellHeader>(&sub).expect("read schema");
        assert_eq!(header.cost, 75);
        assert_eq!(header.spell_flags, 0x0000_0001);
    }

    #[test]
    fn spell_header_schema_rejects_wrong_type() {
        // Schema should reject if sub_type doesn't match CODE.
        let wrong_sub = SubRecord {
            sub_type: *b"XXXX",
            data: vec![0u8; 16],
        };

        let result = read_sub::<SpellHeader>(&wrong_sub);
        assert!(result.is_err(), "should reject mismatched sub_type");
    }

    #[test]
    fn magic_effect_header_schema_reads_correctly() {
        // Test the Phase C schema decoder for DATA (FO3/FNV 36-byte layout).
        let mut data = Vec::new();
        data.extend_from_slice(&0x0000_0003u32.to_le_bytes()); // effect_flags
        data.extend_from_slice(&15.5f32.to_le_bytes()); // base_cost
        data.extend_from_slice(&0x0002_34_56u32.to_le_bytes()); // associated_item
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
        assert_eq!(header.associated_item, 0x0002_34_56);
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
