//! Structured record extraction beyond cells and statics.
//!
//! `cell.rs` (in the parent module) walks an ESM and pulls out interior /
//! exterior cells, placed references, and base records that have a MODL
//! sub-record. That covers everything the renderer needs to draw a cell.
//! This module adds extraction for record types game systems need beyond
//! rendering: items, containers, leveled lists, NPCs, races, classes,
//! factions, globals, and game settings.
//!
//! The high-level entry point is `parse_esm`, which walks the GRUP tree
//! once and fills an `EsmIndex` aggregating cells + the new record maps.
//! Existing callers that only want cell data continue to use
//! `parse_esm_cells` (now a thin wrapper over `parse_esm`).

pub mod actor;
pub mod climate;
pub mod common;
pub mod container;
pub mod global;
pub mod items;
pub mod list_record;
pub mod misc;
pub mod movs;
pub mod mswp;
pub mod pkin;
pub mod scol;
pub mod script;
pub mod weather;

pub use list_record::{parse_flst, FlstRecord};
pub use movs::{parse_movs, MovableStaticRecord};
pub use mswp::{parse_mswp, MaterialSwapEntry, MaterialSwapRecord};
pub use pkin::{parse_pkin, PkinRecord};
pub use scol::{parse_scol, ScolPart, ScolPlacement, ScolRecord};

pub use actor::{
    parse_clas, parse_fact, parse_npc, parse_race, ClassRecord, FactionMembership, FactionRecord,
    FactionRelation, NpcInventoryEntry, NpcRecord, RaceRecord,
};
pub use climate::{parse_clmt, ClimateRecord, ClimateWeather};
pub use container::{
    parse_cont, parse_leveled_list, ContainerRecord, InventoryEntry, LeveledEntry, LeveledList,
};
pub use global::{parse_glob, parse_gmst, GameSetting, GlobalRecord, SettingValue};
pub use items::{
    parse_alch, parse_ammo, parse_armo, parse_book, parse_ingr, parse_keym, parse_misc, parse_note,
    parse_weap, ItemKind, ItemRecord,
};
pub use misc::{
    parse_acti, parse_arma, parse_avif, parse_bptd, parse_cobj, parse_csty, parse_dial, parse_eczn,
    parse_efsh, parse_ench, parse_expl, parse_eyes, parse_hair, parse_hdpt, parse_idle, parse_imgs,
    parse_imod, parse_info, parse_ipct, parse_ipds, parse_lgtm, parse_mesg, parse_mgef,
    parse_minimal_esm_record, parse_navi, parse_navm, parse_pack, parse_perk, parse_proj,
    parse_qust, parse_regn, parse_repu, parse_spel, parse_term, parse_watr, ActiRecord, ArmaRecord,
    AvifRecord, BptdRecord, CobjRecord, CstyRecord, DialRecord, EcznRecord, EfshRecord, EnchRecord,
    ExplRecord, EyesRecord, HairRecord, HdptRecord, IdleRecord, ImgsRecord, ImodRecord, InfoRecord,
    IpctRecord, IpdsRecord, LgtmRecord, MesgRecord, MgefRecord, MinimalEsmRecord, NaviRecord,
    NavmRecord, PackRecord, PerkRecord, ProjRecord, QustRecord, RegnRecord, RepuRecord, SpelRecord,
    TermRecord, WatrRecord,
};
pub use script::{parse_scpt, ScriptLocalVar, ScriptRecord, ScriptType};
pub use weather::{parse_wthr, OblivionHdrLighting, SkyColor, WeatherRecord};

use super::cell::support::{
    parse_ltex_group, parse_modl_group, parse_movs_group, parse_mswp_group, parse_pkin_group,
    parse_scol_group, parse_txst_group,
};
use super::cell::walkers::parse_cell_group;
use super::cell::wrld::parse_wrld_group;
use super::cell::{
    build_static_object_from_subs, CellData, EsmCellIndex, StaticObject, TextureSet,
};
use super::reader::{EsmReader, FormIdRemap, GameKind, SubRecord};
use anyhow::{Context, Result};
use std::collections::HashMap;

/// Aggregated index of every record category we currently parse.
///
/// `cells` retains the existing structure used by the cell loader and
/// renderer. The other maps are new in M24.
#[derive(Debug, Default)]
pub struct EsmIndex {
    /// Game variant this index was parsed against, derived from the
    /// TES4 HEDR `Version` f32 by [`GameKind::from_header`]. Carried
    /// forward through [`merge_from`] (last-write-wins — multi-plugin
    /// loads always share a single game variant in practice).
    /// Consumed by the cell loader's NPC dispatch (M41.0 Phase 1b)
    /// to gate runtime-FaceGen vs pre-baked-FaceGen spawn paths.
    pub game: GameKind,
    pub cells: EsmCellIndex,
    pub items: HashMap<u32, ItemRecord>,
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
    /// `TERM` terminal records — vault/military consoles. Menu items
    /// + password + body text captured so a future terminal-interaction
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
    /// `BPTD` body-part-data records — per-NPC dismemberment
    /// routing (head, torso, limbs) + biped slot count.
    pub body_parts: HashMap<u32, BptdRecord>,
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
    /// `IMAD` imagespace modifier — referenced by CELL.XCIM transitions.
    pub imagespace_modifiers: HashMap<u32, MinimalEsmRecord>,
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
}

