//! Actor-related record parsers — NPC_, RACE, CLAS, FACT.
//!
//! NPC parsing pulls the essentials needed to spawn the NPC into the world:
//! base race/class form IDs, faction memberships, inventory list, and a
//! pointer to the head/body model. Every embedded FormID field on
//! `NpcRecord` is remapped to global load-order space at parse time
//! (`parse_npc`'s `remap` param), the same convention `parse_pack` /
//! `parse_qust` / `parse_perk` / `parse_avif` / `parse_dial` / `parse_info`
//! use — see #1996.

use super::common::{read_lstring_or_zstring, read_zstring, CommonNamedFields};
use crate::esm::reader::{FormIdRemap, GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;

/// One faction the NPC belongs to, with their rank within it.
#[derive(Debug, Clone, Copy)]
pub struct FactionMembership {
    pub faction_form_id: u32,
    pub rank: i8,
}

/// One inventory entry on an NPC (`CNTO` sub-record).
#[derive(Debug, Clone, Copy)]
pub struct NpcInventoryEntry {
    pub item_form_id: u32,
    pub count: i32,
}

/// One face-morph entry on an FO4 NPC. Built from a paired
/// `FMRI` / `FMRS` sub-record sequence — they appear alternating on the
/// wire (I, S, I, S, …) and pair 1-to-1.
///
/// `setting` is 9 floats laid out as `position[3]`, `rotation[3]`,
/// `scale[3]` per the morph target identified by `form_id`. Verified
/// against vanilla `Fallout4.esm` named-NPC records (Hancock, Piper,
/// MQ101 player duplicates) — the audit's claim that FMRS is a single
/// `f32` slider was stale; FMRS payloads are 36 bytes everywhere they
/// appear. See #591 / FO4-DIM6-06.
#[derive(Debug, Clone, Copy)]
pub struct NpcFaceMorph {
    /// FMRI — morph-target FormID (HDPT or face-morph-data form).
    pub form_id: u32,
    /// FMRS — 9 floats: position[3], rotation[3], scale[3].
    pub setting: [f32; 9],
}

/// ACBS flag bit 7 — "PC Level Mult".
///
/// When set, [`NpcRecord::level`] is **not** an absolute level: it is a
/// fixed-point multiplier applied to the player's level, clamped to the
/// record's `calcMin..=calcMax` band. Confirmed against vanilla data
/// (#2955): on `FalloutNV.esm` this bit and `level > 100` select the exact
/// same 268 records, and the out-of-range values are exclusively round steps
/// (`500`, `750`, … `2000`) rather than plausible levels.
///
/// Anything reading `level` as a level — the CHARAL population path, leveled
/// list expansion, XP curves — must consult this first, or a generic raider
/// reads as level 1000 and draws the top tier of every leveled list.
pub const ACBS_PC_LEVEL_MULT: u32 = 0x0080;

/// The actor's effective level for anything that treats level as a number.
///
/// #2955 — [`NpcRecord::level`] is overloaded on FO3/FNV: with the ACBS
/// [`ACBS_PC_LEVEL_MULT`] bit set it carries a fixed-point multiplier on the
/// *player's* level (vanilla values are round steps up to 2000), not an
/// absolute level. Feeding that raw into a leveled-list filter makes every
/// entry eligible, so the actor always draws the top tier; feeding it into an
/// XP curve asks for 150 050 XP instead of ~200.
///
/// The player-relative half is not modelled yet, so multiplier actors resolve
/// to their ACBS [`NpcRecord::calc_min`] — the record's own level floor, which
/// is game data rather than a derived guess. `calc_min` is `0` on records that
/// carry none, so the result is floored at 1: level 0 would make a leveled
/// list resolve to nothing at all, which trades over-levelled gear for no
/// gear.
///
/// The non-multiplier branch does NOT get that same floor: `0` is clamped only
/// up from negative (malformed/corrupt data), not up to `1`. Unlike
/// `calc_min`, a plain `level` of `0` is not a documented "record carries
/// none" sentinel — nothing distinguishes it from an authored `0`, so forcing
/// it to `1` would be inventing data the record never claimed to have. See
/// `pc_level_mult_actors_resolve_to_calc_min_not_the_raw_multiplier`'s
/// `negative` case for the pin.
///
/// #3081 / #3171 — the SINGLE source of truth, and it lives here, beside the
/// record it reads, precisely because the two prior copies drifted. The first
/// (`byroredux/src/inventory.rs`, `09682c71`) had already diverged to
/// `.max(1)` on the non-multiplier branch before anyone noticed and was
/// deleted by #3081; the second (`actor_value_derive.rs`'s
/// `effective_npc_level`, `b434e4c0`) carried the *same* `.max(1)` divergence
/// #3081 had explicitly rejected, and outlived that fix because the
/// regression test only ever called the original. Both call sides now import
/// this one, and the pin calls it through this crate so a future copy has
/// something to fail against.
#[must_use]
pub fn effective_actor_level(npc: &NpcRecord) -> i16 {
    if npc.acbs_flags & ACBS_PC_LEVEL_MULT != 0 {
        npc.calc_min.max(1) as i16
    } else {
        npc.level.max(0)
    }
}

/// FO4 NPC face-morph block. Set on `NpcRecord` only when at least one
/// of the underlying sub-records was present — most generic settler
/// NPCs ship none and stay at `None`. See #591 / FO4-DIM6-06.
#[derive(Debug, Clone, Default)]
pub struct NpcFaceMorphs {
    /// Paired FMRI + FMRS entries. The wire format alternates the two
    /// sub-records; the parser pairs them positionally and truncates
    /// to the shorter of the two arrays if a record is malformed.
    pub morphs: Vec<NpcFaceMorph>,
    /// MSDK — slider key FormIDs (parallel to `slider_values`).
    pub slider_keys: Vec<u32>,
    /// MSDV — slider values (parallel to `slider_keys`).
    pub slider_values: Vec<f32>,
    /// QNAM — RGB texture-lighting tint + alpha (4 × f32). The trailing
    /// component on every vanilla FO4 record sampled was `1.0`; preserve
    /// it verbatim for round-trip rather than dropping it.
    pub texture_lighting: Option<[f32; 4]>,
    /// HCLF — hair-color FormID.
    pub hair_color: Option<u32>,
    /// BCLF — body-color override FormID. Rare on vanilla; preserved
    /// when present so future renderer work doesn't have to re-walk the
    /// record.
    pub body_color: Option<u32>,
    /// PNAM — head-part FormIDs (one FormID per sub-record, multiple).
    pub head_parts: Vec<u32>,
}

impl NpcFaceMorphs {
    fn is_empty(&self) -> bool {
        self.morphs.is_empty()
            && self.slider_keys.is_empty()
            && self.slider_values.is_empty()
            && self.texture_lighting.is_none()
            && self.hair_color.is_none()
            && self.body_color.is_none()
            && self.head_parts.is_empty()
    }
}

/// Pre-FO4 NPC FaceGen recipe — the slider-array form that the
/// legacy engine evaluates at load time against the race base head
/// NIF + its `.egm` (geometry) / `.egt` (texture) / `.tri` (animated
/// targets) sidecars. Carried by Oblivion / Fallout 3 / Fallout NV
/// NPCs; FO4+ uses the typed-morph-target form in [`NpcFaceMorphs`]
/// instead.
///
/// **None** of the floats are validated at parse time — slider arrays
/// in the wild include negative values (under-the-norm features) and
/// values > 1 (exaggerated features). The evaluator (Phase 3b) is
/// responsible for whatever clamping the renderer wants.
///
/// The `eyebrow_form_id` slot captures FNV's actual `PNAM` semantic
/// (a single eyebrow HDPT FormID). Pre-fix, FNV `PNAM` was being
/// accumulated into [`NpcFaceMorphs::head_parts`] (the FO4 semantic),
/// which silently misclassified every FNV named NPC as having FO4
/// face-morph data.
#[derive(Debug, Clone)]
pub struct NpcFaceGenRecipe {
    /// FGGS — 50 symmetric morph weights (left-right mirrored sliders
    /// like nose-bridge-width, jaw-depth, eye-vertical). Indexed by
    /// position in the 50-slot table; the slot semantics are baked
    /// into the race's `.egm` file. Most NPCs carry the full 50; the
    /// parser pads or truncates to 50 if the on-disk count differs.
    pub fggs: [f32; 50],
    /// FGGA — 30 asymmetric morph weights (left-only / right-only
    /// features that FGGS can't express). Same indexing scheme.
    pub fgga: [f32; 30],
    /// FGTS — 50 texture-morph weights driving complexion / age-line
    /// / makeup deltas via the race's `.egt` file. Applied in the
    /// face-tint compositor (Phase 3c).
    pub fgts: [f32; 50],
    /// HCLR — RGB hair color (3 bytes, `r/g/b`). Some FNV records
    /// carry a 4th byte (alpha or padding); per UESP only the first
    /// 3 are authoritative, so the parser drops the tail.
    pub hair_color_rgb: Option<[u8; 3]>,
    /// HNAM — hair style FormID (HAIR record).
    pub hair_form_id: Option<u32>,
    /// LNAM — unused on FNV / FO3 vanilla; preserved as opaque u32 so
    /// future authors can wire it without revisiting the parser.
    pub unused_lnam: Option<u32>,
    /// ENAM — eyes FormID (EYES record).
    pub eyes_form_id: Option<u32>,
    /// PNAM — eyebrow HDPT FormID. `None` when the record carries no
    /// PNAM. **Note:** FO4 reuses the `PNAM` tag for a head-parts
    /// list (multiple sub-records), so the FO4 path captures it in
    /// [`NpcFaceMorphs::head_parts`] instead.
    pub eyebrow_form_id: Option<u32>,
}

impl Default for NpcFaceGenRecipe {
    fn default() -> Self {
        // `[f32; 50]` and `[f32; 30]` have no built-in `Default` impl
        // (the std blanket only covers arrays up to length 32), so the
        // derive can't synthesise one. Hand-roll it; all-zeros matches
        // the slider-array zero-default the legacy engine assumes for
        // any NPC that doesn't override a slot.
        Self {
            fggs: [0.0; 50],
            fgga: [0.0; 30],
            fgts: [0.0; 50],
            hair_color_rgb: None,
            hair_form_id: None,
            unused_lnam: None,
            eyes_form_id: None,
            eyebrow_form_id: None,
        }
    }
}

impl NpcFaceGenRecipe {
    fn is_empty(&self) -> bool {
        self.fggs.iter().all(|f| *f == 0.0)
            && self.fgga.iter().all(|f| *f == 0.0)
            && self.fgts.iter().all(|f| *f == 0.0)
            && self.hair_color_rgb.is_none()
            && self.hair_form_id.is_none()
            && self.unused_lnam.is_none()
            && self.eyes_form_id.is_none()
            && self.eyebrow_form_id.is_none()
    }
}

/// Byte length of a FO3 / FNV `CREA` `DATA` sub-record — see
/// [`CreatureStats`] for the field-by-field layout and its source.
pub const CREATURE_DATA_LEN: usize = 17;

/// FO3 / FNV `CREA` `DATA` — a creature's authored stat block.
///
/// Creature stats are **not** class-derived: `CREA` has no CLAS reference
/// at all (its `CNAM` names an `IPDS` or nothing — 0/1578 FNV and 0/533
/// FO3 records resolve one, #3383), so the `NPC_` auto-calc model does not
/// apply and this record *is* the source (#3390).
///
/// Layout from xEdit `Core/wbDefinitionsFNV.pas` `wbRecord(CREA, …)`
/// `wbStruct(DATA, …)`, byte-identical in `wbDefinitionsFO3.pas`:
///
/// ```text
/// {00} Type            u8   (0 Animal, 1 Mutated Animal, 2 Mutated Insect,
///                            3 Abomination, 4 Super Mutant, 5 Feral Ghoul,
///                            6 Robot, 7 Giant — wbCreatureTypeEnum)
/// {01} Combat Skill    u8
/// {02} Magic Skill     u8
/// {03} Stealth Skill   u8
/// {04} Health          i16
/// {06} (unused)        2 bytes
/// {08} Damage          i16
/// {10} Attributes      u8 × 7  (Strength, Perception, Endurance, Charisma,
///                               Intelligence, Agility, Luck)
/// ```
///
/// 17 bytes total. Verified against `FalloutNV.esm`
/// `VCrTier3GiantRadscorpionMedPers` (`00167EA7`), whose DATA reads
/// `02 41 32 32 96 00 00 00 3C 00 09 06 06 06 05 03 08` — Mutated Insect,
/// combat 65, health 150, damage 60, `S9 P6 E6 C6 I5 A3 L8`.
///
/// The `NPC_` `DATA` of the same games is a *different* struct (`i32` Base
/// Health + the same 7 attributes = 11 bytes, or 25 with the legacy unused
/// tail), which is why the parse arm keys on the exact 17-byte length.
///
/// [`CREATURE_DATA_LEN`] is the size the parse arm keys on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CreatureStats {
    /// `wbCreatureTypeEnum` discriminant. Retained verbatim — the engine
    /// has no creature-family taxonomy to translate it into yet.
    pub creature_type: u8,
    /// The three aggregate skill values that stand in for `NPC_`'s 13
    /// individual skills. Parsed because they are authored data; **not**
    /// derived into `ActorValues`, because FO3/FNV publish no `AVIF` these
    /// three map onto and inventing one would be a guess.
    pub combat_skill: u8,
    pub magic_skill: u8,
    pub stealth_skill: u8,
    /// Base Health, straight to the `Health` `AVIF`.
    pub health: i16,
    /// The creature's attack damage. Authored here rather than on a weapon
    /// (creatures fight unarmed); not an actor value in FO3/FNV, so it is
    /// parsed and left for a future combat consumer.
    pub damage: i16,
    /// S-P-E-C-I-A-L, in `AttributeSet::FALLOUT` order.
    pub attributes: [u8; 7],
}

