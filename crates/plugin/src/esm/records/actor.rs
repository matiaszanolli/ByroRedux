//! Actor-related record parsers — NPC_, RACE, CLAS, FACT.
//!
//! NPC parsing pulls the essentials needed to spawn the NPC into the world:
//! base race/class form IDs, faction memberships, inventory list, and a
//! pointer to the head/body model. Combat stats and AI packages are stored
//! as raw form IDs for now; full evaluation lands when the AI/combat
//! systems come online.

use super::common::{read_lstring_or_zstring, read_zstring, CommonNamedFields};
use crate::esm::reader::{GameKind, SubRecord};
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

#[derive(Debug, Clone)]
pub struct NpcRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Model path (typically from MODL — head/body mesh, optional).
    pub model_path: String,
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
    pub level: i16,
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
    ///   * `0x0001` Use Traits, `0x0002` Use Stats, `0x0004` Factions,
    ///     `0x0008` Actor Effects, `0x0010` AI Data, `0x0020` AI
    ///     Packages, `0x0040` Model/Animation, `0x0080` Base Data,
    ///     `0x0200` Script, `0x0400` Def Pack List — parsed and
    ///     stored for the dispatcher; inventory is the first consumer.
    pub template_flags: u16,
}

#[derive(Debug, Clone)]
pub struct RaceRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// Skill bonuses: 8 pairs of `(skill_index, bonus)` where
    /// `skill_index` is the `SkillIndex` enum value (0x0C..=0x20 for
    /// Oblivion's 21 skills; 0xFF = None for unused slots).
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
    /// FNV / FO3 `INDX` + `MODL` head-part pairs. Each entry is
    /// `(head_part_index, mesh_path, gender_section)` where
    /// `head_part_index` (per UESP RACE_HeadPart):
    ///
    ///   * 0 — Head
    ///   * 1 — Ear (male) — 2 — Ear (female)
    ///   * 3 — Mouth — 4 — Teeth (lower) — 5 — Teeth (upper) — 6 — Tongue
    ///   * 7 — Left Eye — 8 — Right Eye
    ///
    /// `gender_section` tracks which RACE-record section the part
    /// was authored in:
    ///
    ///   * `None`  — shared across both genders (entries before any
    ///     MNAM/FNAM marker — every "Head" entry lands here).
    ///   * `Some(0)` — male-only (after an `MNAM` section marker).
    ///   * `Some(1)` — female-only (after `FNAM`).
    ///
    /// Without this typed pairing the spawner can't tell which
    /// `body_models` entry is which body part — or which gender it
    /// applies to. Pre-fix every NPC rendered with just the head
    /// NIF — no eyes, mouth, teeth, tongue, ears. Empty on
    /// Oblivion / Skyrim+ (different RACE layouts; this list only
    /// populates on FO3 / FNV).
    pub head_parts: Vec<(u32, String, Option<u8>)>,
    /// Default body height per gender, from DATA — `(male, female)`.
    /// Vanilla Oblivion / FO3 / FNV authors values in ~0.9..1.15.
    /// Default `(1.0, 1.0)` when DATA is shorter than 36 bytes
    /// (Skyrim ships 128-byte DATA which falls into a separate
    /// reader arm not yet wired here).
    pub base_height: (f32, f32),
    /// Default body weight per gender, from DATA — `(male, female)`.
    /// Vanilla values typically `(1.0, 1.0)`.
    pub base_weight: (f32, f32),
    /// `RACE_FLAGS` u32 tail of the 36-byte DATA. Bit 0 = Playable.
    /// Other bits are documented per game (`BeastRace`, `Swims`,
    /// `Flies` — vanilla Oblivion uses bit 0 + bit 2 = 0x05 for
    /// playable beast-race overrides).
    pub race_flags: u32,
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
}

/// FNV / FO3 `INDX` head-part identifiers. Values verified by dumping
/// vanilla FNV.esm RACE records (e.g. HispanicOldAged: Head 0, Mouth 2,
/// Teeth Lower 3, Teeth Upper 4, Tongue 5, Left Eye 6, Right Eye 7).
/// UESP's `RACE_HeadPart` table claims 7/8 for eyes — vanilla data
/// disagrees. Used by the NPC spawner to pick the eye mesh paths
/// out of [`RaceRecord::head_parts`] by semantic role instead of
/// list-position guessing.
pub mod head_part {
    pub const HEAD: u32 = 0;
    pub const MOUTH: u32 = 2;
    pub const TEETH_LOWER: u32 = 3;
    pub const TEETH_UPPER: u32 = 4;
    pub const TONGUE: u32 = 5;
    pub const LEFT_EYE: u32 = 6;
    pub const RIGHT_EYE: u32 = 7;
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

#[derive(Debug, Clone)]
pub struct ClassRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    pub description: String,
    /// 7 attribute weights (Strength, Perception, Endurance, Charisma,
    /// Intelligence, Agility, Luck) — order varies per game.
    ///
    /// **Game layout split (#968):** the FNV-era 35-byte DATA layout
    /// encodes 4 × u32 tag skill form IDs + flags + services + trainer
    /// data + 7 × u8 attribute weights. Oblivion's 52-byte DATA has a
    /// completely different shape (2 × u32 primary attributes +
    /// specialization + 7 × u32 major skills + flags + services +
    /// trainer + 2 B pad) — `attribute_weights` is left at `[0; 7]`
    /// for Oblivion-tagged records since the field is FNV-shaped.
    /// Use [`Self::primary_attributes`] + [`Self::specialization`] on
    /// Oblivion records instead.
    pub attribute_weights: [u8; 7],
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
    pub flags_oblivion: Option<u32>,
}

/// Faction-to-faction relation.
#[derive(Debug, Clone, Copy)]
pub struct FactionRelation {
    pub other_faction: u32,
    /// Modifier (-100..100, larger means more friendly).
    pub modifier: i32,
    /// Combat reaction (0=neutral, 1=enemy, 2=ally, 3=friend).
    pub combat_reaction: u8,
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
}

// ── Parsers ───────────────────────────────────────────────────────────

