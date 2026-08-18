//! `EsmIndex` aggregate struct + per-category bookkeeping (`categories`,
//! `total`, `category_breakdown`, `merge_from`).
//!
//! Lifted out of the pre-#1118 monolithic `records/mod.rs` (TD9-003).
//! The struct surface is byte-identical to the original; only the
//! module location changed. `pub use` re-export from `mod.rs` keeps
//! `byroredux_plugin::esm::records::EsmIndex` valid for every external
//! caller.

use super::super::cell::EsmCellIndex;
use super::super::reader::GameKind;
use super::{
    ActiRecord, ArmaRecord, AvifRecord, BptdRecord, ClassRecord, ClimateRecord, CobjRecord,
    ContainerRecord, CstyRecord, DialRecord, EcznRecord, EfshRecord, EnchRecord, ExplRecord,
    EyesRecord, FactionRecord, FlstRecord, GameSetting, GlobalRecord, HairRecord, HdptRecord,
    IdleRecord, ImadRecord, ImgsRecord, ImodRecord, IpctRecord, IpdsRecord, ItemRecord,
    LeveledList, LgtmRecord, MesgRecord, MgefRecord, MinimalEsmRecord, NaviRecord, NavmRecord,
    NpcRecord, OtftRecord, PackRecord, PerkRecord, ProjRecord, QustRecord, RaceRecord, RegnRecord,
    RepuRecord, ScenRecord, ScriptRecord, SlgmRecord, SpelRecord, TermRecord, TreeRecord,
    WatrRecord, WeatherRecord,
};
use std::collections::HashMap;

/// One entry in the [`EsmIndex::categories`] table: a display label, count
/// accessor, and merge operation. Keeping all three together makes the table
/// the single source of truth for top-level record maps.
pub type CategoryEntry = (
    &'static str,
    fn(&EsmIndex) -> usize,
    fn(&mut EsmIndex, &mut EsmIndex),
);

macro_rules! map_category {
    ($label:literal, $field:ident) => {
        (
            $label,
            |index: &EsmIndex| index.$field.len(),
            |target: &mut EsmIndex, source: &mut EsmIndex| {
                target.$field.extend(std::mem::take(&mut source.$field));
            },
        )
    };
}

macro_rules! cell_category {
    ($label:literal, $field:ident) => {
        (
            $label,
            |index: &EsmIndex| index.cells.$field.len(),
            |_target: &mut EsmIndex, _source: &mut EsmIndex| {},
        )
    };
}