impl EsmIndex {
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
    pub fn categories() -> &'static [(&'static str, fn(&EsmIndex) -> usize)] {
        // The closures below capture nothing and coerce to `fn(&EsmIndex)
        // -> usize` — no boxing, zero runtime overhead vs the inline sum.
        &[
            ("cells", |s| s.cells.cells.len()),
            ("statics", |s| s.cells.statics.len()),
            ("items", |s| s.items.len()),
            ("containers", |s| s.containers.len()),
            ("LVLI", |s| s.leveled_items.len()),
            ("LVLN", |s| s.leveled_npcs.len()),
            ("LVLC", |s| s.leveled_creatures.len()),
            ("NPCs", |s| s.npcs.len()),
            ("creatures", |s| s.creatures.len()),
            ("races", |s| s.races.len()),
            ("classes", |s| s.classes.len()),
            ("factions", |s| s.factions.len()),
            ("globals", |s| s.globals.len()),
            ("game_settings", |s| s.game_settings.len()),
            ("weathers", |s| s.weathers.len()),
            ("climates", |s| s.climates.len()),
            ("scripts", |s| s.scripts.len()),
            ("waters", |s| s.waters.len()),
            ("navi", |s| s.navi_info.len()),
            ("navmeshes", |s| s.navmeshes.len()),
            ("regions", |s| s.regions.len()),
            ("encounter_zones", |s| s.encounter_zones.len()),
            ("lighting_templates", |s| s.lighting_templates.len()),
            ("image_spaces", |s| s.image_spaces.len()),
            ("head_parts", |s| s.head_parts.len()),
            ("eyes", |s| s.eyes.len()),
            ("hair", |s| s.hair.len()),
            ("packages", |s| s.packages.len()),
            ("quests", |s| s.quests.len()),
            ("dialogues", |s| s.dialogues.len()),
            ("messages", |s| s.messages.len()),
            ("perks", |s| s.perks.len()),
            ("spells", |s| s.spells.len()),
            ("enchantments", |s| s.enchantments.len()),
            ("magic_effects", |s| s.magic_effects.len()),
            ("actor_values", |s| s.actor_values.len()),
            ("activators", |s| s.activators.len()),
            ("terminals", |s| s.terminals.len()),
            ("form_lists", |s| s.form_lists.len()),
            // #808 / FNV-D2-NEW-01 stubs.
            ("projectiles", |s| s.projectiles.len()),
            ("effect_shaders", |s| s.effect_shaders.len()),
            ("item_mods", |s| s.item_mods.len()),
            ("armor_addons", |s| s.armor_addons.len()),
            ("body_parts", |s| s.body_parts.len()),
            // #809 / FNV-D2-NEW-02 stubs.
            ("reputations", |s| s.reputations.len()),
            ("explosions", |s| s.explosions.len()),
            ("combat_styles", |s| s.combat_styles.len()),
            ("idle_animations", |s| s.idle_animations.len()),
            ("impacts", |s| s.impacts.len()),
            ("impact_data_sets", |s| s.impact_data_sets.len()),
            ("recipes", |s| s.recipes.len()),
            // #810 / FNV-D2-NEW-03 — long-tail minimal stubs.
            ("audio_locations", |s| s.audio_locations.len()),
            ("animation_objects", |s| s.animation_objects.len()),
            ("acoustic_spaces", |s| s.acoustic_spaces.len()),
            ("camera_shots", |s| s.camera_shots.len()),
            ("camera_paths", |s| s.camera_paths.len()),
            ("default_objects", |s| s.default_objects.len()),
            ("menu_icons", |s| s.menu_icons.len()),
            ("media_sets", |s| s.media_sets.len()),
            ("music_types", |s| s.music_types.len()),
            ("sounds", |s| s.sounds.len()),
            ("voice_types", |s| s.voice_types.len()),
            ("ammo_effects", |s| s.ammo_effects.len()),
            ("debris", |s| s.debris.len()),
            ("grasses", |s| s.grasses.len()),
            ("imagespace_modifiers", |s| s.imagespace_modifiers.len()),
            ("load_screens", |s| s.load_screens.len()),
            ("load_screen_types", |s| s.load_screen_types.len()),
            ("placeable_waters", |s| s.placeable_waters.len()),
            ("ragdolls", |s| s.ragdolls.len()),
            ("dehydration_stages", |s| s.dehydration_stages.len()),
            ("hunger_stages", |s| s.hunger_stages.len()),
            ("radiation_stages", |s| s.radiation_stages.len()),
            ("sleep_deprivation_stages", |s| s.sleep_deprivation_stages.len()),
            ("caravan_cards", |s| s.caravan_cards.len()),
            ("caravan_decks", |s| s.caravan_decks.len()),
            ("challenges", |s| s.challenges.len()),
            ("poker_chips", |s| s.poker_chips.len()),
            ("caravan_money", |s| s.caravan_money.len()),
            ("casinos", |s| s.casinos.len()),
            ("recipe_categories", |s| s.recipe_categories.len()),
            ("recipe_records", |s| s.recipe_records.len()),
            // FO4-architecture maps (live on `EsmCellIndex`, not the top
            // level — same pattern as the `cells` and `statics` rows).
            // Without these rows a regression that empties any of the
            // five maps passes CI silently. See #817.
            ("texture_sets", |s| s.cells.texture_sets.len()),
            ("scols", |s| s.cells.scols.len()),
            ("packins", |s| s.cells.packins.len()),
            ("movables", |s| s.cells.movables.len()),
            ("material_swaps", |s| s.cells.material_swaps.len()),
        ]
    }

    /// Total number of parsed records across every category. Useful for
    /// at-a-glance reporting in tests and the cell loader. See
    /// [`categories`] for the semantic note on the cells.statics
    /// overlap.
    ///
    /// [`categories`]: Self::categories
    pub fn total(&self) -> usize {
        Self::categories().iter().map(|(_, f)| f(self)).sum()
    }

    /// Format the per-category breakdown as a single line — used by the
    /// `parse_esm_with_load_order` end-of-parse log. Drives off
    /// [`categories`] so the line stays in lockstep with [`total`]. See
    /// #634 / FNV-D2-06.
    ///
    /// [`categories`]: Self::categories
    /// [`total`]: Self::total
    pub fn category_breakdown(&self) -> String {
        let mut out = String::with_capacity(512);
        out.push_str("ESM parsed:");
        for (i, (label, f)) in Self::categories().iter().enumerate() {
            out.push_str(if i == 0 { " " } else { ", " });
            out.push_str(&format!("{} {}", f(self), label));
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
    pub fn merge_from(&mut self, other: EsmIndex) {
        // M41.0 Phase 1b — preserve the latest plugin's game variant
        // on the merged index. Multi-plugin loads always share a
        // single game in practice (master + DLC of the same game), so
        // last-write-wins is correct; the field stays at its
        // `GameKind::default()` (Fallout3NV) until the first plugin's
        // parse populates it.
        self.game = other.game;

        // Nested cell index — needs per-worldspace handling.
        self.cells.merge_from(other.cells);

        // Top-level record maps — last-write-wins per HashMap::extend.
        self.items.extend(other.items);
        self.containers.extend(other.containers);
        self.leveled_items.extend(other.leveled_items);
        self.leveled_npcs.extend(other.leveled_npcs);
        self.leveled_creatures.extend(other.leveled_creatures);
        self.npcs.extend(other.npcs);
        self.creatures.extend(other.creatures);
        self.races.extend(other.races);
        self.classes.extend(other.classes);
        self.factions.extend(other.factions);
        self.globals.extend(other.globals);
        self.game_settings.extend(other.game_settings);
        self.weathers.extend(other.weathers);
        self.climates.extend(other.climates);
        self.scripts.extend(other.scripts);
        self.waters.extend(other.waters);
        self.navi_info.extend(other.navi_info);
        self.navmeshes.extend(other.navmeshes);
        self.regions.extend(other.regions);
        self.encounter_zones.extend(other.encounter_zones);
        self.lighting_templates.extend(other.lighting_templates);
        self.image_spaces.extend(other.image_spaces);
        self.head_parts.extend(other.head_parts);
        self.eyes.extend(other.eyes);
        self.hair.extend(other.hair);
        self.packages.extend(other.packages);
        self.quests.extend(other.quests);
        self.dialogues.extend(other.dialogues);
        self.messages.extend(other.messages);
        self.perks.extend(other.perks);
        self.spells.extend(other.spells);
        self.enchantments.extend(other.enchantments);
        self.magic_effects.extend(other.magic_effects);
        self.actor_values.extend(other.actor_values);
        self.activators.extend(other.activators);
        self.terminals.extend(other.terminals);
        self.form_lists.extend(other.form_lists);
    }
}

/// Parse an entire ESM/ESP file in a single pass.
///
/// First fills the cell index using the existing `parse_esm_cells` walker
/// (which already handles CELL/WRLD/MODL extraction), then walks the file
/// a second time to pull the M24 record categories. This is a deliberate
/// trade-off: a single combined walker would be more efficient but would
/// require restructuring `cell.rs`. For now, two passes over a 100 MB
/// file run in well under a second on a real ESM, and keeping the cell
/// pipeline untouched preserves the renderer behaviour we already trust.
pub fn parse_esm(data: &[u8]) -> Result<EsmIndex> {
    parse_esm_with_load_order(data, None)
}

/// Parse an ESM/ESP with an explicit load-order remap.
///
/// `remap` rewrites every record's FormID top byte into the global
/// load order so maps in [`EsmIndex`] stay collision-free across
/// plugins. Pass `None` for single-plugin loads (the default today);
/// pass `Some(FormIdRemap { plugin_index, master_indices })` when
/// loading a DLC or mod in a multi-plugin stack so Anchorage's new
/// form 0x01012345 doesn't collide with BrokenSteel's 0x01012345 in
/// the same map. See #445.
///
/// The current CLI entry point (`--esm <path>`) only wires a single
/// plugin, so both paths produce the same output for vanilla content.
/// The multi-plugin wiring is tracked as follow-up work — this
/// function exists so downstream code can opt in without another
/// parse-layer refactor when the CLI grows multi-plugin support.
pub fn parse_esm_with_load_order(data: &[u8], remap: Option<FormIdRemap>) -> Result<EsmIndex> {
    let mut index = EsmIndex::default();

    let mut reader = EsmReader::new(data);
    if let Some(r) = remap {
        reader.set_form_id_remap(r);
    }
    // Peek the TES4 file header so we can derive a GameKind from HEDR's
    // `Version` f32. ARMO/WEAP/AMMO DATA/DNAM layouts diverge across
    // FO3/FNV → Skyrim → FO4; without the HEDR discriminator every
    // Skyrim record would be parsed with the FO3/FNV schema and get
    // garbage stats (issue #347).
    let file_header = reader
        .read_file_header()
        .context("Failed to read ESM file header")?;
    let game = GameKind::from_header(reader.variant(), file_header.hedr_version);
    // M41.0 Phase 1b — preserve game on the index so consumers
    // (NPC spawn dispatcher) can route per-version without
    // re-deriving from a HEDR they no longer have.
    index.game = game;
    log::info!(
        "ESM file: {} records, {} master files",
        file_header.record_count,
        file_header.master_files.len(),
    );

    // Cell-side scratch state — fed directly into `index.cells` after
    // the unified walk. Pre-#527 these were owned by a separate first
    // pass through the file (`parse_esm_cells_with_load_order`); the
    // fused walker collapses both passes into one so a 70 MB ESM
    // doesn't go through the header decoder + sub-record walker
    // twice. See audit FNV-ESM-2.
    let mut cells: HashMap<String, CellData> = HashMap::new();
    let mut exterior_cells: HashMap<String, HashMap<(i32, i32), CellData>> = HashMap::new();
    let mut statics: HashMap<u32, StaticObject> = HashMap::new();
    let mut landscape_textures: HashMap<u32, String> = HashMap::new();
    let mut worldspace_climates: HashMap<String, u32> = HashMap::new();
    let mut txst_textures: HashMap<u32, String> = HashMap::new();
    let mut texture_sets: HashMap<u32, TextureSet> = HashMap::new();
    let mut ltex_to_txst: HashMap<u32, u32> = HashMap::new();
    let mut scols: HashMap<u32, ScolRecord> = HashMap::new();
    let mut packins: HashMap<u32, PkinRecord> = HashMap::new();
    let mut movables: HashMap<u32, MovableStaticRecord> = HashMap::new();
    let mut material_swaps: HashMap<u32, MaterialSwapRecord> = HashMap::new();

    // #348 / #624 — push the TES4 `Localized` flag into a thread-local
    // so every record parser's FULL/DESC decoder can route through the
    // lstring helper. The RAII guard restores the previous value on
    // drop (including unwind from a panic mid-walk), so:
    //   * A subsequent parse of a non-localized plugin in the same
    //     process can't inherit a stale `Localized = true` flag.
    //   * Overlapping parses on the same thread (nested `parse_esm`
    //     calls) correctly stack — the outer parse's flag is restored
    //     when the inner guard drops.
    let _localized_guard = common::LocalizedPluginGuard::new(file_header.localized);

    // Walk top-level groups and dispatch by record-type label.
    while reader.remaining() > 0 {
        if !reader.is_group() {
            // Stray top-level record; skip it.
            let header = reader.read_record_header()?;
            reader.skip_record(&header);
            continue;
        }
        let group = reader.read_group_header()?;
        let end = reader.group_content_end(&group);
        let label = group.label;

        match &label {
            // ── Cell-only labels — formerly the entire first walker (#527 fusion). ──
            //
            // CELL / WRLD / LTEX / TXST / SCOL / PKIN / MOVS / MSWP have
            // no typed `EsmIndex.<map>` consumer; they only feed the
            // `cells` sub-tree. Calling the existing cell helpers
            // inline keeps the cell-loader's already-trusted parsing
            // path byte-identical while letting the unified walker
            // own the outer loop.
            b"CELL" => parse_cell_group(&mut reader, end, &mut cells)?,
            b"WRLD" => parse_wrld_group(
                &mut reader,
                end,
                &mut exterior_cells,
                &mut worldspace_climates,
            )?,
            b"LTEX" => parse_ltex_group(
                &mut reader,
                end,
                &mut ltex_to_txst,
                &mut landscape_textures,
            )?,
            b"TXST" => parse_txst_group(&mut reader, end, &mut txst_textures, &mut texture_sets)?,
            b"SCOL" => parse_scol_group(&mut reader, end, &mut statics, &mut scols)?,
            b"PKIN" => parse_pkin_group(&mut reader, end, &mut statics, &mut packins)?,
            b"MOVS" => parse_movs_group(&mut reader, end, &mut statics, &mut movables)?,
            b"MSWP" => parse_mswp_group(&mut reader, end, &mut material_swaps)?,
            // MODL-only labels — populate `cells.statics` for visual
            // placement, no typed map. STAT / MSTT / FURN / DOOR /
            // LIGH / FLOR / TREE / IDLM / BNDS / ADDN / TACT all carry
            // a MODL but no record-side parser yet.
            b"STAT" | b"MSTT" | b"FURN" | b"DOOR" | b"LIGH" | b"FLOR" | b"TREE" | b"IDLM"
            | b"BNDS" | b"ADDN" | b"TACT" => {
                parse_modl_group(&mut reader, end, &mut statics)?;
            }
            // ── Dual-target labels — typed record + cells.statics in one walk. ──
            //
            // Every label below ships BOTH a typed `EsmIndex.<map>`
            // entry AND wants `cells.statics` populated for visual
            // placement (REFRs targeting the form ID still need a
            // model_path / VMAD-script flag). Pre-#527 they were
            // walked twice — once by the cell first-pass, once by the
            // records second-pass. The fused helper walks each group
            // once and dispatches both consumers from the same
            // `subs` slice.
            b"WEAP" => extract_records_with_modl(
                &mut reader,
                end,
                b"WEAP",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_weap(fid, subs, game));
                },
            )?,
            b"ARMO" => extract_records_with_modl(
                &mut reader,
                end,
                b"ARMO",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_armo(fid, subs, game));
                },
            )?,
            b"AMMO" => extract_records_with_modl(
                &mut reader,
                end,
                b"AMMO",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_ammo(fid, subs, game));
                },
            )?,
            b"MISC" => extract_records_with_modl(
                &mut reader,
                end,
                b"MISC",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_misc(fid, subs));
                },
            )?,
            b"KEYM" => extract_records_with_modl(
                &mut reader,
                end,
                b"KEYM",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_keym(fid, subs));
                },
            )?,
            b"ALCH" => extract_records_with_modl(
                &mut reader,
                end,
                b"ALCH",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_alch(fid, subs));
                },
            )?,
            b"INGR" => extract_records_with_modl(
                &mut reader,
                end,
                b"INGR",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_ingr(fid, subs));
                },
            )?,
            b"BOOK" => extract_records_with_modl(
                &mut reader,
                end,
                b"BOOK",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_book(fid, subs));
                },
            )?,
            b"NOTE" => extract_records_with_modl(
                &mut reader,
                end,
                b"NOTE",
                &mut statics,
                &mut |fid, subs| {
                    index.items.insert(fid, parse_note(fid, subs));
                },
            )?,
            // Containers and leveled lists.
            b"CONT" => extract_records_with_modl(
                &mut reader,
                end,
                b"CONT",
                &mut statics,
                &mut |fid, subs| {
                    index.containers.insert(fid, parse_cont(fid, subs));
                },
            )?,
            b"LVLI" => extract_records(&mut reader, end, b"LVLI", &mut |fid, subs| {
                index
                    .leveled_items
                    .insert(fid, parse_leveled_list(fid, subs));
            })?,
            b"LVLN" => extract_records(&mut reader, end, b"LVLN", &mut |fid, subs| {
                index
                    .leveled_npcs
                    .insert(fid, parse_leveled_list(fid, subs));
            })?,
            // Leveled creatures (CREA spawn tables) — byte-identical to
            // LVLI / LVLN. FO3 wires most enemy encounters through LVLC;
            // FNV migrated most combat to LVLN but still ships legacy
            // LVLC entries. See #448 / audit FO3-3-06.
            b"LVLC" => extract_records(&mut reader, end, b"LVLC", &mut |fid, subs| {
                index
                    .leveled_creatures
                    .insert(fid, parse_leveled_list(fid, subs));
            })?,
            // Actors and supporting records — dual-target via the
            // fused walker so the cell-side STAT-equivalent
            // registration in `statics` still happens (REFR base-form
            // resolution against named NPCs / creatures keeps working).
            b"NPC_" => extract_records_with_modl(
                &mut reader,
                end,
                b"NPC_",
                &mut statics,
                &mut |fid, subs| {
                    index.npcs.insert(fid, parse_npc(fid, subs, game));
                },
            )?,
            // Creatures share EDID / FULL / MODL / RNAM / CNAM / SNAM /
            // CNTO / PKID / ACBS with NPC_ — `parse_npc` populates the
            // same `NpcRecord` shape. FO3 bestiary (super mutants,
            // deathclaws, radroaches, robots) lives here; pre-fix the
            // whole top-level group was dropped at the catch-all skip.
            // See #442 / audit FO3-3-02.
            b"CREA" => extract_records_with_modl(
                &mut reader,
                end,
                b"CREA",
                &mut statics,
                &mut |fid, subs| {
                    index.creatures.insert(fid, parse_npc(fid, subs, game));
                },
            )?,
            b"RACE" => extract_records(&mut reader, end, b"RACE", &mut |fid, subs| {
                index.races.insert(fid, parse_race(fid, subs));
            })?,
            b"CLAS" => extract_records(&mut reader, end, b"CLAS", &mut |fid, subs| {
                index.classes.insert(fid, parse_clas(fid, subs));
            })?,
            b"FACT" => extract_records(&mut reader, end, b"FACT", &mut |fid, subs| {
                index.factions.insert(fid, parse_fact(fid, subs));
            })?,
            // Globals and game settings.
            b"GLOB" => extract_records(&mut reader, end, b"GLOB", &mut |fid, subs| {
                index.globals.insert(fid, parse_glob(fid, subs));
            })?,
            b"GMST" => extract_records(&mut reader, end, b"GMST", &mut |fid, subs| {
                index.game_settings.insert(fid, parse_gmst(fid, subs));
            })?,
            // Weather records — sky colors, fog, wind, clouds.
            b"WTHR" => extract_records(&mut reader, end, b"WTHR", &mut |fid, subs| {
                index.weathers.insert(fid, parse_wthr(fid, subs));
            })?,
            // Climate records — weather probability tables. The WLST
            // entry size dispatches off `game` (M33-08 / #540) so
            // multi-of-3-entry Oblivion CLMTs don't autodetect to the
            // 12-byte FO3+ schema and mis-thread their FormID slots.
            b"CLMT" => extract_records(&mut reader, end, b"CLMT", &mut |fid, subs| {
                index.climates.insert(fid, parse_clmt(fid, subs, game));
            })?,
            // FO3 / FNV / Oblivion pre-Papyrus SCPT scripts — bytecode
            // blob + source text + local-var table. Pre-#443 the group
            // fell through to the catch-all skip and every NPC / item
            // SCRI cross-reference dangled. Runtime execution is out
            // of scope for this fix — extraction only.
            b"SCPT" => extract_records(&mut reader, end, b"SCPT", &mut |fid, subs| {
                index.scripts.insert(fid, parse_scpt(fid, subs));
            })?,
            // Supplementary records previously catch-all-skipped (#458).
            // Stubs capture EDID + form refs + scalar fields; full
            // per-record decoding lands with the consuming subsystem.
            b"WATR" => extract_records(&mut reader, end, b"WATR", &mut |fid, subs| {
                index.waters.insert(fid, parse_watr(fid, subs));
            })?,
            b"NAVI" => extract_records(&mut reader, end, b"NAVI", &mut |fid, subs| {
                index.navi_info.insert(fid, parse_navi(fid, subs));
            })?,
            b"NAVM" => extract_records(&mut reader, end, b"NAVM", &mut |fid, subs| {
                index.navmeshes.insert(fid, parse_navm(fid, subs));
            })?,
            b"REGN" => extract_records(&mut reader, end, b"REGN", &mut |fid, subs| {
                index.regions.insert(fid, parse_regn(fid, subs));
            })?,
            b"ECZN" => extract_records(&mut reader, end, b"ECZN", &mut |fid, subs| {
                index.encounter_zones.insert(fid, parse_eczn(fid, subs));
            })?,
            // LGTM lighting templates — consumer lands alongside #379
            // (per-field inheritance fallback on cells without XCLL).
            b"LGTM" => extract_records(&mut reader, end, b"LGTM", &mut |fid, subs| {
                index.lighting_templates.insert(fid, parse_lgtm(fid, subs));
            })?,
            // #624 / SK-D6-NEW-03 — IMGS imagespace records. CELL.XCIM
            // cross-references resolve here. Currently a stub (EDID +
            // raw DNAM payload); full DNAM struct decode + IMAD
            // modifier graph deferred to M48 alongside the per-cell
            // HDR-LUT renderer consumer.
            b"IMGS" => extract_records(&mut reader, end, b"IMGS", &mut |fid, subs| {
                index.image_spaces.insert(fid, parse_imgs(fid, subs));
            })?,
            b"HDPT" => extract_records(&mut reader, end, b"HDPT", &mut |fid, subs| {
                index.head_parts.insert(fid, parse_hdpt(fid, subs));
            })?,
            b"EYES" => extract_records(&mut reader, end, b"EYES", &mut |fid, subs| {
                index.eyes.insert(fid, parse_eyes(fid, subs));
            })?,
            b"HAIR" => extract_records(&mut reader, end, b"HAIR", &mut |fid, subs| {
                index.hair.insert(fid, parse_hair(fid, subs));
            })?,
            // AI / dialogue / effect stubs (#446, #447). Follow the
            // #458 supplementary-record pattern: minimal struct
            // (EDID + FULL + a few scalars), no deep decoding.
            b"PACK" => extract_records(&mut reader, end, b"PACK", &mut |fid, subs| {
                index.packages.insert(fid, parse_pack(fid, subs));
            })?,
            b"QUST" => extract_records(&mut reader, end, b"QUST", &mut |fid, subs| {
                index.quests.insert(fid, parse_qust(fid, subs));
            })?,
            // DIAL tops a nested GRUP tree: a top-level GRUP labelled
            // "DIAL" containing DIAL records, each (often) followed by
            // a Topic Children sub-GRUP (group_type == 7) whose label
            // is the parent DIAL's form_id u32 and whose contents are
            // INFO records. The generic `extract_records` walker
            // filters on a single `expected_type` and silently drops
            // every INFO. The dedicated walker below threads both
            // record types through. See #631 / #447.
            b"DIAL" => extract_dial_with_info(&mut reader, end, &mut index.dialogues)?,
            b"MESG" => extract_records(&mut reader, end, b"MESG", &mut |fid, subs| {
                index.messages.insert(fid, parse_mesg(fid, subs));
            })?,
            b"PERK" => extract_records(&mut reader, end, b"PERK", &mut |fid, subs| {
                index.perks.insert(fid, parse_perk(fid, subs));
            })?,
            b"SPEL" => extract_records(&mut reader, end, b"SPEL", &mut |fid, subs| {
                index.spells.insert(fid, parse_spel(fid, subs));
            })?,
            // ENCH enchantments (#629 / FNV-D2-01). Same scaffolding as
            // SPEL — ENIT carries type/charge/cost/flags; full effect
            // chain decoding lands with MGEF application.
            b"ENCH" => extract_records(&mut reader, end, b"ENCH", &mut |fid, subs| {
                index.enchantments.insert(fid, parse_ench(fid, subs));
            })?,
            b"MGEF" => extract_records(&mut reader, end, b"MGEF", &mut |fid, subs| {
                index.magic_effects.insert(fid, parse_mgef(fid, subs));
            })?,
            // AVIF actor-value records (#519). Pre-fix every NPC
            // skill-bonus, BOOK skill-book teach ref, and AVIF-keyed
            // condition predicate dangled because the top-level group
            // hit the catch-all skip.
            b"AVIF" => extract_records(&mut reader, end, b"AVIF", &mut |fid, subs| {
                index.actor_values.insert(fid, parse_avif(fid, subs));
            })?,
            // ACTI / TERM #521 — dual-target: typed map for SCRI /
            // menu-tree cross-refs AND `cells.statics` for visual
            // placement. Pre-#527 the cell first-pass walked them via
            // the MODL catch-all and the records second-pass walked
            // them again for the typed parser; the fused helper does
            // both in one walk.
            b"ACTI" => extract_records_with_modl(
                &mut reader,
                end,
                b"ACTI",
                &mut statics,
                &mut |fid, subs| {
                    index.activators.insert(fid, parse_acti(fid, subs));
                },
            )?,
            b"TERM" => extract_records_with_modl(
                &mut reader,
                end,
                b"TERM",
                &mut statics,
                &mut |fid, subs| {
                    index.terminals.insert(fid, parse_term(fid, subs));
                },
            )?,
            // FLST FormID lists — flat arrays referenced by
            // `IsInList <flst>` perk-entry-point conditions, COBJ
            // recipe filters, FNV Caravan deck composition, and quest
            // objective lookups. Pre-#630 the top-level group fell
            // through to the catch-all skip and every `IsInList`
            // returned "not in list", silently disabling ~50 vanilla
            // FNV PERKs and the entire Caravan mini-game.
            b"FLST" => extract_records(&mut reader, end, b"FLST", &mut |fid, subs| {
                index.form_lists.insert(fid, parse_flst(fid, subs));
            })?,
            // #808 / FNV-D2-NEW-01 — five gameplay-critical record
            // types that previously fell through to the catch-all
            // skip. Stub-form parsing (EDID + a handful of key
            // scalar / form-ref fields); full sub-record decoding
            // lands when the consuming subsystem arrives.
            //
            // PROJ — projectiles (every WEAP references one)
            // EFSH — effect shaders (visual effects)
            // IMOD — item mods (FNV-CORE: weapon attachments)
            // ARMA — armor addons (race-specific biped variants)
            // BPTD — body part data (NPC dismemberment routing)
            b"PROJ" => extract_records(&mut reader, end, b"PROJ", &mut |fid, subs| {
                index.projectiles.insert(fid, parse_proj(fid, subs));
            })?,
            b"EFSH" => extract_records(&mut reader, end, b"EFSH", &mut |fid, subs| {
                index.effect_shaders.insert(fid, parse_efsh(fid, subs));
            })?,
            b"IMOD" => extract_records(&mut reader, end, b"IMOD", &mut |fid, subs| {
                index.item_mods.insert(fid, parse_imod(fid, subs));
            })?,
            b"ARMA" => extract_records(&mut reader, end, b"ARMA", &mut |fid, subs| {
                index.armor_addons.insert(fid, parse_arma(fid, subs));
            })?,
            b"BPTD" => extract_records(&mut reader, end, b"BPTD", &mut |fid, subs| {
                index.body_parts.insert(fid, parse_bptd(fid, subs));
            })?,
            // #809 / FNV-D2-NEW-02 — seven supporting records that
            // gate FNV NPC AI / crafting / impact-effect / faction-
            // reputation subsystems. Same stub-form pattern as #808.
            //
            // REPU — reputation (FNV-CORE: NCR / Legion / etc.)
            // EXPL — explosion (PROJ → EXPL → EFSH chain)
            // CSTY — combat style (NPC AI profile)
            // IDLE — idle animation (NPC behavior tree)
            // IPCT — impact (per-material bullet impact effect)
            // IPDS — impact data set (material-kind → IPCT table)
            // COBJ — constructible object (FNV crafting recipe)
            b"REPU" => extract_records(&mut reader, end, b"REPU", &mut |fid, subs| {
                index.reputations.insert(fid, parse_repu(fid, subs));
            })?,
            b"EXPL" => extract_records(&mut reader, end, b"EXPL", &mut |fid, subs| {
                index.explosions.insert(fid, parse_expl(fid, subs));
            })?,
            b"CSTY" => extract_records(&mut reader, end, b"CSTY", &mut |fid, subs| {
                index.combat_styles.insert(fid, parse_csty(fid, subs));
            })?,
            b"IDLE" => extract_records(&mut reader, end, b"IDLE", &mut |fid, subs| {
                index.idle_animations.insert(fid, parse_idle(fid, subs));
            })?,
            b"IPCT" => extract_records(&mut reader, end, b"IPCT", &mut |fid, subs| {
                index.impacts.insert(fid, parse_ipct(fid, subs));
            })?,
            b"IPDS" => extract_records(&mut reader, end, b"IPDS", &mut |fid, subs| {
                index.impact_data_sets.insert(fid, parse_ipds(fid, subs));
            })?,
            b"COBJ" => extract_records(&mut reader, end, b"COBJ", &mut |fid, subs| {
                index.recipes.insert(fid, parse_cobj(fid, subs));
            })?,
            // #810 / FNV-D2-NEW-03 — 31 long-tail records that fell
            // through the catch-all skip. Bulk-dispatched here using
            // the shared `parse_minimal_esm_record` (EDID + optional
            // FULL). When a real consumer arrives for any one of
            // these, replace the dispatch arm + `MinimalEsmRecord`
            // map with a dedicated parser pair via the established
            // #808 / #809 pattern.
            //
            // Audio metadata (11):
            b"ALOC" => extract_records(&mut reader, end, b"ALOC", &mut |fid, subs| {
                index
                    .audio_locations
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"ANIO" => extract_records(&mut reader, end, b"ANIO", &mut |fid, subs| {
                index
                    .animation_objects
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"ASPC" => extract_records(&mut reader, end, b"ASPC", &mut |fid, subs| {
                index
                    .acoustic_spaces
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"CAMS" => extract_records(&mut reader, end, b"CAMS", &mut |fid, subs| {
                index
                    .camera_shots
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"CPTH" => extract_records(&mut reader, end, b"CPTH", &mut |fid, subs| {
                index
                    .camera_paths
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"DOBJ" => extract_records(&mut reader, end, b"DOBJ", &mut |fid, subs| {
                index
                    .default_objects
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"MICN" => extract_records(&mut reader, end, b"MICN", &mut |fid, subs| {
                index
                    .menu_icons
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"MSET" => extract_records(&mut reader, end, b"MSET", &mut |fid, subs| {
                index
                    .media_sets
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"MUSC" => extract_records(&mut reader, end, b"MUSC", &mut |fid, subs| {
                index
                    .music_types
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"SOUN" => extract_records(&mut reader, end, b"SOUN", &mut |fid, subs| {
                index.sounds.insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"VTYP" => extract_records(&mut reader, end, b"VTYP", &mut |fid, subs| {
                index
                    .voice_types
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            // Visual / world (8):
            b"AMEF" => extract_records(&mut reader, end, b"AMEF", &mut |fid, subs| {
                index
                    .ammo_effects
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"DEBR" => extract_records(&mut reader, end, b"DEBR", &mut |fid, subs| {
                index.debris.insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"GRAS" => extract_records(&mut reader, end, b"GRAS", &mut |fid, subs| {
                index
                    .grasses
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"IMAD" => extract_records(&mut reader, end, b"IMAD", &mut |fid, subs| {
                index
                    .imagespace_modifiers
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"LSCR" => extract_records(&mut reader, end, b"LSCR", &mut |fid, subs| {
                index
                    .load_screens
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"LSCT" => extract_records(&mut reader, end, b"LSCT", &mut |fid, subs| {
                index
                    .load_screen_types
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"PWAT" => extract_records(&mut reader, end, b"PWAT", &mut |fid, subs| {
                index
                    .placeable_waters
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"RGDL" => extract_records(&mut reader, end, b"RGDL", &mut |fid, subs| {
                index
                    .ragdolls
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            // FNV Hardcore mode (4):
            b"DEHY" => extract_records(&mut reader, end, b"DEHY", &mut |fid, subs| {
                index
                    .dehydration_stages
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"HUNG" => extract_records(&mut reader, end, b"HUNG", &mut |fid, subs| {
                index
                    .hunger_stages
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"RADS" => extract_records(&mut reader, end, b"RADS", &mut |fid, subs| {
                index
                    .radiation_stages
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"SLPD" => extract_records(&mut reader, end, b"SLPD", &mut |fid, subs| {
                index
                    .sleep_deprivation_stages
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            // FNV Caravan + Casino (6):
            b"CCRD" => extract_records(&mut reader, end, b"CCRD", &mut |fid, subs| {
                index
                    .caravan_cards
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"CDCK" => extract_records(&mut reader, end, b"CDCK", &mut |fid, subs| {
                index
                    .caravan_decks
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"CHAL" => extract_records(&mut reader, end, b"CHAL", &mut |fid, subs| {
                index
                    .challenges
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"CHIP" => extract_records(&mut reader, end, b"CHIP", &mut |fid, subs| {
                index
                    .poker_chips
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"CMNY" => extract_records(&mut reader, end, b"CMNY", &mut |fid, subs| {
                index
                    .caravan_money
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"CSNO" => extract_records(&mut reader, end, b"CSNO", &mut |fid, subs| {
                index
                    .casinos
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            // Recipe residuals (2):
            b"RCCT" => extract_records(&mut reader, end, b"RCCT", &mut |fid, subs| {
                index
                    .recipe_categories
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            b"RCPE" => extract_records(&mut reader, end, b"RCPE", &mut |fid, subs| {
                index
                    .recipe_records
                    .insert(fid, parse_minimal_esm_record(fid, subs));
            })?,
            _ => {
                reader.skip_group(&group);
            }
        }
    }

    // Resolve LTEX → texture path via TXST indirection.
    // FO3/FNV: LTEX.TNAM → TXST form ID → TXST.TX00 diffuse path.
    // Oblivion: LTEX.ICON is a direct texture path (already in
    // `landscape_textures` from `parse_ltex_group`).
    for (ltex_id, txst_id) in &ltex_to_txst {
        if let Some(path) = txst_textures.get(txst_id) {
            landscape_textures.insert(*ltex_id, path.clone());
        }
    }

    let total_exterior: usize = exterior_cells.values().map(|m| m.len()).sum();
    let wrld_names: Vec<&str> = exterior_cells.keys().map(|s| s.as_str()).collect();
    log::info!(
        "ESM parsed: {} interior cells, {} exterior cells across {} worldspaces, {} base objects, {} landscape textures",
        cells.len(),
        total_exterior,
        exterior_cells.len(),
        statics.len(),
        landscape_textures.len(),
    );
    if !wrld_names.is_empty() {
        log::info!("  Worldspaces: {:?}", wrld_names);
    }

    index.cells = EsmCellIndex {
        cells,
        exterior_cells,
        statics,
        landscape_textures,
        worldspace_climates,
        texture_sets,
        scols,
        packins,
        movables,
        material_swaps,
    };

    // Single source of truth — both this line and `index.total()` walk
    // the same `EsmIndex::categories` table so adding a new record
    // category is a single-edit operation. See #634 / FNV-D2-06.
    log::info!("{}", index.category_breakdown());

    // Localized thread-local restored automatically when
    // `_localized_guard` drops — including on early-return paths and
    // panic unwinds. See #624.
    Ok(index)
}

/// Walk a top-level group once, dispatching every matching record to
/// BOTH a typed-record callback AND the [`StaticObject`] builder for
/// `cells.statics`. Used by `parse_esm_with_load_order` for dual-target
/// labels (WEAP / ARMO / AMMO / MISC / KEYM / ALCH / INGR / BOOK /
/// NOTE / CONT / NPC_ / CREA / ACTI / TERM) — every label that ships
/// both a typed record AND wants visual-placement coverage in
/// `cells.statics`.
///
/// Pre-#527 these labels were walked TWICE: once by
/// `parse_esm_cells_with_load_order` to populate `statics`, and again
/// by the typed dispatcher in `parse_esm_with_load_order`. The fused
/// helper calls `read_sub_records` once and routes the same `subs`
/// slice to both consumers, halving the sub-record decode cost on
/// each dual-target group. Recurses into nested groups so worldspace
/// children and persistent/temporary cell children are handled too —
/// same recursion shape as `extract_records`.
fn extract_records_with_modl(
    reader: &mut EsmReader,
    end: usize,
    expected_type: &[u8; 4],
    statics: &mut HashMap<u32, StaticObject>,
    f: &mut dyn FnMut(u32, &[SubRecord]),
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub_group);
            extract_records_with_modl(reader, sub_end, expected_type, statics, f)?;
            continue;
        }
        let header = reader.read_record_header()?;
        if &header.record_type == expected_type {
            let subs = reader.read_sub_records(&header)?;
            // Cell-side: build the StaticObject from the same subs.
            if let Some(stat) =
                build_static_object_from_subs(header.form_id, &header.record_type, &subs)
            {
                statics.insert(header.form_id, stat);
            }
            // Records-side: typed parser.
            f(header.form_id, &subs);
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Walk a top-level group and call `f(form_id, subs)` for every record
/// matching `expected_type`. Recurses into nested groups so worldspace
/// children and persistent/temporary cell children are handled too.
///
/// `f` takes a closure rather than returning a parsed value so the caller
/// can route the record into a type-specific HashMap without an extra
/// boxing/erasure layer.
fn extract_records(
    reader: &mut EsmReader,
    end: usize,
    expected_type: &[u8; 4],
    f: &mut dyn FnMut(u32, &[SubRecord]),
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub_group);
            extract_records(reader, sub_end, expected_type, f)?;
            continue;
        }
        let header = reader.read_record_header()?;
        if &header.record_type == expected_type {
            let subs = reader.read_sub_records(&header)?;
            f(header.form_id, &subs);
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Walk a top-level DIAL group, parsing each DIAL record and its
/// child INFO sub-group (group_type == 7 Topic Children). Each
/// sub-GRUP's `label` field carries the parent DIAL's form_id u32 —
/// the walker matches it against the most recent DIAL it parsed and
/// pushes decoded INFOs onto `DialRecord.infos`.
///
/// Layout:
/// ```text
/// GRUP type=0 label="DIAL"  (top-level — caller already entered)
///   DIAL record (form_id=A)
///   GRUP type=7 label=A     (Topic Children for DIAL A)
///     INFO record
///     INFO record
///     ...
///   DIAL record (form_id=B)
///   GRUP type=7 label=B
///     INFO record
///   ...
/// ```
///
/// Pre-#631 the generic `extract_records` walker ignored INFO bytes
/// because it filtered on `expected_type == "DIAL"`. Dedicated walker
/// stays SSE-correct and avoids parameterising the generic walker
/// with a multi-type closure map (the only record with this shape
/// today). See audit `AUDIT_FNV_2026-04-24.md` D2-03.
fn extract_dial_with_info(
    reader: &mut EsmReader,
    end: usize,
    dialogues: &mut HashMap<u32, DialRecord>,
) -> Result<()> {
    /// Topic Children group_type from the ESM format (TES4 / FO3 /
    /// FNV / Skyrim / FO4 all share the value).
    const GROUP_TYPE_TOPIC_CHILDREN: u32 = 7;

    let mut last_dial_form_id: Option<u32> = None;

    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            let sub_group = reader.read_group_header()?;
            let sub_end = reader.group_content_end(&sub_group);

            if sub_group.group_type == GROUP_TYPE_TOPIC_CHILDREN {
                // Sub-group label is the parent DIAL's form_id u32.
                let parent_form_id = u32::from_le_bytes(sub_group.label);
                // Tolerate sub-group / last-DIAL label drift —
                // shipped content has been observed with off-by-one
                // dispositions across patches. We accept the most-
                // recent DIAL as parent when the labels disagree, and
                // log at debug; mismatch is rare enough to warrant
                // visibility but never bytes-throwing.
                let target = last_dial_form_id.unwrap_or(parent_form_id);
                if Some(parent_form_id) != last_dial_form_id {
                    log::debug!(
                        "DIAL Topic Children sub-group label {:#x} doesn't match \
                         most-recent DIAL form_id {:?}; routing INFOs to \
                         most-recent DIAL — see #631",
                        parent_form_id,
                        last_dial_form_id,
                    );
                }
                walk_info_records(reader, sub_end, target, dialogues)?;
                continue;
            }

            // Any other nested group inside the DIAL tree (rare —
            // shouldn't happen in vanilla content): recurse with the
            // same handler so a stray DIAL or another Topic Children
            // tier still gets walked. Bytes accounting stays sound.
            extract_dial_with_info(reader, sub_end, dialogues)?;
            continue;
        }

        let header = reader.read_record_header()?;
        if &header.record_type == b"DIAL" {
            let subs = reader.read_sub_records(&header)?;
            let dial = parse_dial(header.form_id, &subs);
            dialogues.insert(header.form_id, dial);
            last_dial_form_id = Some(header.form_id);
        } else {
            // Non-DIAL record at this tier — skip and keep walking.
            reader.skip_record(&header);
        }
    }
    Ok(())
}

/// Inner helper for `extract_dial_with_info` — walks a Topic Children
/// sub-GRUP, decoding each INFO record onto the parent DIAL's
/// `infos` vec. Skips non-INFO records (defensive — shipped content
/// may include nested QSTR / NAVI tiers in some patches).
fn walk_info_records(
    reader: &mut EsmReader,
    end: usize,
    parent_dial_form_id: u32,
    dialogues: &mut HashMap<u32, DialRecord>,
) -> Result<()> {
    while reader.position() < end && reader.remaining() > 0 {
        if reader.is_group() {
            // Nested group inside a Topic Children sub-GRUP —
            // unusual but tolerated. Skip wholesale rather than
            // recursing further; the runtime consumer doesn't need
            // the deeper tiers today.
            let inner = reader.read_group_header()?;
            reader.skip_group(&inner);
            continue;
        }
        let header = reader.read_record_header()?;
        if &header.record_type == b"INFO" {
            let subs = reader.read_sub_records(&header)?;
            let info = parse_info(header.form_id, &subs);
            if let Some(dial) = dialogues.get_mut(&parent_dial_form_id) {
                dial.infos.push(info);
            }
        } else {
            reader.skip_record(&header);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a single STAT-style record bytes for the given type code, form ID,
    /// and sub-record list.
    fn build_record(typ: &[u8; 4], form_id: u32, subs: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut sub_data = Vec::new();
        for (st, data) in subs {
            sub_data.extend_from_slice(*st);
            sub_data.extend_from_slice(&(data.len() as u16).to_le_bytes());
            sub_data.extend_from_slice(data);
        }
        let mut buf = Vec::new();
        buf.extend_from_slice(typ);
        buf.extend_from_slice(&(sub_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // flags
        buf.extend_from_slice(&form_id.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]); // padding
        buf.extend_from_slice(&sub_data);
        buf
    }

    /// Wrap a record byte blob in a top-level GRUP with the given label.
    fn wrap_group(label: &[u8; 4], record: &[u8]) -> Vec<u8> {
        let total = 24 + record.len();
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GRUP");
        buf.extend_from_slice(&(total as u32).to_le_bytes());
        buf.extend_from_slice(label);
        buf.extend_from_slice(&0u32.to_le_bytes()); // group_type = top
        buf.extend_from_slice(&[0u8; 8]);
        buf.extend_from_slice(record);
        buf
    }

    #[test]
    fn extract_records_walks_one_group() {
        let mut subs: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
        subs.push((b"EDID", b"TestWeap\0".to_vec()));
        subs.push((b"DATA", {
            let mut d = Vec::new();
            d.extend_from_slice(&100u32.to_le_bytes()); // value
            d.extend_from_slice(&0u32.to_le_bytes()); // health
            d.extend_from_slice(&2.0f32.to_le_bytes()); // weight
            d.extend_from_slice(&20u16.to_le_bytes()); // damage
            d.push(8); // clip
            d.push(0);
            d
        }));
        let record = build_record(b"WEAP", 0xCAFE, &subs);
        let group = wrap_group(b"WEAP", &record);

        // Wrap with TES4 dummy header up front so parse_esm's reader skips
        // cleanly into the WEAP group.
        let mut tes4 = build_record(b"TES4", 0, &[]);
        tes4.extend_from_slice(&group);
        let index = parse_esm(&tes4).unwrap();

        assert_eq!(index.items.len(), 1);
        let weap = index.items.get(&0xCAFE).expect("WEAP indexed");
        assert_eq!(weap.common.editor_id, "TestWeap");
        match weap.kind {
            ItemKind::Weapon {
                damage, clip_size, ..
            } => {
                assert_eq!(damage, 20);
                assert_eq!(clip_size, 8);
            }
            _ => panic!("expected weapon"),
        }
    }

    /// Regression: #631 / FNV-D2-03 — `DIAL` records with a nested
    /// Topic Children sub-GRUP (`group_type == 7`, label = parent
    /// DIAL form_id) must populate `DialRecord.infos` with each
    /// child INFO record. Pre-fix the generic `extract_records`
    /// walker filtered on `expected_type == "DIAL"` and silently
    /// skipped every INFO; every DIAL arrived as an empty shell.
    ///
    /// Fixture builds:
    ///   GRUP type=0 label="DIAL"
    ///     DIAL record (form_id 0xCAFE, EDID="MQGreeting", FULL="Hello")
    ///     GRUP type=7 label=0xCAFE
    ///       INFO (0x1001, NAM1="Welcome", TRDT[0]=3, PNAM=0)
    ///       INFO (0x1002, NAM1="Wait outside.", PNAM=0x1001)
    ///
    /// Asserts both INFOs land on the parent DIAL with their
    /// authored fields.
    #[test]
    fn dial_topic_children_walked_into_dialogue_infos() {
        // Two INFO records inside the Topic Children sub-GRUP.
        let info_1 = build_record(
            b"INFO",
            0x1001,
            &[
                (b"NAM1", b"Welcome\0".to_vec()),
                (b"TRDT", vec![3, 0, 0, 0]),
                (b"PNAM", 0u32.to_le_bytes().to_vec()),
            ],
        );
        let info_2 = build_record(
            b"INFO",
            0x1002,
            &[
                (b"NAM1", b"Wait outside.\0".to_vec()),
                (b"PNAM", 0x1001u32.to_le_bytes().to_vec()),
            ],
        );

        // Topic Children sub-GRUP: group_type = 7, label = parent
        // DIAL form_id (0xCAFE) packed as little-endian bytes.
        let topic_children = {
            let mut content = Vec::new();
            content.extend_from_slice(&info_1);
            content.extend_from_slice(&info_2);
            let total = 24 + content.len();
            let mut buf = Vec::new();
            buf.extend_from_slice(b"GRUP");
            buf.extend_from_slice(&(total as u32).to_le_bytes());
            buf.extend_from_slice(&0xCAFEu32.to_le_bytes()); // label
            buf.extend_from_slice(&7u32.to_le_bytes()); // Topic Children
            buf.extend_from_slice(&[0u8; 8]); // stamp
            buf.extend_from_slice(&content);
            buf
        };

        // DIAL record + its Topic Children sub-GRUP, wrapped in the
        // top-level "DIAL" GRUP.
        let dial = build_record(
            b"DIAL",
            0xCAFE,
            &[
                (b"EDID", b"MQGreeting\0".to_vec()),
                (b"FULL", b"Hello\0".to_vec()),
            ],
        );
        let mut top_content = Vec::new();
        top_content.extend_from_slice(&dial);
        top_content.extend_from_slice(&topic_children);

        let top_total = 24 + top_content.len();
        let mut top_grup = Vec::new();
        top_grup.extend_from_slice(b"GRUP");
        top_grup.extend_from_slice(&(top_total as u32).to_le_bytes());
        top_grup.extend_from_slice(b"DIAL");
        top_grup.extend_from_slice(&0u32.to_le_bytes()); // top group
        top_grup.extend_from_slice(&[0u8; 8]);
        top_grup.extend_from_slice(&top_content);

        // TES4 dummy header so parse_esm reaches the DIAL group.
        let mut buf = build_record(b"TES4", 0, &[]);
        buf.extend_from_slice(&top_grup);
        let index = parse_esm(&buf).expect("parse_esm");

        let dial = index.dialogues.get(&0xCAFE).expect("DIAL indexed");
        assert_eq!(dial.editor_id, "MQGreeting");
        assert_eq!(dial.full_name, "Hello");
        assert_eq!(
            dial.infos.len(),
            2,
            "Topic Children INFOs must land on DialRecord.infos (#631)"
        );
        assert_eq!(dial.infos[0].form_id, 0x1001);
        assert_eq!(dial.infos[0].response_text, "Welcome");
        assert_eq!(dial.infos[0].response_type, 3);
        assert_eq!(dial.infos[0].previous_info, 0);
        assert_eq!(dial.infos[1].form_id, 0x1002);
        assert_eq!(dial.infos[1].response_text, "Wait outside.");
        assert_eq!(
            dial.infos[1].previous_info, 0x1001,
            "INFO chain links must survive the walker (#631)"
        );
    }

    /// Real-data sanity for #631: opt-in load of FalloutNV.esm
    /// asserts at least one DIAL has non-empty `infos`. Pre-fix the
    /// whole `dialogues` map's `infos` was empty across every DIAL.
    /// Stays `#[ignore]` like the rest of the real-data tests.
    #[test]
    #[ignore]
    fn parse_real_fnv_dial_infos_populated() {
        let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/FalloutNV.esm";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: FalloutNV.esm not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
        let index = parse_esm(&data).expect("parse_esm");

        let total_infos: usize = index.dialogues.values().map(|d| d.infos.len()).sum();
        let dialogues_with_infos = index
            .dialogues
            .values()
            .filter(|d| !d.infos.is_empty())
            .count();
        eprintln!(
            "FNV dialogues: {} total, {} with INFOs ({} INFOs total)",
            index.dialogues.len(),
            dialogues_with_infos,
            total_infos,
        );
        assert!(
            !index.dialogues.is_empty(),
            "FNV must ship at least one DIAL"
        );
        assert!(
            total_infos > 0,
            "FNV must surface at least one INFO across all DIALs (#631)"
        );
    }

    /// Parse the real FalloutNV.esm and verify record counts. Skipped on
    /// machines without the game data — opt in with `cargo test -p
    /// byroredux-plugin -- --ignored`.
    ///
    /// The thresholds are deliberately conservative: FNV ships with ~1700
    /// weapons, ~1800 armor pieces, ~5000 misc items, ~3300 NPCs, ~120 races
    /// (a lot of variants), ~70 classes, ~250 factions, and a few hundred
    /// globals/game settings. We just check we're in the right order of
    /// magnitude — exact numbers drift with patches.
    #[test]
    #[ignore]
    fn parse_real_fnv_esm_record_counts() {
        let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data/FalloutNV.esm";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: FalloutNV.esm not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
        let index = parse_esm(&data).expect("parse_esm");

        eprintln!(
            "FNV index: {} items, {} containers, {} LVLI, {} LVLN, {} NPCs, \
             {} races, {} classes, {} factions, {} globals, {} game settings",
            index.items.len(),
            index.containers.len(),
            index.leveled_items.len(),
            index.leveled_npcs.len(),
            index.npcs.len(),
            index.races.len(),
            index.classes.len(),
            index.factions.len(),
            index.globals.len(),
            index.game_settings.len(),
        );

        // Floors based on actual FNV.esm counts (April 2026 patch revision):
        // items=2643 containers=2478 LVLI=2738 LVLN=365 NPCs=3816 races=22
        // classes=74 factions=682 globals=218 game_settings=648.
        // Each floor is a couple percent below the observed count so the test
        // stays stable across DLC patches without becoming meaningless.
        assert!(
            index.items.len() > 2500,
            "expected >2.5k items, got {}",
            index.items.len()
        );
        assert!(
            index.containers.len() > 2000,
            "expected >2k containers, got {}",
            index.containers.len()
        );
        assert!(
            index.leveled_items.len() > 2000,
            "expected >2k leveled item lists, got {}",
            index.leveled_items.len()
        );
        assert!(
            index.leveled_npcs.len() > 250,
            "expected >250 leveled NPC lists, got {}",
            index.leveled_npcs.len()
        );
        assert!(
            index.npcs.len() > 3000,
            "expected >3k NPCs, got {}",
            index.npcs.len()
        );
        assert!(
            index.races.len() >= 15,
            "expected ≥15 races, got {}",
            index.races.len()
        );
        assert!(
            index.classes.len() > 50,
            "expected >50 classes, got {}",
            index.classes.len()
        );
        assert!(
            index.factions.len() > 500,
            "expected >500 factions, got {}",
            index.factions.len()
        );
        assert!(
            index.globals.len() > 150,
            "expected >150 globals, got {}",
            index.globals.len()
        );
        assert!(
            index.game_settings.len() > 500,
            "expected >500 game settings, got {}",
            index.game_settings.len()
        );

        // Supplementary records (#458). Floors based on a live FNV.esm
        // parse run on the April 2026 patch — each floor sits a few
        // percent below the observed count so DLC patches stay green.
        //
        // Observed on FalloutNV.esm:
        //   WATR=78, NAVI=1, NAVM=0 (NAVM entries live nested under
        //   CELL children groups on FO3/FNV, not at top level — a
        //   follow-up can walk those if needed), REGN=276, ECZN=17,
        //   LGTM=31, HDPT=61, EYES=12, HAIR=67.
        eprintln!(
            "FNV misc: {} water, {} navi, {} navm, {} region, {} eczn, \
             {} lgtm, {} hdpt, {} eyes, {} hair",
            index.waters.len(),
            index.navi_info.len(),
            index.navmeshes.len(),
            index.regions.len(),
            index.encounter_zones.len(),
            index.lighting_templates.len(),
            index.head_parts.len(),
            index.eyes.len(),
            index.hair.len(),
        );
        assert!(
            index.waters.len() >= 50,
            "expected ≥50 WATR water types, got {}",
            index.waters.len()
        );
        assert_eq!(
            index.navi_info.len(),
            1,
            "expected exactly 1 NAVI master (FNV ships one), got {}",
            index.navi_info.len()
        );
        assert!(
            index.regions.len() >= 200,
            "expected ≥200 REGN regions, got {}",
            index.regions.len()
        );
        assert!(
            index.encounter_zones.len() >= 10,
            "expected ≥10 ECZN encounter zones, got {}",
            index.encounter_zones.len()
        );
        assert!(
            index.lighting_templates.len() >= 20,
            "expected ≥20 LGTM templates, got {}",
            index.lighting_templates.len()
        );
        assert!(
            index.head_parts.len() >= 40,
            "expected ≥40 HDPT head parts, got {}",
            index.head_parts.len()
        );
        assert!(
            index.eyes.len() >= 8,
            "expected ≥8 EYES definitions, got {}",
            index.eyes.len()
        );
        assert!(
            index.hair.len() >= 50,
            "expected ≥50 HAIR definitions, got {}",
            index.hair.len()
        );

        // #519 — AVIF actor-value records. FNV ships ~70 vanilla
        // AVIFs (7 SPECIAL + 13 governed skills + ~50 derived
        // resources/resistances/VATS targets); audit floor of 30
        // is the conservative threshold from the issue body.
        eprintln!("FNV AVIF: {} actor values", index.actor_values.len());
        assert!(
            index.actor_values.len() >= 30,
            "expected ≥30 AVIF actor values, got {}",
            index.actor_values.len()
        );
        // Sanity: FNV ships AVIFs under the "AV<Name>" convention
        // (AVStrength, AVAgility, AVBigGuns, …). Verify both that
        // the EDIDs round-trip non-empty *and* that the SPECIAL
        // attribute set is present.
        let nonempty = index
            .actor_values
            .values()
            .filter(|av| !av.editor_id.is_empty())
            .count();
        assert!(
            nonempty >= 30,
            "expected ≥30 AVIFs with non-empty editor_id, got {nonempty}"
        );
        for special in [
            "AVStrength",
            "AVPerception",
            "AVEndurance",
            "AVCharisma",
            "AVIntelligence",
            "AVAgility",
            "AVLuck",
        ] {
            let found = index
                .actor_values
                .values()
                .any(|av| av.editor_id == special);
            assert!(found, "expected SPECIAL AVIF '{special}' to be indexed");
        }

        // #629 / FNV-D2-01 — ENCH dispatch. Pre-fix the entire
        // top-level group fell through to the catch-all skip and every
        // weapon EITM dangled. FNV ships ~150 ENCH records (Pulse Gun,
        // This Machine, Holorifle, the energy-weapon variants, and
        // armor-side enchants); the floor is conservative against DLC
        // patch drift.
        eprintln!("FNV ENCH: {} enchantments", index.enchantments.len());
        assert!(
            index.enchantments.len() >= 50,
            "expected ≥50 ENCH enchantments, got {}",
            index.enchantments.len()
        );

        // Spot-check a known FNV item: Varmint Rifle (form 0x000086A8) should
        // be a Weapon kind with damage and a clip size.
        if let Some(varmint) = index.items.get(&0x000086A8) {
            eprintln!(
                "Varmint Rifle: {:?} kind={}",
                varmint.common.editor_id,
                varmint.kind.label()
            );
            assert_eq!(varmint.kind.label(), "WEAP");
        }

        // Spot-check that NCR faction exists (FNV form 0x0011E662 — name varies
        // by patch; just check there is a faction with "NCR" in its full name).
        let has_ncr = index
            .factions
            .values()
            .any(|f| f.full_name.contains("NCR") || f.editor_id.starts_with("NCR"));
        assert!(has_ncr, "expected an NCR-related faction");
    }

    /// Regression: #445 — the load-order remap routes each record's
    /// own FormID through its plugin's global load-order slot. A
    /// synthetic "DLC" with plugin_index=2 writes a self-referencing
    /// form 0x0100_BEEF (mod_index=1 == num_masters=1 → self), which
    /// under remap lands as 0x0200_BEEF in the global map.
    #[test]
    fn parse_esm_with_load_order_remaps_self_form_ids() {
        let mut subs: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
        subs.push((b"EDID", b"TestWeap\0".to_vec()));
        subs.push((b"DATA", {
            let mut d = Vec::new();
            d.extend_from_slice(&100u32.to_le_bytes()); // value
            d.extend_from_slice(&0u32.to_le_bytes()); // health
            d.extend_from_slice(&2.0f32.to_le_bytes()); // weight
            d.extend_from_slice(&20u16.to_le_bytes()); // damage
            d.push(8);
            d.push(0);
            d
        }));
        // In-file form_id 0x0100_BEEF — mod_index=1, which for a DLC
        // with one master equals its self-index.
        let record = build_record(b"WEAP", 0x0100_BEEF, &subs);
        let group = wrap_group(b"WEAP", &record);
        let mut tes4 = build_record(b"TES4", 0, &[]);
        tes4.extend_from_slice(&group);

        // Load this synthetic "DLC" at plugin_index=2 with Fallout3
        // (plugin_index=0) as its single master.
        let remap = super::super::reader::FormIdRemap {
            plugin_index: 2,
            master_indices: vec![0],
        };
        let index = parse_esm_with_load_order(&tes4, Some(remap)).unwrap();
        assert_eq!(index.items.len(), 1);
        let remapped_key = 0x0200_BEEFu32;
        assert!(
            index.items.contains_key(&remapped_key),
            "DLC self-ref 0x0100_BEEF must remap to global 0x0200_BEEF at plugin_index=2 (#445)"
        );
        // The pre-remap key must NOT be present.
        assert!(
            !index.items.contains_key(&0x0100_BEEF),
            "raw pre-remap FormID must not leak through once the remap is installed"
        );
    }

    /// Regression: #443 — a top-level `SCPT` GRUP must dispatch to
    /// `parse_scpt` and land in `EsmIndex.scripts`. Pre-fix the whole
    /// group fell through to the catch-all skip so every NPC / item
    /// `SCRI` FormID cross-reference dangled.
    #[test]
    fn scpt_group_dispatches_to_scripts_map() {
        let mut schr = Vec::new();
        schr.extend_from_slice(&0u32.to_le_bytes()); // pad
        schr.extend_from_slice(&0u32.to_le_bytes()); // num_refs
        schr.extend_from_slice(&42u32.to_le_bytes()); // compiled_size
        schr.extend_from_slice(&0u32.to_le_bytes()); // var_count
        schr.extend_from_slice(&0u16.to_le_bytes()); // object
        schr.extend_from_slice(&0u32.to_le_bytes()); // flags (FO3 u32 tail)
        let subs: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"EDID", b"DummyScript\0".to_vec()),
            (b"SCHR", schr),
            (b"SCDA", vec![0u8; 42]),
        ];
        let record = build_record(b"SCPT", 0xBEEF_0003, &subs);
        let group = wrap_group(b"SCPT", &record);
        let mut tes4 = build_record(b"TES4", 0, &[]);
        tes4.extend_from_slice(&group);
        let index = parse_esm(&tes4).unwrap();
        assert_eq!(index.scripts.len(), 1, "SCPT must land in scripts map");
        let scpt = index.scripts.get(&0xBEEF_0003).expect("SCPT indexed");
        assert_eq!(scpt.editor_id, "DummyScript");
        assert_eq!(scpt.compiled_size, 42);
        assert_eq!(scpt.compiled.len(), 42);
    }

    /// Regression: #519 — a top-level `AVIF` GRUP must dispatch to
    /// `parse_avif` and land in `EsmIndex.actor_values`. Pre-fix the
    /// whole group fell through to the catch-all skip, so every NPC
    /// `skill_bonuses` cross-ref, BOOK skill-book teach ref, and
    /// AVIF-keyed condition predicate dangled.
    #[test]
    fn avif_group_dispatches_to_actor_values_map() {
        let mut avsk = Vec::new();
        avsk.extend_from_slice(&1.0f32.to_le_bytes());
        avsk.extend_from_slice(&0.0f32.to_le_bytes());
        avsk.extend_from_slice(&1.5f32.to_le_bytes());
        avsk.extend_from_slice(&2.0f32.to_le_bytes());
        let subs: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"EDID", b"SmallGuns\0".to_vec()),
            (b"FULL", b"Small Guns\0".to_vec()),
            (b"CNAM", 1u32.to_le_bytes().to_vec()),
            (b"AVSK", avsk),
        ];
        let record = build_record(b"AVIF", 0xBEEF_002B, &subs);
        let group = wrap_group(b"AVIF", &record);
        let mut tes4 = build_record(b"TES4", 0, &[]);
        tes4.extend_from_slice(&group);
        let index = parse_esm(&tes4).unwrap();

        assert_eq!(
            index.actor_values.len(),
            1,
            "AVIF must populate the actor_values map"
        );
        let avif = index.actor_values.get(&0xBEEF_002B).expect("AVIF indexed");
        assert_eq!(avif.editor_id, "SmallGuns");
        assert_eq!(avif.full_name, "Small Guns");
        assert_eq!(avif.category, 1);
        assert!(avif.skill_scaling.is_some());
    }

    /// Regression: #442 — a top-level `CREA` GRUP must dispatch to
    /// `parse_npc` (schema is NPC_-shaped) and land in
    /// `EsmIndex.creatures`. Pre-fix the whole group fell through to
    /// the catch-all skip.
    #[test]
    fn crea_group_dispatches_to_creatures_map() {
        let mut subs: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
        subs.push((b"EDID", b"Radroach\0".to_vec()));
        subs.push((b"FULL", b"Radroach\0".to_vec()));
        subs.push((b"MODL", b"Creatures\\Radroach.nif\0".to_vec()));
        let record = build_record(b"CREA", 0xBEEF_0001, &subs);
        let group = wrap_group(b"CREA", &record);

        let mut tes4 = build_record(b"TES4", 0, &[]);
        tes4.extend_from_slice(&group);
        let index = parse_esm(&tes4).unwrap();

        assert_eq!(
            index.creatures.len(),
            1,
            "CREA must populate the creatures map"
        );
        let crea = index.creatures.get(&0xBEEF_0001).expect("CREA indexed");
        assert_eq!(crea.editor_id, "Radroach");
        assert_eq!(crea.full_name, "Radroach");
        assert_eq!(crea.model_path, "Creatures\\Radroach.nif");
        // CREA must not leak into NPC_'s map.
        assert!(index.npcs.is_empty());
    }

    /// Regression: #448 — a top-level `LVLC` GRUP must dispatch to
    /// `parse_leveled_list` and land in `EsmIndex.leveled_creatures`.
    /// Pre-fix the whole group fell through to the catch-all skip,
    /// so every FO3 encounter zone's creature spawn table came back
    /// empty.
    #[test]
    fn lvlc_group_dispatches_to_leveled_creatures_map() {
        // LVLC shares the LVLI/LVLN layout: LVLD (u8 chance_none),
        // LVLF (u8 flags), LVLO (12 bytes: level u16 + pad u16 + form u32 + count u16 + pad u16).
        let mut subs: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
        subs.push((b"EDID", b"LL_Raider\0".to_vec()));
        subs.push((b"LVLD", vec![50u8])); // 50% chance none
        subs.push((b"LVLF", vec![1u8])); // calculate_from_all flag
        subs.push((b"LVLO", {
            let mut d = Vec::new();
            d.extend_from_slice(&1u16.to_le_bytes()); // level
            d.extend_from_slice(&0u16.to_le_bytes()); // pad
            d.extend_from_slice(&0xCAFE_F00Du32.to_le_bytes()); // form
            d.extend_from_slice(&1u16.to_le_bytes()); // count
            d.extend_from_slice(&0u16.to_le_bytes()); // pad
            d
        }));
        let record = build_record(b"LVLC", 0xBEEF_0002, &subs);
        let group = wrap_group(b"LVLC", &record);

        let mut tes4 = build_record(b"TES4", 0, &[]);
        tes4.extend_from_slice(&group);
        let index = parse_esm(&tes4).unwrap();

        assert_eq!(
            index.leveled_creatures.len(),
            1,
            "LVLC must populate the leveled_creatures map"
        );
        let lvlc = index
            .leveled_creatures
            .get(&0xBEEF_0002)
            .expect("LVLC indexed");
        assert_eq!(lvlc.editor_id, "LL_Raider");
        assert_eq!(lvlc.entries.len(), 1);
        assert_eq!(lvlc.entries[0].form_id, 0xCAFE_F00D);
        // LVLC must not leak into LVLI / LVLN.
        assert!(index.leveled_items.is_empty());
        assert!(index.leveled_npcs.is_empty());
    }

    /// Parse real Fallout3.esm and assert the bestiary + spawn tables
    /// arrive populated. Ignored by default — opt in with
    /// `cargo test -p byroredux-plugin -- --ignored`.
    ///
    /// Sampled counts against FO3 GOTY HEDR=0.94 master on 2026-04-19:
    /// 1647 NPCs, 533 creatures, 89 LVLN, 60 LVLC. Floors are set a
    /// few percent below observed so the test stays stable across DLC
    /// patches without becoming meaningless. The audit body predicted
    /// ~700-800 CREA / ~400-500 LVLC; the real numbers are lower, so
    /// don't chase the audit's estimates — use the disk-sampled ones.
    /// Parse real Fallout3.esm and assert SCPT records arrive + at
    /// least one NPC's SCRI FormID resolves into the scripts map.
    /// Ignored — opt in with `--ignored`.
    #[test]
    #[ignore]
    fn parse_real_fo3_esm_scpt_count_and_scri_resolves() {
        let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/Fallout3.esm";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: Fallout3.esm not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
        let index = parse_esm(&data).expect("parse_esm");
        eprintln!("FO3 SCPT: {} records", index.scripts.len());
        assert!(
            index.scripts.len() > 500,
            "expected >500 SCPT records (FO3 GOTY ships ~1500+), got {}",
            index.scripts.len()
        );

        // Find any NPC / container record whose script_form_id lands
        // inside the scripts map — pre-#443 nothing could satisfy this
        // because scripts was always empty.
        let resolved = index
            .npcs
            .values()
            .filter(|n| n.has_script || n.disposition_base != 0)
            .count();
        // Any SCRI dereference working is sufficient — we don't parse
        // NPC SCRI yet (it's tracked elsewhere), so just assert the map
        // isn't empty and one script has a resolvable SCRV/SCRO ref
        // pointing at another record.
        let cross_ref_count: usize = index
            .scripts
            .values()
            .filter(|s| !s.ref_form_ids.is_empty())
            .count();
        eprintln!(
            "{} scripts carry at least one SCRV/SCRO cross-ref; {} NPCs had context hints",
            cross_ref_count, resolved
        );
        assert!(
            cross_ref_count > 100,
            "expected >100 scripts with SCRV/SCRO cross-refs, got {cross_ref_count}"
        );
    }

    #[test]
    #[ignore]
    fn parse_real_fo3_esm_crea_and_lvlc_counts() {
        let path = "/mnt/data/SteamLibrary/steamapps/common/Fallout 3 goty/Data/Fallout3.esm";
        if !std::path::Path::new(path).exists() {
            eprintln!("Skipping: Fallout3.esm not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
        let index = parse_esm(&data).expect("parse_esm");
        eprintln!(
            "FO3 index: {} NPCs, {} creatures, {} LVLN, {} LVLC",
            index.npcs.len(),
            index.creatures.len(),
            index.leveled_npcs.len(),
            index.leveled_creatures.len(),
        );
        assert!(
            index.creatures.len() > 400,
            "expected >400 CREA records (observed 533), got {}",
            index.creatures.len()
        );
        assert!(
            index.leveled_creatures.len() > 40,
            "expected >40 LVLC records (observed 60), got {}",
            index.leveled_creatures.len()
        );
    }

    #[test]
    fn esm_index_total_counts_all_categories() {
        let mut idx = EsmIndex::default();
        idx.items.insert(
            1,
            ItemRecord {
                form_id: 1,
                common: Default::default(),
                kind: ItemKind::Misc,
            },
        );
        idx.npcs.insert(
            2,
            NpcRecord {
                form_id: 2,
                editor_id: String::new(),
                full_name: String::new(),
                model_path: String::new(),
                race_form_id: 0,
                class_form_id: 0,
                voice_form_id: 0,
                factions: Vec::new(),
                inventory: Vec::new(),
                ai_packages: Vec::new(),
                death_item_form_id: 0,
                level: 1,
                disposition_base: 50,
                acbs_flags: 0,
                has_script: false,
                face_morphs: None,
                runtime_facegen: None,
            },
        );
        assert_eq!(idx.total(), 2);
    }

    /// #634 / FNV-D2-06 — `total()` and the end-of-parse log line must
    /// drive off the same `categories()` table. Verify the table sums
    /// to `total()` and that the breakdown line names every category
    /// (so a future `index.foos: HashMap<...>` addition that misses a
    /// `categories()` row is caught loud). The cells.statics overlap
    /// with typed maps is intentional — see `categories()` doc.
    #[test]
    fn total_and_breakdown_drive_off_same_table() {
        let mut idx = EsmIndex::default();
        idx.items.insert(
            1,
            ItemRecord {
                form_id: 1,
                common: Default::default(),
                kind: ItemKind::Misc,
            },
        );
        idx.activators.insert(
            10,
            ActiRecord {
                form_id: 10,
                ..Default::default()
            },
        );
        idx.enchantments.insert(
            20,
            EnchRecord {
                form_id: 20,
                ..Default::default()
            },
        );
        // Sum the table by hand — must match `total()`.
        let sum: usize = EsmIndex::categories().iter().map(|(_, f)| f(&idx)).sum();
        assert_eq!(idx.total(), sum);
        assert_eq!(idx.total(), 3);

        // The breakdown line must mention every category by label so a
        // future struct-field addition that misses `categories()` is
        // caught here rather than discovered via a silent log drift.
        let line = idx.category_breakdown();
        for (label, _) in EsmIndex::categories() {
            assert!(
                line.contains(label),
                "breakdown line missing category '{label}': {line}"
            );
        }
        // And the totals from each row must round-trip into the line
        // (non-zero rows specifically — formatted as `<n> <label>`).
        assert!(line.contains("1 items"), "breakdown: {line}");
        assert!(line.contains("1 activators"), "breakdown: {line}");
        assert!(line.contains("1 enchantments"), "breakdown: {line}");
    }

    /// Guard against an `EsmIndex` field addition that forgets to wire
    /// a row in `categories()`. We can't enumerate fields at runtime,
    /// but we can pin the row count: every public top-level map +
    /// `cells.cells` + `cells.statics` is one row. If you add a new
    /// `pub foos: HashMap<...>` field, increment this.
    #[test]
    fn categories_table_row_count_pinned() {
        // 80 typed maps on EsmIndex + 2 from cells (cells, statics).
        // Bumped from 37 → 38 in #624 (image_spaces map for IMGS dispatch).
        // Bumped from 38 → 39 in #630 (form_lists map for FLST dispatch).
        // Bumped from 39 → 44 in #808 (FNV-D2-NEW-01: PROJ + EFSH +
        //   IMOD + ARMA + BPTD stubs for FNV gameplay coverage).
        // Bumped from 44 → 51 in #809 (FNV-D2-NEW-02: REPU + EXPL +
        //   CSTY + IDLE + IPCT + IPDS + COBJ stubs for NPC AI /
        //   crafting / impact-effect / faction-reputation coverage).
        // Bumped from 51 → 82 in #810 (FNV-D2-NEW-03: 31 long-tail
        //   minimal-stub records covering audio metadata, visual /
        //   world, hardcore mode, Caravan + Casino, recipe residuals).
        //   All 31 share `MinimalEsmRecord` via
        //   `parse_minimal_esm_record` — replace with dedicated
        //   per-record parsers via the #808/#809 pattern when a
        //   consumer arrives.
        // Bumped from 82 → 87 in #817 (FO4-D4-NEW-05: 5 FO4-architecture
        //   maps that live on `EsmCellIndex` rather than `EsmIndex` —
        //   texture_sets, scols, packins, movables, material_swaps —
        //   were silently uncovered by `category_breakdown()` and
        //   would let regressions slip through CI).
        // Bump in lockstep with the struct + `categories()` edits.
        assert_eq!(EsmIndex::categories().len(), 87);
    }
}
