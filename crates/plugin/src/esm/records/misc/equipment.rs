//! Equipment / crafting / generic-record records.

use super::super::common::{read_lstring_or_zstring, read_zstring, CommonNamedFields};
use super::super::condition::{push_ctda, ConditionList};
use crate::esm::reader::{FormIdRemap, GameKind, SubRecord};
use crate::esm::records::common::remap_fid;
use crate::esm::sub_reader::SubReader;

/// ARMA — armor addon record. Race-specific biped slot variants for
/// armor. The ARMO → ARMA → race-specific MODL chain resolves armor
/// rendering on non-default-race NPCs (Vipers, Ghouls, Super Mutants,
/// Centaurs, Deathclaws, etc.).
///
/// Stub captures EDID + biped flags + DT/DR + the male/female biped
/// model paths the Skyrim+ equip pipeline dispatches against. See
/// audits `FNV-D2-NEW-01` / #808 (initial stub) and #896 (Phase B
/// extension for Skyrim+/FO4 actor-worn meshes).
///
/// Per-game sub-record meanings sourced from the xEdit project (by
/// ElminsterAU and the xEdit team, MPL-2.0,
/// <https://github.com/TES5Edit/TES5Edit>) at tag `dev-4.1.6`
/// (2026-05-07):
///
/// | sub  | FNV / FO3 ARMA          | Skyrim+ ARMA              |
/// |------|-------------------------|---------------------------|
/// | BMDT | biped_flags + general   | (replaced by BOD2)        |
/// | BOD2 | n/a                     | biped_flags + armor_type  |
/// | RNAM | n/a                     | race FormID               |
/// | MODL | male **biped** path     | additional-races FormID[] |
/// | MOD2 | male world model path   | male **biped** path       |
/// | MOD3 | female biped path       | female biped path         |
/// | MOD4 | female world model path | male 1st-person biped     |
///
/// The `male_biped_model` / `female_biped_model` fields below
/// normalise across games: they always hold the *worn* mesh paths,
/// regardless of which sub-record code carried them on disk.
#[derive(Debug, Clone, Default)]
pub struct ArmaRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `BMDT` (FNV/FO3) or `BOD2` (Skyrim+) offset 0..4 — biped slot
    /// bitfield. Drives the ARMO → ARMA biped-slot routing.
    pub biped_flags: u32,
    /// `BMDT` offset 4..8 — general flags (Heavy / Medium / Light,
    /// power armor, etc.). Decoded lazily per-game. 0 on Skyrim+
    /// (BOD2 carries `armor_type` instead, captured below).
    pub general_flags: u32,
    /// `DNAM` offset 0..2 — Damage Threshold (i16, FNV-specific).
    pub dt: i16,
    /// `DNAM` offset 2..4 — Damage Resistance (i16, FNV-specific).
    pub dr: i16,
    /// `RNAM` — race FormID this addon is for (Skyrim+ only).
    /// 0 on Oblivion/FO3/FNV — race linkage there flows through
    /// ARMO records or per-race world models, not ARMA.
    pub race_form_id: u32,
    /// `MODL` (FNV/FO3 — male biped) or `MOD2` (Skyrim+ — male
    /// biped). Normalised: always the male worn mesh path.
    pub male_biped_model: String,
    /// `MOD3` on every supported game — female worn mesh path.
    pub female_biped_model: String,
    /// Skyrim+ only: additional race FormIDs from the `Additional
    /// Races` MODL FormID array. Empty on FNV/FO3 (MODL there is the
    /// male biped path, captured into `male_biped_model`).
    pub additional_races: Vec<u32>,
}

/// Parse an ARMA record. Game-aware because MODL has different
/// meanings (string path vs FormID list) on Skyrim+ vs FNV/FO3.
///
/// #3714 — takes the load-order `remap` because `RNAM` (the race this
/// addon fits) and the Skyrim+ additional-race `MODL` array are embedded
/// FormIDs. They are matched against `NPC_.RNAM`, which IS remapped
/// (`actor/mod.rs`), so leaving these raw failed the race match for every
/// DLC-added-race addon and dropped the actor into the deliberately
/// single-valued pass-2 fallback — the multi-addon collapse #3357 fixed.
pub fn parse_arma(
    form_id: u32,
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> ArmaRecord {
    let mut out = ArmaRecord {
        form_id,
        ..Default::default()
    };
    let is_skyrim_or_later = matches!(
        game,
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield
    );

    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    out.editor_id = common.editor_id;
    for sub in subs {
        match &sub.sub_type {
            b"BMDT" if sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                out.biped_flags = r.u32_or_default();
                out.general_flags = r.u32_or_default();
            }
            b"BOD2" if is_skyrim_or_later && sub.data.len() >= 4 => {
                out.biped_flags = SubReader::new(&sub.data).u32_or_default();
            }
            // BODT — Skyrim's pre-BOD2 body template (xEdit `wbBODTBOD2`):
            // the first u32 is the same biped mask BOD2 carries; then
            // general_flags u8 + 3 pad (+ armor_type u32 in the 12-byte
            // form, which the ARMA consumer does not read). #5358 — all
            // 766 Skyrim.esm ARMAs author BODT, not BOD2, so BOD2-only
            // decoding left every Skyrim addon claiming no region.
            b"BODT" if is_skyrim_or_later && sub.data.len() >= 8 => {
                out.biped_flags = SubReader::new(&sub.data).u32_or_default();
            }
            b"DNAM" if !is_skyrim_or_later && sub.data.len() >= 4 => {
                let mut r = SubReader::new(&sub.data);
                out.dt = r.i16_or_default();
                out.dr = r.i16_or_default();
            }
            b"RNAM" if is_skyrim_or_later && sub.data.len() >= 4 => {
                let raw = SubReader::new(&sub.data).u32_or_default();
                out.race_form_id = remap_fid(raw, remap);
            }
            b"MODL" => {
                if is_skyrim_or_later {
                    // Skyrim+ ARMA additional-races: 4-byte FormID per
                    // entry. Multiple MODL sub-records can appear.
                    if let Ok(id) = SubReader::new(&sub.data).u32() {
                        out.additional_races.push(remap_fid(id, remap));
                    }
                } else if out.male_biped_model.is_empty() {
                    // FNV/FO3 ARMA male biped — first MODL only;
                    // ignore subsequent MODT (texture variant) refs.
                    out.male_biped_model = read_zstring(&sub.data);
                }
            }
            b"MOD2" if is_skyrim_or_later && out.male_biped_model.is_empty() => {
                // Skyrim+ male biped model lives at MOD2 (FNV's MOD2
                // is the world/drop model — uninteresting for the
                // worn-mesh consumer).
                out.male_biped_model = read_zstring(&sub.data);
            }
            b"MOD3" if out.female_biped_model.is_empty() => {
                out.female_biped_model = read_zstring(&sub.data);
            }
            _ => {}
        }
    }
    out
}