#[derive(Debug, Clone, Default)]
pub struct NpcRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Model path (typically from MODL — head/body mesh, optional).
    ///
    /// **On `CREA` this is the creature's SKELETON**, not a body mesh —
    /// e.g. `Creatures\Rat\Skeleton.NIF`. The body meshes are in
    /// [`Self::body_part_models`]. See #2567.
    pub model_path: String,
    /// `CREA` `NIFZ` — the creature's body-part mesh list, as authored:
    /// bare filenames relative to [`Self::model_path`]'s directory (e.g.
    /// `Eyes.NIF`, `Head.NIF`, `Rat.NIF`, `Whiskers.NIF` beside
    /// `Creatures\Rat\Skeleton.NIF`). Empty for `NPC_`, which has no such
    /// sub-record — humanoid bodies come from the per-game canonical
    /// paths instead. See #2567.
    pub body_part_models: Vec<String>,
    /// True when this record came from a `CREA` group rather than `NPC_`.
    ///
    /// The two share `parse_npc` because they share almost every
    /// sub-record, but they do **not** share a spawn shape: creatures carry
    /// their own skeleton + part list and reference no `RACE` (Oblivion
    /// `CREA` `RNAM` is a 1-byte attack reach, not a race FormID), so the
    /// humanoid body/head/hair/eye recipe does not apply to them. See #2567.
    pub is_creature: bool,
    /// FO3 / FNV `CREA` `DATA` — the creature's own stat block (#3390).
    ///
    /// `None` for `NPC_` (whose `DATA` is a different, 11-byte struct) and
    /// for every other game. See [`CreatureStats`].
    pub creature_stats: Option<CreatureStats>,
    /// Race form ID (RNAM).
    pub race_form_id: u32,
    /// Class form ID (CNAM).
    pub class_form_id: u32,
    /// Voice type form ID.
    pub voice_form_id: u32,
    /// Faction memberships (`SNAM` sub-records).
    pub factions: Vec<FactionMembership>,
    /// Inventory list (`CNTO` sub-records).
    pub inventory: Vec<NpcInventoryEntry>,
    /// Default outfit FormID (Skyrim+ `DOFT`). Resolves to an `OTFT`
    /// record whose `INAM` array names the actor's default-equipped
    /// armor pieces. `None` on FO3 / FNV / Oblivion (those games
    /// equip from the inventory list directly). `None` on Skyrim+
    /// when the NPC ships no DOFT — generic settlers etc.
    pub default_outfit: Option<u32>,
    /// AI packages (`PKID` sub-records, in priority order).
    pub ai_packages: Vec<u32>,
    /// Death item leveled list (DEST in some games, INAM in others).
    pub death_item_form_id: u32,
    /// Base level (from DATA).
    ///
    /// **Overloaded on FO3/FNV.** When [`ACBS_PC_LEVEL_MULT`] is set in
    /// [`Self::acbs_flags`], this field is a fixed-point *multiplier* on the
    /// player's level, not an absolute level — vanilla values are round steps
    /// (`500`, `1000`, `2000`, …), and `1000` alone covers 184 FNV / 103 FO3
    /// records. Consumers must check the flag before treating it as a level;
    /// use [`Self::calc_min`] for those actors (#2955).
    pub level: i16,
    /// ACBS `calcMin` — the level floor for PC-level-multiplier actors.
    ///
    /// Meaningful only when [`ACBS_PC_LEVEL_MULT`] is set: the actor's real
    /// level is a function of the player's, clamped to `calc_min..=calc_max`.
    /// The player-relative half is not modelled yet, so the floor is the
    /// honest stand-in — it is the game's own data rather than a derived
    /// guess. `0` when the record does not carry one.
    pub calc_min: u16,
    /// Skyrim ACBS signed Magicka offset (`i16 @ 4`). Combined with the
    /// actor's race starting Magicka by the TES5 character ruleset. Parsed
    /// alongside Health even though combat does not consume it yet, so the
    /// three TES5 resource offsets stay one verified wire-layout unit.
    pub magicka_offset: i16,
    /// Skyrim ACBS signed Stamina offset (`i16 @ 6`). See
    /// [`Self::magicka_offset`].
    pub stamina_offset: i16,
    /// Skyrim ACBS signed Health offset (`i16 @ 20`). Combat combines this
    /// with [`RaceRecord::starting_health`] to seed the Health actor value.
    pub health_offset: i16,
    /// Disposition base (from ACBS — i16 at offset 20). FNV vanilla
    /// default is 50; values are signed so unfriendly NPCs can sit
    /// below 0. Reading the high byte was being dropped pre-#377,
    /// silently truncating any disposition outside 0..=127.
    pub disposition_base: i16,
    /// Flags (from ACBS).
    pub acbs_flags: u32,
    /// True when the NPC record carries a `VMAD` sub-record (Skyrim+
    /// Papyrus VM attached-script blob). Presence flag only; full
    /// decoding deferred to scripting-as-ECS work. See #369.
    pub has_script: bool,
    /// Pre-Skyrim `SCRI` attached-script FormID. References an SCPT
    /// record carrying compiled Obscript bytecode that runs against
    /// this actor at runtime (`OnLoad`, `OnHit`, `OnActivate` event
    /// handlers). `0` = no script attached (the common case for
    /// generic NPCs). 24 % of FO3 named NPCs (398 of 1,647), 27 % of
    /// FO3 creatures (148 of 533), and 27 % of FNV named NPCs (1,046
    /// of 3,816) author SCRI — Three Dog's broadcast triggers, Moira
    /// Brown's questline gates, the FNV companion wheel, every
    /// faction-leader reactive dialogue, etc. Skyrim+ NPCs use VMAD
    /// instead (see `has_script`). The two paths are mutually
    /// exclusive in vanilla content. See #1273.
    pub script_form_id: u32,
    /// Decoded `VMAD` script attachments + property bindings (Skyrim+).
    /// `None` when the record carries no `VMAD`; the presence flag is
    /// [`Self::has_script`]. Consumed by the M47.2 scripting-translation
    /// layer to fetch + decompile the attached `.pex`. See #369 / M47.2.
    pub script_instance: Option<super::script_instance::ScriptInstanceData>,
    /// FO4 face-morph block (FMRI/FMRS/MSDK/MSDV/QNAM/HCLF/BCLF/PNAM).
    /// `None` when the record carries no face-morph sub-records (most
    /// pre-FO4 NPCs and FO4 generic settlers). Driven by audit
    /// FO4-DIM6-06 / #591 — actual morph-target application is
    /// downstream of HDPT mesh linking + the skinning pipeline.
    ///
    /// **Parse-but-don't-consume gate (TD5-013):** gated on M41.0.5
    /// (GPU per-vertex morph runtime, Tier 5). The sibling field
    /// `runtime_facegen` IS consumed in `npc_spawn.rs:619` (M41.0
    /// Phase 3b); `face_morphs` unlocks when the `.tri`-morph weight
    /// application pass lands.
    pub face_morphs: Option<NpcFaceMorphs>,
    /// Pre-FO4 FaceGen recipe (FGGS/FGGA/FGTS slider arrays + HCLR /
    /// HNAM / LNAM / ENAM / PNAM). `None` when the record carries
    /// none of those sub-records. Mutually exclusive with
    /// [`face_morphs`] in vanilla content (no record carries both
    /// forms — they're per-game). M41.0 Phase 3b consumes the slider
    /// arrays against the race's `.egm` sidecar to deform the base
    /// head mesh per NPC.
    pub runtime_facegen: Option<NpcFaceGenRecipe>,
    /// FNV / FO3 `TPLT` template form ID — points at the NPC_ (or
    /// LVLN) this record inherits per-field data from. Vanilla
    /// `Lvl*` template NPCs (LvlGoodspringsPowderGanger,
    /// LvlNCRTrooper, etc.) author themselves as thin shells with
    /// every field-bearing subrecord missing and rely on TPLT +
    /// `template_flags` to pull race / class / inventory / AI / etc.
    /// from a base record. Sentinel `0` = no template (the common
    /// case for unique named NPCs).
    pub template_form_id: u32,
    /// FNV / FO3 template-inheritance bitmask from `ACBS` (u16 at
    /// offset 22). Each bit gates whether one category of fields is
    /// pulled from [`template_form_id`] at runtime:
    ///
    ///   * `0x0100` — **Use Inventory** (CNTO list). Empty on the
    ///     template host; pulled from `TPLT` at spawn time. Without
    ///     this resolution every Lvl* NPC spawns with no armor /
    ///     weapon / aid items.
    ///   * `0x0002` — **Use Stats** (SPECIAL / class / level). Consumed by
    ///     `equip::resolve_inherited_stats` (#2956) — without it, CHARAL
    ///     population read a templated shell's own (frequently wrong)
    ///     class/level and silently mis-derived a full SPECIAL + 15-skill
    ///     set.
    ///   * `0x0001` — **Use Traits** (race). Consumed by
    ///     `equip::resolve_inherited_traits` (#2956).
    ///   * `0x0004` Factions, `0x0008` Actor Effects, `0x0010` AI Data,
    ///     `0x0020` AI Packages, `0x0040` Model/Animation, `0x0080` Base
    ///     Data, `0x0200` Script, `0x0400` Def Pack List — parsed and
    ///     stored for the dispatcher; no consumer yet.
    pub template_flags: u16,
    /// FO4+ `PRPS` "Properties" — the actor's actor values stored as
    /// `(AVIF FormID, value)` pairs (8 bytes each on the wire; xEdit
    /// `wbObjectProperty`). SPECIAL is here as the Strength..Luck AVIF
    /// FormID + its value, alongside any other authored AV overrides.
    /// These are *already* the [`ActorValues::from_pairs`] shape — the
    /// FO4 arm of `derive_npc_actor_values` returns them verbatim. FormIDs
    /// are remapped to global load-order space at parse time (`parse_npc`'s
    /// `remap` param — see #1996), same as `factions` / `class_form_id`.
    /// Empty for pre-FO4 games and for FO4 NPCs that inherit all stats from
    /// `RACE`/template. Gated on [`GameKind::uses_actor_value_properties`].
    ///
    /// [`ActorValues::from_pairs`]: byroredux_core::ecs::components::ActorValues::from_pairs
    pub actor_value_props: Vec<(u32, f32)>,
    /// FO4+ `DNAM` baked `Calculated Health` (u16 @ 0). The engine stores
    /// an NPC's derived Health precomputed — NPCs do **not** run the
    /// player END/level curve (which is why the wiki Health formula is
    /// "player only"). `0` = absent (no live NPC has 0 base Health, so the
    /// sentinel is unambiguous and avoids an `Option` discriminant).
    pub calculated_health: u16,
    /// FO4+ `DNAM` baked `Calculated Action Points` (u16 @ 2). Same
    /// precomputed-derived treatment as [`Self::calculated_health`];
    /// `0` = absent.
    pub calculated_action_points: u16,
    /// Skyrim+ `PRKR` perks — `(PERK FormID, rank)` pairs. Each `PRKR`
    /// sub-record is 5 bytes on FO4 and 8 on Skyrim (u32 FormID + u8 rank,
    /// plus three unused bytes on Skyrim; xEdit `NPC_`); a `PRKZ` count
    /// precedes them but is a benign hint we skip. Populates a `Perks`
    /// component at spawn. Empty for Oblivion / FO3 / FNV, which ship no
    /// `PRKR` at all (censused — see [`GameKind::uses_npc_perk_entries`],
    /// the gate this is read under since #3158).
    pub perks: Vec<(u32, u8)>,
}