/// Aggregated index of every record category we currently parse.
///
/// `cells` retains the existing structure used by the cell loader and
/// renderer. The other maps are new in M24.
#[derive(Debug, Default)]
pub struct EsmIndex {
    /// Game variant this index was parsed against, derived from the
    /// TES4 HEDR `Version` and record-header version by
    /// [`GameKind::from_header`]. Carried
    /// forward through [`merge_from`] (last-write-wins — multi-plugin
    /// loads always share a single game variant in practice).
    /// Consumed by the cell loader's NPC dispatch (M41.0 Phase 1b)
    /// to gate runtime-FaceGen vs pre-baked-FaceGen spawn paths.
    pub game: GameKind,
    /// Canonical character policy selected once from the source master header.
    /// Unlike `game`, this deliberately distinguishes FO3 from New Vegas.
    pub character_rules: byroredux_core::character::CharacterRulesProfile,
    pub cells: EsmCellIndex,
    pub items: HashMap<u32, ItemRecord>,
    /// Fallout 4 / 76 OMOD FormID → optional loose MISC item (`LNAM`).
    /// Kept separate from `items`: the OMOD is a modification definition,
    /// while the referenced MISC is the object carried in the Mods category.
    /// A zero value records an override that deliberately clears LNAM.
    pub object_mod_loose_items: HashMap<u32, u32>,
    pub containers: HashMap<u32, ContainerRecord>,
    pub leveled_items: HashMap<u32, LeveledList>,
    pub leveled_npcs: HashMap<u32, LeveledList>,
    /// Leveled creature lists (CREA spawn tables). Byte-compatible with
    /// LVLI / LVLN so the same `parse_leveled_list` handles them. FO3
    /// uses LVLC for most enemy encounters; FNV migrated the bulk to
    /// LVLN but still ships some legacy LVLC entries. See #448.
    pub leveled_creatures: HashMap<u32, LeveledList>,
    pub npcs: HashMap<u32, NpcRecord>,
    /// Creature base records (FO3 bestiary: super mutants, deathclaws,
    /// radroaches, robots, brahmin, etc.). CREA shares EDID / FULL /
    /// MODL / RNAM / CNAM / SNAM / CNTO / PKID / ACBS with NPC_ so the
    /// same `parse_npc` populates `NpcRecord`; the only divergence is
    /// ACBS flags semantics, which the current reader ignores.
    /// FNV migrated most combat to NPC_ but vanilla still keeps ~70
    /// CREA entries for legacy content. See #442.
    pub creatures: HashMap<u32, NpcRecord>,
    pub races: HashMap<u32, RaceRecord>,
    pub classes: HashMap<u32, ClassRecord>,
    pub factions: HashMap<u32, FactionRecord>,
    pub globals: HashMap<u32, GlobalRecord>,
    pub game_settings: HashMap<u32, GameSetting>,
    pub weathers: HashMap<u32, WeatherRecord>,
    pub climates: HashMap<u32, ClimateRecord>,
    /// FO3 / FNV / Oblivion pre-Papyrus SCPT bytecode records (#443).
    /// Every `SCRI` FormID on NPC_ / CONT / item / ACTI records resolves
    /// here instead of dangling. The bytecode itself (`compiled`) is
    /// stored opaquely — an ECS-native runtime lands separately.
    pub scripts: HashMap<u32, ScriptRecord>,
    // ── Supplementary records (stubs, #458) ──────────────────────────
    //
    // Nine record types that pre-#458 fell through to the catch-all
    // skip. Each map stores a minimal extraction (EDID + a handful of
    // form refs + scalar fields) — enough for dangling references
    // into these records to resolve at lookup time. Full per-record
    // decoding lands with the consuming subsystem.
    /// `WATR` water type records — referenced by `CELL.XCWT`.
    pub waters: HashMap<u32, WatrRecord>,
    /// `NAVI` navigation mesh master.
    pub navi_info: HashMap<u32, NaviRecord>,
    /// `NAVM` per-cell navigation meshes.
    pub navmeshes: HashMap<u32, NavmRecord>,
    /// `REGN` worldspace regions.
    pub regions: HashMap<u32, RegnRecord>,
    /// `ECZN` encounter-zone descriptors.
    pub encounter_zones: HashMap<u32, EcznRecord>,
    /// `LGTM` lighting templates — ties to #379 (per-field inheritance
    /// fallback on cells without XCLL).
    pub lighting_templates: HashMap<u32, LgtmRecord>,
    /// `IMGS` image-space records — Skyrim per-cell HDR / cinematic
    /// tone-map LUTs referenced by `CELL.XCIM`. Pre-#624 the entire
    /// top-level group fell through to the catch-all skip and every
    /// XCIM cross-reference dangled. The current parse captures
    /// `EDID` + raw `DNAM` payload so a future per-cell HDR-LUT
    /// consumer (M48) can decode the tone-map fields lazily.
    pub image_spaces: HashMap<u32, ImgsRecord>,
    /// `HDPT` head-part records (FaceGen).
    pub head_parts: HashMap<u32, HdptRecord>,
    /// `EYES` eye definitions (FO3/FNV NPC_ face variation).
    pub eyes: HashMap<u32, EyesRecord>,
    /// `HAIR` hair definitions (FO3/FNV NPC_ face variation).
    pub hair: HashMap<u32, HairRecord>,
    // ── AI / dialogue / effect stubs (#446, #447) ───────────────────
    /// `PACK` AI packages — 30-procedure scheduling system referenced
    /// by `NpcRecord.ai_packages`.
    pub packages: HashMap<u32, PackRecord>,
    /// `QUST` quests — Story Manager / Radiant Story entry points.
    pub quests: HashMap<u32, QustRecord>,
    /// Skyrim+ `SCEN` records — condition-gated phase timelines containing
    /// dialogue, package, and timer actions plus Papyrus event fragments.
    pub scenes: HashMap<u32, ScenRecord>,
    /// `DIAL` dialogue topics — owned by quests via QSTI refs. INFO
    /// children land on `DialRecord.infos` via the dedicated
    /// `extract_dial_with_info` walker (group_type == 7 Topic
    /// Children sub-GRUPs). See #631.
    pub dialogues: HashMap<u32, DialRecord>,
    /// `MESG` quest messages / tutorial popups.
    pub messages: HashMap<u32, MesgRecord>,
    /// `PERK` perks + traits — condition-gated entry-point producers.
    pub perks: HashMap<u32, PerkRecord>,
    /// `SPEL` spells / abilities / auto-cast effects.
    pub spells: HashMap<u32, SpelRecord>,
    /// `ENCH` enchantment records — `WEAP/AMMO/ARMO.eitm` cross-refs
    /// resolve here. Pre-#629 the entire top-level group fell through
    /// to the catch-all skip and every weapon enchantment dangled
    /// (Pulse Gun, This Machine, Holorifle on FNV; the full Skyrim
    /// weapon-enchant table). See FNV-D2-01.
    pub enchantments: HashMap<u32, EnchRecord>,
    /// `MGEF` magic effects — universal bridge for Actor Value mods.
    pub magic_effects: HashMap<u32, MgefRecord>,
    /// Oblivion-only secondary index: 4-char effect code → MGEF FormID.
    /// On Oblivion, SPEL/ENCH/ALCH/INGR cross-reference effects via
    /// `EFID` whose raw bytes ARE the 4-char effect code (e.g., `b"FIDG"`
    /// for Feather, `b"DGFA"` for Damage Fatigue), NOT a u32 FormID.
    /// A FormID-keyed lookup on Oblivion EFID values resolves to
    /// garbage; this secondary map lets a consumer
    /// `magic_effects_by_code[code]` → MGEF FormID → `magic_effects[fid]`.
    /// Populated only when `game == GameKind::Oblivion` and the EDID
    /// is exactly 4 ASCII bytes (the fixed-format Oblivion shape).
    /// FO3/FNV/Skyrim+ leave this map empty and use the FormID-keyed
    /// `magic_effects` map directly. See #969 / OBL-D3-NEW-05.
    pub magic_effects_by_code: HashMap<[u8; 4], u32>,
    /// `AVIF` actor-value definitions — SPECIAL attributes, governed
    /// skills, resistances, resources. Cross-referenced by NPC
    /// `skill_bonuses`, BOOK skill-book teach forms, perk entry-point
    /// math, VATS attack costs, and ~300 condition predicates. Pre-fix
    /// the whole top-level group fell through to the catch-all skip.
    /// See #519.
    pub actor_values: HashMap<u32, AvifRecord>,
    // ── Activators / terminals (#521) ───────────────────────────────
    /// `ACTI` activator records — wall switches, vending machines,
    /// lever-activated doors, anything "use"-able that isn't a
    /// container/door/NPC. SCRI cross-references resolve here instead
    /// of dangling.
    pub activators: HashMap<u32, ActiRecord>,
    /// `TERM` terminal records — vault/military consoles. Menu items,
    /// password, and body text captured so a future terminal-interaction
    /// system doesn't have to re-parse them.
    pub terminals: HashMap<u32, TermRecord>,
    /// `FLST` FormID list records — flat arrays of form IDs referenced
    /// by `IsInList` perk-entry-point conditions, COBJ recipe
    /// ingredient lists, the FNV CCRD/CDCK Caravan deck, and quest
    /// objective filters. Pre-#630 the entire top-level group fell
    /// through to the catch-all skip and every `IsInList <flst>`
    /// returned "not in list" because the lookup map was empty —
    /// silently disabling ~50 vanilla FNV PERKs and the Caravan
    /// mini-game. See audit `FNV-D2-02` / #630.
    pub form_lists: HashMap<u32, FlstRecord>,
    // ── #808 / FNV-D2-NEW-01 — gameplay-critical record stubs ──────
    //
    // Five record types that gate FNV gameplay subsystems: weapon
    // firing (PROJ), visual effects (EFSH), weapon mods (IMOD),
    // race-specific armor (ARMA), and dismemberment (BPTD). Pre-fix
    // each of these top-level groups fell through to the catch-all
    // skip — every WEAP→PROJ link, every IMOD attachment, every
    // EFSH visual reference, every ARMO→ARMA chain, every NPC
    // dismemberment route dangled.
    /// `PROJ` projectile records — every WEAP references a PROJ for
    /// muzzle velocity, gravity, AoE, lifetime, impact behavior.
    pub projectiles: HashMap<u32, ProjRecord>,
    /// `EFSH` effect-shader records — visual effects for spells,
    /// grenades, muzzle flashes, blood splatter. Referenced from
    /// MGEF / SPEL / EXPL.
    pub effect_shaders: HashMap<u32, EfshRecord>,
    /// `IMOD` item-mod records (FNV-CORE) — weapon attachments
    /// (sights, suppressors, extended mags, scopes).
    pub item_mods: HashMap<u32, ImodRecord>,
    /// `ARMA` armor-addon records — race-specific biped slot
    /// variants for ARMO. Drives ARMO → ARMA → race-specific MODL
    /// rendering chain on non-default-race NPCs.
    pub armor_addons: HashMap<u32, ArmaRecord>,
    /// `OTFT` outfit records (Skyrim+) — flat lists of armor or
    /// leveled-item FormIDs that compose an NPC's default-equipped
    /// set. Referenced via `NPC_.DOFT` / `NPC_.SOFT`. Empty on
    /// pre-Skyrim games (those equip from inventory directly).
    /// See #896.
    pub outfits: HashMap<u32, OtftRecord>,
    /// `BPTD` body-part-data records — per-NPC dismemberment
    /// routing (head, torso, limbs) + biped slot count.
    pub body_parts: HashMap<u32, BptdRecord>,
    /// `TREE` tree base records — Oblivion / FO3 / FNV reference an
    /// external SpeedTree binary (`.spt`) here; Skyrim+ points at a
    /// regular NIF rooted at `BSTreeNode`. Pre-fix this group fell
    /// through the generic MODL-only path alongside STAT / FLOR / etc.,
    /// dropping ICON / SNAM / CNAM / BNAM / PFIG silently. The
    /// SpeedTree compatibility plan's Phase 1 consumes this map for
    /// leaf-texture / wind-parameter / canopy-param routing.
    pub trees: HashMap<u32, TreeRecord>,
    // ── #809 / FNV-D2-NEW-02 — supporting record stubs ──────────────
    //
    // Seven records that gate FNV NPC AI / crafting / impact-effect
    // / faction-reputation subsystems. Pre-fix each of these top-level
    // groups fell through to the catch-all skip.
    /// `REPU` reputation records (FNV-CORE) — NCR / Legion / Powder
    /// Gangers / Boomers / Brotherhood / Followers. Drives the
    /// faction-reputation system and quest gating.
    pub reputations: HashMap<u32, RepuRecord>,
    /// `EXPL` explosion records — frag grenades, mines, explosive
    /// ammo blast effects. Linked from PROJ via PROJ→EXPL→EFSH.
    pub explosions: HashMap<u32, ExplRecord>,
    /// `CSTY` combat-style records — per-NPC AI behavior profile
    /// (aggression, stealth preference, ranged vs melee).
    pub combat_styles: HashMap<u32, CstyRecord>,
    /// `IDLE` idle-animation records — NPC behavior tree refs
    /// ("lean against wall", "smoke", "drink", etc.).
    pub idle_animations: HashMap<u32, IdleRecord>,
    /// `IPCT` impact records — per-material bullet-impact visual
    /// effects (puff of dust on stone, splinters on wood, etc.).
    pub impacts: HashMap<u32, IpctRecord>,
    /// `IPDS` impact data sets — 12-entry table mapping per-material
    /// surface kinds to their respective IPCT records.
    pub impact_data_sets: HashMap<u32, IpdsRecord>,
    /// `COBJ` constructible-object records — FNV crafting recipes.
    pub recipes: HashMap<u32, CobjRecord>,
    // ── #810 / FNV-D2-NEW-03 — long-tail catch-all stubs ────────────
    //
    // 31 record types in the FNV catch-all-skip long tail. None has
    // a concrete consumer driving a per-record parser; bulk-dispatched
    // here so the catch-all skip approaches parity with FalloutNV.esm's
    // authored content set. Each field stores [`MinimalEsmRecord`]
    // (EDID + optional FULL); records that gain a real consumer later
    // can grow per-record fields via the established #808 / #809
    // pattern.
    //
    // Audio metadata (11):
    /// `ALOC` audio location controller.
    pub audio_locations: HashMap<u32, MinimalEsmRecord>,
    /// `ANIO` animation object.
    pub animation_objects: HashMap<u32, MinimalEsmRecord>,
    /// `ASPC` acoustic space.
    pub acoustic_spaces: HashMap<u32, MinimalEsmRecord>,
    /// `CAMS` camera shot.
    pub camera_shots: HashMap<u32, MinimalEsmRecord>,
    /// `CPTH` camera path.
    pub camera_paths: HashMap<u32, MinimalEsmRecord>,
    /// `DOBJ` default object.
    pub default_objects: HashMap<u32, MinimalEsmRecord>,
    /// `MICN` menu icon.
    pub menu_icons: HashMap<u32, MinimalEsmRecord>,
    /// `MSET` media set.
    pub media_sets: HashMap<u32, MinimalEsmRecord>,
    /// `MUSC` music type.
    pub music_types: HashMap<u32, MinimalEsmRecord>,
    /// `SOUN` sound.
    pub sounds: HashMap<u32, MinimalEsmRecord>,
    /// `VTYP` voice type.
    pub voice_types: HashMap<u32, MinimalEsmRecord>,
    // Visual / world (8):
    /// `AMEF` ammunition effect.
    pub ammo_effects: HashMap<u32, MinimalEsmRecord>,
    /// `DEBR` debris.
    pub debris: HashMap<u32, MinimalEsmRecord>,
    /// `GRAS` grass.
    pub grasses: HashMap<u32, MinimalEsmRecord>,
    /// `IMAD` image-space modifier — timed lens/color curves applied by
    /// Papyrus cinematics and CELL.XCIM transitions.
    pub imagespace_modifiers: HashMap<u32, ImadRecord>,
    /// `LSCR` load screen.
    pub load_screens: HashMap<u32, MinimalEsmRecord>,
    /// `LSCT` load screen type.
    pub load_screen_types: HashMap<u32, MinimalEsmRecord>,
    /// `PWAT` placeable water.
    pub placeable_waters: HashMap<u32, MinimalEsmRecord>,
    /// `RGDL` ragdoll.
    pub ragdolls: HashMap<u32, MinimalEsmRecord>,
    // FNV Hardcore mode (4):
    /// `DEHY` dehydration stages (FNV hardcore).
    pub dehydration_stages: HashMap<u32, MinimalEsmRecord>,
    /// `HUNG` hunger stages (FNV hardcore).
    pub hunger_stages: HashMap<u32, MinimalEsmRecord>,
    /// `RADS` radiation stages.
    pub radiation_stages: HashMap<u32, MinimalEsmRecord>,
    /// `SLPD` sleep deprivation stages (FNV hardcore).
    pub sleep_deprivation_stages: HashMap<u32, MinimalEsmRecord>,
    // FNV Caravan + Casino (6):
    /// `CCRD` caravan card.
    pub caravan_cards: HashMap<u32, MinimalEsmRecord>,
    /// `CDCK` caravan deck.
    pub caravan_decks: HashMap<u32, MinimalEsmRecord>,
    /// `CHAL` challenge.
    pub challenges: HashMap<u32, MinimalEsmRecord>,
    /// `CHIP` poker chip.
    pub poker_chips: HashMap<u32, MinimalEsmRecord>,
    /// `CMNY` caravan money.
    pub caravan_money: HashMap<u32, MinimalEsmRecord>,
    /// `CSNO` casino.
    pub casinos: HashMap<u32, MinimalEsmRecord>,
    // Recipe residuals (2):
    /// `RCCT` recipe category — superseded by COBJ in #809 but FNV
    /// ships both record types.
    pub recipe_categories: HashMap<u32, MinimalEsmRecord>,
    /// `RCPE` recipe — superseded by COBJ; FNV ships both.
    pub recipe_records: HashMap<u32, MinimalEsmRecord>,
    // ── #966 / OBL-D3-NEW-02 — Oblivion-unique base records ────────────
    //
    // Five record types that Oblivion (TES4) authors as distinct
    // categories; FO3 onwards folded most into ARMO / MISC / ALCH.
    // Pre-fix all five fell through the catch-all skip — birthsign
    // starting bonuses dangled, gear-tier displays for clothing
    // reported `unknown record`, and ENCH cross-refs to SLGM dangled.
    /// `BSGN` birthsign — Oblivion class-pick screen. ~13 vanilla
    /// records. References SPEL list for the auto-applied abilities
    /// (The Mage → Atronach absorb, The Atronach → Stunted Magicka).
    pub birthsigns: HashMap<u32, MinimalEsmRecord>,
    /// `CLOT` clothing — Oblivion-only. Same biped-slot shape as ARMO
    /// but no armour rating; folded into ARMO from FO3 onward.
    /// ~150 vanilla records (robes, hoods, shirts, pants, shoes).
    pub clothing: HashMap<u32, MinimalEsmRecord>,
    /// `APPA` alchemical apparatus — Oblivion-only. The four crafting
    /// tools (mortar & pestle, alembic, calcinator, retort) that gate
    /// alchemy quality. Folded into MISC from FO3 onward.
    pub apparatuses: HashMap<u32, MinimalEsmRecord>,
    /// `SGST` sigil stone — Oblivion-only. Daedric-quality enchantment
    /// sources from Oblivion Gates; carries embedded EFID/EFIT effect
    /// list. Vanilla Oblivion ships ~30 SGSTs across the quality tiers.
    pub sigil_stones: HashMap<u32, MinimalEsmRecord>,
    /// `SLGM` soul gem — Oblivion / Skyrim soul-magic carrier.
    /// Referenced by `ENCH` for the enchantment charge model.
    /// `SlgmRecord.soul_capacity` (SLCP byte 0) is the gem's max
    /// soul magnitude; `current_soul` (SOUL byte 0) is the pre-loaded
    /// soul. FO3 / FNV drop the record (no soul magic in the
    /// Wasteland) so the map is empty there. See #966.
    pub soul_gems: HashMap<u32, SlgmRecord>,
    // ── Skip telemetry (#1568 / SF-D4-02) ───────────────────────────
    /// Top-level GRUP labels the walker consciously skipped because no
    /// consumer exists for them yet — recorded once per label per parse
    /// (warned-once, no per-record spam). Unlike the anonymous catch-all
    /// (`_ => skip_group`), these are *named* here so the skip is visible
    /// to telemetry / tests instead of silently inflating the unresolved
    /// bucket. Currently only `PDCL` (Starfield `BGSProjectedDecal`):
    /// decals are projected onto surrounding geometry and have no MODL,
    /// so they can't ride the `statics` path even if dispatched — a real
    /// decal-projection system is needed before they have a consumer.
    /// Not a record category (carries no count), so it stays out of
    /// [`categories`](EsmIndex::categories) / [`total`](EsmIndex::total).
    pub skipped_unconsumed_groups: Vec<[u8; 4]>,
}