/// One body part of a [`BptdRecord`] — the `BPTN`/`BPNN`/`BPNT`/`BPNI`
/// string run plus its 84-byte `BPND` payload.
///
/// The `BPND` layout is corpus-established (#6350 spike, value-shape
/// clustering per the #3324 method; evidence table on the issue):
/// offsets are identical across vanilla `FalloutNV.esm` and
/// `Fallout3.esm`. Fields past `@20` (gore art, hit-reaction twist in
/// radians, blood/decal IPDS refs) are Phase-4 dismemberment material
/// and are not decoded yet.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BptdPart {
    /// `BPTN` — display name ("Head", "Left Legs", …). VATS groups
    /// parts by [`Self::actor_value`] in the UI, not by this name.
    pub name: String,
    /// `BPNN` — the bone the part is attached to (e.g. `Bip01 R Calf`).
    pub node: String,
    /// `BPNT` — the bone VATS aims at. NOT always `node`: measured,
    /// `Bip01 R UpperArm` parts target `Bip01 R Forearm`.
    pub vats_target_node: String,
    /// `BPNI` — fourth string in the run (gore/IK-flavored); retained
    /// raw for the Phase-4 dismemberment consumer.
    pub bpni: String,
    /// `BPND` @0 — damage multiplier applied to hits on this part.
    pub damage_mult: f32,
    /// `BPND` @4 — flags (xEdit `wbDefinitionsFNV.pas:5963-5982`, FO3 the same): Severable 0x01,
    /// IK Data 0x02, IK Biped 0x04, Explodable 0x08, IK Is Head 0x10, IK Headtracking 0x20,
    /// To-Hit Absolute 0x40. Measured on `DefaultBodyPartData`: head 59 (severable, IK, explodable,
    /// is-head, headtracking), torso 8 (explodable only), limbs 9 / 15.
    pub flags: u8,
    /// `BPND` @5 — part type, the `wbBodyLocationEnum` (torso 0, head 1, left arm 3, right arm 5,
    /// left leg 7, right leg 10, ...). Not a flag: the "head = 1, torso = 0" pattern an earlier
    /// reading took for a severable bit is this enum (#6448).
    pub part_type: u8,
    /// `BPND` @6 — the part's health as a percent of the actor's total. Differs per record
    /// (`PlayerBodyPartData` 75 head / 255 torso, NPC default 20 / 60), which fits a share of the
    /// actor's health, not a hit chance (#6448).
    pub health_percent: u8,
    /// `BPND` @7 — limb-condition actor-value enum (`25..=31` in the
    /// FO3/FNV AV layout; `255` = none). Parts sharing a value group
    /// into one VATS UI entry.
    pub actor_value: u8,
    /// `BPND` @8 — base to-hit chance in VATS (percent). The same on both default records'
    /// peers (head 30, torso 60, arms 45, legs 50), which fits a per-part VATS chance.
    pub to_hit_chance: u8,
    /// `BPND` @9 — chance the part explodes when destroyed (percent;
    /// torso 0, arms 30-50, heads 95-100 in vanilla).
    pub explosion_chance: u8,
    /// `BPND` @10 — debris count (`u16`).
    pub debris_count: u16,
    /// `BPND` @12 — generic debris FormID (DEBR; 0 = none).
    pub generic_debris: u32,
    /// `BPND` @16 — explosion FormID (EXPL; 0 = none).
    pub explosion: u32,
}

/// BPTD — body part data record. Per-actor limb table: what VATS aims
/// at (`vats_target_node` + `to_hit_chance`), how limb damage routes
/// (`actor_value`, `damage_mult`, `health_percent`), and the
/// dismemberment/gore metadata the Phase-4 consumer reads.
///
/// Linkage (#6350 spike): `RACE` carries **no** BPTD reference in
/// FO3/FNV. Creatures attach theirs via a standalone 4-byte `PNAM` on
/// `CREA` (1578/1578 FNV, 533/533 FO3); humanoids need none — the
/// engine-reserved default-object slots are
/// [`PLAYER_BODY_PART_DATA`] (`0x1C`, the player) and
/// [`DEFAULT_BODY_PART_DATA`] (`0x1D`, everyone else).
#[derive(Debug, Clone, Default)]
pub struct BptdRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `MODL` — the skeleton model whose bones the part nodes name.
    pub skeleton_model: String,
    pub parts: Vec<BptdPart>,
}

