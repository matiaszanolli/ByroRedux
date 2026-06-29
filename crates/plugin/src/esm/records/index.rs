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
    IdleRecord, ImgsRecord, ImodRecord, IpctRecord, IpdsRecord, ItemRecord, LeveledList,
    LgtmRecord, MesgRecord, MgefRecord, MinimalEsmRecord, NaviRecord, NavmRecord, NpcRecord,
    OtftRecord, PackRecord, PerkRecord, ProjRecord, QustRecord, RaceRecord, RegnRecord, RepuRecord,
    ScriptRecord, SlgmRecord, SpelRecord, TermRecord, TreeRecord, WatrRecord, WeatherRecord,
};
use std::collections::HashMap;

/// One entry in the [`EsmIndex::categories`] table: a display label paired
/// with a closure that returns the live count for that category.
pub type CategoryEntry = (&'static str, fn(&EsmIndex) -> usize);

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
            // #969 / OBL-D3-NEW-05 — Oblivion-only 4-char-code → MGEF
            // FormID secondary map. Empty on non-Oblivion games.
            ("magic_effects_by_code", |s| s.magic_effects_by_code.len()),
            ("actor_values", |s| s.actor_values.len()),
            ("activators", |s| s.activators.len()),
            ("terminals", |s| s.terminals.len()),
            ("form_lists", |s| s.form_lists.len()),
            // #808 / FNV-D2-NEW-01 stubs.
            ("projectiles", |s| s.projectiles.len()),
            ("effect_shaders", |s| s.effect_shaders.len()),
            ("item_mods", |s| s.item_mods.len()),
            ("armor_addons", |s| s.armor_addons.len()),
            ("outfits", |s| s.outfits.len()),
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
            // #1773 / FNV-D4-NEW-01 — TREE is dispatched into `index.trees`
            // (mod.rs `parse_tree`) but was the lone populated typed map
            // missing from this table, so it never counted toward `total()`
            // and a TREE category-wipe passed the parse-rate CI floor silently.
            // FNV ships 3; FO3/Oblivion ship many more (SpeedTree content).
            ("trees", |s| s.trees.len()),
            ("imagespace_modifiers", |s| s.imagespace_modifiers.len()),
            ("load_screens", |s| s.load_screens.len()),
            ("load_screen_types", |s| s.load_screen_types.len()),
            ("placeable_waters", |s| s.placeable_waters.len()),
            ("ragdolls", |s| s.ragdolls.len()),
            ("dehydration_stages", |s| s.dehydration_stages.len()),
            ("hunger_stages", |s| s.hunger_stages.len()),
            ("radiation_stages", |s| s.radiation_stages.len()),
            ("sleep_deprivation_stages", |s| {
                s.sleep_deprivation_stages.len()
            }),
            ("caravan_cards", |s| s.caravan_cards.len()),
            ("caravan_decks", |s| s.caravan_decks.len()),
            ("challenges", |s| s.challenges.len()),
            ("poker_chips", |s| s.poker_chips.len()),
            ("caravan_money", |s| s.caravan_money.len()),
            ("casinos", |s| s.casinos.len()),
            ("recipe_categories", |s| s.recipe_categories.len()),
            ("recipe_records", |s| s.recipe_records.len()),
            // #966 / OBL-D3-NEW-02 — Oblivion-unique base records.
            ("birthsigns", |s| s.birthsigns.len()),
            ("clothing", |s| s.clothing.len()),
            ("apparatuses", |s| s.apparatuses.len()),
            ("sigil_stones", |s| s.sigil_stones.len()),
            ("soul_gems", |s| s.soul_gems.len()),
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

    /// Resolve an actor value's global FormID by its `AVIF` EditorID
    /// (case-insensitive), or `None` when no such `AVIF` was parsed.
    ///
    /// Used to key `ActorValues` by the standard SPECIAL / skill actor
    /// values during NPC stat population (#1663) — the EditorIDs are the
    /// GECK names (`"Strength"`, `"Sneak"`, `"Guns"`, …). The returned
    /// FormID is in the index's load-order space, the same space a
    /// remapped CTDA `param_1` (and therefore `GetActorValue`) compares
    /// against. Linear over `actor_values` (~100 records) — cheap enough
    /// to call per-stat at spawn; cache the handful of standard ids on the
    /// caller if a hot path ever needs it.
    pub fn actor_value_form_id(&self, editor_id: &str) -> Option<u32> {
        self.actor_values
            .values()
            .find(|avif| avif.editor_id.eq_ignore_ascii_case(editor_id))
            .map(|avif| avif.form_id)
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
    /// activators, containers, NPCs/creatures — in the same priority
    /// order. Returns `None` when the record is absent or carries no
    /// `VMAD`. (Items / terminals don't decode `VMAD` yet, so they
    /// never match here.)
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
        None
    }

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
        // #969 / OBL-D3-NEW-05 — Oblivion DLC may redefine an MGEF;
        // last-write-wins matches `magic_effects` itself. The map is
        // empty on non-Oblivion plugins so extending is a no-op there.
        self.magic_effects_by_code
            .extend(other.magic_effects_by_code);
        self.actor_values.extend(other.actor_values);
        self.activators.extend(other.activators);
        self.terminals.extend(other.terminals);
        self.form_lists.extend(other.form_lists);
        self.trees.extend(other.trees);

        // #966 / OBL-D3-NEW-02 — Oblivion-unique base records.
        self.birthsigns.extend(other.birthsigns);
        self.clothing.extend(other.clothing);
        self.apparatuses.extend(other.apparatuses);
        self.sigil_stones.extend(other.sigil_stones);
        self.soul_gems.extend(other.soul_gems);

        // #1568 — skip telemetry accumulates across the plugin stack so a
        // master + DLC that both ship PDCL each surface their skip.
        self.skipped_unconsumed_groups
            .extend(other.skipped_unconsumed_groups);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::ActiRecord;

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
        );
        idx.npcs.insert(0x000A_0001, npc);

        let crea = parse_npc(
            0x000B_0002,
            &[
                sub(b"EDID", b"ScriptedCreature\0"),
                sub(b"SCRI", &0xBBBB_0002u32.to_le_bytes()),
            ],
            GameKind::Fallout3NV,
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

        let acti = parse_acti(0xAAAA_0003, &[sub(b"EDID", b"VmadActi\0"), sub(b"VMAD", &vmad)]);

        let mut idx = EsmIndex::default();
        idx.activators.insert(0xAAAA_0003, acti);

        let si = idx
            .base_record_script_instance(0xAAAA_0003)
            .expect("ACTI VMAD decoded into script_instance");
        assert_eq!(si.scripts.len(), 1);
        assert_eq!(si.scripts[0].name, "MyActivatorScript");

        // A record with no VMAD resolves to None (not an empty struct).
        let plain = parse_acti(0xAAAA_0004, &[sub(b"EDID", b"PlainActi\0")]);
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
        );
        idx.npcs.insert(0x0A_0001, npc);
        let si = idx
            .base_record_script_instance(0x0A_0001)
            .expect("NPC_ VMAD retained");
        assert_eq!(si.scripts[0].name, "QuestGiverScript");

        // A container with no VMAD still resolves to None on this path.
        let plain = parse_cont(0xC0_0002, &[sub(b"EDID", b"PlainChest\0")]);
        idx.containers.insert(0xC0_0002, plain);
        assert!(idx.base_record_script_instance(0xC0_0002).is_none());
    }
}