impl EsmIndex {
    /// Reconcile Fallout inventory categories after a complete parse or
    /// load-order merge. OMOD groups can appear after MISC, and later plugins
    /// may override either side of the relationship, so this cannot safely be
    /// done in the individual record parser.
    pub(crate) fn classify_fallout_inventory_kinds(&mut self) {
        if !matches!(self.game, GameKind::Fallout4 | GameKind::Fallout76) {
            return;
        }

        for item in self.items.values_mut() {
            if matches!(item.kind, super::ItemKind::Mod) {
                item.kind = super::ItemKind::Misc;
            }
        }
        for &loose_item in self.object_mod_loose_items.values() {
            if loose_item == 0 {
                continue;
            }
            if let Some(item) = self.items.get_mut(&loose_item) {
                if matches!(item.kind, super::ItemKind::Misc | super::ItemKind::Junk) {
                    item.kind = super::ItemKind::Mod;
                }
            }
        }
    }

    /// The base actor record a placed actor REFR points at — `NPC_` first,
    /// then `CREA`.
    ///
    /// #2567 (OBL-D3-01) — `NPC_` and `CREA` parse into two disjoint maps of
    /// the same `NpcRecord` type, and every "is this REFR an actor?" test in
    /// the cell loader consulted **only** `npcs`. `creatures` had zero readers
    /// anywhere under `byroredux/src/`, so a placed `ACRE` (Oblivion) or
    /// `ACHR`→`CREA` (FO3+) fell through to the generic static-mesh path: it
    /// rendered its MODL — which for a creature is the *skeleton* — and never
    /// animated. Route both through this one accessor so the two maps cannot
    /// drift apart again at a call site.
    pub fn actor(&self, form_id: u32) -> Option<&NpcRecord> {
        self.npcs
            .get(&form_id)
            .or_else(|| self.creatures.get(&form_id))
    }