/// Engine-reserved body-part-data slots, measured on vanilla masters
/// (#6350): `PlayerBodyPartData` ships at FormID `0x1C` and
/// `DefaultBodyPartData` at `0x1D` in both FalloutNV.esm and
/// Fallout3.esm — the Bethesda default-object convention (cf. the
/// player reference `0x14`). Humanoid actors resolve body data through
/// these; only `CREA.PNAM` overrides.
pub const PLAYER_BODY_PART_DATA: u32 = 0x1C;
pub const DEFAULT_BODY_PART_DATA: u32 = 0x1D;

impl BptdRecord {
    /// Number of body parts (the old count-only stub's field, kept as a
    /// method so census callers read the same number as before #6350).
    pub fn part_count(&self) -> u32 {
        self.parts.len() as u32
    }
}

/// The `BPND` payload length of the dialect [`parse_bptd`] decodes (FO3 / FNV, and Skyrim's
/// 84-byte payloads). A longer payload is another layout (#6465).
const BPND_LEGACY_LEN: usize = 84;

// #1057's Tier-7 note is half-retired: the full per-part array decode
// landed with VATS (#6350) because targeting needs the limb table. The
// gore/dismemberment half (BPND @20+, NAM1/NAM4/NAM5, RAGA, and the
// physics-crate wiring) still lands with the ragdoll consumer.
pub fn parse_bptd(
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<FormIdRemap>,
) -> BptdRecord {
    let mut out = BptdRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    out.editor_id = common.editor_id;
    out.skeleton_model = common.model_path;
    // The per-part run is `BPTN, BPNN, BPNT, BPNI, BPND` repeating
    // (two vanilla records start mid-run at BPNN — a nameless part is
    // legal, so nothing here assumes BPTN arrived). BPND finalizes the
    // accumulated part; NAM1/NAM4/NAM5/RAGA trail each part as gore
    // data and are skipped until Phase 4.
    let mut part = BptdPart::default();
    for sub in subs {
        let d = &sub.data;
        match &sub.sub_type {
            b"BPTN" => part.name = read_lstring_or_zstring(d),
            b"BPNN" => part.node = read_lstring_or_zstring(d),
            b"BPNT" => part.vats_target_node = read_lstring_or_zstring(d),
            b"BPNI" => part.bpni = read_lstring_or_zstring(d),
            b"BPND" => {
                // Width-guarded field reads (#3724): a short payload
                // decodes what it carries and defaults the rest rather
                // than failing the record — modded content must parse.
                let f32_at = |o: usize| d.get(o..o + 4).map_or(0.0, |b| {
                    f32::from_le_bytes([b[0], b[1], b[2], b[3]])
                });
                let u8_at = |o: usize| d.get(o).copied().unwrap_or(0);
                let u32_at = |o: usize| {
                    d.get(o..o + 4)
                        .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                };
                // #6465 — this is the 84-byte FO3 / FNV (and Skyrim) dialect. FO4's BPND is 101
                // bytes with a different layout: decoding it here made every field garbage and fed
                // bogus FormIDs (float 1.0 = 0x3F800000) through `remap_fid`, 152 false warnings on a
                // vanilla load. Leave it undecoded until a sourced FO4 layout is in hand; the name
                // run (BPTN / BPNN / BPNT / BPNI) is kept.
                if d.len() <= BPND_LEGACY_LEN {
                    let u16_at = |o: usize| d.get(o..o + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
                    part.damage_mult = f32_at(0);
                    part.flags = u8_at(4);
                    part.part_type = u8_at(5);
                    part.health_percent = u8_at(6);
                    part.actor_value = u8_at(7);
                    part.to_hit_chance = u8_at(8);
                    part.explosion_chance = u8_at(9);
                    part.debris_count = u16_at(10);
                    part.generic_debris = remap_fid(u32_at(12), remap);
                    part.explosion = remap_fid(u32_at(16), remap);
                }
                out.parts.push(std::mem::take(&mut part));
            }
            _ => {}
        }
    }
    out
}

// ── #809 / FNV-D2-NEW-02 — supporting record stubs ─────────────────
//
// Seven records that gate FNV NPC AI / crafting / impact-effect /
// faction-reputation subsystems. All stub-form (EDID + a handful of
// key scalar / form-ref fields per record); full sub-record decoding
// lands when the consuming subsystem arrives. Pattern matches #808.

/// COBJ — constructible object record (FNV crafting). Workbench /
/// reloading bench / campfire recipes. Stub captures EDID + created
/// form (CNAM) + workbench filter (BNAM). The CNTO component list
/// is deferred to the crafting consumer. See audit
/// `FNV-D2-NEW-02` / #809.
#[derive(Debug, Clone, Default)]
pub struct CobjRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `CNAM` — created form ID (the recipe's output item).
    pub created_form: u32,
    /// `BNAM` — workbench filter form ID (which workbench category
    /// the recipe shows up under).
    pub workbench_form: u32,
    /// Skyrim `CNTO` — the ingredients: `(item form, count)` in authored order. Empty on the games
    /// whose `COBJ` carries components another way (Fallout 4's `FVPA`).
    pub components: Vec<(u32, u32)>,
    /// Skyrim `NAM1` — how many of the created item one craft yields (1 when absent).
    pub created_count: u32,
    /// Skyrim `CTDA` — the conditions the player must meet to craft it (perk gates such as
    /// `HasPerk`). Empty = always craftable at its bench.
    pub conditions: ConditionList,
}

pub fn parse_cobj(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> CobjRecord {
    let mut out = CobjRecord {
        form_id,
        created_count: 1,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    out.editor_id = common.editor_id;
    for sub in subs {
        match &sub.sub_type {
            // #3715 — CNAM/BNAM are embedded FormIDs (the recipe's output
            // item and workbench-category filter); both need the same
            // load-order remap as every other cross-reference.
            b"CNAM" if sub.data.len() >= 4 => {
                out.created_form = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
            }
            b"BNAM" if sub.data.len() >= 4 => {
                out.workbench_form = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap);
            }
            // Skyrim ingredient: item FormID + count.
            b"CNTO" if sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                let item = remap_fid(r.u32_or_default(), remap);
                let count = r.u32_or_default();
                if item != 0 {
                    out.components.push((item, count));
                }
            }
            b"CTDA" | b"CIS1" | b"CIS2" => push_ctda(sub, remap, &mut out.conditions),
            b"NAM1" if sub.data.len() >= 2 => {
                out.created_count = u32::from(u16::from_le_bytes([sub.data[0], sub.data[1]])).max(1);
            }
            _ => {}
        }
    }
    out
}

// ── #810 / FNV-D2-NEW-03 — long-tail catch-all stubs ───────────────
//
// 31 record types in the FNV catch-all-skip long tail. None had a
// concrete consumer driving a per-record parser (the audit's defer-
// until-consumer recommendation). The user opted to bulk-dispatch
// anyway so the catch-all skip approaches parity with FalloutNV.esm's
// authored content set.
//
// All 31 share a single minimal-stub form: EDID + optional FULL.
// Records that gain a real consumer later can grow per-record fields
// via the established #808 / #809 pattern (each record gets its own
// dedicated struct). Until then, this is the cheapest dispatch shape
// that takes them off the catch-all skip without 31× boilerplate.
//
// Records covered (5 clusters):
//   Audio metadata (11): ALOC ANIO ASPC CAMS CPTH DOBJ MICN MSET MUSC SOUN VTYP
//   Visual / world (8):  AMEF DEBR GRAS IMAD LSCR LSCT PWAT RGDL
//   Hardcore mode (4):   DEHY HUNG RADS SLPD
//   Caravan + Casino (6): CCRD CDCK CHAL CHIP CMNY CSNO
//   Recipe residuals (2): RCCT RCPE

/// Minimal-stub record for the long-tail dispatch coverage. EDID +
/// optional FULL captured; all other sub-records skipped. When a
/// consumer for a specific record type arrives, replace its
/// `HashMap<u32, MinimalEsmRecord>` field on `EsmIndex` with a
/// dedicated struct + parser pair following the `#808` / `#809`
/// established pattern. See audit `FNV-D2-NEW-03` / #810.
#[derive(Debug, Clone, Default)]
pub struct MinimalEsmRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
}