#[derive(Debug, Clone, Default)]
pub struct RaceRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// Skill bonuses: `(skill_index, bonus)` pairs, with `0xFF` (None)
    /// slots dropped. Oblivion / FO3 / FNV author 8 pairs of the 0x0C..=0x20
    /// `SkillIndex` values; Skyrim authors 7 pairs of its own actor-value
    /// skill indices (6 = One-Handed … 23 = Enchanting).
    ///
    /// Always empty for FO4 / FO76 / Starfield: those games have no skills,
    /// and their RACE `DATA` carries no bonus array at all (#2455).
    ///
    /// Pre-#967 this field was typed as `Vec<(u32, i8)>` under the
    /// premise that the bonus was form-keyed by `AVIF` reference.
    /// OpenMW's `esm4/loadrace.cpp:135-153` (the canonical TES4 /
    /// FO3 / FONV reader) reads `(u8 skill_index, u8 bonus) × 8`
    /// from a 36-byte DATA — confirmed by the `subHdr.dataSize == 36`
    /// gate they ship against vanilla content. Our previous wider
    /// type was reading 5-byte strides through a 16-byte payload
    /// and surfacing garbage `form_id` values.
    pub skill_bonuses: Vec<(u8, i8)>,
    /// Body part model paths (head, body, hand, foot).
    pub body_models: Vec<String>,
    /// `INDX` + `MODL` head-part pairs from the RACE **head** section
    /// (the run opened by `NAM0` and closed by `NAM1`). Each entry is
    /// `(head_part_index, mesh_path, gender_section)`; the index is
    /// the game's own numbering, so resolve it through
    /// [`head_part::index_of`] rather than by literal value — Oblivion
    /// and FO3 / FNV disagree from `Mouth` down.
    ///
    /// `gender_section` tracks which RACE-record section the part
    /// was authored in:
    ///
    ///   * `None`  — shared across both genders (entries before any
    ///     MNAM/FNAM marker). Every Oblivion entry lands here: its
    ///     head section is one ungendered run, and the male/female
    ///     split lives in the *ear* index instead.
    ///   * `Some(0)` — male-only (after an `MNAM` section marker).
    ///   * `Some(1)` — female-only (after `FNAM`). FO3 / FNV author
    ///     the entire head section twice, once per gender — including
    ///     the head itself, so the female head is `Some(1)` at index
    ///     `Head`, not a `None` entry (#3418).
    ///
    /// Without this typed pairing the spawner can't tell which
    /// `body_models` entry is which body part — or which gender it
    /// applies to. Pre-fix every NPC rendered with just the head NIF —
    /// no eyes, mouth, teeth, tongue. Populated on Oblivion and
    /// FO3 / FNV; empty on Skyrim+, which moved head parts out to
    /// standalone `HDPT` records.
    ///
    /// The body section past `NAM1` restarts `INDX` at 0 for a
    /// different vocabulary and is deliberately **not** collected here
    /// (#3419) — it is still appended to [`Self::body_models`], which
    /// stays a flat append-ordered list of every `MODL` in the record
    /// and therefore also carries `.egt` texture paths.
    pub head_parts: Vec<(u32, String, Option<u8>)>,
    /// Default body height per gender, from DATA — `(male, female)`.
    /// Vanilla values run ~0.8..1.2 (Skyrim `NordRace` is 1.03; FO4
    /// `HumanChildRace` is 0.825). Decoded for Oblivion / FO3 / FNV
    /// (offset 16), Skyrim (offset 16 after its 7 skill pairs), and
    /// FO4 / FO76 (offset 0) — see `parse_race` for the three layouts.
    /// Stays `(1.0, 1.0)` when the game ships no RACE `DATA` at all
    /// (Starfield) or the sub-record is too short.
    pub base_height: (f32, f32),
    /// Default body weight per gender, from DATA — `(male, female)`.
    /// Vanilla values typically `(1.0, 1.0)`.
    ///
    /// Populated for Oblivion / FO3 / FNV and Skyrim. **Not** populated for
    /// FO4 / FO76: those encode a three-axis (thin / muscular / large) morph
    /// per gender that this two-field pair cannot represent, and the bytes
    /// are invariant across every shipped race so no reading of them can be
    /// validated against the data (#2455).
    pub base_weight: (f32, f32),
    /// `RACE_FLAGS` u32 from DATA (offset 32 on every decoded layout).
    /// Bit 0 = Playable. Left at `0` for FO4 / FO76, whose flag semantics
    /// are unverified — their non-playable `HumanChildRace` sets bit 0,
    /// contradicting the TES-lineage meaning (#2455).
    /// Other bits are documented per game (`BeastRace`, `Swims`,
    /// `Flies` — vanilla Oblivion uses bit 0 + bit 2 = 0x05 for
    /// playable beast-race overrides).
    pub race_flags: u32,
    /// TES5 RACE `DATA` starting Health (`f32 @ 36`). `None` for games whose
    /// layout has not been modelled and for malformed/non-finite values.
    pub starting_health: Option<f32>,
    /// TES5 RACE `DATA` starting Magicka (`f32 @ 40`).
    pub starting_magicka: Option<f32>,
    /// TES5 RACE `DATA` starting Stamina (`f32 @ 44`).
    pub starting_stamina: Option<f32>,
    /// Per-gender base attributes from the Oblivion-only `ATTR`
    /// sub-record. 8 attributes per gender (Strength / Intelligence
    /// / Willpower / Agility / Speed / Endurance / Personality /
    /// Luck) × 2 = 16 bytes total. `None` outside Oblivion.
    pub base_attributes: Option<RaceAttributes>,
    /// Default hair form IDs from the Oblivion `DNAM` sub-record —
    /// `(male, female)`. `None` when DNAM is absent (FO3 / FNV /
    /// Skyrim use a different default-hair mechanism).
    pub default_hair: Option<(u32, u32)>,
    /// Default voice form IDs from the Oblivion `VNAM` sub-record —
    /// `(male, female)`. `None` when VNAM is absent or has the
    /// TES5 4-byte shape.
    pub voice_forms: Option<(u32, u32)>,
    /// FaceGen main clamp from `PNAM` (1 × f32). Vanilla value is
    /// 5.0; `None` when the sub-record is absent.
    pub facegen_main_clamp: Option<f32>,
    /// FaceGen face clamp from `UNAM` (1 × f32). Vanilla value is
    /// 3.0; `None` when the sub-record is absent.
    pub facegen_face_clamp: Option<f32>,
    /// Race-vs-race disposition adjustments from repeated `XNAM`
    /// sub-records — each pair is `(other_race_form_id, adjustment)`.
    /// Drives the Radiant-AI faction-mood calculation for Oblivion
    /// NPCs interacting across racial lines.
    pub race_reactions: Vec<(u32, i32)>,
    /// Default "naked skin" ARMO form ID from the Skyrim+ `WNAM`
    /// sub-record — the race's implicit base-layer armor every actor
    /// wears beneath OTFT/CNTO gear. `None` outside `uses_prebaked_
    /// facegen()` games (Skyrim/FO4/FO76/Starfield) or when the RACE
    /// omits `WNAM` (see #2093 / SKY-D3-NEW-01: without this, a
    /// prebaked NPC whose equipped gear doesn't cover a biped region
    /// has zero mesh source for it — the FaceGeom NIF bakes head-only
    /// geometry, no body).
    pub default_skin: Option<u32>,
}

/// RACE `INDX` head-part identifiers, resolved by semantic role.
///
/// The raw `INDX` number is **not** portable across games: Oblivion
/// numbers a nine-entry table that carries a separate male and female
/// ear slot, while FO3 / FNV drop the second ear (their head section is
/// already split by `MNAM` / `FNAM`) and every role from Mouth down
/// shifts one lower. Both tables were dumped from vanilla content:
///
/// | role         | Oblivion | FO3 / FNV |
/// |--------------|----------|-----------|
/// | Head         | 0        | 0         |
/// | Ear (male)   | 1        | 1         |
/// | Ear (female) | 2        | 1         |
/// | Mouth        | 3        | 2         |
/// | Teeth lower  | 4        | 3         |
/// | Teeth upper  | 5        | 4         |
/// | Tongue       | 6        | 5         |
/// | Left eye     | 7        | 6         |
/// | Right eye    | 8        | 7         |
///
/// (`Oblivion.esm` `Imperial` `00000907` — `HeadHuman` 0, `EarsHuman`
/// 1 + 2, `MouthHuman` 3, `TeethLowerHuman` 4, `TeethUpperHuman` 5,
/// `TongueHuman` 6, `EyeLeftHuman` 7, `EyeRightHuman` 8;
/// `FalloutNV.esm` `CaucasianOldAged` `000987DF` — `HeadOld` 0, ear
/// ICON-only 1, `MouthHuman` 2, `TeethLowerHuman` 3,
/// `TeethUpperHuman` 4, `TongueHuman` 5, `EyeLeftHuman` 6,
/// `EyeRightHuman` 7.) UESP's `RACE_HeadPart` table documents the
/// Oblivion numbering only; the FO3 / FNV shift is why the spawner
/// must ask by [`Role`] rather than by literal index. Pre-#3420 the
/// spawner hard-coded 6 / 7 for the eyes, which on the Oblivion arm
/// selected the *tongue* and the *left eye* — and then painted the
/// NPC's eye texture over both.
pub mod head_part {
    use crate::esm::reader::GameKind;

