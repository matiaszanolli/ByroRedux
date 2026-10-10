use super::super::common::{remap_fid, CommonNamedFields};
use crate::esm::reader::{FormIdRemap, GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;
use super::{
    decode_ai_data, ActorAiData, CreatureStats, FactionMembership, NpcInventoryEntry,
    CREATURE_DATA_LEN,
};

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
    /// / makeup deltas via the race's `.egt` file. Parsed and carried
    /// here, but #3544 (SK-D3-02): no face-tint compositor exists yet
    /// to apply them — `crates/facegen::egt` has no consumer.
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

#[derive(Debug, Clone, Default)]
pub struct NpcRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// TES4 "Starts Dead" base flag (`0x80000`, #5013). True only when this
    /// record came from an Oblivion `NPC_`/`CREA` group whose record header
    /// carries the bit (xEdit `wbDefinitionsTES4.pas` — `CREA:1942`,
    /// `NPC_:2783`); every other game leaves it `false` because the same
    /// header bit means something else on their base actors. Placements of
    /// such a base spawn dead through the cell loader's `apply_starts_dead`.
    ///
    /// FO3 / FNV set it from the BASE's `DATA` health instead (#5005): a
    /// base with health ≤ 0 is a corpse wherever it is placed — xNVSE's
    /// reverse-engineered `TESObjectREFR::Unk_8B` ("IsDead = HasNoHealth
    /// (baseForm health <= 0 …)", `nvse/nvse/GameObjects.h:99`) and the
    /// Oblivion-generation authoring rule it descends from ("A dead actor
    /// is simply, an actor who is defined in the CS by having 0 health",
    /// CS wiki "Creating Dead Actors"). The xNVSE comment's second
    /// disjunct ("or Flags bit23 set") is deliberately NOT applied: on
    /// vanilla data that bit marks healthy live NPCs (`NVMerchantMaleB`,
    /// `VES09Merchant03`) and, on creatures, `kFlags_CreatureImmobile`
    /// (turrets, ZAX eyes) — killable actors either way. `XRGD` on the
    /// placement is pose-only data (7 FNV / 6 FO3 vanilla refs carry it
    /// over live bases — Fisto, `VHDKimballRangerAmbushed01`), never the
    /// death marker.
    pub starts_dead: bool,
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
    /// FO3 / FNV `NPC_` `DATA` Base Health (`i32 @ 0`, #5005). `None` for
    /// `CREA` (see [`Self::creature_stats`]) and every other game. Health
    /// ≤ 0 stamps [`Self::starts_dead`] — the sourced FO3/FNV corpse rule;
    /// a positive value stays here for the actor-value seeding path.
    pub data_base_health: Option<i32>,
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
    /// #5359 — per-NPC intrinsic skin ARMO (`NPC_.WNAM`, Skyrim+). The
    /// CK's Traits-tab **Skin** field: the authored per-actor body that
    /// overrides the race's `RACE.WNAM` default ([`RaceRecord::
    /// default_skin`]) — mesh and texture set through the ARMO → ARMA
    /// chain. 664 of 5,118 vanilla Skyrim `NPC_` records author it
    /// (every Draugr variant skin, Alduin's `skinDragonAlduin`, the
    /// Falmer variants, horse hides…); template-flag `0x0001` (Use
    /// Traits) governs inheriting it, so consumers read it off the
    /// [`ResolvedNpc`](crate::equip::ResolvedNpc) traits terminal.
    /// `None` on games whose `NPC_` ships no `WNAM` (Oblivion / FO3 /
    /// FNV equip from the inventory list) or when the record omits it.
    pub worn_skin: Option<u32>,
    /// AI packages (`PKID` sub-records, in priority order).
    pub ai_packages: Vec<u32>,
    /// #4414 — the actor's `AIDT` combat disposition, canonical. `None`
    /// when the record has no `AIDT` or the game's layout is not decoded
    /// (Oblivion's 0-100 aggression scale feeds a disposition formula with
    /// no settled source).
    pub ai_data: Option<ActorAiData>,
    /// #4415 — the actor's spell list (`SPLO`, one FormID each, load-order
    /// remapped): SPEL / SHOU / LVSP targets, in authored order. FO3/FNV
    /// call it the "Actor Effect" list and author it on NPC_ and CREA;
    /// Skyrim/FO4 precede it with an `SPCT` count, which is not needed.
    pub spells: Vec<u32>,
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
    pub script_instance: Option<super::super::script_instance::ScriptInstanceData>,
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
    /// Template-inheritance bitmask from `ACBS`. Sourced from xEdit's
    /// `wbDefinitionsFNV.pas` for the FNV/FO3 offset and bit meanings, but
    /// since `7445506c` these same three constants (`equip.rs`'s
    /// `TEMPLATE_FLAG_*`) gate `derive_npc_actor_values` for **every**
    /// game, at three different ACBS offsets: FO4 `u16 @ 14`, Skyrim
    /// `u16 @ 18`, FNV/FO3 `u16 @ 22` (see the three ACBS parse arms below).
    /// Each bit gates whether one category of fields is
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
    /// #5498 — FO4 `TPTA` "Template Actors": 13 per-flag template
    /// FormIDs, one per `template_flags` bit in xEdit's
    /// `wbTemplateFlags` order (Traits, Stats, Factions, Spell List,
    /// AI Data, AI Packages, Model/Animation, Base Data, Inventory,
    /// Script, Def Package List, Attack Data, Keywords —
    /// `wbDefinitionsFO4.pas:10350-10369`). `TPLT` stays the *default*
    /// template; a non-zero `tpta[bit]` overrides it for that one flag
    /// ("The FO4 CK Templates tab has a Template Form per flag; a null
    /// entry falls back to the default template"). Remapped into global
    /// load-order space like every cross-record FormID. FO4-only data;
    /// all-zero elsewhere, so the resolution needs no game branch. The
    /// census: 763 `Fallout4.esm` records have a per-flag override that
    /// differs from their TPLT (Traits 464, Inventory 249, Stats 136…),
    /// 1,157 across all seven masters — pre-fix every one resolved
    /// through TPLT and spawned with the wrong gear, stats, AI, or race.
    pub template_actors: [u32; 13],
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
        // Set by the dispatch layer from the record header's game-gated
        // Starts-Dead bit — `parse_npc` sees no header (#5013).
        starts_dead: false,
        model_path: common.model_path,
        body_part_models: Vec::new(),
        is_creature: false,
        creature_stats: None,
        data_base_health: None,
        race_form_id: 0,
        class_form_id: 0,
        voice_form_id: 0,
        factions: Vec::new(),
        inventory: Vec::new(),
        default_outfit: None,
        worn_skin: None,
        ai_packages: Vec::new(),
        ai_data: None,
        spells: Vec::new(),
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
        template_actors: [0; 13],
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
            // #5005 — same sourced rule as the NPC_ arm below: a creature
            // base with health ≤ 0 is a corpse wherever it is placed.
            if health <= 0 {
                record.starts_dead = true;
            }
        }
        // #5005 — FO3 / FNV `NPC_` `DATA`: i32 Base Health + the 7 SPECIAL
        // attributes (11 bytes), optionally padded by a legacy unused tail
        // (25 bytes). The Base Health is the FO3/FNV corpse marker — see
        // [`NpcRecord::starts_dead`] for the sources, and the field doc on
        // [`NpcRecord::data_base_health`] for why only the health term of
        // the xNVSE comment is applied.
        b"DATA"
            if matches!(game, GameKind::Fallout3NV) && matches!(sub.data.len(), 11 | 25) =>
        {
            let health = SubReader::new(&sub.data).i32_or_default();
            record.data_base_health = Some(health);
            if health <= 0 {
                record.starts_dead = true;
            }
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
        b"CNTO" if sub.data.len() >= super::super::container::InventoryEntry::WIRE_SIZE => {
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
        b"SPLO" if sub.data.len() >= 4 => {
            let raw = SubReader::new(&sub.data).u32_or_default();
            record.spells.push(remap_fid(raw, remap));
        }
        b"AIDT" => record.ai_data = decode_ai_data(&sub.data, game),
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
        // WNAM — Skyrim+ per-NPC skin ARMO (#5359): the CK Traits-tab
        // "Skin" field, overriding the race's RACE.WNAM default as the
        // intrinsic body layer. Pre-Skyrim games never emit NPC_ WNAM,
        // so no game gate is needed (same posture as DOFT above). Load-
        // order remapped like every cross-record FormID — the target
        // lives in `EsmIndex.items`, keyed in global space.
        b"WNAM" if sub.data.len() >= 4 => {
            record.worn_skin = SubReader::new(&sub.data)
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
        // #5498 — FO4 TPTA 'Template Actors': 13 per-flag template
        // FormIDs in wbTemplateFlags order (see the field doc). A short
        // payload keeps its authored prefix; the missing entries stay 0
        // ("use the default template"), same as an absent sub.
        b"TPTA" => {
            for (slot, raw) in sub.data.as_chunks::<4>().0.into_iter().take(13).enumerate() {
                record.template_actors[slot] = remap_fid(u32::from_le_bytes(*raw), remap);
            }
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
            for chunk in sub.data.as_chunks::<4>().0 {
                face.slider_keys
                    .push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
            }
        }
        b"MSDV" if sub.data.len() >= 4 => {
            for chunk in sub.data.as_chunks::<4>().0 {
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
            for chunk in sub.data.as_chunks::<8>().0 {
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
/// parsed with an empty `perks` list, the NPC spawn path then skipped the
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