pub fn parse_minimal_esm_record(form_id: u32, subs: &[SubRecord]) -> MinimalEsmRecord {
    let mut out = MinimalEsmRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, &None);
    out.editor_id = common.editor_id;
    out.full_name = common.full_name;
    out
}

/// `SLGM` — soul-gem record. Oblivion ships ~15 vanilla SLGMs; FO3
/// drops the record type (no soul magic in the Wasteland), Skyrim
/// keeps it. Referenced by `ENCH` for the enchantment charge model
/// and by quest scripts for the Azura's Star / Soul Trap chain.
///
/// Wire layout per UESP / xEdit (Oblivion / Skyrim TES4 schema):
/// - `EDID` — editor ID
/// - `FULL` — display name
/// - `MODL` — model path (dual-target — also lands in `cells.statics`
///   via [`extract_records_with_modl`] so REFR → SLGM placements
///   render the right mesh)
/// - `ICON` — inventory icon path
/// - `SCRI` — attached script FormID (rare on SLGM)
/// - `DATA` — value (i32) + weight (f32), 8 bytes
/// - `SOUL` — single-byte enum: 0 None, 1 Petty, 2 Lesser, 3 Common,
///   4 Greater, 5 Grand. The soul *currently contained*; pre-loaded
///   soul gems carry a non-zero value.
/// - `SLCP` — single-byte enum (same scale as SOUL): the gem's
///   *capacity*. A Grand Soul Gem can hold a Lesser soul, etc.
///
/// The audit text suggested decoding "DATA byte 0 soul capacity" —
/// that's incorrect (DATA byte 0 is the LSB of `value: i32`). The
/// authoritative capacity field is `SLCP`. See OBL-D3-NEW-02 / #966.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SlgmRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// MODL path — the inventory / world-placement mesh.
    pub model_path: String,
    /// Item value in caps / gold.
    pub value: i32,
    /// Item weight in encumbrance units.
    pub weight: f32,
    /// `SOUL` byte: currently-contained soul magnitude.
    /// 0 None, 1 Petty, 2 Lesser, 3 Common, 4 Greater, 5 Grand.
    pub current_soul: u8,
    /// `SLCP` byte: the gem's capacity (same enum). The audit
    /// originally called this "DATA byte 0"; that was a misread.
    pub soul_capacity: u8,
}