    /// A head sub-mesh slot, independent of the per-game `INDX`
    /// numbering. [`index_of`] maps one to the number a given game's
    /// RACE record actually authors.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Role {
        Head,
        EarMale,
        EarFemale,
        Mouth,
        TeethLower,
        TeethUpper,
        Tongue,
        LeftEye,
        RightEye,
    }

    /// The head sub-parts the NPC spawner mounts on the skeleton
    /// beside the base head NIF. Ears are game-dependent (FNV models
    /// them into the head mesh and authors only an `ICON` at index 1;
    /// Oblivion ships a real `EarsHuman.nif`), so they are resolved
    /// per gender by the caller rather than listed here.
    pub const ORAL_ROLES: [Role; 4] = [
        Role::Mouth,
        Role::TeethLower,
        Role::TeethUpper,
        Role::Tongue,
    ];

    /// The `INDX` value a game authors for `role`, or `None` when the
    /// game's RACE record has no head-part table at all (Skyrim+ moved
    /// head parts out to standalone `HDPT` records).
    pub fn index_of(game: GameKind, role: Role) -> Option<u32> {
        match game {
            GameKind::Oblivion => Some(match role {
                Role::Head => 0,
                Role::EarMale => 1,
                Role::EarFemale => 2,
                Role::Mouth => 3,
                Role::TeethLower => 4,
                Role::TeethUpper => 5,
                Role::Tongue => 6,
                Role::LeftEye => 7,
                Role::RightEye => 8,
            }),
            GameKind::Fallout3NV => Some(match role {
                Role::Head => 0,
                // One ear slot for both genders — the gender split is
                // the MNAM / FNAM section, not the index.
                Role::EarMale | Role::EarFemale => 1,
                Role::Mouth => 2,
                Role::TeethLower => 3,
                Role::TeethUpper => 4,
                Role::Tongue => 5,
                Role::LeftEye => 6,
                Role::RightEye => 7,
            }),
            GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield => {
                None
            }
        }
    }
}

/// Per-gender attribute block for `RaceRecord.base_attributes`
/// (Oblivion `ATTR` sub-record). 8 attributes × 2 genders = 16 bytes.
#[derive(Debug, Clone, Default)]
pub struct RaceAttributes {
    pub male: GenderedAttributes,
    pub female: GenderedAttributes,
}

#[derive(Debug, Clone, Default)]
pub struct GenderedAttributes {
    pub strength: u8,
    pub intelligence: u8,
    pub willpower: u8,
    pub agility: u8,
    pub speed: u8,
    pub endurance: u8,
    pub personality: u8,
    pub luck: u8,
}

#[derive(Debug, Clone, Default)]
pub struct ClassRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// 7 base SPECIAL attribute values (Strength, Perception, Endurance,
    /// Charisma, Intelligence, Agility, Luck), each 0–10.
    ///
    /// FNV/FO3 source: the **`ATTR` subrecord** (fopdoc `CLAS`) — one
    /// 7-byte struct on FNV, seven single-byte `ATTR` subrecords on FO3
    /// (same order). These are **absolute base attributes, not weights**:
    /// an auto-calc NPC adopts its class's base attributes as its SPECIAL,
    /// from which skills derive (#1663). The FNV `DATA` subrecord carries
    /// only the tag skills + flags/services (28 bytes, no attributes) —
    /// the pre-#1663 reader looked for them at `DATA[28..35]`, a layout
    /// that never matched real 28-byte FNV `DATA`. `[0; 7]` on Oblivion
    /// (which uses [`Self::primary_attributes`] + [`Self::specialization`])
    /// and whenever no `ATTR` is present.
    pub base_attributes: [u8; 7],
    /// Tag skill form IDs from FNV DATA. Empty on Oblivion (see
    /// [`Self::major_skills`] for the analogous field).
    pub tag_skills: Vec<u32>,
    /// Oblivion-only: 2 × u32 primary attribute indices (0=Strength
    /// .. 7=Luck per OpenMW's `SkillIndex` neighbour set). Read from
    /// bytes 0..8 of the 52-byte DATA. `None` outside Oblivion.
    pub primary_attributes: Option<(u32, u32)>,
    /// Oblivion-only: u32 specialization at DATA offset 8.
    /// `0 = Combat`, `1 = Magic`, `2 = Stealth`. `None` outside Oblivion.
    pub specialization: Option<u32>,
    /// Oblivion-only: 7 × u32 major skill indices (`SkillIndex` enum
    /// values 0x0C..=0x20) at DATA offset 12..40. Empty outside
    /// Oblivion.
    ///
    /// The audit description (#968) at filing time said "14 × u32",
    /// but its own test assertion said `len() == 7`. Empirical probe
    /// against vanilla `Oblivion.esm` confirms 7 majors: every CLAS
    /// DATA sub-record is exactly 52 bytes, and Knight (form 0x836)
    /// decodes as `[Block, Illusion, HeavyArmor, Blunt, Blade,
    /// Speechcraft, HandToHand]`.
    pub major_skills: Vec<u32>,
    /// Oblivion-only: u32 race-class flags at DATA offset 40. Bit 0
    /// = Playable. `None` outside Oblivion (the FNV 35-byte arm
    /// reads its own `flags` into a different position; not split
    /// out here because it's not a current consumer).
    ///
    /// Parsed and real-data-verified (`clas_oblivion_knight_against_vanilla`)
    /// but intentionally has **no production consumer yet** — this is
    /// forward-sequencing for CHARAL (the per-game character-rules
    /// abstraction layer, `docs/engine/charal.md`), whose Oblivion
    /// class-flag pass is the consumer (e.g. a playable/spellmaking-
    /// eligibility gate). Flagged here so it reads as deliberate, not
    /// as a rediscovered "surprise" gap. See #2089 (DIM3-OBL-02).
    pub flags_oblivion: Option<u32>,
}

/// Faction-to-faction relation.
#[derive(Debug, Clone, Copy)]
pub struct FactionRelation {
    pub other_faction: u32,
    /// Modifier (-100..100, larger means more friendly).
    pub modifier: i32,
    /// Combat reaction (0=neutral, 1=enemy, 2=ally, 3=friend).
    ///
    /// Stored at the on-disk width (`u32` per UESP), not narrowed to the
    /// vanilla 0..=3 range. Pre-#3339 this was `u8` while the parser read
    /// a full `u32` and cast — so the `as u8` truncated back to exactly the
    /// eight bits the pre-#482 `sub.data[8]` read had, defeating the stated
    /// purpose of the wider read. Vanilla FNV never exceeds 3 (all 1,314
    /// `XNAM` sub-records are 12 bytes; values `{0: 179, 1: 264, 2: 472,
    /// 3: 399}`), so no live data changes — this only makes the field
    /// honour what the parser actually decodes.
    pub combat_reaction: u32,
}

#[derive(Debug, Clone)]
pub struct FactionRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Hidden flag etc. (from DATA).
    pub flags: u32,
    pub relations: Vec<FactionRelation>,
    /// Rank index → label.
    pub ranks: Vec<String>,
    /// `REPU` FormID this faction's standing moves, from `WMI1` (#3325).
    ///
    /// FNV replaces FO3's single global karma with per-faction reputation,
    /// and this sub-record is the **only** edge from a faction to the meter
    /// it moves — without it `EsmIndex::reputations` is an orphan map that no
    /// runtime can key off, so vendor pricing, disguise reactions, quest
    /// branching and hostile/idolized greetings have no input.
    ///
    /// Byte-proven rather than assumed: all 46 `FACT` `WMI1` payloads in
    /// `FalloutNV.esm` resolve to a real `REPU` record against a whole-file
    /// FormID→type map (100%, alongside 36 more on `REFR`). `None` on every
    /// other game — no other title in the corpus authors `WMI1`.
    pub reputation: Option<u32>,
}

// ── Parsers ───────────────────────────────────────────────────────────

/// Remap a raw plugin-local FormID to global space, leaving 0 (no
/// FormID / null ref) untouched. Same convention as `misc/ai.rs`'s
/// `remap_fid` — kept local rather than shared since neither module
/// depends on the other's record types.
fn remap_fid(raw: u32, remap: &Option<FormIdRemap>) -> u32 {
    if raw == 0 {
        return 0;
    }
    remap.as_ref().map_or(raw, |r| r.remap(raw))
}