    /// Whether `form_id` names a placeable actor base record (`NPC_` or
    /// `CREA`) — the cheap predicate half of [`Self::actor`].
    pub fn is_actor(&self, form_id: u32) -> bool {
        self.npcs.contains_key(&form_id) || self.creatures.contains_key(&form_id)
    }

    /// Single source of truth for the per-category breakdown.
    ///
    /// Each row is `(label, count_fn)`. [`total`] sums these counts;
    /// [`category_breakdown`] formats them. Adding a new top-level
    /// record category is now a single-edit operation — pre-#634 the
    /// `total()` math and the end-of-parse `log::info!` line drifted
    /// independently, and at least one consumer (the cell.statics +
    /// activators/terminals overlap) was already silently miscounted.
    ///
    /// **Semantic**: `cells.statics` is populated by `parse_modl_group`
    /// over every record-type that carries a `MODL` sub-record (STAT,
    /// MSTT, FURN, DOOR, ACTI, CONT, LIGH, MISC, ARMO, WEAP, …). That
    /// overlaps with the typed maps (`items`, `containers`, `activators`,
    /// `terminals`, …) — `total()` counts both, so the value is a "sum
    /// of bucket fills" rather than a unique-record count. Callers that
    /// need uniqueness should walk the typed maps directly. The
    /// integration-test floors in `tests/parse_real_esm.rs` were
    /// authored against the overlapping sum, so the semantic is locked
    /// in until those baselines are re-cut.
    ///
    /// [`total`]: Self::total
    /// [`category_breakdown`]: Self::category_breakdown
    pub fn categories() -> &'static [CategoryEntry] {
        // The closures below capture nothing and coerce to function pointers:
        // no boxing and no runtime overhead versus hand-written count/merge code.
        &[
            cell_category!("cells", cells),
            cell_category!("statics", statics),
            map_category!("items", items),
            map_category!("containers", containers),
            map_category!("LVLI", leveled_items),
            map_category!("LVLN", leveled_npcs),
            map_category!("LVLC", leveled_creatures),
            map_category!("NPCs", npcs),
            map_category!("creatures", creatures),
            map_category!("races", races),
            map_category!("classes", classes),
            map_category!("factions", factions),
            map_category!("globals", globals),
            map_category!("game_settings", game_settings),
            map_category!("weathers", weathers),
            map_category!("climates", climates),
            map_category!("scripts", scripts),
            map_category!("waters", waters),
            map_category!("navi", navi_info),
            map_category!("navmeshes", navmeshes),
            map_category!("regions", regions),
            map_category!("encounter_zones", encounter_zones),
            map_category!("lighting_templates", lighting_templates),
            map_category!("image_spaces", image_spaces),
            map_category!("head_parts", head_parts),
            map_category!("eyes", eyes),
            map_category!("hair", hair),
            map_category!("packages", packages),
            map_category!("quests", quests),
            map_category!("scenes", scenes),
            map_category!("dialogues", dialogues),
            map_category!("messages", messages),
            map_category!("perks", perks),
            map_category!("spells", spells),
            map_category!("enchantments", enchantments),
            map_category!("magic_effects", magic_effects),
            // #969 / OBL-D3-NEW-05 — Oblivion-only 4-char-code → MGEF
            // FormID secondary map. Empty on non-Oblivion games.
            map_category!("magic_effects_by_code", magic_effects_by_code),
            map_category!("actor_values", actor_values),
            map_category!("activators", activators),
            map_category!("terminals", terminals),
            map_category!("form_lists", form_lists),
            // #808 / FNV-D2-NEW-01 stubs.
            map_category!("projectiles", projectiles),
            map_category!("effect_shaders", effect_shaders),
            map_category!("item_mods", item_mods),
            map_category!("armor_addons", armor_addons),
            map_category!("outfits", outfits),
            map_category!("body_parts", body_parts),
            // #809 / FNV-D2-NEW-02 stubs.
            map_category!("reputations", reputations),
            map_category!("explosions", explosions),
            map_category!("combat_styles", combat_styles),
            map_category!("idle_animations", idle_animations),
            map_category!("impacts", impacts),
            map_category!("impact_data_sets", impact_data_sets),
            map_category!("recipes", recipes),
            // #810 / FNV-D2-NEW-03 — long-tail minimal stubs.
            map_category!("audio_locations", audio_locations),
            map_category!("animation_objects", animation_objects),
            map_category!("acoustic_spaces", acoustic_spaces),
            map_category!("camera_shots", camera_shots),
            map_category!("camera_paths", camera_paths),
            map_category!("default_objects", default_objects),
            map_category!("menu_icons", menu_icons),
            map_category!("media_sets", media_sets),
            map_category!("music_types", music_types),
            map_category!("sounds", sounds),
            map_category!("voice_types", voice_types),
            map_category!("ammo_effects", ammo_effects),
            map_category!("debris", debris),
            map_category!("grasses", grasses),
            // #1773 / FNV-D4-NEW-01 — TREE is dispatched into `index.trees`
            // (mod.rs `parse_tree`) but was the lone populated typed map
            // missing from this table, so it never counted toward `total()`
            // and a TREE category-wipe passed the parse-rate CI floor silently.
            // FNV ships 3; FO3/Oblivion ship many more (SpeedTree content).
            map_category!("trees", trees),
            map_category!("imagespace_modifiers", imagespace_modifiers),
            map_category!("load_screens", load_screens),
            map_category!("load_screen_types", load_screen_types),
            map_category!("placeable_waters", placeable_waters),
            map_category!("ragdolls", ragdolls),
            map_category!("dehydration_stages", dehydration_stages),
            map_category!("hunger_stages", hunger_stages),
            map_category!("radiation_stages", radiation_stages),
            map_category!("sleep_deprivation_stages", sleep_deprivation_stages),
            map_category!("caravan_cards", caravan_cards),
            map_category!("caravan_decks", caravan_decks),
            map_category!("challenges", challenges),
            map_category!("poker_chips", poker_chips),
            map_category!("caravan_money", caravan_money),
            map_category!("casinos", casinos),
            map_category!("recipe_categories", recipe_categories),
            map_category!("recipe_records", recipe_records),
            // #966 / OBL-D3-NEW-02 — Oblivion-unique base records.
            map_category!("birthsigns", birthsigns),
            map_category!("clothing", clothing),
            map_category!("apparatuses", apparatuses),
            map_category!("sigil_stones", sigil_stones),
            map_category!("soul_gems", soul_gems),
            // FO4-architecture maps (live on `EsmCellIndex`, not the top
            // level — same pattern as the `cells` and `statics` rows).
            // Without these rows a regression that empties any of the
            // five maps passes CI silently. See #817.
            cell_category!("texture_sets", texture_sets),
            cell_category!("scols", scols),
            cell_category!("packins", packins),
            cell_category!("movables", movables),
            cell_category!("material_swaps", material_swaps),
        ]
    }

    /// Total number of parsed records across every category. Useful for
    /// at-a-glance reporting in tests and the cell loader. See
    /// [`categories`] for the semantic note on the cells.statics
    /// overlap.
    ///
    /// [`categories`]: Self::categories
    pub fn total(&self) -> usize {
        Self::categories()
            .iter()
            .map(|(_, count, _)| count(self))
            .sum()
    }

    /// Resolve an actor value's global FormID by its canonical EditorID
    /// (case-insensitive), or `None` when no such `AVIF` was parsed.
    ///
    /// FO3/FNV/Skyrim author every AVIF EditorID with an `AV` prefix
    /// (`AVStrength`); FO4+ use the bare canonical spelling (`Strength`).
    /// That wire-format spelling difference is normalized here so the
    /// shared CHARAL rosters remain game-independent. The returned FormID is
    /// non-null and in the index's load-order space, the same space a remapped
    /// CTDA `param_1` (and therefore `GetActorValue`) compares against.
    /// Linear over `actor_values` (~100 records) — cheap enough to call
    /// per-stat at spawn.
    pub fn actor_value_form_id(&self, editor_id: &str) -> Option<u32> {
        let usable = |avif: &AvifRecord| avif.form_id != 0 && avif.form_id != u32::MAX;
        self.actor_values
            .values()
            .find(|avif| usable(avif) && avif.editor_id.eq_ignore_ascii_case(editor_id))
            .or_else(|| {
                self.actor_values.values().find(|avif| {
                    usable(avif)
                        && avif
                            .editor_id
                            .get(..2)
                            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("AV"))
                        && avif
                            .editor_id
                            .get(2..)
                            .is_some_and(|suffix| suffix.eq_ignore_ascii_case(editor_id))
                })
            })
            .map(|avif| avif.form_id)
    }

    /// Resolve Health in the same global AVIF FormID space as every other
    /// actor value and CTDA `GetActorValue` operand.
    pub fn health_actor_value_key(&self) -> Option<u32> {
        self.actor_value_form_id("Health")
    }

    /// Resolve a numeric GMST by EditorID for CHARAL's authored leveling
    /// constants. GMST records are keyed by FormID on the wire, so consumers
    /// must use this normalized EditorID lookup rather than guessing IDs.
    pub fn game_setting_float(&self, editor_id: &str) -> Option<f32> {
        self.game_settings
            .values()
            .find(|setting| setting.editor_id.eq_ignore_ascii_case(editor_id))
            .and_then(|setting| match setting.value {
                super::SettingValue::Float(value) => Some(value),
                super::SettingValue::Int(value) => Some(value as f32),
                super::SettingValue::Short(value) => Some(f32::from(value)),
                super::SettingValue::String(_) => None,
            })
    }

    /// Format the per-category breakdown as a single line — used by the
    /// `parse_esm_with_load_order` end-of-parse log. Drives off
    /// [`categories`] so the line stays in lockstep with [`total`]. See
    /// #634 / FNV-D2-06.
    ///
    /// M47.0 Phase 3 — look up the SCPT `form_id` attached to a base
    /// record by walking every record map that captures
    /// `script_form_id` in its parser today: activators (ACTI),
    /// containers (CONT), terminals (TERM), items (WEAP / ARMO /
    /// AMMO / MISC / KEYM / ALCH / INGR / BOOK / NOTE — anything that
    /// routes through `CommonItemFields`). Returns `None` when
    /// `base_form_id` isn't found OR the matched record has
    /// `script_form_id == 0` (the "no script attached" sentinel).
    ///
    /// **Coverage gaps to close later:**
    /// - DOOR / LIGH / FURN / etc. — these currently land in
    ///   `cells.statics` (bulk MODL catch-all), not typed maps, and
    ///   the static record doesn't carry `script_form_id`. Lifting
    ///   them into typed maps (with the SCRI field) is sibling work
    ///   tracked alongside M47.0.
    /// - Skyrim+ VMAD-attached scripts — the per-instance script
    ///   override mechanism. Decoded by M47.2, not by this lookup.
    ///
    /// **Stable contract**: the returned form_id is always either
    /// a valid SCPT key in `EsmIndex.scripts` OR `None`. Callers can
    /// chain `.and_then(|fid| index.scripts.get(&fid))` safely.
    pub fn base_record_script(&self, base_form_id: u32) -> Option<u32> {
        // Helper to nil-out the "0 = no script" sentinel.
        fn nonzero(form_id: u32) -> Option<u32> {
            if form_id == 0 {
                None
            } else {
                Some(form_id)
            }
        }
        if let Some(r) = self.activators.get(&base_form_id) {
            return nonzero(r.script_form_id);
        }
        if let Some(r) = self.containers.get(&base_form_id) {
            return nonzero(r.script_form_id);
        }
        if let Some(r) = self.terminals.get(&base_form_id) {
            return nonzero(r.script_form_id);
        }
        if let Some(r) = self.items.get(&base_form_id) {
            return nonzero(r.common.script_form_id);
        }
        // #1273 — NPC_ and CREA share `parse_npc` and `NpcRecord`, so
        // a single SCRI arm in the parser covers both. The two maps
        // are disjoint by form_id (vanilla content), so the order
        // here doesn't matter; we walk NPCs first because they're
        // the larger group on every shipped master.
        if let Some(r) = self.npcs.get(&base_form_id) {
            return nonzero(r.script_form_id);
        }
        if let Some(r) = self.creatures.get(&base_form_id) {
            return nonzero(r.script_form_id);
        }
        None
    }

    /// The decoded `VMAD` script attachments for a base record, if it
    /// carries any (Skyrim+ inline Papyrus). The sibling of
    /// [`base_record_script`](Self::base_record_script): that one returns
    /// the FO3/FNV/Oblivion `SCRI` → SCPT form id (Obscript), this one
    /// returns the Skyrim+ per-instance script bindings the M47.2
    /// translation layer decompiles to canonical ECS behavior.
    ///
    /// Covers the same base-record families `base_record_script` walks —
    /// activators, containers, NPCs/creatures, items — plus (#2663) the
    /// MODL-only world-placement family (STAT/MSTT/FURN/DOOR/LIGH/FLOR/
    /// IDLM/BNDS/ADDN/TACT, via `cells.statics`) and terminals, in that
    /// priority order. Returns `None` when the record is absent or
    /// carries no `VMAD`.
    pub fn base_record_script_instance(
        &self,
        base_form_id: u32,
    ) -> Option<&super::script_instance::ScriptInstanceData> {
        if let Some(r) = self.activators.get(&base_form_id) {
            return r.script_instance.as_ref();
        }
        if let Some(r) = self.containers.get(&base_form_id) {
            return r.script_instance.as_ref();
        }
        if let Some(r) = self.npcs.get(&base_form_id) {
            return r.script_instance.as_ref();
        }
        if let Some(r) = self.creatures.get(&base_form_id) {
            return r.script_instance.as_ref();
        }
        // #2189 — the item family (WEAP/ARMO/AMMO/MISC/KEYM/ALCH/INGR/
        // BOOK/NOTE). Absent until `CommonItemFields` gained a decoded
        // `script_instance`; before that this arm had nothing to return,
        // so every scripted item silently declined to attach.
        if let Some(r) = self.items.get(&base_form_id) {
            return r.common.script_instance.as_ref();
        }
        // #2663 (SCR-D7-NEW11-02) — the MODL-only world-placement family
        // (STAT/MSTT/FURN/DOOR/LIGH/FLOR/IDLM/BNDS/ADDN/TACT), the exact
        // sibling gap to #2189: `build_static_object_from_subs` decoded
        // `VMAD` as a presence-only flag with the payload dropped, and
        // `self.cells.statics` had nowhere to keep it even if it hadn't.
        // Placed LAST — deliberately lower priority than every typed map
        // above, so a form that also has a typed entry (e.g. an ACTI also
        // reachable via `self.activators`) resolves through the more
        // specific map first.
        if let Some(r) = self.cells.statics.get(&base_form_id) {
            return r.script_instance.as_ref();
        }
        // #2663 — TERM is parsed through `CommonNamedFields` (full VMAD
        // decode) but `parse_term` discarded `script_instance`; FO4 ships
        // 207 VMAD-bearing TERM records, so the "TERM is FO3/FNV-only"
        // premise that justified skipping this arm was wrong.
        if let Some(r) = self.terminals.get(&base_form_id) {
            return r.script_instance.as_ref();
        }
        None
    }

    /// [`categories`]: Self::categories
    /// [`total`]: Self::total
    pub fn category_breakdown(&self) -> String {
        let mut out = String::with_capacity(512);
        out.push_str("ESM parsed:");
        for (i, (label, count, _)) in Self::categories().iter().enumerate() {
            out.push_str(if i == 0 { " " } else { ", " });
            out.push_str(&format!("{} {}", count(self), label));
        }
        out
    }

    /// Merge `other` into `self` with **later-plugin-wins** semantics
    /// — the canonical Bethesda load-order rule. A DLC ESM that
    /// redefines a base-game form's STAT, item, NPC, or cell record
    /// overrides the master's entry; cells / statics defined only in
    /// the master pass through.
    ///
    /// Callers parse plugins in load order (masters first, main ESM
    /// last) and call `merge_from` on each successive parse so the
    /// final `EsmIndex` resolves cross-plugin REFRs and applies
    /// override layers in the right order.
    ///
    /// HashMap::extend already implements last-write-wins on key
    /// collisions, which exactly matches the load-order semantics; we
    /// just need to thread it through every map. The exterior-cells
    /// nested map merges per-worldspace so a DLC adding a new
    /// worldspace doesn't stomp the base game's entry. See M46.0 / #561.
    pub fn merge_from(&mut self, mut other: EsmIndex) {
        // M41.0 Phase 1b — preserve the latest plugin's game variant
        // on the merged index. Multi-plugin loads always share a
        // single game in practice (master + DLC of the same game), so
        // last-write-wins is correct; the field stays at its
        // `GameKind::default()` (Fallout3NV) until the first plugin's
        // parse populates it.
        self.game = other.game;
        self.character_rules = other.character_rules;

        // Nested cell index — needs per-worldspace handling.
        self.cells.merge_from(std::mem::take(&mut other.cells));

        // This auxiliary map is deliberately absent from `categories()`:
        // it supports inventory classification but is not a record category.
        self.object_mod_loose_items
            .extend(std::mem::take(&mut other.object_mod_loose_items));

        // Every top-level record category is merged by the same table that
        // drives `total()` and `category_breakdown()`. Adding a category can
        // no longer silently omit it from load-order folding (#2907).
        for (_, _, merge) in Self::categories() {
            merge(self, &mut other);
        }
        self.classify_fallout_inventory_kinds();

        // #1568 — skip telemetry accumulates across the plugin stack so a
        // master + DLC that both ship PDCL each surface their skip.
        self.skipped_unconsumed_groups
            .extend(other.skipped_unconsumed_groups);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::{ActiRecord, ScenePhase, SettingValue};

    #[test]
    fn merge_from_preserves_scene_timelines() {
        let mut plugin = EsmIndex::default();
        plugin.scenes.insert(
            0x000B_ECD4,
            ScenRecord {
                form_id: 0x000B_ECD4,
                editor_id: "MQ101Scene1".into(),
                phases: vec![ScenePhase {
                    name: "Cart ride".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        );

        let mut merged = EsmIndex::default();
        merged.merge_from(plugin);

        assert_eq!(merged.scenes[&0x000B_ECD4].phases[0].name, "Cart ride");
    }

    #[test]
    fn merge_from_preserves_and_overrides_imagespace_modifiers() {
        let mut merged = EsmIndex::default();
        merged.imagespace_modifiers.insert(
            0x10,
            ImadRecord {
                form_id: 0x10,
                duration_seconds: 1.0,
                ..Default::default()
            },
        );
        let mut plugin = EsmIndex::default();
        plugin.imagespace_modifiers.insert(
            0x10,
            ImadRecord {
                form_id: 0x10,
                duration_seconds: 7.0,
                ..Default::default()
            },
        );

        merged.merge_from(plugin);

        assert_eq!(merged.imagespace_modifiers[&0x10].duration_seconds, 7.0);
    }

    #[test]
    fn merge_from_preserves_skyrim_plus_equipment_graph() {
        use crate::esm::records::{ArmaRecord, OtftRecord};

        let mut plugin = EsmIndex::default();
        plugin.outfits.insert(
            0x000D_96D6,
            OtftRecord {
                form_id: 0x000D_96D6,
                editor_id: "DesdemonaOutfit".into(),
                items: vec![0x0011_53D9],
            },
        );
        plugin.armor_addons.insert(
            0x0011_53D8,
            ArmaRecord {
                form_id: 0x0011_53D8,
                female_biped_model: r"actors\character\characterassets\desdemona.nif".into(),
                ..Default::default()
            },
        );

        let mut merged = EsmIndex::default();
        merged.merge_from(plugin);

        assert_eq!(merged.outfits[&0x000D_96D6].items, vec![0x0011_53D9]);
        assert_eq!(
            merged.armor_addons[&0x0011_53D8].female_biped_model,
            r"actors\character\characterassets\desdemona.nif"
        );
    }

    /// #2907 — categories introduced after the original hand-written merge
    /// list must survive even a one-plugin load-order fold.
    #[test]
    fn merge_from_covers_every_category_table_entry() {
        let mut plugin = EsmIndex::default();
        plugin.idle_animations.insert(
            0x0001_2345,
            IdleRecord {
                form_id: 0x0001_2345,
                editor_id: "IdleSmoke".into(),
                ..Default::default()
            },
        );

        let mut merged = EsmIndex::default();
        merged.merge_from(plugin);

        assert_eq!(merged.idle_animations[&0x0001_2345].editor_id, "IdleSmoke");
    }

    #[test]
    fn base_record_script_returns_none_for_unknown_id() {
        let idx = EsmIndex::default();
        assert!(idx.base_record_script(0x0000_1234).is_none());
    }

    #[test]
    fn base_record_script_finds_activator_script() {
        let mut idx = EsmIndex::default();
        idx.activators.insert(
            0xAAAA_0001,
            ActiRecord {
                form_id: 0xAAAA_0001,
                script_form_id: 0xBBBB_0001,
                ..Default::default()
            },
        );
        assert_eq!(idx.base_record_script(0xAAAA_0001), Some(0xBBBB_0001));
    }

    /// #1273 — NPC_ and CREA SCRI script-attachment lookups.
    /// Inserts via the typed map (which is what `parse_esm` does) and
    /// asserts `base_record_script` walks both bins. Uses `parse_npc`
    /// to construct the fixtures so the test doubles as integration
    /// coverage for the new SCRI arm.
    #[test]
    fn base_record_script_finds_npc_and_creature_scripts() {
        use crate::esm::records::{parse_npc, GameKind};
        let sub = |t: &[u8; 4], data: &[u8]| crate::esm::reader::SubRecord {
            sub_type: *t,
            data: data.to_vec(),
        };

        let mut idx = EsmIndex::default();
        // Insert a script-bearing NPC and a script-bearing creature.
        let npc = parse_npc(
            0x000A_0001,
            &[
                sub(b"EDID", b"ScriptedNpc\0"),
                sub(b"SCRI", &0xBBBB_0001u32.to_le_bytes()),
            ],
            GameKind::Fallout3NV,
            &None,
        );
        idx.npcs.insert(0x000A_0001, npc);

        let crea = parse_npc(
            0x000B_0002,
            &[
                sub(b"EDID", b"ScriptedCreature\0"),
                sub(b"SCRI", &0xBBBB_0002u32.to_le_bytes()),
            ],
            GameKind::Fallout3NV,
            &None,
        );
        idx.creatures.insert(0x000B_0002, crea);

        assert_eq!(idx.base_record_script(0x000A_0001), Some(0xBBBB_0001));
        assert_eq!(idx.base_record_script(0x000B_0002), Some(0xBBBB_0002));

        // NPC without SCRI must resolve to None (the zero-sentinel
        // gate applies to NPCs / creatures too).
        let unscripted = parse_npc(
            0x000A_0009,
            &[sub(b"EDID", b"UnscriptedNpc\0")],
            GameKind::Fallout3NV,
            &None,
        );
        idx.npcs.insert(0x000A_0009, unscripted);
        assert!(idx.base_record_script(0x000A_0009).is_none());
    }

    /// #1773 / FNV-D4-NEW-01 — the `trees` typed map must be counted by
    /// `categories()` (→ `total()` / `category_breakdown()`). It was the lone
    /// populated typed map missing from the table, so a TREE category-wipe
    /// passed the parse-rate CI floor silently (the #817 failure mode).
    #[test]
    fn trees_are_counted_in_total_and_breakdown() {
        use crate::esm::records::parse_tree;
        let mut idx = EsmIndex::default();
        let before = idx.total();
        idx.trees.insert(0x000C_0001, parse_tree(0x000C_0001, &[]));
        assert_eq!(
            idx.total(),
            before + 1,
            "an inserted TREE must increment total()",
        );
        assert!(
            idx.category_breakdown().contains("trees"),
            "category_breakdown() must list the trees category",
        );
    }

    #[test]
    fn base_record_script_treats_zero_script_form_id_as_no_script() {
        // ACTI with script_form_id == 0 (the "no script attached"
        // sentinel) must resolve to None, NOT Some(0). Without this
        // gate the caller would chain into `index.scripts.get(&0)`
        // which would always miss and the caller couldn't distinguish
        // "this base record has no script" from "this base record has
        // a dangling script reference."
        let mut idx = EsmIndex::default();
        idx.activators.insert(
            0xAAAA_0002,
            ActiRecord {
                form_id: 0xAAAA_0002,
                script_form_id: 0,
                ..Default::default()
            },
        );
        assert!(idx.base_record_script(0xAAAA_0002).is_none());
    }

    /// M47.2 — the Skyrim+ sibling: a base record carrying a `VMAD`
    /// resolves through `base_record_script_instance` to its decoded
    /// attached-script name(s), which the attach path decompiles to ECS
    /// behavior. Build the ACTI through `parse_acti` so the test doubles
    /// as coverage for the new VMAD arm.
    #[test]
    fn base_record_script_instance_resolves_vmad_script_name() {
        use crate::esm::records::parse_acti;
        let sub = |t: &[u8; 4], data: &[u8]| crate::esm::reader::SubRecord {
            sub_type: *t,
            data: data.to_vec(),
        };

        // Minimal Skyrim-shape VMAD: version 5, objectFormat 2, one
        // script "MyActivatorScript", zero properties.
        let name = b"MyActivatorScript";
        let mut vmad = Vec::new();
        vmad.extend_from_slice(&5i16.to_le_bytes()); // version
        vmad.extend_from_slice(&2i16.to_le_bytes()); // objectFormat
        vmad.extend_from_slice(&1u16.to_le_bytes()); // scriptCount
        vmad.extend_from_slice(&(name.len() as u16).to_le_bytes());
        vmad.extend_from_slice(name);
        vmad.push(0); // script status
        vmad.extend_from_slice(&0u16.to_le_bytes()); // propCount = 0

        let acti = parse_acti(
            0xAAAA_0003,
            &[sub(b"EDID", b"VmadActi\0"), sub(b"VMAD", &vmad)],
            &None,
        );

        let mut idx = EsmIndex::default();
        idx.activators.insert(0xAAAA_0003, acti);

        let si = idx
            .base_record_script_instance(0xAAAA_0003)
            .expect("ACTI VMAD decoded into script_instance");
        assert_eq!(si.scripts.len(), 1);
        assert_eq!(si.scripts[0].name, "MyActivatorScript");

        // A record with no VMAD resolves to None (not an empty struct).
        let plain = parse_acti(0xAAAA_0004, &[sub(b"EDID", b"PlainActi\0")], &None);
        idx.activators.insert(0xAAAA_0004, plain);
        assert!(idx.base_record_script_instance(0xAAAA_0004).is_none());
        // And an unknown id is None.
        assert!(idx.base_record_script_instance(0x0000_9999).is_none());
    }

    /// Build a minimal Skyrim-shape VMAD payload naming a single script
    /// with zero properties (the shared fixture for the retention tests).
    fn synthetic_vmad(script_name: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&5i16.to_le_bytes()); // version
        v.extend_from_slice(&2i16.to_le_bytes()); // objectFormat
        v.extend_from_slice(&1u16.to_le_bytes()); // scriptCount
        v.extend_from_slice(&(script_name.len() as u16).to_le_bytes());
        v.extend_from_slice(script_name);
        v.push(0); // script status
        v.extend_from_slice(&0u16.to_le_bytes()); // propCount = 0
        v
    }

    /// The Container and NPC paths of `base_record_script_instance`: both
    /// retain the VMAD their shared `CommonNamedFields` decodes, so a
    /// scripted chest / NPC resolves its attached-script name through the
    /// accessor. (ACTI is covered above; this pins the other two families
    /// `base_record_script_instance` walks.)
    #[test]
    fn base_record_script_instance_resolves_container_and_npc_vmad() {
        use crate::esm::records::{parse_cont, parse_npc, GameKind};
        let sub = |t: &[u8; 4], data: &[u8]| crate::esm::reader::SubRecord {
            sub_type: *t,
            data: data.to_vec(),
        };

        let mut idx = EsmIndex::default();

        // Scripted container (CONT).
        let cont = parse_cont(
            0xC0_0001,
            &[
                sub(b"EDID", b"ScriptedChest\0"),
                sub(b"VMAD", &synthetic_vmad(b"TreasureChestScript")),
            ],
            &None,
        );
        idx.containers.insert(0xC0_0001, cont);
        let si = idx
            .base_record_script_instance(0xC0_0001)
            .expect("CONT VMAD retained");
        assert_eq!(si.scripts[0].name, "TreasureChestScript");

        // Scripted NPC (NPC_).
        let npc = parse_npc(
            0x0A_0001,
            &[
                sub(b"EDID", b"ScriptedNpc\0"),
                sub(b"VMAD", &synthetic_vmad(b"QuestGiverScript")),
            ],
            GameKind::Skyrim,
            &None,
        );
        idx.npcs.insert(0x0A_0001, npc);
        let si = idx
            .base_record_script_instance(0x0A_0001)
            .expect("NPC_ VMAD retained");
        assert_eq!(si.scripts[0].name, "QuestGiverScript");

        // A container with no VMAD still resolves to None on this path.
        let plain = parse_cont(0xC0_0002, &[sub(b"EDID", b"PlainChest\0")], &None);
        idx.containers.insert(0xC0_0002, plain);
        assert!(idx.base_record_script_instance(0xC0_0002).is_none());
    }

    /// #2189 — `base_record_script_instance` must resolve a VMAD-attached
    /// script off an `items` entry, not just activators/containers/actors.
    ///
    /// This is the accessor the M47.2 attach path calls
    /// (`cell_loader::references::attach`), so a miss here is the whole
    /// mechanism by which a scripted weapon/potion/book silently loses its
    /// script. Before the fix this arm could not exist: `ItemRecord.common`
    /// had no decoded `script_instance` to return.
    #[test]
    fn base_record_script_instance_resolves_an_item_records_vmad() {
        use crate::esm::records::items::{ItemKind, ItemRecord};
        use crate::esm::records::script_instance::{ScriptInstance, ScriptInstanceData};

        const SWORD: u32 = 0x0001_3989;

        let mut idx = EsmIndex::default();
        idx.items.insert(
            SWORD,
            ItemRecord {
                form_id: SWORD,
                common: crate::esm::records::common::CommonItemFields {
                    editor_id: "ScriptedSword".to_string(),
                    has_script: true,
                    script_instance: Some(ScriptInstanceData {
                        version: 5,
                        object_format: 2,
                        scripts: vec![ScriptInstance {
                            name: "WeaponEnchantScript".to_string(),
                            status: 0,
                            properties: Vec::new(),
                        }],
                    }),
                    ..Default::default()
                },
                kind: ItemKind::Misc,
            },
        );

        let inst = idx
            .base_record_script_instance(SWORD)
            .expect("an item record's VMAD must be reachable from the attach path (#2189)");
        assert_eq!(inst.scripts.len(), 1);
        assert_eq!(inst.scripts[0].name, "WeaponEnchantScript");
    }

    /// An item with no VMAD still declines — the new arm must not
    /// manufacture an attachment.
    #[test]
    fn base_record_script_instance_declines_an_item_without_vmad() {
        use crate::esm::records::items::{ItemKind, ItemRecord};

        const PLAIN: u32 = 0x0001_398A;

        let mut idx = EsmIndex::default();
        idx.items.insert(
            PLAIN,
            ItemRecord {
                form_id: PLAIN,
                common: Default::default(),
                kind: ItemKind::Misc,
            },
        );
        assert!(idx.base_record_script_instance(PLAIN).is_none());
    }

    /// Regression for #2663 (SCR-D7-NEW11-02) — the MODL-only
    /// world-placement family (STAT/MSTT/FURN/DOOR/LIGH/FLOR/IDLM/BNDS/
    /// ADDN/TACT), reached via `cells.statics` since none of those types
    /// gets its own typed map. Mirrors
    /// `base_record_script_instance_resolves_an_item_records_vmad`.
    #[test]
    fn base_record_script_instance_resolves_a_statics_familys_vmad() {
        use crate::esm::cell::StaticObject;
        use crate::esm::records::script_instance::{ScriptInstance, ScriptInstanceData};

        const FURNITURE: u32 = 0x0002_4001;

        let mut idx = EsmIndex::default();
        idx.cells.statics.insert(
            FURNITURE,
            StaticObject {
                form_id: FURNITURE,
                editor_id: "GenPullChainAnim01NoPlayer".to_string(),
                model_path: "furniture\\leverpull01.nif".to_string(),
                record_type: crate::record::RecordType::FURN,
                light_data: None,
                addon_data: None,
                has_script: true,
                script_instance: Some(ScriptInstanceData {
                    version: 5,
                    object_format: 2,
                    scripts: vec![ScriptInstance {
                        name: "LeverPullScript".to_string(),
                        status: 0,
                        properties: Vec::new(),
                    }],
                }),
                visible_when_distant: false,
            },
        );

        let inst = idx
            .base_record_script_instance(FURNITURE)
            .expect("a world-placement base record's VMAD must be reachable (#2663)");
        assert_eq!(inst.scripts.len(), 1);
        assert_eq!(inst.scripts[0].name, "LeverPullScript");
    }

    /// A `cells.statics` entry with no VMAD still declines.
    #[test]
    fn base_record_script_instance_declines_a_static_without_vmad() {
        use crate::esm::cell::StaticObject;

        const PLAIN: u32 = 0x0002_4002;

        let mut idx = EsmIndex::default();
        idx.cells.statics.insert(
            PLAIN,
            StaticObject {
                form_id: PLAIN,
                editor_id: "PlainStatic".to_string(),
                model_path: "clutter\\plain01.nif".to_string(),
                record_type: crate::record::RecordType::STAT,
                light_data: None,
                addon_data: None,
                has_script: false,
                script_instance: None,
                visible_when_distant: false,
            },
        );
        assert!(idx.base_record_script_instance(PLAIN).is_none());
    }

    /// Regression for #2663 (SCR-D7-NEW11-02) — TERM is parsed through
    /// `CommonNamedFields` (full VMAD decode) but `parse_term` used to
    /// discard `script_instance`, and this arm didn't exist. FO4 ships
    /// 207 VMAD-bearing TERM records; the "TERM is FO3/FNV-only" premise
    /// that justified skipping this was factually wrong.
    #[test]
    fn base_record_script_instance_resolves_a_terminals_vmad() {
        use crate::esm::records::script_instance::{ScriptInstance, ScriptInstanceData};
        use crate::esm::records::TermRecord;

        const TERMINAL: u32 = 0x0002_5001;

        let mut idx = EsmIndex::default();
        idx.terminals.insert(
            TERMINAL,
            TermRecord {
                form_id: TERMINAL,
                editor_id: "VRWorkshopShared_VRTerminalMusicSubMenu".to_string(),
                script_instance: Some(ScriptInstanceData {
                    version: 5,
                    object_format: 2,
                    scripts: vec![ScriptInstance {
                        name: "TerminalMenuScript".to_string(),
                        status: 0,
                        properties: Vec::new(),
                    }],
                }),
                ..Default::default()
            },
        );

        let inst = idx
            .base_record_script_instance(TERMINAL)
            .expect("an FO4 TERM record's VMAD must be reachable (#2663)");
        assert_eq!(inst.scripts.len(), 1);
        assert_eq!(inst.scripts[0].name, "TerminalMenuScript");
    }

    /// A terminal with no VMAD (the FO3/FNV common case) still declines.
    #[test]
    fn base_record_script_instance_declines_a_terminal_without_vmad() {
        use crate::esm::records::TermRecord;

        const TERMINAL: u32 = 0x0002_5002;

        let mut idx = EsmIndex::default();
        idx.terminals.insert(
            TERMINAL,
            TermRecord {
                form_id: TERMINAL,
                editor_id: "MainframeTerminal".to_string(),
                ..Default::default()
            },
        );
        assert!(idx.base_record_script_instance(TERMINAL).is_none());
    }

    #[test]
    fn game_setting_float_resolves_editor_id_and_numeric_variants() {
        let mut idx = EsmIndex::default();
        idx.game_settings.insert(
            0x100,
            GameSetting {
                form_id: 0x100,
                editor_id: "fXPLevelUpBase".to_string(),
                value: SettingValue::Float(80.0),
            },
        );
        idx.game_settings.insert(
            0x101,
            GameSetting {
                form_id: 0x101,
                editor_id: "iXPBase".to_string(),
                value: SettingValue::Int(150),
            },
        );
        assert_eq!(idx.game_setting_float("fxplevelupbase"), Some(80.0));
        assert_eq!(idx.game_setting_float("iXPBase"), Some(150.0));
        assert_eq!(idx.game_setting_float("missing"), None);
    }
}