pub fn parse_slgm(form_id: u32, subs: &[SubRecord]) -> SlgmRecord {
    let mut out = SlgmRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, &None);
    out.editor_id = common.editor_id;
    out.full_name = common.full_name;
    out.model_path = common.model_path;
    for sub in subs {
        match &sub.sub_type {
            b"DATA" if sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                out.value = r.i32_or_default();
                out.weight = r.f32_or_default();
            }
            b"SOUL" if !sub.data.is_empty() => out.current_soul = sub.data[0],
            b"SLCP" if !sub.data.is_empty() => out.soul_capacity = sub.data[0],
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::sub;

    /// Skyrim `COBJ`: ingredients in `CNTO`, the yield in `NAM1`, the output in `CNAM`, the bench
    /// keyword in `BNAM`; a record with no `NAM1` yields one.
    #[test]
    fn parse_cobj_reads_skyrim_ingredients_yield_and_bench() {
        let pair = |item: u32, count: u32| {
            let mut v = Vec::new();
            v.extend_from_slice(&item.to_le_bytes());
            v.extend_from_slice(&count.to_le_bytes());
            v
        };
        let subs = vec![
            sub(b"EDID", b"RecipeIronDagger\0"),
            sub(b"COCT", 2u32.to_le_bytes()),
            sub(b"CNTO", pair(0x5ACE4, 1)),
            sub(b"CNTO", pair(0x800E4, 3)),
            sub(b"CNAM", 0x12EB7u32.to_le_bytes()),
            sub(b"BNAM", 0x88105u32.to_le_bytes()),
            sub(b"NAM1", 2u16.to_le_bytes()),
        ];
        let cobj = parse_cobj(0x1, &subs, &None);
        assert_eq!(cobj.components, vec![(0x5ACE4, 1), (0x800E4, 3)]);
        assert_eq!((cobj.created_form, cobj.workbench_form, cobj.created_count), (0x12EB7, 0x88105, 2));
        let cobj = parse_cobj(0x2, &subs[..6], &None);
        assert_eq!(cobj.created_count, 1, "no NAM1 yields one");
    }

    #[test]
    fn parse_arma_picks_edid_biped_flags_dt_dr() {
        // `MetalArmor` shape: BMDT = (i32 biped_flags, i32 general_flags),
        // DNAM = (i16 dt, i16 dr). Stub captures the biped flags so the
        // ARMO → ARMA → biped-slot routing has the data it needs.
        let mut bmdt = [0u8; 8];
        bmdt[0..4].copy_from_slice(&0x0000_000C_u32.to_le_bytes()); // body+legs slot
        bmdt[4..8].copy_from_slice(&0x0000_0001_u32.to_le_bytes()); // metal flag
        let mut dnam = [0u8; 4];
        dnam[0..2].copy_from_slice(&15_i16.to_le_bytes()); // DT
        dnam[2..4].copy_from_slice(&30_i16.to_le_bytes()); // DR
        let subs = vec![
            sub(b"EDID", b"MetalArmor\0"),
            sub(b"BMDT", bmdt),
            sub(b"DNAM", dnam),
        ];
        let a = parse_arma(0x0006_2103, &subs, GameKind::Fallout3NV, &None);
        assert_eq!(a.editor_id, "MetalArmor");
        assert_eq!(a.biped_flags, 0x0000_000C);
        assert_eq!(a.general_flags, 0x0000_0001);
        assert_eq!(a.dt, 15);
        assert_eq!(a.dr, 30);
    }

    /// #5358 — Skyrim's ARMA carries its biped mask in `BODT` (the
    /// pre-BOD2 body template, xEdit `wbBODTBOD2`), not `BOD2`: all 766
    /// Skyrim.esm ARMAs author BODT. Both widths decode the same leading
    /// u32; the 12-byte form's tail (general flags u8 + 3 pad + armor
    /// type u32) is nothing an ARMA consumer reads.
    #[test]
    fn parse_arma_reads_bodt_biped_flags() {
        // 12-byte form: mask (Hair 0x02), general flags + pad, armor type.
        let bodt12 = [
            0x0000_0002_u32.to_le_bytes().to_vec(),
            vec![0x01, 0, 0, 0],
            1_u32.to_le_bytes().to_vec(),
        ]
        .concat();
        let subs = vec![sub(b"EDID", b"SkinDraugrHair01\0"), sub(b"BODT", bodt12)];
        let a = parse_arma(0x0006_7D33, &subs, GameKind::Skyrim, &None);
        assert_eq!(a.biped_flags, 0x0000_0002);

        // 8-byte form: mask + general flags, no armor-type tail.
        let bodt8 = [0x0000_0010_u32.to_le_bytes().to_vec(), vec![0x01, 0, 0, 0]].concat();
        let subs = vec![sub(b"EDID", b"SkinDraugrBeard01\0"), sub(b"BODT", bodt8)];
        let a = parse_arma(0x0006_7D37, &subs, GameKind::Skyrim, &None);
        assert_eq!(a.biped_flags, 0x0000_0010);

        // Truncated (<8 bytes) BODT stays ignored, like a truncated BOD2.
        let a = parse_arma(
            1,
            &[sub(b"BODT", 0x0000_0004_u32.to_le_bytes())],
            GameKind::Skyrim,
            &None,
        );
        assert_eq!(a.biped_flags, 0);
    }

    /// #6350 — the pre-VATS stub counted `BPTN` sub-records directly;
    /// the real grammar finalizes a part only at `BPND` (the string run
    /// accumulates, the payload commits). A `BPTN` with no following
    /// `BPND` contributes no part — pinned here so the rule is a
    /// checked claim, not a comment.
    #[test]
    fn parse_bptd_requires_bpnd_to_finalize_a_part() {
        let subs = vec![
            sub(b"EDID", b"HumanRace\0"),
            sub(b"BPTN", b"Head\0"),
            sub(b"BPTN", b"Torso\0"),
            sub(b"BPTN", b"Left Arm\0"),
        ];
        let b = parse_bptd(0x0009_29DC, &subs, &None);
        assert_eq!(b.editor_id, "HumanRace");
        assert_eq!(b.part_count(), 0);
    }

    /// #6350 — the full per-part decode against the corpus-established
    /// BPND layout: a humanoid head-shaped part with the vanilla value
    /// profile (damage mult 2, severable flag, AV 25 = head condition,
    /// high explosion chance) round-trips field-for-field.
    #[test]
    fn parse_bptd_decodes_the_part_run_and_bpnd_payload() {
        let bpnd = |damage: f32,
                    part_type: u8,
                    flags: u8,
                    hit: u8,
                    av: u8,
                    health: u8,
                    boom: u8,
                    debris_n: u8,
                    debris_form: u32,
                    expl_form: u32| {
            let mut d = Vec::new();
            d.extend_from_slice(&damage.to_le_bytes());
            d.push(flags);
            d.push(part_type);
            d.push(health);
            d.push(av);
            d.push(hit);
            d.push(boom);
            d.extend_from_slice(&u16::from(debris_n).to_le_bytes());
            d.extend_from_slice(&debris_form.to_le_bytes());
            d.extend_from_slice(&expl_form.to_le_bytes());
            d.extend_from_slice(&[0u8; 84 - 20]); // gore tail, Phase 4
            d
        };
        let subs = vec![
            sub(b"EDID", b"DefaultBodyPartData\0"),
            sub(b"MODL", b"characters\\skeleton.nif\0"),
            sub(b"BPTN", b"Head\0"),
            sub(b"BPNN", b"Bip01 Head\0"),
            sub(b"BPNT", b"Bip01 Head\0"),
            sub(b"BPNI", b"GoreHead\0"),
            sub(
                b"BPND",
                bpnd(2.0, 1, 59, 30, 25, 20, 95, 3, 0x000A_0001, 0x000B_0002),
            ),
            sub(b"NAM1", [0]),
            sub(b"BPTN", b"Torso\0"),
            sub(b"BPNN", b"Bip01 Spine1\0"),
            sub(b"BPNT", b"Bip01 Spine1\0"),
            sub(b"BPNI", b"\0"),
            sub(b"BPND", bpnd(1.0, 0, 8, 60, 26, 60, 0, 0, 0, 0)),
        ];
        // A mod-plugin remap so the embedded DEBR/EXPL refs prove the
        // #3714 path: this plugin's first master lives at global slot
        // 1, so an embedded FormID with master index 0 remaps upward.
        let remap = FormIdRemap::regular(2, vec![1]);
        let b = parse_bptd(0x0000_001D, &subs, &Some(remap));
        assert_eq!(b.editor_id, "DefaultBodyPartData");
        assert_eq!(b.skeleton_model, "characters\\skeleton.nif");
        assert_eq!(b.parts.len(), 2);

        let head = &b.parts[0];
        assert_eq!(head.name, "Head");
        assert_eq!(head.node, "Bip01 Head");
        assert_eq!(head.vats_target_node, "Bip01 Head");
        assert_eq!(head.bpni, "GoreHead");
        assert_eq!(head.damage_mult, 2.0);
        assert_eq!(head.part_type, 1, "the body-location enum: head");
        assert_eq!(head.flags, 59, "severable | IK | explodable | is-head | headtracking");
        assert_eq!(head.to_hit_chance, 30);
        assert_eq!(head.actor_value, 25);
        assert_eq!(head.health_percent, 20);
        assert_eq!(head.explosion_chance, 95);
        assert_eq!(head.debris_count, 3);
        assert_eq!(head.generic_debris, 0x010A_0001);
        assert_eq!(head.explosion, 0x010B_0002);

        let torso = &b.parts[1];
        assert_eq!(torso.actor_value, 26);
        assert_eq!(torso.part_type, 0);
        assert_eq!(torso.flags, 8);
        assert_eq!(torso.to_hit_chance, 60);
        assert_eq!(torso.health_percent, 60);
        assert_eq!(torso.generic_debris, 0);
        assert_eq!(torso.explosion, 0);
    }

    /// #6465 — FO4's BPND is 101 bytes in a different layout. It must neither populate the fields
    /// nor reach `remap_fid`: `0x3F800000` (the float 1.0 at @12) is a mod-index-63 FormID that a
    /// one-master plugin logs as an out-of-range remap. The name run still lands.
    #[test]
    fn parse_bptd_leaves_an_fo4_sized_bpnd_undecoded() {
        let mut fo4 = Vec::new();
        fo4.extend_from_slice(&1.0f32.to_le_bytes());
        fo4.extend_from_slice(&[0u8; 8]);
        fo4.extend_from_slice(&1.0f32.to_le_bytes()); // @12: 0x3F800000 if misread as a FormID
        fo4.resize(101, 0);
        let subs = vec![sub(b"BPTN", b"Head\0"), sub(b"BPNN", b"Head\0"), sub(b"BPND", fo4)];
        let b = parse_bptd(0x0000_001D, &subs, &Some(FormIdRemap::regular(2, vec![1])));
        assert_eq!(b.parts.len(), 1);
        let part = &b.parts[0];
        assert_eq!(part.name, "Head");
        assert_eq!(part.damage_mult, 0.0, "undecoded, not read at the 84-byte offsets");
        assert_eq!(part.generic_debris, 0);
        assert_eq!(part.explosion, 0);
    }

    /// #6350 — short BPND payloads decode what they carry and default
    /// the rest: a modded record must parse, never fail.
    #[test]
    fn parse_bptd_tolerates_short_bpnd() {
        let subs = vec![
            sub(b"EDID", b"ModdedPart\0"),
            sub(b"BPTN", b"Head\0"),
            sub(b"BPNN", b"Bip01 Head\0"),
            sub(b"BPND", 1.5f32.to_le_bytes()), // 4 bytes: damage mult only
        ];
        let b = parse_bptd(0x0000_001D, &subs, &None);
        assert_eq!(b.parts.len(), 1);
        assert_eq!(b.parts[0].damage_mult, 1.5);
        assert_eq!(b.parts[0].to_hit_chance, 0);
        assert_eq!(b.parts[0].actor_value, 0);
        assert_eq!(b.parts[0].explosion, 0);
    }

    // ── #809 / FNV-D2-NEW-02 stubs ─────────────────────────────────
    #[test]
    fn parse_cobj_picks_edid_created_workbench() {
        // `RecipeStimpak` shape: EDID + CNAM (created stimpak form) +
        // BNAM (workbench filter form).
        let subs = vec![
            sub(b"EDID", b"RecipeStimpak\0"),
            sub(b"CNAM", 0x0014_4F10_u32.to_le_bytes()),
            sub(b"BNAM", 0x000A_6001_u32.to_le_bytes()),
        ];
        let c = parse_cobj(0x0014_F800, &subs, &None);
        assert_eq!(c.editor_id, "RecipeStimpak");
        assert_eq!(c.created_form, 0x0014_4F10);
        assert_eq!(c.workbench_form, 0x000A_6001);
    }
    #[test]
    fn parse_minimal_record_picks_edid_full() {
        let subs = vec![
            sub(b"EDID", b"MUSCMainTitle\0"),
            sub(b"FULL", b"Main Title Theme\0"),
            // Other sub-records ignored:
            sub(b"FNAM", b"music\\base\\maintitle.mp3\0"),
        ];
        let m = parse_minimal_esm_record(0x0001_5C8C, &subs);
        assert_eq!(m.form_id, 0x0001_5C8C);
        assert_eq!(m.editor_id, "MUSCMainTitle");
        assert_eq!(m.full_name, "Main Title Theme");
    }

    #[test]
    fn parse_minimal_record_handles_edid_only() {
        // Many long-tail records ship EDID only (no FULL). Stub must
        // tolerate the missing FULL without panic; full_name stays
        // empty.
        let subs = vec![sub(b"EDID", b"DOBJDefaultObject\0")];
        let m = parse_minimal_esm_record(0xDEAD_BEEF, &subs);
        assert_eq!(m.editor_id, "DOBJDefaultObject");
        assert!(m.full_name.is_empty());
    }

    // ── #966 / OBL-D3-NEW-02 — SLGM dedicated decode ────────────────

    #[test]
    fn parse_slgm_decodes_full_oblivion_layout() {
        // Synthetic Grand Soul Gem (pre-loaded with a Grand soul) —
        // exercises every sub-record arm the parser is supposed to
        // consume. DATA = value (i32) + weight (f32). SOUL = current
        // soul magnitude (0..=5). SLCP = capacity (0..=5).
        let mut data = Vec::with_capacity(8);
        data.extend_from_slice(&750_i32.to_le_bytes());
        data.extend_from_slice(&1.0_f32.to_le_bytes());

        let subs = vec![
            sub(b"EDID", b"SoulGemGrandFilled\0"),
            sub(b"FULL", b"Grand Soul Gem\0"),
            sub(b"MODL", b"Clutter\\SoulGems\\Grand.NIF\0"),
            sub(b"ICON", b"Clutter\\SoulGems\\Grand.dds\0"),
            sub(b"DATA", &data),
            sub(b"SOUL", [5]), // current = Grand
            sub(b"SLCP", [5]), // capacity = Grand
        ];
        let g = parse_slgm(0x0002_3F1B, &subs);
        assert_eq!(g.form_id, 0x0002_3F1B);
        assert_eq!(g.editor_id, "SoulGemGrandFilled");
        assert_eq!(g.full_name, "Grand Soul Gem");
        assert_eq!(g.model_path, "Clutter\\SoulGems\\Grand.NIF");
        assert_eq!(g.value, 750);
        assert!((g.weight - 1.0).abs() < f32::EPSILON);
        assert_eq!(g.current_soul, 5);
        assert_eq!(g.soul_capacity, 5);
    }

    #[test]
    fn parse_slgm_handles_empty_petty_gem_no_soul() {
        // Empty petty gem — no SOUL sub-record at all. Capacity comes
        // from SLCP. Pre-fix the audit text suggested reading capacity
        // from DATA byte 0; this test pins that the correct field is
        // SLCP and that absence of SOUL leaves `current_soul` at 0.
        let mut data = Vec::with_capacity(8);
        data.extend_from_slice(&25_i32.to_le_bytes());
        data.extend_from_slice(&0.1_f32.to_le_bytes());

        let subs = vec![
            sub(b"EDID", b"SoulGemPettyEmpty\0"),
            sub(b"DATA", &data),
            sub(b"SLCP", [1]), // petty capacity, no SOUL = empty gem
        ];
        let g = parse_slgm(0x0002_3F00, &subs);
        assert_eq!(g.value, 25);
        assert_eq!(g.current_soul, 0, "missing SOUL means an empty gem");
        assert_eq!(g.soul_capacity, 1);
    }

    #[test]
    fn parse_slgm_truncated_data_does_not_panic() {
        // 5-byte DATA (gated by the `>= 8` arm) → value/weight stay at
        // default. Empty SOUL / SLCP also tolerated.
        let subs = vec![
            sub(b"EDID", b"SoulGemMalformed\0"),
            sub(b"DATA", [1u8; 5]),
            sub(b"SOUL", []),
            sub(b"SLCP", [3]),
        ];
        let g = parse_slgm(0x0002_3F02, &subs);
        assert_eq!(g.value, 0, "short DATA must not bleed bytes into value");
        assert_eq!(g.current_soul, 0, "empty SOUL stays at default");
        assert_eq!(g.soul_capacity, 3);
    }
}