pub fn parse_npc(
    form_id: u32,
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> NpcRecord {
    // The FMRI/FMRS/MSDK/MSDV/QNAM/HCLF/BCLF face-morph block was
    // introduced in FO4. FNV/FO3/Skyrim NPCs ship none of those
    // sub-records — and crucially, FNV `PNAM` carries a single
    // eyebrow HDPT FormID, NOT a head-part list, so accumulating it
    // into `face.head_parts` (the FO4 semantic) was a silent
    // mis-classification pre-fix. The two paths are mutually
    // exclusive in vanilla content; both arms key off `GameKind`
    // semantic predicates so adding new games extends the table at
    // one site.
    let captures_fo4_face = game.uses_prebaked_facegen();
    let captures_runtime_facegen = game.has_runtime_facegen_recipe();
    let captures_av_props = game.uses_actor_value_properties();
    // #3158 — strictly wider than `captures_av_props`: Skyrim ships
    // `PRKR` on 1620 of its 5118 `NPC_` records but predates the AVIF
    // property model. See `GameKind::uses_npc_perk_entries`.
    let captures_perks = game.uses_npc_perk_entries();
    // EDID / FULL / MODL / VMAD shared with every named record — drain
    // them through the helper so the per-record loop below only carries
    // NPC-specific subrecords. TD3-203 / #1113.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    let mut record = NpcRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        model_path: common.model_path,
        body_part_models: Vec::new(),
        is_creature: false,
        creature_stats: None,
        race_form_id: 0,
        class_form_id: 0,
        voice_form_id: 0,
        factions: Vec::new(),
        inventory: Vec::new(),
        default_outfit: None,
        ai_packages: Vec::new(),
        death_item_form_id: 0,
        level: 1,
        calc_min: 0,
        magicka_offset: 0,
        stamina_offset: 0,
        health_offset: 0,
        disposition_base: 50,
        acbs_flags: 0,
        has_script: common.has_script,
        script_form_id: 0,
        script_instance: common.script_instance,
        face_morphs: None,
        runtime_facegen: None,
        template_form_id: 0,
        template_flags: 0,
        actor_value_props: Vec::new(),
        calculated_health: 0,
        calculated_action_points: 0,
        perks: Vec::new(),
    };
    // FMRI and FMRS are collected separately and zipped after the walk
    // since they appear alternating on the wire and we don't want to
    // assume a strict ordering inside the sub-record list.
    let mut fmri_forms: Vec<u32> = Vec::new();
    let mut fmrs_settings: Vec<[f32; 9]> = Vec::new();
    let mut face = NpcFaceMorphs::default();
    let mut recipe = NpcFaceGenRecipe::default();

    // Slim dispatch loop (#2055): each sub-record is offered to the
    // relevant per-group helper. The groups are keyed on disjoint
    // sub-record tags — the only shared tag is `PNAM`, split between the
    // runtime-FaceGen and FO4 face-morph helpers, which are mutually
    // exclusive per `GameKind` (`captures_runtime_facegen` vs
    // `captures_fo4_face`). So at most one helper acts on any given sub,
    // preserving the single-match-arm-wins semantics of the pre-split
    // form. The per-arm `captures_*` guards moved up to these gates.
    for sub in subs {
        parse_npc_core(&mut record, sub, game, remap);
        if captures_runtime_facegen {
            parse_npc_runtime_facegen(&mut recipe, sub, remap);
        }
        if captures_fo4_face {
            parse_npc_fo4_facemorph(&mut face, &mut fmri_forms, &mut fmrs_settings, sub, remap);
        }
        if captures_av_props {
            parse_npc_actor_values(&mut record, sub, remap);
        }
        if captures_perks {
            parse_npc_perks(&mut record, sub, remap);
        }
    }

    // Pair FMRI + FMRS positionally. Truncation to the shorter length
    // is the defensive choice — Bethesda's authoring tool emits paired
    // sub-records, but a malformed mod could ship one without the
    // other, and silently dropping the unpaired tail is preferable to
    // panicking the cell load.
    let pair_count = fmri_forms.len().min(fmrs_settings.len());
    if fmri_forms.len() != fmrs_settings.len() {
        log::debug!(
            "NPC {form_id:08X}: FMRI/FMRS count mismatch ({} vs {}); pairing first {}",
            fmri_forms.len(),
            fmrs_settings.len(),
            pair_count,
        );
    }
    // MILESTONE: M41.0.5 (per-vertex morph runtime) — see #1057.
    // FMRI + FMRS pairs decoded here populate `face_morphs.morphs`
    // (typed-morph-target form). `byroredux/src/npc_spawn.rs` ignores
    // the array today; FaceGen Phase 4 (#794 family) only consumed the
    // `runtime_facegen` recipe path. Wire when the per-vertex morph
    // GPU runtime lands.
    for i in 0..pair_count {
        face.morphs.push(NpcFaceMorph {
            form_id: fmri_forms[i],
            setting: fmrs_settings[i],
        });
    }

    if !face.is_empty() {
        record.face_morphs = Some(face);
    }
    if !recipe.is_empty() {
        record.runtime_facegen = Some(recipe);
    }

    record
}

/// Identity, faction, inventory and actor-configuration sub-records
/// shared by every NPC_/CREA record regardless of game era. Split out
/// of [`parse_npc`] (#2055); each match arm is preserved verbatim,
/// including its length guard and `remap` remapping. Tags handled here
/// are disjoint from the FaceGen / actor-value helpers.
fn parse_npc_core(
    record: &mut NpcRecord,
    sub: &SubRecord,
    game: GameKind,
    remap: &Option<FormIdRemap>,
) {
    match &sub.sub_type {
        b"RNAM" if sub.data.len() >= 4 => {
            let raw = SubReader::new(&sub.data).u32_or_default();
            record.race_form_id = remap_fid(raw, remap);
        }
        b"CNAM" if sub.data.len() >= 4 => {
            let raw = SubReader::new(&sub.data).u32_or_default();
            record.class_form_id = remap_fid(raw, remap);
        }
        b"VTCK" if sub.data.len() >= 4 => {
            let raw = SubReader::new(&sub.data).u32_or_default();
            record.voice_form_id = remap_fid(raw, remap);
        }
        // SCRI — pre-Skyrim attached-script FormID. NPC_ + CREA
        // share `parse_npc` so this arm covers both. See #1273.
        b"SCRI" if sub.data.len() >= 4 => {
            let raw = SubReader::new(&sub.data).u32_or_default();
            record.script_form_id = remap_fid(raw, remap);
        }
        // NIFZ — CREA-only body-part mesh list (#2567). A run of
        // null-terminated names with a trailing empty terminator, each
        // relative to MODL's directory:
        //
        //   "Eyes.NIF\0Head.NIF\0mange.NIF\0Rat.NIF\0Whiskers.NIF\0\0"
        //
        // (verified against `Oblivion.esm`'s SE11SanctifiedRat). Without
        // these a creature has only its skeleton, which carries no
        // geometry — the animated actor path would spawn an invisible
        // actor. `NPC_` never ships NIFZ, so this arm is self-gating.
        b"NIFZ" if !sub.data.is_empty() => {
            record.body_part_models.extend(
                sub.data
                    .split(|&b| b == 0)
                    .filter(|part| !part.is_empty())
                    .map(|part| String::from_utf8_lossy(part).into_owned()),
            );
        }
        // #3390 — FO3 / FNV `CREA` `DATA`: the creature's own stat block,
        // the only source of actor values for the whole bestiary. Layout
        // and its provenance are documented on [`CreatureStats`].
        //
        // Keyed on the exact 17-byte length rather than on the record
        // group, because `parse_npc` is shared by `NPC_` and `CREA` and
        // does not know which one it read (`is_creature` is stamped by the
        // dispatcher afterwards). The two `DATA` structs are unambiguous by
        // size in these games: `NPC_` authors 11 bytes (`i32` Base Health +
        // 7 attributes), or 25 with the legacy unused tail. Neither is 17.
        b"DATA" if matches!(game, GameKind::Fallout3NV) && sub.data.len() == CREATURE_DATA_LEN => {
            let mut r = SubReader::new(&sub.data);
            let creature_type = r.u8_or_default();
            let combat_skill = r.u8_or_default();
            let magic_skill = r.u8_or_default();
            let stealth_skill = r.u8_or_default();
            let health = r.u16_or_default() as i16;
            let _unused = r.u16_or_default();
            let damage = r.u16_or_default() as i16;
            let mut attributes = [0u8; 7];
            for slot in &mut attributes {
                *slot = r.u8_or_default();
            }
            record.creature_stats = Some(CreatureStats {
                creature_type,
                combat_skill,
                magic_skill,
                stealth_skill,
                health,
                damage,
                attributes,
            });
        }
        // SNAM (FNV NPC_): faction form ID (u32) + rank (i8) + pad x3
        b"SNAM" if sub.data.len() >= 8 => {
            let mut r = SubReader::new(&sub.data);
            let faction = r.u32_or_default();
            let rank = r.u8_or_default() as i8;
            record.factions.push(FactionMembership {
                faction_form_id: remap_fid(faction, remap),
                rank,
            });
        }
        // CNTO: shared with CONT (size const lives on InventoryEntry, #1631)
        b"CNTO" if sub.data.len() >= super::container::InventoryEntry::WIRE_SIZE => {
            let mut r = SubReader::new(&sub.data);
            let item = r.u32_or_default();
            record.inventory.push(NpcInventoryEntry {
                item_form_id: remap_fid(item, remap),
                count: r.i32_or_default(),
            });
        }
        b"PKID" if sub.data.len() >= 4 => {
            let raw = SubReader::new(&sub.data).u32_or_default();
            record.ai_packages.push(remap_fid(raw, remap));
        }
        // DOFT — Skyrim+ default outfit FormID. Pre-Skyrim games
        // don't emit DOFT (NPCs equip directly from inventory).
        // Stored as Option so the equip pipeline can dispatch on
        // presence without ambiguity vs the null-form sentinel.
        b"DOFT" if sub.data.len() >= 4 => {
            record.default_outfit = SubReader::new(&sub.data)
                .u32()
                .ok()
                .map(|raw| remap_fid(raw, remap));
        }
        b"INAM" if sub.data.len() >= 4 => {
            let raw = SubReader::new(&sub.data).u32_or_default();
            record.death_item_form_id = remap_fid(raw, remap);
        }
        // TPLT — FNV / FO3 template-inheritance pointer. Vanilla
        // Lvl* NPCs author this and rely on `template_flags` (in
        // ACBS) to pull per-field categories from the referenced
        // base. See `NpcRecord::template_form_id` for the bitmap.
        b"TPLT" if sub.data.len() >= 4 => {
            let raw = SubReader::new(&sub.data).u32_or_default();
            record.template_form_id = remap_fid(raw, remap);
        }
        // Oblivion ACBS (NPC_ / CREA Configuration) is a fixed
        // 16-byte layout with NO disposition or template-flags field:
        //   flags(u32)@0, baseSpell(u16)@4, fatigue(u16)@6,
        //   barterGold(u16)@8, level(i16)@10, calcMin(u16)@12,
        //   calcMax(u16)@14.
        // 16 < 24 so it never reaches the FNV/FO3 arm below — gate on
        // GameKind here. Verified by byte-decode over vanilla
        // Oblivion.esm (all 914 NPC_/CREA ACBS are exactly 16 bytes).
        // Without this every Oblivion actor kept level=1 / acbs_flags=0
        // → wrong leveled-list tier + every actor resolved Male. #1650.
        b"ACBS" if matches!(game, GameKind::Oblivion) && sub.data.len() >= 16 => {
            let mut r = SubReader::new(&sub.data);
            record.acbs_flags = r.u32_or_default();
            r.skip_or_eof(6); // baseSpell(u16) + fatigue(u16) + barterGold(u16)
            record.level = r.i16_or_default();
            // calcMin@12 (#2955).
            record.calc_min = r.u16_or_default();
            // disposition_base / template_flags stay at their
            // constructor defaults — Oblivion ACBS carries neither.
        }
        // Fallout 4 ACBS is a distinct 20-byte layout (xEdit
        // `wbDefinitionsFO4.pas`, NPC_ Configuration):
        //   flags(u32), xp_offset(i16), level(i16), calc_min(u16),
        //   calc_max(u16), disposition(i16), template_flags(u16),
        //   bleedout_override(u16), unknown(u16).
        //
        // It must precede the 24-byte FNV arm. Pre-fix the generic guard
        // rejected every vanilla FO4 NPC ACBS, leaving flags=0 (all actors
        // resolved Male), level=1, and template_flags=0. Female-only outfit
        // ARMAs then had no male mesh to attach, producing floating heads.
        b"ACBS" if matches!(game, GameKind::Fallout4) && sub.data.len() >= 20 => {
            let mut r = SubReader::new(&sub.data);
            record.acbs_flags = r.u32_or_default();
            r.skip_or_eof(2); // XP value offset (i16)
            record.level = r.i16_or_default();
            record.calc_min = r.u16_or_default(); // #2955
            r.skip_or_eof(2); // calc_max (u16)
            record.disposition_base = r.i16_or_default();
            record.template_flags = r.u16_or_default();
        }
        // Skyrim ACBS is a distinct 24-byte TES5 layout:
        //   flags(u32), magicka_offset(i16), stamina_offset(i16),
        //   level_or_pc_mult(u16), calc_min(u16), calc_max(u16),
        //   speed_mult(u16), disposition(i16), template_flags(u16),
        //   health_offset(i16), bleedout_override(u16).
        //
        // It must precede the generic 24-byte FNV/FO3 arm below. The shared
        // byte length hid the mismatch: that arm read magicka/stamina as
        // fatigue/barter, shifted level by two bytes, and interpreted the
        // TES5 tail as karma/disposition/template flags.
        b"ACBS" if matches!(game, GameKind::Skyrim) && sub.data.len() >= 24 => {
            let mut r = SubReader::new(&sub.data);
            record.acbs_flags = r.u32_or_default();
            record.magicka_offset = r.i16_or_default();
            record.stamina_offset = r.i16_or_default();
            record.level = r.u16_or_default() as i16;
            record.calc_min = r.u16_or_default();
            r.skip_or_eof(4); // calc_max(u16) + speed_mult(u16)
            record.disposition_base = r.i16_or_default();
            record.template_flags = r.u16_or_default();
            record.health_offset = r.i16_or_default();
        }
        // ACBS (FNV NPC_): flags(u32), fatigue(u16), barter(u16), level(i16),
        // calc_min(u16), calc_max(u16), speed_mult(u16), karma(f32),
        // disposition_base(i16), template_flags(u16)
        b"ACBS" if sub.data.len() >= 24 => {
            let mut r = SubReader::new(&sub.data);
            record.acbs_flags = r.u32_or_default();
            r.skip_or_eof(4); // fatigue(u16) + barter(u16)
            record.level = r.i16_or_default();
            // disposition_base is i16 at offset 20 (per UESP /
            // FalloutSnip). Pre-#377 the parser read a single byte
            // here, so any value outside 0..=127 lost its high byte
            // (and signed values past -128 had the sign chopped).
            record.calc_min = r.u16_or_default(); // #2955
            r.skip_or_eof(8); // calc_max/speed_mult (u16 × 2) + karma (f32)
            if sub.data.len() >= 22 {
                record.disposition_base = r.i16_or_default();
            }
            // template_flags — u16 at offset 22. Drives the
            // TPLT-inheritance dispatcher at spawn time (see
            // `NpcRecord::template_flags`). Without this every
            // FNV `Lvl*` NPC spawns with empty CNTO and no armor
            // dispatch fires.
            if sub.data.len() >= 24 {
                record.template_flags = r.u16_or_default();
            }
        }
        _ => {}
    }
}