pub fn parse_npc(form_id: u32, subs: &[SubRecord], game: GameKind) -> NpcRecord {
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
    // EDID / FULL / MODL / VMAD shared with every named record — drain
    // them through the helper so the per-record loop below only carries
    // NPC-specific subrecords. TD3-203 / #1113.
    let common = CommonNamedFields::from_subs(subs);
    let mut record = NpcRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        model_path: common.model_path,
        race_form_id: 0,
        class_form_id: 0,
        voice_form_id: 0,
        factions: Vec::new(),
        inventory: Vec::new(),
        default_outfit: None,
        ai_packages: Vec::new(),
        death_item_form_id: 0,
        level: 1,
        disposition_base: 50,
        acbs_flags: 0,
        has_script: common.has_script,
        script_form_id: 0,
        face_morphs: None,
        runtime_facegen: None,
        template_form_id: 0,
        template_flags: 0,
    };
    // FMRI and FMRS are collected separately and zipped after the walk
    // since they appear alternating on the wire and we don't want to
    // assume a strict ordering inside the sub-record list.
    let mut fmri_forms: Vec<u32> = Vec::new();
    let mut fmrs_settings: Vec<[f32; 9]> = Vec::new();
    let mut face = NpcFaceMorphs::default();
    let mut recipe = NpcFaceGenRecipe::default();

    for sub in subs {
        match &sub.sub_type {
            b"RNAM" if sub.data.len() >= 4 => {
                record.race_form_id = SubReader::new(&sub.data).u32_or_default();
            }
            b"CNAM" if sub.data.len() >= 4 => {
                record.class_form_id = SubReader::new(&sub.data).u32_or_default();
            }
            b"VTCK" if sub.data.len() >= 4 => {
                record.voice_form_id = SubReader::new(&sub.data).u32_or_default();
            }
            // SCRI — pre-Skyrim attached-script FormID. NPC_ + CREA
            // share `parse_npc` so this arm covers both. See #1273.
            b"SCRI" if sub.data.len() >= 4 => {
                record.script_form_id = SubReader::new(&sub.data).u32_or_default();
            }
            // SNAM (FNV NPC_): faction form ID (u32) + rank (i8) + pad x3
            b"SNAM" if sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                let faction = r.u32_or_default();
                let rank = r.u8_or_default() as i8;
                record.factions.push(FactionMembership {
                    faction_form_id: faction,
                    rank,
                });
            }
            // CNTO: shared with CONT
            b"CNTO" if sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                record.inventory.push(NpcInventoryEntry {
                    item_form_id: r.u32_or_default(),
                    count: r.i32_or_default(),
                });
            }
            b"PKID" if sub.data.len() >= 4 => {
                record
                    .ai_packages
                    .push(SubReader::new(&sub.data).u32_or_default());
            }
            // DOFT — Skyrim+ default outfit FormID. Pre-Skyrim games
            // don't emit DOFT (NPCs equip directly from inventory).
            // Stored as Option so the equip pipeline can dispatch on
            // presence without ambiguity vs the null-form sentinel.
            b"DOFT" if sub.data.len() >= 4 => {
                record.default_outfit = SubReader::new(&sub.data).u32().ok();
            }
            b"INAM" if sub.data.len() >= 4 => {
                record.death_item_form_id = SubReader::new(&sub.data).u32_or_default();
            }
            // TPLT — FNV / FO3 template-inheritance pointer. Vanilla
            // Lvl* NPCs author this and rely on `template_flags` (in
            // ACBS) to pull per-field categories from the referenced
            // base. See `NpcRecord::template_form_id` for the bitmap.
            b"TPLT" if sub.data.len() >= 4 => {
                record.template_form_id = SubReader::new(&sub.data).u32_or_default();
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
                r.skip_or_eof(10); // calc_min/calc_max/speed_mult (u16 × 3) + karma (f32)
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
            // ── M41.0 Phase 1a — Pre-FO4 FaceGen recipe ────────────────
            // FGGS (50 × f32 sym morph weights), FGGA (30 × f32 asym),
            // FGTS (50 × f32 texture morphs). Vanilla bytes are exactly
            // the documented sizes; the parser pads short payloads with
            // zeros and truncates over-long ones rather than panicking.
            b"FGGS" if captures_runtime_facegen && !sub.data.is_empty() => {
                read_f32_array_into(&sub.data, &mut recipe.fggs);
            }
            b"FGGA" if captures_runtime_facegen && !sub.data.is_empty() => {
                read_f32_array_into(&sub.data, &mut recipe.fgga);
            }
            b"FGTS" if captures_runtime_facegen && !sub.data.is_empty() => {
                read_f32_array_into(&sub.data, &mut recipe.fgts);
            }
            // HCLR carries 3-byte RGB on FNV vanilla; some records ship
            // a 4th alpha/padding byte — drop it per UESP (only the
            // first 3 are authoritative).
            b"HCLR" if captures_runtime_facegen && sub.data.len() >= 3 => {
                recipe.hair_color_rgb = Some([sub.data[0], sub.data[1], sub.data[2]]);
            }
            b"HNAM" if captures_runtime_facegen && sub.data.len() >= 4 => {
                recipe.hair_form_id = Some(SubReader::new(&sub.data).u32_or_default());
            }
            b"LNAM" if captures_runtime_facegen && sub.data.len() >= 4 => {
                recipe.unused_lnam = Some(SubReader::new(&sub.data).u32_or_default());
            }
            b"ENAM" if captures_runtime_facegen && sub.data.len() >= 4 => {
                recipe.eyes_form_id = Some(SubReader::new(&sub.data).u32_or_default());
            }
            // FNV / FO3 PNAM = single eyebrow HDPT FormID. The FO4 PNAM
            // arm below carries a different semantic (head-parts list);
            // the two are guarded by `captures_runtime_facegen` vs
            // `captures_fo4_face` and never both fire on a single record.
            b"PNAM" if captures_runtime_facegen && sub.data.len() >= 4 => {
                recipe.eyebrow_form_id = Some(SubReader::new(&sub.data).u32_or_default());
            }
            // ── #591 / FO4-DIM6-06 face-morph block ────────────────────
            // FO4+/FO76/Starfield only. Pre-fix, all of these arms ran
            // unconditionally; FNV `PNAM` (eyebrow HDPT FormID) was
            // misread as an FO4 head-parts entry. See `captures_fo4_face`.
            b"FMRI" if captures_fo4_face && sub.data.len() >= 4 => {
                fmri_forms.push(SubReader::new(&sub.data).u32_or_default());
            }
            b"FMRS" if captures_fo4_face && sub.data.len() >= 36 => {
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
            b"MSDK" if captures_fo4_face && sub.data.len() >= 4 => {
                for chunk in sub.data.chunks_exact(4) {
                    face.slider_keys
                        .push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                }
            }
            b"MSDV" if captures_fo4_face && sub.data.len() >= 4 => {
                for chunk in sub.data.chunks_exact(4) {
                    face.slider_values
                        .push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                }
            }
            // QNAM (FO4): 4 × f32 = texture-lighting tint (RGB + alpha).
            // The `captures_fo4_face` gate replaces the previous
            // length-only `>= 16` heuristic — Skyrim WTHR-record
            // siblings sharing the QNAM tag never reach this parser.
            b"QNAM" if captures_fo4_face && sub.data.len() >= 16 => {
                let t = SubReader::new(&sub.data)
                    .f32_array::<4>()
                    .unwrap_or([0.0; 4]);
                face.texture_lighting = Some(t);
            }
            b"HCLF" if captures_fo4_face && sub.data.len() >= 4 => {
                face.hair_color = Some(SubReader::new(&sub.data).u32_or_default());
            }
            b"BCLF" if captures_fo4_face && sub.data.len() >= 4 => {
                face.body_color = Some(SubReader::new(&sub.data).u32_or_default());
            }
            // PNAM on FO4+ NPCs accumulates head-part FormIDs (one per
            // sub-record). FNV / FO3 PNAM is captured by the kf-era
            // arm above as a single eyebrow HDPT FormID; the two arms
            // are mutually exclusive via `captures_fo4_face` vs
            // `captures_runtime_facegen`, both keyed off `GameKind`
            // semantic predicates.
            b"PNAM" if captures_fo4_face && sub.data.len() >= 4 => {
                face.head_parts
                    .push(SubReader::new(&sub.data).u32_or_default());
            }
            _ => {}
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
        base_attributes: None,
        default_hair: None,
        voice_forms: None,
        facegen_main_clamp: None,
        facegen_face_clamp: None,
        race_reactions: Vec::new(),
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
    let mut pending_indx: Option<u32> = None;
    let mut gender_section: Option<u8> = None;

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
            // TES5 DATA is 128 / 164 bytes with a different layout —
            // not yet wired here.
            b"DATA" if sub.data.len() >= 36 => {
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
            b"MODL" => {
                let path = read_zstring(&sub.data);
                if let Some(idx) = pending_indx.take() {
                    record.head_parts.push((idx, path.clone(), gender_section));
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
        attribute_weights: [0u8; 7],
        tag_skills: Vec::new(),
        primary_attributes: None,
        specialization: None,
        major_skills: Vec::new(),
        flags_oblivion: None,
    };

    let is_oblivion = matches!(game, GameKind::Oblivion);

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
            // DATA layout (FNV CLAS — 35 bytes): tag1..tag4 (4 × u32
            // form), flags (u32), services (u32), trainer skill (i8),
            // trainer level (u8), teaches level (u8), teaches max
            // (u8), then 7 attribute weights (u8). Pre-#968 this arm
            // ran for ALL games and read garbage from Oblivion's
            // wider 52-byte layout.
            b"DATA" if !is_oblivion && sub.data.len() >= 35 => {
                let mut r = SubReader::new(&sub.data);
                for _ in 0..4 {
                    if let Ok(f) = r.u32() {
                        if f != 0 {
                            record.tag_skills.push(f);
                        }
                    }
                }
                // Skip flags + services + skill/level/teaches bytes (4 + 4 + 4 = 12).
                // Attribute weights start at offset 28 (cursor sits at 16 after the 4 × u32 tag block).
                r.skip_or_eof(12);
                for i in 0..7 {
                    record.attribute_weights[i] = r.u8_or_default();
                }
            }
            _ => {}
        }
    }

    record
}

pub fn parse_fact(form_id: u32, subs: &[SubRecord]) -> FactionRecord {
    let common = CommonNamedFields::from_subs(subs);
    let mut record = FactionRecord {
        form_id,
        editor_id: common.editor_id,
        full_name: common.full_name,
        flags: 0,
        relations: Vec::new(),
        ranks: Vec::new(),
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
            // truncate any future mod that extends the enum past 255.
            b"XNAM" if sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                let other = r.u32_or_default();
                let modifier = r.i32_or_default();
                let combat = if sub.data.len() >= 12 {
                    r.u32_or_default() as u8
                } else {
                    0
                };
                record.relations.push(FactionRelation {
                    other_faction: other,
                    modifier,
                    combat_reaction: combat,
                });
            }
            // MNAM: male rank label (string)
            b"MNAM" => record.ranks.push(read_zstring(&sub.data)),
            _ => {}
        }
    }

    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::reader::SubRecord;

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    #[test]
    fn npc_extracts_race_class_factions_inventory() {
        let mut acbs = Vec::new();
        acbs.extend_from_slice(&0x100u32.to_le_bytes()); // flags
        acbs.extend_from_slice(&[0u8; 4]); // fatigue + barter
        acbs.extend_from_slice(&5i16.to_le_bytes()); // level
        acbs.extend_from_slice(&[0u8; 14]); // pad to 24 bytes total

        let mut snam = Vec::new();
        snam.extend_from_slice(&0xAAAAu32.to_le_bytes());
        snam.push(2u8);
        snam.extend_from_slice(&[0u8; 3]);

        let mut cnto = Vec::new();
        cnto.extend_from_slice(&0xBBBBu32.to_le_bytes());
        cnto.extend_from_slice(&3i32.to_le_bytes());

        let subs = vec![
            sub(b"EDID", b"NpcTest\0"),
            sub(b"FULL", b"Test NPC\0"),
            sub(b"RNAM", &0xCCCCu32.to_le_bytes()),
            sub(b"CNAM", &0xDDDDu32.to_le_bytes()),
            sub(b"ACBS", &acbs),
            sub(b"SNAM", &snam),
            sub(b"CNTO", &cnto),
            sub(b"PKID", &0xEEEEu32.to_le_bytes()),
        ];
        let n = parse_npc(0x500, &subs, GameKind::Fallout3NV);
        assert_eq!(n.editor_id, "NpcTest");
        assert_eq!(n.race_form_id, 0xCCCC);
        assert_eq!(n.class_form_id, 0xDDDD);
        assert_eq!(n.factions.len(), 1);
        assert_eq!(n.factions[0].faction_form_id, 0xAAAA);
        assert_eq!(n.factions[0].rank, 2);
        assert_eq!(n.inventory.len(), 1);
        assert_eq!(n.inventory[0].item_form_id, 0xBBBB);
        assert_eq!(n.inventory[0].count, 3);
        assert_eq!(n.ai_packages, vec![0xEEEE]);
        assert_eq!(n.acbs_flags, 0x100);
        assert_eq!(n.level, 5);
    }

    /// Regression for #1273 — `SCRI` attached-script FormID on NPC_
    /// and CREA records was silently dropped. 24 % of FO3 named NPCs
    /// + 27 % of FO3 creatures author SCRI; FNV similar. The audit
    /// fixture mirrors the Three Dog (`MQGalaxyNewsRadio` broadcast
    /// trigger) shape — a thin NPC record where the only meaningful
    /// payload is the attached script.
    #[test]
    fn npc_extracts_scri_attached_script() {
        let subs = vec![
            sub(b"EDID", b"ThreeDog\0"),
            sub(b"SCRI", &0xDEAD_BEEFu32.to_le_bytes()),
        ];
        let n = parse_npc(0x000A_0001, &subs, GameKind::Fallout3NV);
        assert_eq!(n.script_form_id, 0xDEAD_BEEF);
        assert_eq!(n.editor_id, "ThreeDog");
    }

    /// Same arm fires for CREA records: `parse_npc` is shared between
    /// NPC_ and CREA (see `records/mod.rs:b"CREA"` dispatch). Asserts
    /// the parser doesn't gate SCRI on a record-type discriminator
    /// we don't carry.
    #[test]
    fn crea_extracts_scri_attached_script() {
        let subs = vec![
            sub(b"EDID", b"SuperMutantBrute\0"),
            sub(b"SCRI", &0xCAFE_0001u32.to_le_bytes()),
        ];
        let n = parse_npc(0x000B_0002, &subs, GameKind::Fallout3NV);
        assert_eq!(n.script_form_id, 0xCAFE_0001);
    }

    /// Zero-byte SCRI (rare but legal in modded content) must NOT
    /// fall through to a stale value; the field defaults to 0 and
    /// the arm is gated on `>= 4`, so a 0-length SCRI no-ops.
    #[test]
    fn npc_short_scri_is_ignored() {
        let subs = vec![sub(b"EDID", b"NoScript\0"), sub(b"SCRI", &[])];
        let n = parse_npc(0x000A_0003, &subs, GameKind::Fallout3NV);
        assert_eq!(n.script_form_id, 0);
    }

    /// Regression for #377 (FNV F2-03): ACBS `disposition_base` is an
    /// i16 at offset 20, not a u8. Pre-fix the parser pulled
    /// `sub.data[20]` (one byte), so values outside 0..=127 got their
    /// high byte dropped and the sign destroyed. Verify both a negative
    /// disposition (Raider-tier) and a positive value > 127 round-trip.
    #[test]
    fn npc_acbs_disposition_base_reads_signed_i16() {
        // ACBS layout (FNV NPC_, 24 bytes): flags u32, fatigue u16,
        // barter u16, level i16, calc_min u16, calc_max u16, speed_mult
        // u16, karma f32, disposition_base i16, template_flags u16.
        fn acbs_with_disposition(d: i16) -> Vec<u8> {
            let mut a = Vec::with_capacity(24);
            a.extend_from_slice(&0u32.to_le_bytes()); // flags
            a.extend_from_slice(&[0u8; 4]); // fatigue + barter
            a.extend_from_slice(&1i16.to_le_bytes()); // level
            a.extend_from_slice(&[0u8; 10]); // calc_min + calc_max + speed_mult + karma
            a.extend_from_slice(&d.to_le_bytes()); // disposition_base
            a.extend_from_slice(&0u16.to_le_bytes()); // template_flags
            a
        }

        let neg = parse_npc(
            0x700,
            &[
                sub(b"EDID", b"Raider\0"),
                sub(b"ACBS", &acbs_with_disposition(-40)),
            ],
            GameKind::Fallout3NV,
        );
        assert_eq!(
            neg.disposition_base, -40,
            "negative disposition must keep its sign"
        );

        let high = parse_npc(
            0x701,
            &[
                sub(b"EDID", b"Friendly\0"),
                sub(b"ACBS", &acbs_with_disposition(200)),
            ],
            GameKind::Fallout3NV,
        );
        assert_eq!(
            high.disposition_base, 200,
            "values > 127 must not lose the high byte"
        );
    }

    #[test]
    fn npc_vmad_flips_has_script() {
        // Regression: #369 — Skyrim NPCs with attached Papyrus scripts
        // were not discoverable. The presence-only `has_script` flag
        // is the audit's minimum-viable signal.
        let subs = vec![
            sub(b"EDID", b"ScriptedActor\0"),
            sub(b"VMAD", b"\x05\x00\x02\x00\x00\x00"),
        ];
        let n = parse_npc(0x501, &subs, GameKind::Skyrim);
        assert!(n.has_script);
    }

    #[test]
    fn npc_without_vmad_has_script_false() {
        // Sibling check — bare NPC must keep has_script at default.
        let subs = vec![sub(b"EDID", b"PlainActor\0")];
        let n = parse_npc(0x502, &subs, GameKind::Fallout3NV);
        assert!(!n.has_script);
    }

    #[test]
    fn fact_extracts_relations_and_ranks() {
        let mut xnam = Vec::new();
        xnam.extend_from_slice(&0x123u32.to_le_bytes());
        xnam.extend_from_slice(&(-50i32).to_le_bytes());
        xnam.extend_from_slice(&1u32.to_le_bytes()); // combat reaction = enemy

        let subs = vec![
            sub(b"EDID", b"NCR\0"),
            sub(b"FULL", b"NCR\0"),
            sub(b"DATA", &0x01u32.to_le_bytes()),
            sub(b"XNAM", &xnam),
            sub(b"MNAM", b"Recruit\0"),
            sub(b"MNAM", b"Trooper\0"),
            sub(b"MNAM", b"Veteran\0"),
        ];
        let f = parse_fact(0x42, &subs);
        assert_eq!(f.editor_id, "NCR");
        assert_eq!(f.flags, 0x01);
        assert_eq!(f.relations.len(), 1);
        assert_eq!(f.relations[0].other_faction, 0x123);
        assert_eq!(f.relations[0].modifier, -50);
        assert_eq!(f.relations[0].combat_reaction, 1);
        assert_eq!(f.ranks, vec!["Recruit", "Trooper", "Veteran"]);
    }

    /// Regression for #482: the reaction field is a 4-byte u32 per
    /// UESP spec, not a single byte. A typical u32 like `0x00000002`
    /// (ally) must round-trip through the parser correctly — this is
    /// the minimal "parser reads the right field width" check.
    ///
    /// Pre-fix the parser read only `sub.data[8]` (the low byte). For
    /// vanilla values 0..=3 the low byte happens to equal the full
    /// value, so the test passes with the old code too — its job is
    /// to document the spec and catch a future regression that goes
    /// back to byte access.
    #[test]
    fn fact_xnam_combat_reaction_reads_full_u32() {
        let mut xnam = Vec::new();
        xnam.extend_from_slice(&0x999u32.to_le_bytes()); // other faction
        xnam.extend_from_slice(&0i32.to_le_bytes()); // modifier
        xnam.extend_from_slice(&2u32.to_le_bytes()); // combat reaction = ally (full 4 bytes)

        let subs = vec![
            sub(b"EDID", b"AllyFaction\0"),
            sub(b"DATA", &0x00u32.to_le_bytes()),
            sub(b"XNAM", &xnam),
        ];
        let f = parse_fact(0x77, &subs);
        assert_eq!(f.relations.len(), 1);
        assert_eq!(
            f.relations[0].combat_reaction, 2,
            "ally (combat_reaction=2) must round-trip — parser must read 4 bytes"
        );
    }

    /// Regression for #481 (FNV-2-L1): FACT DATA is a single-byte
    /// flags field on FO3 / FNV per UESP. Pre-fix the parser read 4
    /// bytes, so any garbage in bytes 1..=3 of the DATA payload
    /// (variable tail, neighbour padding) leaked into the high 24
    /// bits. Only bits 0–2 are authoritative; verify the fix rejects
    /// the high bytes.
    #[test]
    fn fact_data_reads_only_low_byte() {
        // Simulate a DATA sub-record whose first byte holds the real
        // flags (bit 0 = hidden) and whose remaining bytes are the
        // FNV tail (e.g. `unknown: u8 + crime_gold_multiplier: f32`)
        // or just padding. Pre-fix the parser treated all 4 bytes as
        // flags and reported `0x0EFF_FF01`; post-fix it reports `0x01`.
        let data = [
            0x01u8, // real flags — bit 0 = hidden
            0xFFu8, 0xFFu8, 0xEFu8, // tail / padding bytes; must NOT become flags
        ];
        let subs = vec![sub(b"EDID", b"SpookyFaction\0"), sub(b"DATA", &data)];
        let f = parse_fact(0x88, &subs);
        assert_eq!(
            f.flags, 0x01,
            "only byte 0 of DATA carries flag bits on FO3 / FNV (#481)"
        );
    }

    /// Edge case: a zero-length DATA sub-record must not crash and
    /// must leave flags at the default (0).
    #[test]
    fn fact_data_empty_leaves_flags_default() {
        let subs = vec![sub(b"EDID", b"PlaceholderFaction\0"), sub(b"DATA", &[])];
        let f = parse_fact(0x89, &subs);
        assert_eq!(
            f.flags, 0,
            "empty DATA must not override the FactionRecord default"
        );
    }

    // ── #591 / FO4-DIM6-06 face-morph capture ──────────────────────────

    /// Build a 36-byte FMRS payload from 9 floats.
    fn fmrs_bytes(values: [f32; 9]) -> Vec<u8> {
        let mut out = Vec::with_capacity(36);
        for v in values {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    /// FMRI / FMRS appear in alternating order on the wire and pair
    /// 1-to-1 inside the parsed record. Shape verified against vanilla
    /// `Fallout4.esm` named-NPC sub-records (Hancock has 6 paired
    /// FMRI/FMRS; MQ101KelloggScene player duplicate has 30).
    #[test]
    fn npc_pairs_fmri_with_fmrs_in_order() {
        let s0 = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let s1 = [-1.0, -2.0, -3.0, -4.0, -5.0, -6.0, -7.0, -8.0, -9.0];
        let subs = vec![
            sub(b"EDID", b"NamedNpc\0"),
            sub(b"FMRI", &0xDEADu32.to_le_bytes()),
            sub(b"FMRS", &fmrs_bytes(s0)),
            sub(b"FMRI", &0xBEEFu32.to_le_bytes()),
            sub(b"FMRS", &fmrs_bytes(s1)),
        ];
        let n = parse_npc(0x600, &subs, GameKind::Fallout4);
        let face = n
            .face_morphs
            .as_ref()
            .expect("face_morphs must be Some when FMRI/FMRS present");
        assert_eq!(face.morphs.len(), 2);
        assert_eq!(face.morphs[0].form_id, 0xDEAD);
        assert_eq!(face.morphs[0].setting, s0);
        assert_eq!(face.morphs[1].form_id, 0xBEEF);
        assert_eq!(face.morphs[1].setting, s1);
    }

    /// MSDK / MSDV are parallel arrays: u32 keys + matching f32 values.
    /// One sub-record carries the full table on vanilla FO4 NPCs;
    /// `chunks_exact` walks every entry without dropping a tail.
    #[test]
    fn npc_msdk_msdv_walk_full_table() {
        let mut msdk = Vec::new();
        msdk.extend_from_slice(&0x10u32.to_le_bytes());
        msdk.extend_from_slice(&0x20u32.to_le_bytes());
        msdk.extend_from_slice(&0x30u32.to_le_bytes());
        let mut msdv = Vec::new();
        msdv.extend_from_slice(&0.25f32.to_le_bytes());
        msdv.extend_from_slice(&0.5f32.to_le_bytes());
        msdv.extend_from_slice(&0.75f32.to_le_bytes());
        let subs = vec![
            sub(b"EDID", b"Slidered\0"),
            sub(b"MSDK", &msdk),
            sub(b"MSDV", &msdv),
        ];
        let n = parse_npc(0x601, &subs, GameKind::Fallout4);
        let face = n.face_morphs.as_ref().unwrap();
        assert_eq!(face.slider_keys, vec![0x10, 0x20, 0x30]);
        assert_eq!(face.slider_values, vec![0.25, 0.5, 0.75]);
    }

    /// QNAM is 4 × f32 on FO4 NPCs (texture-lighting tint). HCLF / BCLF
    /// each are u32 FormIDs; multiple PNAM head-part FormIDs accumulate.
    #[test]
    fn npc_captures_qnam_hclf_bclf_pnam() {
        let mut qnam = Vec::new();
        for v in [0.6f32, 0.7, 0.8, 1.0] {
            qnam.extend_from_slice(&v.to_le_bytes());
        }
        let subs = vec![
            sub(b"EDID", b"FullFace\0"),
            sub(b"QNAM", &qnam),
            sub(b"HCLF", &0x1111u32.to_le_bytes()),
            sub(b"BCLF", &0x2222u32.to_le_bytes()),
            sub(b"PNAM", &0xAAAAu32.to_le_bytes()),
            sub(b"PNAM", &0xBBBBu32.to_le_bytes()),
            sub(b"PNAM", &0xCCCCu32.to_le_bytes()),
        ];
        let n = parse_npc(0x602, &subs, GameKind::Fallout4);
        let face = n.face_morphs.as_ref().unwrap();
        assert_eq!(face.texture_lighting, Some([0.6, 0.7, 0.8, 1.0]));
        assert_eq!(face.hair_color, Some(0x1111));
        assert_eq!(face.body_color, Some(0x2222));
        assert_eq!(face.head_parts, vec![0xAAAA, 0xBBBB, 0xCCCC]);
    }

    /// Face-morph block stays `None` for NPCs that ship none of the
    /// covered sub-records — pre-FO4 NPCs and FO4 generic settlers
    /// land in this branch. Regression pin so the
    /// `if !face.is_empty()` gate doesn't drift to `Some(Default)`.
    #[test]
    fn npc_without_face_subs_leaves_face_morphs_none() {
        let subs = vec![sub(b"EDID", b"PlainSettler\0")];
        let n = parse_npc(0x603, &subs, GameKind::Fallout4);
        assert!(n.face_morphs.is_none());
    }

    /// Mismatched FMRI/FMRS counts truncate to the shorter array
    /// instead of panicking. Defensive against malformed mod records;
    /// vanilla Bethesda content always pairs them 1-to-1.
    #[test]
    fn npc_mismatched_fmri_fmrs_truncates_to_shorter() {
        let s = [1.0; 9];
        // 3 FMRI but only 2 FMRS — should yield 2 paired entries.
        let subs = vec![
            sub(b"EDID", b"Malformed\0"),
            sub(b"FMRI", &0xA1u32.to_le_bytes()),
            sub(b"FMRI", &0xA2u32.to_le_bytes()),
            sub(b"FMRI", &0xA3u32.to_le_bytes()),
            sub(b"FMRS", &fmrs_bytes(s)),
            sub(b"FMRS", &fmrs_bytes(s)),
        ];
        let n = parse_npc(0x604, &subs, GameKind::Fallout4);
        let face = n.face_morphs.as_ref().unwrap();
        assert_eq!(face.morphs.len(), 2);
        assert_eq!(face.morphs[0].form_id, 0xA1);
        assert_eq!(face.morphs[1].form_id, 0xA2);
    }

    /// FNV NPC `PNAM` carries a single eyebrow HDPT FormID, NOT an
    /// FO4-style head-parts list. The `game`-aware gate keeps FNV
    /// PNAMs out of `face_morphs.head_parts`; M41.0 Phase 1a now
    /// captures them into `runtime_facegen.eyebrow_form_id` instead
    /// of dropping them on the floor.
    #[test]
    fn npc_fnv_pnam_lands_in_runtime_facegen_eyebrow() {
        let subs = vec![
            sub(b"EDID", b"FnvNpc\0"),
            // FNV-style PNAM: a single 4-byte eyebrow HDPT FormID.
            sub(b"PNAM", &0xDEADu32.to_le_bytes()),
        ];
        let n = parse_npc(0x606, &subs, GameKind::Fallout3NV);
        assert!(
            n.face_morphs.is_none(),
            "FNV PNAM must not populate face_morphs.head_parts (FO4 semantic)"
        );
        let recipe = n
            .runtime_facegen
            .as_ref()
            .expect("FNV PNAM must produce runtime_facegen");
        assert_eq!(recipe.eyebrow_form_id, Some(0xDEAD));
    }

    /// FGGS / FGGA / FGTS slider arrays land in fixed-size float
    /// arrays. Pre-Phase-3b the parser is the only consumer; the
    /// spawn-side morph evaluator picks them up from
    /// `runtime_facegen.fggs` directly.
    #[test]
    fn npc_fnv_fggs_fgga_fgts_populate_runtime_facegen() {
        let mut fggs = Vec::with_capacity(50 * 4);
        for i in 0..50 {
            fggs.extend_from_slice(&(i as f32 * 0.1).to_le_bytes());
        }
        let mut fgga = Vec::with_capacity(30 * 4);
        for i in 0..30 {
            fgga.extend_from_slice(&(i as f32 * -0.05).to_le_bytes());
        }
        let mut fgts = Vec::with_capacity(50 * 4);
        for i in 0..50 {
            fgts.extend_from_slice(&(i as f32 * 0.02).to_le_bytes());
        }
        let subs = vec![
            sub(b"EDID", b"SunnyMockup\0"),
            sub(b"FGGS", &fggs),
            sub(b"FGGA", &fgga),
            sub(b"FGTS", &fgts),
        ];
        let n = parse_npc(0x607, &subs, GameKind::Fallout3NV);
        let recipe = n
            .runtime_facegen
            .as_ref()
            .expect("FGGS/FGGA/FGTS must produce runtime_facegen");
        assert!((recipe.fggs[7] - 0.7).abs() < 1e-6);
        assert!((recipe.fgga[5] - -0.25).abs() < 1e-6);
        assert!((recipe.fgts[3] - 0.06).abs() < 1e-6);
        // Slot beyond the table stays at the default 0.0.
        assert_eq!(recipe.fggs[49], 4.9_f32);
        assert_eq!(recipe.fgga[29], -1.45_f32);
    }

    /// Short FGGS payload pads with zeros — the parser must not
    /// over-read or panic on truncated mod records.
    #[test]
    fn npc_fnv_short_fggs_pads_with_zero() {
        // 5 × f32 = 20 bytes; far short of the canonical 200.
        let mut fggs = Vec::with_capacity(5 * 4);
        for v in [1.0f32, 2.0, 3.0, 4.0, 5.0] {
            fggs.extend_from_slice(&v.to_le_bytes());
        }
        let subs = vec![sub(b"EDID", b"TruncMod\0"), sub(b"FGGS", &fggs)];
        let n = parse_npc(0x608, &subs, GameKind::Fallout3NV);
        let recipe = n.runtime_facegen.as_ref().unwrap();
        assert_eq!(recipe.fggs[0], 1.0);
        assert_eq!(recipe.fggs[4], 5.0);
        for v in &recipe.fggs[5..] {
            assert_eq!(*v, 0.0);
        }
    }

    /// HCLR / HNAM / LNAM / ENAM all land in `runtime_facegen` on
    /// kf-era games. HCLR's optional 4th byte is dropped per UESP.
    #[test]
    fn npc_fnv_hclr_hnam_lnam_enam_populate_runtime_facegen() {
        let subs = vec![
            sub(b"EDID", b"FullRecipe\0"),
            sub(b"HCLR", &[0x33, 0x55, 0x77, 0xFF]), // 4-byte; alpha dropped
            sub(b"HNAM", &0xCAFEu32.to_le_bytes()),
            sub(b"LNAM", &0xBEEFu32.to_le_bytes()),
            sub(b"ENAM", &0xF00Du32.to_le_bytes()),
        ];
        let n = parse_npc(0x609, &subs, GameKind::Fallout3NV);
        let recipe = n.runtime_facegen.as_ref().unwrap();
        assert_eq!(recipe.hair_color_rgb, Some([0x33, 0x55, 0x77]));
        assert_eq!(recipe.hair_form_id, Some(0xCAFE));
        assert_eq!(recipe.unused_lnam, Some(0xBEEF));
        assert_eq!(recipe.eyes_form_id, Some(0xF00D));
    }

    /// FO4 NPCs ship none of the kf-era recipe sub-records — and
    /// even if a malformed mod adds an FGGS payload to an FO4 NPC,
    /// the gate keeps `runtime_facegen` at `None`. Mirror property:
    /// kf-era NPCs with FO4-shaped FMRI/FMRS don't populate
    /// `face_morphs`. Both are pinned to keep the predicates honest.
    #[test]
    fn npc_runtime_facegen_and_face_morphs_are_mutually_exclusive() {
        let fggs = vec![0u8; 200];
        let subs_fo4 = vec![sub(b"EDID", b"Fo4Stray\0"), sub(b"FGGS", &fggs)];
        let n = parse_npc(0x60A, &subs_fo4, GameKind::Fallout4);
        assert!(n.runtime_facegen.is_none(), "FO4 must not parse FGGS");

        let mut fmrs = Vec::with_capacity(36);
        for v in [0.1f32, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9] {
            fmrs.extend_from_slice(&v.to_le_bytes());
        }
        let subs_fnv = vec![
            sub(b"EDID", b"FnvStray\0"),
            sub(b"FMRI", &0xDEADu32.to_le_bytes()),
            sub(b"FMRS", &fmrs),
        ];
        let n = parse_npc(0x60B, &subs_fnv, GameKind::Fallout3NV);
        assert!(n.face_morphs.is_none(), "FNV must not parse FMRI/FMRS");
    }

    /// Wrong-sized FMRS (e.g. a Skyrim record that ships a smaller
    /// payload, or a corrupt mod) is dropped silently — the length
    /// gate `>= 36` keeps malformed bytes from being re-interpreted as
    /// a partial setting array. The matched FMRI then becomes an
    /// orphan and the truncation rule above drops it too.
    #[test]
    fn npc_undersized_fmrs_is_dropped() {
        let subs = vec![
            sub(b"EDID", b"BadBytes\0"),
            sub(b"FMRI", &0xF00Du32.to_le_bytes()),
            sub(b"FMRS", &[0u8; 16]), // < 36 bytes
        ];
        let n = parse_npc(0x605, &subs, GameKind::Fallout4);
        // FMRI captured but FMRS dropped → mismatched (1 vs 0) →
        // truncate to 0 → no morphs → block is empty → None.
        assert!(n.face_morphs.is_none());
    }

    // ── #967 / OBL-D3-NEW-03 — RACE Oblivion-shape DATA + subs ────────

    /// Build a 36-byte Oblivion DATA payload: 8 × (u8 skill_index, u8
    /// bonus) + heightM + heightF + weightM + weightF + raceFlags.
    fn oblivion_data(
        pairs: [(u8, i8); 8],
        height: (f32, f32),
        weight: (f32, f32),
        flags: u32,
    ) -> Vec<u8> {
        let mut data = Vec::with_capacity(36);
        for (skill, bonus) in pairs {
            data.push(skill);
            data.push(bonus as u8);
        }
        data.extend_from_slice(&height.0.to_le_bytes());
        data.extend_from_slice(&height.1.to_le_bytes());
        data.extend_from_slice(&weight.0.to_le_bytes());
        data.extend_from_slice(&weight.1.to_le_bytes());
        data.extend_from_slice(&flags.to_le_bytes());
        assert_eq!(data.len(), 36);
        data
    }

    #[test]
    fn race_oblivion_data_reads_8_skill_pairs_plus_heights() {
        // Nord-like sample: bonuses on Blade(0x0E) + Block(0x0F) +
        // HeavyArmor(0x12) + Restoration(0x19) + LightArmor(0x1B);
        // remaining slots = 0xFF (Skill_None sentinel, should drop).
        let pairs = [
            (0x0E_u8, 10_i8), // Blade +10
            (0x0F, 5),        // Block +5
            (0x12, 5),        // HeavyArmor +5
            (0x19, 5),        // Restoration +5
            (0x1B, 5),        // LightArmor +5
            (0xFF, 0),        // Skill_None — drop
            (0xFF, 0),
            (0xFF, 0),
        ];
        let data = oblivion_data(pairs, (1.04, 1.0), (1.0, 1.0), 0x01);
        let subs = vec![
            sub(b"EDID", b"Nord\0"),
            sub(b"FULL", b"Nord\0"),
            sub(b"DATA", &data),
        ];
        let r = parse_race(0x10001, &subs, GameKind::Oblivion);
        // 5 real bonuses, 3 None-sentinel slots dropped.
        assert_eq!(r.skill_bonuses.len(), 5);
        assert_eq!(r.skill_bonuses[0], (0x0E, 10));
        assert_eq!(r.skill_bonuses[4], (0x1B, 5));
        assert!((r.base_height.0 - 1.04).abs() < 1e-6);
        assert!((r.base_height.1 - 1.0).abs() < 1e-6);
        assert_eq!(r.race_flags, 0x01);
    }

    #[test]
    fn race_oblivion_subrecords_captured() {
        let attr = [
            // male
            50, 40, 30, 40, 30, 50, 30, 50, //
            // female
            40, 40, 30, 50, 30, 50, 40, 50,
        ];
        let mut dnam = Vec::new();
        dnam.extend_from_slice(&0x000Au32.to_le_bytes()); // male hair
        dnam.extend_from_slice(&0x000Bu32.to_le_bytes()); // female hair
        let mut vnam = Vec::new();
        vnam.extend_from_slice(&0x0100u32.to_le_bytes());
        vnam.extend_from_slice(&0x0101u32.to_le_bytes());
        let pnam = 5.0_f32.to_le_bytes();
        let unam = 3.0_f32.to_le_bytes();
        let mut xnam_breton = Vec::new();
        xnam_breton.extend_from_slice(&0x10001u32.to_le_bytes()); // other race
        xnam_breton.extend_from_slice(&(-5_i32).to_le_bytes());
        let data = oblivion_data([(0xFF, 0); 8], (1.0, 1.0), (1.0, 1.0), 0);
        let subs = vec![
            sub(b"EDID", b"Breton\0"),
            sub(b"DATA", &data),
            sub(b"ATTR", &attr),
            sub(b"DNAM", &dnam),
            sub(b"VNAM", &vnam),
            sub(b"PNAM", &pnam),
            sub(b"UNAM", &unam),
            sub(b"XNAM", &xnam_breton),
        ];
        let r = parse_race(0x10002, &subs, GameKind::Oblivion);
        let a = r.base_attributes.expect("ATTR captured");
        assert_eq!(a.male.strength, 50);
        assert_eq!(a.male.luck, 50);
        assert_eq!(a.female.strength, 40);
        assert_eq!(r.default_hair, Some((0x000A, 0x000B)));
        assert_eq!(r.voice_forms, Some((0x0100, 0x0101)));
        assert_eq!(r.facegen_main_clamp, Some(5.0));
        assert_eq!(r.facegen_face_clamp, Some(3.0));
        assert_eq!(r.race_reactions, vec![(0x10001, -5)]);
    }

    /// SIBLING gate (audit completeness check #1) — FNV-tagged RACE
    /// reuses the 36-byte DATA shape per OpenMW, but the Oblivion-only
    /// sub-records (ATTR / DNAM / VNAM / PNAM / UNAM / XNAM) MUST be
    /// dropped when `game != GameKind::Oblivion`. Otherwise a future
    /// loader walking the same arm on TES5 would mis-read VNAM's
    /// 4-byte equipment-type-flags payload as 2 form IDs.
    #[test]
    fn race_oblivion_subrecords_skipped_on_non_oblivion_games() {
        let attr = [10u8; 16];
        let mut dnam = Vec::new();
        dnam.extend_from_slice(&0x000Au32.to_le_bytes());
        dnam.extend_from_slice(&0x000Bu32.to_le_bytes());
        let data = oblivion_data([(0xFF, 0); 8], (1.0, 1.0), (1.0, 1.0), 0);
        let subs = vec![
            sub(b"EDID", b"FnvHuman\0"),
            sub(b"DATA", &data),
            sub(b"ATTR", &attr),
            sub(b"DNAM", &dnam),
        ];
        let r = parse_race(0x10003, &subs, GameKind::Fallout3NV);
        assert!(r.base_attributes.is_none());
        assert!(r.default_hair.is_none());
        // DATA path still runs — FNV shares the 36-byte shape.
        assert_eq!(r.race_flags, 0);
    }

    /// Multiple XNAM sub-records — each pair appends to the
    /// `race_reactions` list in authoring order.
    #[test]
    fn race_multiple_xnam_pairs_collected() {
        let data = oblivion_data([(0xFF, 0); 8], (1.0, 1.0), (1.0, 1.0), 0);
        let mut x1 = Vec::new();
        x1.extend_from_slice(&0x10010u32.to_le_bytes());
        x1.extend_from_slice(&5_i32.to_le_bytes());
        let mut x2 = Vec::new();
        x2.extend_from_slice(&0x10011u32.to_le_bytes());
        x2.extend_from_slice(&(-3_i32).to_le_bytes());
        let subs = vec![
            sub(b"EDID", b"Imperial\0"),
            sub(b"DATA", &data),
            sub(b"XNAM", &x1),
            sub(b"XNAM", &x2),
        ];
        let r = parse_race(0x10004, &subs, GameKind::Oblivion);
        assert_eq!(r.race_reactions.len(), 2);
        assert_eq!(r.race_reactions[0], (0x10010, 5));
        assert_eq!(r.race_reactions[1], (0x10011, -3));
    }

    // ── #968 / OBL-D3-NEW-04 — CLAS Oblivion-shape DATA ──────────────

    /// Build a 52-byte Oblivion CLAS DATA payload per the empirical
    /// vanilla layout (#968):
    ///   2 × u32 primary attributes (8 B)
    ///   u32 specialization         (4 B)
    ///   7 × u32 major skills       (28 B)
    ///   u32 flags                  (4 B)
    ///   u32 services               (4 B)
    ///   i8 trainer + u8 level + 2 B pad (4 B)
    fn oblivion_clas_data(attrs: (u32, u32), spec: u32, majors: [u32; 7], flags: u32) -> Vec<u8> {
        let mut data = Vec::with_capacity(52);
        data.extend_from_slice(&attrs.0.to_le_bytes());
        data.extend_from_slice(&attrs.1.to_le_bytes());
        data.extend_from_slice(&spec.to_le_bytes());
        for s in majors {
            data.extend_from_slice(&s.to_le_bytes());
        }
        data.extend_from_slice(&flags.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes()); // services
        data.extend_from_slice(&[0u8; 4]); // trainer skill + level + 2 pad
        assert_eq!(data.len(), 52);
        data
    }

    #[test]
    fn clas_oblivion_knight_round_trips() {
        // Knight (form 0x836 in vanilla Oblivion.esm) — primary
        // attrs (Strength=0, Personality=6), specialization 0 = Combat,
        // 7 majors per the empirical probe.
        let data = oblivion_clas_data(
            (0, 6),
            0,
            [0x0F, 0x17, 0x12, 0x10, 0x0E, 0x20, 0x11],
            0x01, // Playable
        );
        let subs = vec![
            sub(b"EDID", b"Knight\0"),
            sub(b"FULL", b"Knight\0"),
            sub(b"DATA", &data),
        ];
        let c = parse_clas(0x836, &subs, GameKind::Oblivion);
        assert_eq!(c.primary_attributes, Some((0, 6)));
        assert_eq!(c.specialization, Some(0));
        assert_eq!(
            c.major_skills,
            vec![0x0F, 0x17, 0x12, 0x10, 0x0E, 0x20, 0x11]
        );
        assert_eq!(c.flags_oblivion, Some(0x01));
        // FNV-shape fields stay empty on Oblivion.
        assert!(c.tag_skills.is_empty());
        assert_eq!(c.attribute_weights, [0u8; 7]);
    }

    /// SIBLING gate (audit completeness check) — FNV-tagged CLAS hits
    /// the 35-byte arm, NOT the Oblivion 52-byte arm. Even if the
    /// payload happens to be 52 bytes (shouldn't be in real FNV
    /// content, but defensive), the game gate routes correctly.
    #[test]
    fn clas_fnv_path_unchanged() {
        let mut data = Vec::with_capacity(35);
        // 4 × u32 tag skill form IDs
        data.extend_from_slice(&0xC0DE_0001u32.to_le_bytes());
        data.extend_from_slice(&0xC0DE_0002u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes()); // (filtered by !=0)
        data.extend_from_slice(&0xC0DE_0003u32.to_le_bytes());
        // flags + services + skill/level/teaches/max (16 + 4 = 20 bytes)
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&[0u8; 4]);
        // 7 attribute weights
        data.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(data.len(), 35);
        let subs = vec![sub(b"EDID", b"NCRTrooper\0"), sub(b"DATA", &data)];
        let c = parse_clas(0x600, &subs, GameKind::Fallout3NV);
        assert_eq!(c.tag_skills, vec![0xC0DE_0001, 0xC0DE_0002, 0xC0DE_0003]);
        assert_eq!(c.attribute_weights, [1, 2, 3, 4, 5, 6, 7]);
        // Oblivion-only fields stay None.
        assert!(c.primary_attributes.is_none());
        assert!(c.specialization.is_none());
        assert!(c.major_skills.is_empty());
        assert!(c.flags_oblivion.is_none());
    }

    /// Boundary: a malformed Oblivion CLAS with < 52-byte DATA must
    /// fall through cleanly (no panic, no off-the-end read). Both
    /// game-specific arms gate on length.
    #[test]
    fn clas_oblivion_short_data_drops_silently() {
        let data = vec![0u8; 40]; // less than 52
        let subs = vec![sub(b"EDID", b"BadClass\0"), sub(b"DATA", &data)];
        let c = parse_clas(0x837, &subs, GameKind::Oblivion);
        // No arm fired; nothing crashed; all Oblivion-only fields stay None.
        assert!(c.primary_attributes.is_none());
        assert!(c.major_skills.is_empty());
        // FNV arm would have fired at >= 35 — but we're game=Oblivion,
        // so the gate skipped it.
        assert!(c.tag_skills.is_empty());
    }
}