#[cfg(test)]
mod remap_tests {
    use super::*;
    use crate::esm::reader::GlobalSlot;
    use crate::esm::records::{parse_armo, parse_race};

    /// The remap shape of every shipped FO4 DLC master: one master
    /// (Fallout4.esm), so the plugin's own forms are authored `0x01…` while
    /// `allocate_global_slot` keys them `0x02…`.
    fn dlc_remap() -> Option<FormIdRemap> {
        Some(FormIdRemap {
            plugin_slot: GlobalSlot::Regular(0x02),
            master_slots: vec![GlobalSlot::Regular(0x00)],
            scheme: Default::default(),
        })
    }

    fn mk(typ: &[u8; 4], v: u32) -> SubRecord {
        crate::esm::records::test_support::sub(typ, v.to_le_bytes())
    }

    /// #3714 — walk the whole FO4 armor-equip chain under a non-identity
    /// remap: `RACE.WNAM` -> `ARMO.MODL` (armature list) -> `ARMA.RNAM`
    /// (race match). `EsmIndex.armor_addons` / `.items` / `.races` are keyed
    /// in GLOBAL space because `read_record_header` remaps every record's
    /// own FormID, so a raw embedded id misses every lookup.
    ///
    /// 326 of 408 armature references across the three shipped FO4 DLC
    /// masters are self-referential, so this is the majority case on any
    /// DLC load, and non-identity by construction for every ESL. Left raw,
    /// `resolve_armor_meshes` ended `Vec::new()` and DLC armor rendered as
    /// empty space — indistinguishable from an unshipped mesh.
    #[test]
    fn the_fo4_armor_equip_chain_is_remapped_into_global_space() {
        let remap = dlc_remap();

        // RACE.WNAM — the default-skin ARMO this plugin adds.
        let race = parse_race(
            0x0200_0001,
            &[mk(b"WNAM", 0x0100_1111)],
            GameKind::Fallout4,
            &remap,
        );
        assert_eq!(
            race.default_skin,
            Some(0x0200_1111),
            "RACE.WNAM is looked up in `EsmIndex.items`, which is global-keyed"
        );

        // ARMO.MODL — the armature list. One self-owned, one master-owned;
        // the master reference must pass through unchanged.
        let armo = parse_armo(
            0x0200_1111,
            &[mk(b"MODL", 0x0100_2222), mk(b"MODL", 0x0000_3333)],
            GameKind::Fallout4,
            &remap,
        );
        let crate::esm::records::ItemKind::Armor { armatures, .. } = &armo.kind else {
            panic!("ARMO must parse as ItemKind::Armor");
        };
        assert_eq!(*armatures, vec![0x0200_2222, 0x0000_3333]);

        // ARMA.RNAM — the race this addon fits. Matched against
        // `NPC_.RNAM`, which is already remapped, so leaving it raw failed
        // the race match and collapsed the actor into the single-valued
        // pass-2 fallback (#3357).
        let arma = parse_arma(
            0x0200_2222,
            &[
                mk(b"RNAM", 0x0100_0001),
                mk(b"MODL", 0x0100_0004),
                mk(b"MODL", 0x0000_0005),
            ],
            GameKind::Fallout4,
            &remap,
        );
        assert_eq!(arma.race_form_id, 0x0200_0001);
        assert_eq!(arma.additional_races, vec![0x0200_0004, 0x0000_0005]);

        // The chain closes: the race's skin resolves to the ARMO, whose
        // armature resolves to the ARMA, whose race matches the race.
        assert_eq!(race.default_skin, Some(armo.form_id));
        assert_eq!(armatures[0], arma.form_id);
        assert_eq!(arma.race_form_id, race.form_id);
    }