/// Pre-FO4 FaceGen recipe sub-records (M41.0 Phase 1a). Only invoked
/// when `game.has_runtime_facegen_recipe()`, so the former per-arm
/// `captures_runtime_facegen` guard now lives at the [`parse_npc`] call
/// site. FGGS (50 × f32 sym morph weights), FGGA (30 × f32 asym), FGTS
/// (50 × f32 texture morphs); vanilla bytes are exactly the documented
/// sizes and short/long payloads pad/truncate rather than panicking.
fn parse_npc_runtime_facegen(
    recipe: &mut NpcFaceGenRecipe,
    sub: &SubRecord,
    remap: &Option<FormIdRemap>,
) {
    match &sub.sub_type {
        b"FGGS" if !sub.data.is_empty() => {
            read_f32_array_into(&sub.data, &mut recipe.fggs);
        }
        b"FGGA" if !sub.data.is_empty() => {
            read_f32_array_into(&sub.data, &mut recipe.fgga);
        }
        b"FGTS" if !sub.data.is_empty() => {
            read_f32_array_into(&sub.data, &mut recipe.fgts);
        }
        // HCLR carries 3-byte RGB on FNV vanilla; some records ship
        // a 4th alpha/padding byte — drop it per UESP (only the
        // first 3 are authoritative).
        b"HCLR" if sub.data.len() >= 3 => {
            recipe.hair_color_rgb = Some([sub.data[0], sub.data[1], sub.data[2]]);
        }
        // #2080 / FNV-D4-02 — HNAM/ENAM/PNAM-eyebrow are embedded
        // FormIDs, same as RNAM/CNAM/VTCK/SCRI/SNAM/CNTO/PKID/DOFT/
        // INAM/TPLT/PRPS/PRKR; #1996 threaded `remap` through this
        // function but missed this FaceGen-recipe block. Without it,
        // an NPC defined in a non-base plugin whose hair/eyes/eyebrow
        // reference points at content in that same plugin resolves the
        // wrong (or no) `index.hair`/`index.eyes`/`index.head_parts`
        // entry — silently bald/browless, or wrong-textured on a
        // FormID collision across plugins. `LNAM` is deliberately left
        // unremapped: nothing downstream ever reads `unused_lnam` (see
        // its field doc), so remapping it would fix no observable
        // behavior.
        b"HNAM" if sub.data.len() >= 4 => {
            let raw = SubReader::new(&sub.data).u32_or_default();
            recipe.hair_form_id = Some(remap_fid(raw, remap));
        }
        b"LNAM" if sub.data.len() >= 4 => {
            recipe.unused_lnam = Some(SubReader::new(&sub.data).u32_or_default());
        }
        b"ENAM" if sub.data.len() >= 4 => {
            let raw = SubReader::new(&sub.data).u32_or_default();
            recipe.eyes_form_id = Some(remap_fid(raw, remap));
        }
        // FNV / FO3 PNAM = single eyebrow HDPT FormID. The FO4 PNAM
        // arm in `parse_npc_fo4_facemorph` carries a different semantic
        // (head-parts list); the two never both fire on a single record
        // since `captures_runtime_facegen` and `captures_fo4_face` are
        // mutually exclusive per `GameKind`.
        b"PNAM" if sub.data.len() >= 4 => {
            let raw = SubReader::new(&sub.data).u32_or_default();
            recipe.eyebrow_form_id = Some(remap_fid(raw, remap));
        }
        _ => {}
    }
}