    /// A single-plugin vanilla load has an identity remap, which is why the
    /// defect never showed on `Fallout4.esm` alone. `None` must be a no-op.
    #[test]
    fn no_remap_leaves_the_chain_unchanged() {
        let race = parse_race(
            0x0000_0001,
            &[mk(b"WNAM", 0x0001_1111)],
            GameKind::Fallout4,
            &None,
        );
        assert_eq!(race.default_skin, Some(0x0001_1111));

        let armo = parse_armo(
            0x0001_1111,
            &[mk(b"MODL", 0x0001_2222)],
            GameKind::Fallout4,
            &None,
        );
        let crate::esm::records::ItemKind::Armor { armatures, .. } = &armo.kind else {
            panic!("ARMO must parse as ItemKind::Armor");
        };
        assert_eq!(*armatures, vec![0x0001_2222]);

        let arma = parse_arma(
            0x0001_2222,
            &[mk(b"RNAM", 0x0000_0001)],
            GameKind::Fallout4,
            &None,
        );
        assert_eq!(arma.race_form_id, 0x0000_0001);
    }

    /// #3715 — `parse_cobj` never took a remap at all. `CNAM`/`BNAM` are
    /// looked up against `EsmIndex.items`, which is global-keyed.
    #[test]
    fn cobj_cnam_and_bnam_are_remapped_into_global_space() {
        let remap = dlc_remap();
        let c = parse_cobj(
            0x0200_0001,
            &[mk(b"CNAM", 0x0100_1234), mk(b"BNAM", 0x0000_5678)],
            &remap,
        );
        assert_eq!(c.created_form, 0x0200_1234, "self-reference remaps");
        assert_eq!(c.workbench_form, 0x0000_5678, "master reference is unchanged");
    }
}