/// FO4+/FO76/Starfield typed face-morph block (#591 / FO4-DIM6-06).
/// Only invoked when `game.uses_prebaked_facegen()`; the former per-arm
/// `captures_fo4_face` guard now lives at the [`parse_npc`] call site.
/// FMRI/FMRS are collected into parallel vectors and zipped by the
/// caller after the walk (they appear alternating on the wire).
fn parse_npc_fo4_facemorph(
    face: &mut NpcFaceMorphs,
    fmri_forms: &mut Vec<u32>,
    fmrs_settings: &mut Vec<[f32; 9]>,
    sub: &SubRecord,
    remap: &Option<FormIdRemap>,
) {
    match &sub.sub_type {
        b"FMRI" if sub.data.len() >= 4 => {
            let raw = SubReader::new(&sub.data).u32_or_default();
            fmri_forms.push(remap_fid(raw, remap));
        }
        b"FMRS" if sub.data.len() >= 36 => {
            let s = SubReader::new(&sub.data)
                .f32_array::<9>()
                .unwrap_or([0.0; 9]);
            fmrs_settings.push(s);
        }
        // MSDK / MSDV are parallel arrays of u32 / f32 entries; on
        // vanilla FO4 they're single sub-records carrying the full
        // table. Reading them as variable-length flat arrays is
        // forward-compatible with malformed records that split the
        // table across multiple sub-records (last-wins per arm
        // would silently drop earlier entries — `extend` preserves).
        b"MSDK" if sub.data.len() >= 4 => {
            for chunk in sub.data.chunks_exact(4) {
                face.slider_keys
                    .push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
        b"MSDV" if sub.data.len() >= 4 => {
            for chunk in sub.data.chunks_exact(4) {
                face.slider_values
                    .push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
        // QNAM (FO4): 4 × f32 = texture-lighting tint (RGB + alpha).
        // The `captures_fo4_face` gate replaces the previous
        // length-only `>= 16` heuristic — Skyrim WTHR-record
        // siblings sharing the QNAM tag never reach this parser.
        b"QNAM" if sub.data.len() >= 16 => {
            let t = SubReader::new(&sub.data)
                .f32_array::<4>()
                .unwrap_or([0.0; 4]);
            face.texture_lighting = Some(t);
        }
        // HCLF/BCLF/PNAM (FO4+ head-parts) are embedded FormIDs too —
        // same #2080 completeness sweep as the pre-FO4 recipe block.
        b"HCLF" if sub.data.len() >= 4 => {
            let raw = SubReader::new(&sub.data).u32_or_default();
            face.hair_color = Some(remap_fid(raw, remap));
        }
        b"BCLF" if sub.data.len() >= 4 => {
            let raw = SubReader::new(&sub.data).u32_or_default();
            face.body_color = Some(remap_fid(raw, remap));
        }
        // PNAM on FO4+ NPCs accumulates head-part FormIDs (one per
        // sub-record). FNV / FO3 PNAM is captured by
        // `parse_npc_runtime_facegen` as a single eyebrow HDPT FormID;
        // the two arms are mutually exclusive via `captures_fo4_face`
        // vs `captures_runtime_facegen`.
        b"PNAM" if sub.data.len() >= 4 => {
            let raw = SubReader::new(&sub.data).u32_or_default();
            face.head_parts.push(remap_fid(raw, remap));
        }
        _ => {}
    }
}

/// FO4+ actor-value model — PRPS properties and baked DNAM derived stats.
/// Only invoked when `game.uses_actor_value_properties()`; the former
/// per-arm `captures_av_props` guard now lives at the [`parse_npc`] call
/// site. Perks moved out to [`parse_npc_perks`] under #3158 — they need a
/// wider gate than this one.
fn parse_npc_actor_values(record: &mut NpcRecord, sub: &SubRecord, remap: &Option<FormIdRemap>) {
    match &sub.sub_type {
        // PRPS "Properties": an array of (AVIF FormID, f32) pairs —
        // SPECIAL plus any authored AV overrides. Already the
        // `ActorValues::from_pairs` shape, so the FO4 arm of
        // `derive_npc_actor_values` returns them verbatim. 8 bytes per
        // entry (u32 FormID + f32 value); `chunks_exact` drops a
        // malformed trailing partial rather than panicking cell load.
        b"PRPS" => {
            record.actor_value_props.reserve(sub.data.len() / 8);
            for chunk in sub.data.chunks_exact(8) {
                let avif = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                let value = f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
                record
                    .actor_value_props
                    .push((remap_fid(avif, remap), value));
            }
        }
        // DNAM (FO4+): 8-byte struct whose head is two u16 baked
        // derived stats — Calculated Health @ 0, Calculated Action
        // Points @ 2 (xEdit NPC_ definition). NPCs ship these
        // precomputed instead of running the player derived-stat
        // curves. The length guard reads only the verified 4-byte
        // prefix; the far-model-distance / geared-up tail is ignored.
        b"DNAM" if sub.data.len() >= 4 => {
            let mut r = SubReader::new(&sub.data);
            record.calculated_health = r.u16_or_default();
            record.calculated_action_points = r.u16_or_default();
        }
        _ => {}
    }
}

/// `PRKZ`/`PRKR` perk entries — one sub-record per perk.
///
/// Split out of [`parse_npc_actor_values`] under #3158: perks predate the
/// FO4 actor-value property model by one release, so they need the wider
/// [`GameKind::uses_npc_perk_entries`] gate. Under the old
/// `uses_actor_value_properties` gate all 1620 perk-carrying Skyrim NPCs
/// parsed with an empty `perks` list, `spawn_npc_entity` then skipped the
/// `Perks` component entirely, and every `HasPerk` CTDA on Skyrim
/// evaluated a structural `0.0`.
///
/// Layout is `{ PERK FormID u32, rank u8, .. }`: 5 bytes on FO4, 8 on
/// Skyrim (three trailing unused bytes). The FormID and rank sit at the
/// same offsets in both, so one read covers them; the length guard stays
/// at the 5-byte minimum. The preceding `PRKZ` count is a benign hint, not
/// read.
fn parse_npc_perks(record: &mut NpcRecord, sub: &SubRecord, remap: &Option<FormIdRemap>) {
    if &sub.sub_type == b"PRKR" && sub.data.len() >= 5 {
        let perk = SubReader::new(&sub.data).u32_or_default();
        record.perks.push((remap_fid(perk, remap), sub.data[4]));
    }
}

/// Read up to `dst.len()` consecutive `f32` values out of `src` into
/// `dst`, padding with zero on under-read and silently dropping any
/// over-read tail. Used by [`parse_npc`] to land FGGS/FGGA/FGTS
/// payloads against fixed-size slider arrays.
fn read_f32_array_into(src: &[u8], dst: &mut [f32]) {
    for (i, slot) in dst.iter_mut().enumerate() {
        let off = i * 4;
        if off + 4 <= src.len() {
            *slot = f32::from_le_bytes([src[off], src[off + 1], src[off + 2], src[off + 3]]);
        } else {
            *slot = 0.0;
        }
    }
}

pub fn parse_race(form_id: u32, subs: &[SubRecord], game: GameKind) -> RaceRecord {
    // Helper claims a single MODL; RACE records carry multiple body
    // parts in MODL — keep that arm custom and ignore the helper's
    // last-MODL string. TD3-203 / #1113.
    let common = CommonNamedFields::from_subs(subs);
    let mut record = RaceRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        description: String::new(),
        skill_bonuses: Vec::new(),
        body_models: Vec::new(),
        head_parts: Vec::new(),
        base_height: (1.0, 1.0),
        base_weight: (1.0, 1.0),
        race_flags: 0,
        starting_health: None,
        starting_magicka: None,
        starting_stamina: None,
        base_attributes: None,
        default_hair: None,
        voice_forms: None,
        facegen_main_clamp: None,
        facegen_face_clamp: None,
        race_reactions: Vec::new(),
        default_skin: None,
    };

    let is_oblivion = matches!(game, GameKind::Oblivion);

    // FNV / FO3 head-part pairing — each MODL is preceded by an INDX
    // that names which body part the model is (0 = Head, 7 = Left Eye,
    // 8 = Right Eye, …). Track the most-recent INDX so we can attach
    // an index to the next MODL we see. Reset on consumption so a
    // stray MODL with no INDX prefix isn't mis-labelled. See
    // `RaceRecord::head_parts`.
    //
    // Vanilla FO3 / FNV RACE records split per-gender body parts via
    // `MNAM` (Male marker) and `FNAM` (Female marker) sub-records,
    // after the shared INDX/MODL block. Track which section we're in
    // so the spawner can pick gender-appropriate models without
    // double-rendering male+female eyes on the same NPC.
    //
    // #3419: `INDX` is re-used — with its own 0..3 numbering and its
    // own `MNAM` / `FNAM` markers — by the *body* section that opens
    // at `NAM1`. Without a section gate the head-part map absorbs
    // `characters\_Male\UpperBody.nif` under `HEAD = 0`,
    // `RightHand.nif` under FNV's `MOUTH = 2` and an `.egt` texture
    // path under `TEETH_LOWER = 3` (dumped from `CaucasianOldAged`,
    // `000987DF`). `NAM0` opens the head section, `NAM1` closes it;
    // both markers also reset the gender + pending-INDX state, since
    // each section restarts its own MNAM/FNAM/INDX run. Records that
    // author neither marker (none in vanilla, but a mod could) keep
    // the pre-#3419 behaviour of treating everything as head data.
    let mut pending_indx: Option<u32> = None;
    let mut gender_section: Option<u8> = None;
    let mut in_head_section = true;

    for sub in subs {
        match &sub.sub_type {
            b"DESC" => record.description = read_lstring_or_zstring(&sub.data),
            // DATA (TES4 / FO3 / FONV — 36 bytes total):
            //   8 × (u8 skill_index, u8 bonus)    16 B
            //   heightMale + heightFemale (2 × f32) 8 B
            //   weightMale + weightFemale (2 × f32) 8 B
            //   raceFlags (u32)                     4 B
            // Pre-#967 this arm read 7 × (u32, i8) starting at offset 0,
            // surfacing garbage form-keyed bonuses and dropping height/
            // weight/flags entirely. Layout per OpenMW
            // `esm4/loadrace.cpp:135-153` (canonical TES4-era reader).
            // Gate on the TES4/FO3/FNV era so a Skyrim 128/164-byte DATA
            // (which also satisfies `len >= 36`) is not mis-decoded with the
            // 36-byte layout into garbage skill bonuses / height / weight /
            // flags (#1629). Skyrim+ is handled by the two arms below.
            b"DATA"
                if matches!(game, GameKind::Oblivion | GameKind::Fallout3NV)
                    && sub.data.len() >= 36 =>
            {
                let mut r = SubReader::new(&sub.data);
                for _ in 0..8 {
                    let skill = r.u8_or_default();
                    let bonus = r.u8_or_default() as i8;
                    // Skip 0xFF (Skill_None sentinel) — OpenMW maps
                    // these into a HashMap keyed by SkillIndex, but
                    // our flat Vec preserves authoring order without
                    // dropping non-None slots. Skipping None keeps
                    // the public Vec semantically "real skill
                    // bonuses only," mirroring the FNV-era intent.
                    if skill != 0xFF {
                        record.skill_bonuses.push((skill, bonus));
                    }
                }
                let h_m = r.f32_or_default();
                let h_f = r.f32_or_default();
                let w_m = r.f32_or_default();
                let w_f = r.f32_or_default();
                record.base_height = (h_m, h_f);
                record.base_weight = (w_m, w_f);
                record.race_flags = r.u32_or_default();
            }
            // DATA (TES5 — Skyrim LE 128 B / SE 164 B):
            //   7 × (u8 skill_index, u8 bonus)      14 B
            //   u16 padding                          2 B
            //   heightMale + heightFemale (2 × f32)  8 B
            //   weightMale + weightFemale (2 × f32)  8 B
            //   raceFlags (u32)                      4 B
            //   … then 92 / 128 B of TES5-only tail (starting health /
            //     magicka / stamina, carry weight, accel + decel rates,
            //     regen rates, unarmed damage and reach, biped object slots)
            //     of which the combat actor-value path currently consumes
            //     starting health, magicka, and stamina.
            //
            // Layout per OpenMW `esm4/loadrace.cpp:154-170`, and verified
            // byte-for-byte against vanilla `Skyrim.esm` (2026-08-12), which
            // ships 164-byte DATA for all 99 of its RACE records:
            //   * `NordRace` decodes to skill indices 7/6/9/10/17/12 with
            //     bonuses +10/+5/+5/+5/+5/+5 — exactly vanilla Nord's
            //     Two-Handed +10 and One-Handed / Block / Smithing / Speech /
            //     Light Armor +5 — plus height 1.03 and weight 1.0.
            //   * `ElderRace` decodes to seven `0xFF` Skill_None slots, which
            //     is correct: the Elder race grants no skill bonuses.
            // Both are strong evidence for the offsets *and* the skill-index
            // mapping, which a length check alone could not establish.
            //
            // Note the pair count differs from the TES4 arm above: TES5 has 7
            // skill slots plus two padding bytes where TES4/FO3/FNV has 8.
            b"DATA" if matches!(game, GameKind::Skyrim) && matches!(sub.data.len(), 128 | 164) => {
                let mut r = SubReader::new(&sub.data);
                for _ in 0..7 {
                    let skill = r.u8_or_default();
                    let bonus = r.u8_or_default() as i8;
                    // Same Skill_None handling as the TES4 arm — keep the
                    // public Vec "real bonuses only".
                    if skill != 0xFF {
                        record.skill_bonuses.push((skill, bonus));
                    }
                }
                let _padding = r.u16_or_default();
                let h_m = r.f32_or_default();
                let h_f = r.f32_or_default();
                let w_m = r.f32_or_default();
                let w_f = r.f32_or_default();
                record.base_height = (h_m, h_f);
                record.base_weight = (w_m, w_f);
                record.race_flags = r.u32_or_default();
                let starting_health = r.f32_or_default();
                if starting_health.is_finite() && starting_health > 0.0 {
                    record.starting_health = Some(starting_health);
                }
                let starting_magicka = r.f32_or_default();
                if starting_magicka.is_finite() && starting_magicka > 0.0 {
                    record.starting_magicka = Some(starting_magicka);
                }
                let starting_stamina = r.f32_or_default();
                if starting_stamina.is_finite() && starting_stamina > 0.0 {
                    record.starting_stamina = Some(starting_stamina);
                }
            }
            // DATA (FO4 200 B / FO76 216 B) — a third layout again, and
            // deliberately only partially decoded.
            //
            // Measured against vanilla `Fallout4.esm` (45 RACE records) and
            // `SeventySix.esm` (157) on 2026-08-12:
            //   * There is **no skill-bonus array at all** — the record opens
            //     with floats at offset 0. That is consistent with the games
            //     themselves: neither FO4 nor FO76 has skills. Feeding these
            //     bytes through the TES5 arm would read `00 00 80 3f` as the
            //     pairs `(0,0) (128,63)` — precisely the garbage #1629 removed.
            //   * `heightMale` / `heightFemale` sit at offsets 0 and 4, and
            //     they carry real signal: `HumanRace` is 1.0 / 0.98 while
            //     `HumanChildRace` and `GhoulChildRace` are 0.825 / 0.825.
            //     A child race being shorter is what identifies the field.
            //   * Offsets 8..32 are **byte-identical across every shipped race
            //     in both games** (`0.5, 0.5, 0.0` twice). Being invariant they
            //     carry no information to validate an interpretation against,
            //     so they are left undecoded rather than guessed. The shape
            //     suggests FO4's three-axis (thin / muscular / large) body
            //     morph per gender, which `base_weight`'s `(male, female)`
            //     pair could not represent even if it were confirmed — so
            //     `base_weight` stays at its default here, by design.
            // A `race_flags`-shaped word sits at offset 32 as it does in TES5,
            // but its bit meanings are unverified for these games (FO4's
            // non-playable `HumanChildRace` has bit 0 set, which contradicts
            // the TES-lineage "bit 0 = Playable"), so it is not surfaced.
            b"DATA"
                if matches!(game, GameKind::Fallout4 | GameKind::Fallout76)
                    && sub.data.len() >= 8 =>
            {
                let mut r = SubReader::new(&sub.data);
                record.base_height = (r.f32_or_default(), r.f32_or_default());
            }
            // Any DATA that reached here is a shape no arm claims. Log it
            // rather than dropping it silently: the whole point of #1629 /
            // #2455 is that a RACE record quietly left at defaults reads
            // identically to one that decoded fine. Mirrors the
            // `xcll_size_sanity_warn` pattern. `debug` not `warn` because
            // Starfield legitimately ships zero RACE DATA sub-records, so a
            // louder level would be pure noise on a correct load.
            b"DATA" => {
                log::debug!(
                    "RACE {:08X}: DATA sub-record ({} bytes) has no decoder for {:?} — \
                     skill bonuses / height / weight / flags stay at defaults",
                    form_id,
                    sub.data.len(),
                    game,
                );
            }
            // MODL appears multiple times in RACE for body parts. Collect them all.
            //
            // FNV / FO3: each MODL is preceded by an INDX naming the
            // body part. Pair them so the spawner can pick out the
            // eyes (INDX 7 / 8) without guessing by list position.
            // `gender_section` tracks the MNAM / FNAM split so the
            // spawner can pick gender-appropriate variants.
            b"INDX" if sub.data.len() >= 4 => {
                pending_indx = Some(SubReader::new(&sub.data).u32_or_default());
            }
            b"MNAM" => {
                gender_section = Some(0); // Male
            }
            b"FNAM" => {
                gender_section = Some(1); // Female
            }
            // Head-data marker: opens the INDX/MODL run whose indices
            // are head-part roles. Oblivion authors it with no gender
            // markers at all (one shared run of 0..8); FO3 / FNV split
            // it MNAM / FNAM. #3419.
            b"NAM0" => {
                in_head_section = true;
                gender_section = None;
                pending_indx = None;
            }
            // Body-data marker: closes the head section. Its INDX run
            // restarts at 0 for a different vocabulary (upper body /
            // hands / `.egt`), so nothing past here belongs in
            // `head_parts`. #3419.
            b"NAM1" => {
                in_head_section = false;
                gender_section = None;
                pending_indx = None;
            }
            b"MODL" => {
                let path = read_zstring(&sub.data);
                if let Some(idx) = pending_indx.take() {
                    if in_head_section {
                        record.head_parts.push((idx, path.clone(), gender_section));
                    }
                }
                record.body_models.push(path);
            }
            // ── Oblivion-only sub-records (#967 / OBL-D3-NEW-03) ───────
            // Plumbed under `is_oblivion` because TES5+ reuses these
            // FourCCs with different payloads (e.g. TES5 VNAM is a
            // 4-byte u32 instead of TES4's two form IDs at 8 bytes).
            // Gating on `GameKind::Oblivion` avoids cross-game
            // misreads when a future loader walks the same arm.
            b"ATTR" if is_oblivion && sub.data.len() >= 16 => {
                let mut attrs = RaceAttributes::default();
                attrs.male.strength = sub.data[0];
                attrs.male.intelligence = sub.data[1];
                attrs.male.willpower = sub.data[2];
                attrs.male.agility = sub.data[3];
                attrs.male.speed = sub.data[4];
                attrs.male.endurance = sub.data[5];
                attrs.male.personality = sub.data[6];
                attrs.male.luck = sub.data[7];
                attrs.female.strength = sub.data[8];
                attrs.female.intelligence = sub.data[9];
                attrs.female.willpower = sub.data[10];
                attrs.female.agility = sub.data[11];
                attrs.female.speed = sub.data[12];
                attrs.female.endurance = sub.data[13];
                attrs.female.personality = sub.data[14];
                attrs.female.luck = sub.data[15];
                record.base_attributes = Some(attrs);
            }
            b"DNAM" if is_oblivion && sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                let male = r.u32_or_default();
                let female = r.u32_or_default();
                record.default_hair = Some((male, female));
            }
            b"VNAM" if is_oblivion && sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                let male = r.u32_or_default();
                let female = r.u32_or_default();
                record.voice_forms = Some((male, female));
            }
            b"PNAM" if is_oblivion && sub.data.len() >= 4 => {
                record.facegen_main_clamp = Some(SubReader::new(&sub.data).f32_or_default());
            }
            b"UNAM" if is_oblivion && sub.data.len() >= 4 => {
                record.facegen_face_clamp = Some(SubReader::new(&sub.data).f32_or_default());
            }
            b"XNAM" if is_oblivion && sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                let other_race = r.u32_or_default();
                let adjustment = r.i32_or_default();
                record.race_reactions.push((other_race, adjustment));
            }
            // CNAM intentionally skipped — its 4-byte payload mixes a
            // bitmask + 2-byte field that OpenMW also skips (see
            // `esm4/loadrace.cpp:232-251`). Authoritative semantics
            // are undocumented; revisit when M41.0 Phase 3b needs it.
            //
            // WNAM — default skin ARMO, Skyrim+ only (#2093 /
            // SKY-D3-NEW-01). Gated on `uses_prebaked_facegen()`
            // rather than a hardcoded game list so FO76/Starfield ride
            // along automatically; TES4/FO3/FNV RACE records don't
            // author WNAM at all.
            b"WNAM" if game.uses_prebaked_facegen() && sub.data.len() >= 4 => {
                record.default_skin = Some(SubReader::new(&sub.data).u32_or_default());
            }
            _ => {}
        }
    }

    record
}

pub fn parse_clas(form_id: u32, subs: &[SubRecord], game: GameKind) -> ClassRecord {
    let common = CommonNamedFields::from_subs(subs);
    let mut record = ClassRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        description: String::new(),
        base_attributes: [0u8; 7],
        tag_skills: Vec::new(),
        primary_attributes: None,
        specialization: None,
        major_skills: Vec::new(),
        flags_oblivion: None,
    };

    let is_oblivion = matches!(game, GameKind::Oblivion);
    // FO3 splits the 7 base attributes across 7 single-byte `ATTR`
    // subrecords; this tracks the next slot to fill so they accumulate in
    // order. FNV's one 7-byte `ATTR` fills all 7 in a single pass.
    let mut attr_idx = 0usize;

    for sub in subs {
        match &sub.sub_type {
            b"DESC" => record.description = read_lstring_or_zstring(&sub.data),
            // DATA layout (Oblivion CLAS — 48 or 52 bytes per empirical
            // probe against vanilla Oblivion.esm, #968; histogram is
            // 79 × 52-byte + 31 × 48-byte):
            //   2 × u32 primary attribute indices         (offset 0..8)
            //   u32 specialization (0=Combat/1=Mag/2=Sth) (offset 8..12)
            //   7 × u32 major skill indices               (offset 12..40)
            //   u32 race-class flags (bit 0 = Playable)   (offset 40..44)
            //   u32 services                              (offset 44..48)
            //   i8 trainer skill + u8 trainer level + 2 B (offset 48..52, OPTIONAL)
            //
            // Knight (form 0x836) is a 52-byte record: primary=(0=Strength,
            // 6=Personality), spec=0 (Combat), majors=[0x0F Block, 0x17
            // Illusion, 0x12 HeavyArmor, 0x10 Blunt, 0x0E Blade, 0x20
            // Speechcraft, 0x11 HandToHand]. 31 vanilla classes
            // (Hunter, Priest, Noble, TGGrayFoxClass, etc.) ship the
            // 48-byte variant — same primary block, no trainer tail.
            //
            // The audit (#968) described the layout as 60 bytes / 14
            // major skills — wrong on both counts. Its own test
            // assertion said `len() == 7`, which matches the empirical
            // truth.
            b"DATA" if is_oblivion && sub.data.len() >= 48 => {
                let mut r = SubReader::new(&sub.data);
                let a0 = r.u32_or_default();
                let a1 = r.u32_or_default();
                record.primary_attributes = Some((a0, a1));
                record.specialization = r.u32().ok();
                for _ in 0..7 {
                    if let Ok(s) = r.u32() {
                        record.major_skills.push(s);
                    }
                }
                record.flags_oblivion = r.u32().ok();
            }
            // DATA layout (FNV/FO3 CLAS — 28 bytes, fopdoc `CLAS`):
            // tag1..tag4 (4 × i32 skill enum), flags (u32), buys/sells +
            // services (u32), teaches (i8), max training level (u8),
            // unused (2 B). Only the 4 tag skills are read here. The base
            // SPECIAL attributes are NOT in DATA — they're in the separate
            // `ATTR` subrecord (below). Pre-#1663 this arm gated on `>= 35`
            // and read 7 attribute bytes from `DATA[28..35]`, a layout that
            // never matched real 28-byte FNV `DATA` (so it silently no-op'd
            // on real content). Gate at `>= 16` — all we consume is the tag
            // block. Stays `!is_oblivion`-gated so the wider Oblivion DATA
            // routes to its own arm above.
            b"DATA" if !is_oblivion && sub.data.len() >= 16 => {
                let mut r = SubReader::new(&sub.data);
                for _ in 0..4 {
                    if let Ok(f) = r.u32() {
                        if f != 0 {
                            record.tag_skills.push(f);
                        }
                    }
                }
            }
            // ATTR subrecord (FNV/FO3 CLAS, fopdoc): the 7 base SPECIAL
            // attributes (Str, Per, End, Cha, Int, Agi, Luck), each a u8.
            // FNV ships one 7-byte struct; FO3 ships 7 single-byte `ATTR`
            // subrecords. Folding both: append every byte into the next
            // open slot until the 7 are filled (a 7-byte struct fills them
            // in one pass; seven 1-byte records fill one each). Oblivion's
            // race-style `ATTR` is handled in `parse_race`, not here.
            b"ATTR" if !is_oblivion => {
                for &byte in sub.data.iter() {
                    if attr_idx < record.base_attributes.len() {
                        record.base_attributes[attr_idx] = byte;
                        attr_idx += 1;
                    }
                }
            }
            _ => {}
        }
    }

    record
}

pub fn parse_fact(form_id: u32, subs: &[SubRecord], remap: &Option<FormIdRemap>) -> FactionRecord {
    let common = CommonNamedFields::from_subs(subs);
    let mut record = FactionRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        flags: 0,
        relations: Vec::new(),
        ranks: Vec::new(),
        reputation: None,
    };

    for sub in subs {
        match &sub.sub_type {
            // DATA (FNV FACT): flags is a single byte per UESP
            // `Mod_File_Format/FACT` (FO3 / FNV). The tail is a
            // variable-width payload (FNV adds `u8 unknown + f32 crime
            // gold multiplier`) that different vanilla records truncate
            // differently — reading 4 bytes pulled padding / neighbor
            // bytes into the high 24 bits, producing spurious bits 8+.
            // Only bits 0 (hidden from PC), 1 (evil), 2 (special
            // combat) are authoritative on FO3 / FNV.
            //
            // Skyrim and FO4 extend DATA to a full u32; if / when those
            // parse paths get added here, split per `GameKind`. See
            // #481 / FNV-2-L1.
            b"DATA" if !sub.data.is_empty() => {
                record.flags = sub.data[0] as u32;
            }
            // XNAM: relation entry — other faction (u32) + modifier (i32) + reaction (u32).
            // The reaction field is a full 4-byte u32 per UESP; pre-#482 the
            // parser read only the low byte via `sub.data[8]`, which happened
            // to be correct for vanilla values 0..=3 but would silently
            // truncate any future mod that extends the enum past 255. The
            // decoded value is stored at its full width (#3339) — the old
            // `as u8` cast here threw away the extra 24 bits immediately,
            // leaving the wider read purely a cursor-alignment step.
            b"XNAM" if sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                let other = r.u32_or_default();
                let modifier = r.i32_or_default();
                let combat = if sub.data.len() >= 12 {
                    r.u32_or_default()
                } else {
                    0
                };
                record.relations.push(FactionRelation {
                    other_faction: other,
                    modifier,
                    combat_reaction: combat,
                });
            }
            // WMI1 (FNV): the faction's `REPU` FormID — the faction →
            // reputation edge (#3325). Remapped to global load-order space
            // like every other embedded FormID, so `index.reputations`
            // lookups (keyed by the remapped record header FormID) hit.
            b"WMI1" if sub.data.len() >= 4 => {
                let raw = SubReader::new(&sub.data).u32_or_default();
                record.reputation = (raw != 0).then(|| remap_fid(raw, remap));
            }
            // MNAM: male rank label (string)
            b"MNAM" => record.ranks.push(read_zstring(&sub.data)),
            _ => {}
        }
    }

    record
}

#[cfg(test)]
mod tests;
