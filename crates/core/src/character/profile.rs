//! Data-driven per-game character-rule profiles.
//!
//! The ESM parser has a broad `GameKind` for binary-layout compatibility;
//! FO3 and New Vegas deliberately
//! share one such kind. Character rules are narrower and differ between those
//! games, so the parser translates its header once into this profile and all
//! downstream character consumers read the same table.

use super::{
    fallout3_ruleset, fallout4_ruleset, falloutnv_ruleset, skyrim_ruleset, CharacterRuleset, CrimeModel,
    SkillSet,
};

/// A sourced FO3/FNV NPC Health curve (#5238): the record's `DATA` Base
/// Health plus two GMST-driven terms,
/// `base_health + endurance_multiplier·(END + offset) + level_multiplier·(L + level_anchor)`.
///
/// All three coefficients are engine settings, not invented constants: the
/// masters author `fAVDNPCHealthEnduranceMult` = `fAVDNPCHealthLevelMult` =
/// 5.0 (FormIDs 0xAE66A / 0xAE66B, both Fallout3.esm and FalloutNV.esm,
/// measured with `crates/plugin/examples/health_gmst_dump.rs`), and the exe
/// default table carries `fAVDNPCHealthEnduranceOffset` = −1.0 ( Fallout3.exe
/// packed default-GMST table, 0xcf8a9c; neither master authors an override;
/// GECK's settings list corroborates the setting's existence). The GECK
/// *Stats Tab — NPC* note ("Base Health: Health is calculated with Endurance
/// and level. This value is then added to that result") supplies the base
/// term, which each record authors per-NPC.
///
/// #5340 — the level term's −1 is **not** `fAVDNPCHealthEnduranceOffset`:
/// a `strings` sweep of both exes finds no level-offset setting at all
/// (the only NPC Health settings are the three above), and the authored
/// player-side `fAVDHealthEnduranceOffset` is 0.0 while the player
/// formula still anchors the level term at `(Level − 1)` — so the anchor
/// is an engine-hardcoded constant, mirroring the player formula, and no
/// GMST overlays it. Binding it to the Endurance offset was a plausible
/// decomposition, not a citation: a mod retuning
/// `fAVDNPCHealthEnduranceOffset` would have moved every NPC's Health
/// twice. On vanilla data the split is value-identical: with the offset
/// at −1 and the anchor at −1 the curve composes to the −10 constant
/// every sample shows, and the fit `base + 5·(END−1) + 5·(L−1)`
/// reproduces 15/15 fandom infobox `hp` values across both masters
/// (Easy Pete 65, Sunny Smiles 90, Ringo 120, Boone 230, Benny 205,
/// Veronica 235, Cass 210, Raul 280; Three Dog 25, Gob 35, Nova
/// 40, Moira 80, Lucas Simms 60, Charon 240, Clover 240) — pinned by
/// `fo3_fnv_npc_health_curve_reproduces_the_vanilla_samples`. The previous
/// "curve" here was the *player* formula with "sourced" in its doc, which
/// every actual source contradicts (the capture's own Health row said
/// "player formulas (NPCs derive separately)").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NpcHealthCurve {
    pub endurance_multiplier: f32,
    pub level_multiplier: f32,
    pub offset: f32,
    /// The level term's anchor — engine-hardcoded `−1.0` (the player
    /// formula's `(Level − 1)`), deliberately outside [`Self::with_gmst`]:
    /// no level-offset GMST exists in either exe (#5340).
    pub level_anchor: f32,
}

impl NpcHealthCurve {
    /// The derived Health value: the record's own Base Health plus the two
    /// GMST-driven terms. `base_health` is the `NPC_` `DATA` field (#5005),
    /// clamped non-negative by the caller — a non-positive Base Health is
    /// the authored starts-dead convention, and the corpse rule owns it.
    #[must_use]
    pub fn evaluate(self, base_health: f32, endurance: f32, level: f32) -> f32 {
        base_health
            + self.endurance_multiplier * (endurance + self.offset)
            + self.level_multiplier * (level + self.level_anchor)
    }

    /// Overlay authored GMST values on the engine-supplied fallback curve.
    /// Missing settings retain the measured constants, keeping older masters
    /// and synthetic fixtures deterministic while allowing mods to retune
    /// the parsed curve — the same discipline as
    /// [`super::leveling::LevelingModel::with_gmst`]. Only the Endurance
    /// term's settings are overlaid; [`Self::level_anchor`] has no GMST to
    /// overlay (#5340).
    #[must_use]
    pub fn with_gmst(self, gmst: impl Fn(&str) -> Option<f32>) -> Self {
        Self {
            endurance_multiplier: gmst("fAVDNPCHealthEnduranceMult")
                .unwrap_or(self.endurance_multiplier),
            level_multiplier: gmst("fAVDNPCHealthLevelMult").unwrap_or(self.level_multiplier),
            offset: gmst("fAVDNPCHealthEnduranceOffset").unwrap_or(self.offset),
            level_anchor: self.level_anchor,
        }
    }
}

/// How an ESM-era NPC obtains its initial actor-value set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NpcStatModel {
    /// This profile has no wired NPC actor-value population path yet.
    None,
    /// FO3/FNV: class SPECIAL + governed skills + a sourced Health curve.
    ClassAutoCalc { health: NpcHealthCurve },
    /// Skyrim: race resource bases plus signed NPC offsets.
    RaceBaseOffsets,
    /// FO4+: stored `PRPS` actor values plus baked `DNAM` resources.
    Stored,
    /// FO3/FNV `CREA`: SPECIAL and Health authored in the record's own
    /// `DATA`, with no class and no auto-calc (#3390).
    ///
    /// A *creature* model, not a game model — it is reached through
    /// [`CharacterRulesProfile::creature_stat_model`], so consumers still
    /// branch on record kind rather than on game identity.
    CreatureData,
}

impl NpcStatModel {
    /// The [`NpcHealthCurve`] when this model class-auto-calculates
    /// (`ClassAutoCalc`), for consumers and tests that need the curve off
    /// the model without matching the variant again.
    #[must_use]
    pub const fn health_curve(self) -> NpcHealthCurve {
        match self {
            Self::ClassAutoCalc { health } => health,
            _ => NpcHealthCurve {
                endurance_multiplier: 0.0,
                level_multiplier: 0.0,
                offset: 0.0,
                level_anchor: 0.0,
            },
        }
    }
}

/// What carrying more than the `CarryWeight` capacity does (#5921).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverEncumberedRule {
    /// No over-encumbrance rule is wired for the game.
    None,
    /// Movement speed scales by `fMoveEncumEffect` (FO3 / FNV GECK).
    SpeedMult,
    /// The actor cannot sprint (Skyrim).
    NoSprint,
}

/// The perk entry-point numbering a game's `PERK` records author (#5921):
/// which entry-point index is which hook, and which function ids exist.
/// The value-math decode is wired for these two; every other game's
/// entries are not composed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PerkEntryPointDialect {
    #[default]
    None,
    /// FO3 / FNV (`wbDefinitionsFO3.pas`, FNV identical for these hooks).
    Fallout3NV,
    /// Skyrim (`wbDefinitionsTES5.pas`).
    Skyrim,
}

/// A game's barter price model (#5921): the executable defaults the
/// masters' GMSTs override, by setting name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarterModel {
    /// The AVIF editor id prices scale with.
    pub skill: &'static str,
    /// The skill value that reaches `barter_min`.
    pub skill_scale: f32,
    /// The skill value past which skill stops mattering.
    pub skill_cap: Option<f32>,
    /// `fBarterMax` / `fBarterMin` defaults.
    pub barter_max: f32,
    pub barter_min: f32,
    /// The setting naming the purchase floor, and its default.
    pub buy_floor_setting: &'static str,
    pub buy_floor: f32,
    /// `fBarterSellMax` default.
    pub sell_ceiling: f32,
    /// The (buy, sell) price perk entry points, where the game's perk
    /// entries are decoded.
    pub perk_hooks: Option<(u8, u8)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RulesetBuilder {
    /// No runtime ruleset. Oblivion is the only *wired* game that lands
    /// here, and deliberately: its actor values predate `AVIF`, so
    /// `oblivion_ruleset`'s `resolve` closure has nothing to resolve
    /// against until a legacy actor-value resolver exists (#3768). Read
    /// this arm as blocked, not as an oversight — the Skyrim arm below was
    /// missing for the opposite reason and that was a real gap (#3848).
    None,
    Fallout3,
    FalloutNewVegas,
    Fallout4,
    Skyrim,
}

/// One canonical character-policy row selected at the parser boundary.
///
/// This is deliberately data: consumers do not branch on game identity. A
/// profile owns the skill roster, NPC population model, Health coefficients,
/// the body-condition seeding base, and the matching runtime ruleset builder
/// as one coherent unit.
///
/// **Documented exception (#4448)** — the profile doubles as the codebase's
/// only FO3-vs-FNV discriminator (both share the broad
/// `GameKind::Fallout3NV`), and two non-character consumers rely on that
/// duty: `attach.rs::obscript_dialect_for` (script dialect) and
/// `consumables.rs`'s CTDA fn-586 whitelist. The data they branch on is not
/// character-ruleset data, so restructuring this type is a
/// scripting/consumables-visible change — the two consumers are pinned by
/// tests (`obscript_dialect_follows_the_profile_not_the_game_kind`,
/// `fn586_condition_gate_is_profile_scoped_not_game_scoped`) so such a
/// restructure fails loudly instead of drifting silently. Prefer adding a
/// policy row of the consumer's own before adding a third such consumer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterRulesProfile {
    name: &'static str,
    skills: SkillSet,
    npc_stats: NpcStatModel,
    /// How a `CREA`-family actor obtains its stats, when the game has such
    /// a record class at all. Split from [`Self::npc_stats`] because the
    /// two are genuinely different models within one game: an FO3/FNV
    /// `NPC_` auto-calculates from its CLAS, while a `CREA` reads its own
    /// `DATA` and references no class whatsoever (#3390).
    creature_stats: NpcStatModel,
    /// Base value the game's body-condition actor values are seeded at on
    /// every populated actor, when the game has such a value set at all.
    /// FO3/FNV seed the seven GECK limb-condition AVs at 100 ("GECK Stats
    /// List" — the AV names live in `consumables::BODY_CONDITION_VALUES`);
    /// every other family authors none, so the field is `None` there and
    /// the seeding no-ops. #4447 — this is profile data, not a
    /// consumer-side `game ==` branch.
    body_condition_base: Option<f32>,
    /// #4679 (CHAR-2026-09-21-D1-02) — the game's vital-pool roster as
    /// `(console label, AVIF editor id)` pairs, in draw order. Profile
    /// data, not a consumer-side `GameKind` match (same move #4447 made
    /// for `body_condition_base`): a future family divergence or new game
    /// is a profile-row edit. The editor ids resolve through
    /// `EsmIndex::actor_value_form_id` — a pool the game does not author
    /// (or a profile with no resolver yet, Oblivion pre-#3768) drops out
    /// per candidate rather than erroring.
    vital_pools: &'static [(&'static str, &'static str)],
    /// The radiation gauge as `(HUD label, AVIF editor id)`: an accumulating
    /// value drawn against the master's highest radiation sickness stage
    /// (`RADS` threshold), not a depleting pool. FO3 / FNV meter the
    /// player's `RadiationRads` (GECK *Actor Value Codes* 54; *GetRadiationLevel*:
    /// "the current radiation value of an actor use GetAV radiationrads").
    /// `None` on games with no rad meter.
    radiation_gauge: Option<(&'static str, &'static str)>,
    ruleset: RulesetBuilder,
    /// #5921 — what over-encumbrance does.
    over_encumbered: OverEncumberedRule,
    /// #5921 — whether killing an actor awards experience. The GECK
    /// documents FO3 / FNV kill rewards; Skyrim.esm authors the same
    /// `iXPLevelKill*` names but levels by skill use and awards none.
    kill_experience: bool,
    /// #5921 — the engine's `fEssentialDeathTime` default (seconds an
    /// essential actor stays down), read from each executable's setting
    /// table; `None` where the executable carries no such setting. That is
    /// not "the game has no bleedout recovery" (#6259): Starfield drops the
    /// setting but authors recovery data instead (RACE `DAT2` / `NPC_`
    /// `ONA2` bleedout times, `fEssentialHostileDeathTimeMult`,
    /// `OnRecoverFromBleedout`), and how those combine is unsourced — so a
    /// `None` game keeps its downed essentials down until scripted.
    essential_death_time_secs: Option<f32>,
    /// #5921 — the barter price model, where one is sourced.
    barter: Option<BarterModel>,
    /// #5921 — the perk entry-point numbering.
    perk_entry_points: PerkEntryPointDialect,
    /// #6227 — level-cap raises the executable applies, not a master: each
    /// `(master stem, levels)` adds its levels to the cap (after the
    /// `iMaxCharacterLevel` overlay) when a plugin of that stem is loaded.
    level_cap_raises: &'static [(&'static str, u16)],
    /// What a crime costs the criminal, where one is sourced and wired.
    crime: Option<CrimeModel>,
}

/// Skyrim's *Mod Buy Prices* / *Mod Sell Prices* entry points
/// (`wbEntryPointsEnum`, xEdit `wbDefinitionsTES5`).
const SKYRIM_MOD_BUY_PRICES: u8 = 8;
const SKYRIM_MOD_SELL_PRICES: u8 = 60;

impl CharacterRulesProfile {
    pub const NONE: Self = Self {
        name: "unsupported",
        skills: SkillSet::NONE,
        npc_stats: NpcStatModel::None,
        creature_stats: NpcStatModel::None,
        body_condition_base: None,
        vital_pools: &[],
        radiation_gauge: None,
        ruleset: RulesetBuilder::None,
        over_encumbered: OverEncumberedRule::None,
        kill_experience: false,
        essential_death_time_secs: None,
        barter: None,
        perk_entry_points: PerkEntryPointDialect::None,
        level_cap_raises: &[],
        crime: None,
    };

    pub const OBLIVION: Self = Self {
        name: "Oblivion",
        skills: SkillSet::OBLIVION,
        npc_stats: NpcStatModel::None,
        creature_stats: NpcStatModel::None,
        body_condition_base: None,
        // Oblivion authors no AVIF records, so every candidate drops out
        // at resolution until #3768's pre-AVIF resolver lands — the
        // roster states the intent, the resolver gates reality.
        vital_pools: &[("Health", "Health"), ("Magicka", "Magicka"), ("Fatigue", "Fatigue")],
        radiation_gauge: None,
        ruleset: RulesetBuilder::None,
        over_encumbered: OverEncumberedRule::None,
        kill_experience: false,
        // Oblivion.exe loads it as a constant beside the name.
        essential_death_time_secs: Some(10.0),
        barter: None,
        perk_entry_points: PerkEntryPointDialect::None,
        level_cap_raises: &[],
        crime: Some(CrimeModel::OBLIVION),
    };

    pub const FALLOUT3: Self = Self {
        name: "Fallout 3",
        skills: SkillSet::FALLOUT3,
        // #5238 — the authored NPC pair (fAVDNPCHealth{Endurance,Level}Mult =
        // 5.0/5.0) plus the exe-default fAVDNPCHealthEnduranceOffset = −1.0;
        // see NpcHealthCurve for the full sourcing. The old row here was the
        // player curve (bias 90 = 100 − the level mult), which no source
        // applies to NPCs.
        npc_stats: NpcStatModel::ClassAutoCalc {
            health: NpcHealthCurve {
                endurance_multiplier: 5.0,
                level_multiplier: 5.0,
                offset: -1.0,
                level_anchor: -1.0,
            },
        },
        creature_stats: NpcStatModel::CreatureData,
        body_condition_base: Some(100.0),
        vital_pools: &[("HP", "Health"), ("AP", "ActionPoints")],
        radiation_gauge: Some(("RAD", "RadiationRads")),
        ruleset: RulesetBuilder::Fallout3,
        over_encumbered: OverEncumberedRule::SpeedMult,
        kill_experience: true,
        // Fallout3.exe's setting table; the GECK says 10 s.
        essential_death_time_secs: Some(10.0),
        barter: None,
        perk_entry_points: PerkEntryPointDialect::Fallout3NV,
        level_cap_raises: &[],
        crime: None,
    };

    pub const FALLOUT_NEW_VEGAS: Self = Self {
        name: "Fallout: New Vegas",
        skills: SkillSet::FALLOUT_NV,
        // #5238 — same sourced NPC pair as FO3; the two masters author the
        // identical fAVDNPCHealth{Endurance,Level}Mult = 5.0/5.0 (their
        // *player* level mults differ: 10 vs 5).
        npc_stats: NpcStatModel::ClassAutoCalc {
            health: NpcHealthCurve {
                endurance_multiplier: 5.0,
                level_multiplier: 5.0,
                offset: -1.0,
                level_anchor: -1.0,
            },
        },
        creature_stats: NpcStatModel::CreatureData,
        body_condition_base: Some(100.0),
        vital_pools: &[("HP", "Health"), ("AP", "ActionPoints")],
        radiation_gauge: Some(("RAD", "RadiationRads")),
        ruleset: RulesetBuilder::FalloutNewVegas,
        over_encumbered: OverEncumberedRule::SpeedMult,
        kill_experience: true,
        // FalloutNV.exe is encrypted on disk; New Vegas runs the Fallout 3
        // engine and the GECK page documents both.
        essential_death_time_secs: Some(10.0),
        barter: None,
        perk_entry_points: PerkEntryPointDialect::Fallout3NV,
        // #6227 — each story add-on raises the cap by 5 (Fallout wiki *Level*:
        // "each of the four add-ons raises the level cap by 5 levels"; Dead
        // Money's / Honest Hearts' load MESGs: "Your level cap has been
        // raised by 5"). No master authors it: `iMaxCharacterLevel` is in
        // FalloutNV.esm alone. FalloutNV.exe carries exactly these four
        // stems as a string table beside its master-file loading strings.
        level_cap_raises: &[("DeadMoney", 5), ("HonestHearts", 5), ("OldWorldBlues", 5), ("LonesomeRoad", 5)],
        crime: None,
    };

    pub const SKYRIM: Self = Self {
        name: "Skyrim",
        skills: SkillSet::SKYRIM,
        // #4454 — `RaceBaseOffsets` implements 2 of the capture's 3 pool
        // composition terms (race base + ACBS offset); the "0–10/level
        // from class" term is uncaptured in detail and uncoded, so leveled
        // NPCs get flat pools until the CK per-class growth table is
        // sourced. See `derive_skyrim_actor_values`'s documented gap.
        npc_stats: NpcStatModel::RaceBaseOffsets,
        creature_stats: NpcStatModel::None,
        // #3170 / #3848 — the cheapest step that makes
        // `LevelingModel::with_gmst`'s one handled variant (`SkillXp`,
        // Skyrim's own) production-reachable at all: every other
        // `RulesetBuilder` arm carries `XpCurve` (Fallout) or falls through
        // `RulesetBuilder::None` (Oblivion), so without this arm `with_gmst`
        // executed only inside its own unit test.
        body_condition_base: None,
        vital_pools: &[("Health", "Health"), ("Magicka", "Magicka"), ("Stamina", "Stamina")],
        radiation_gauge: None,
        ruleset: RulesetBuilder::Skyrim,
        over_encumbered: OverEncumberedRule::NoSprint,
        kill_experience: false,
        // SkyrimSE.exe's setting table.
        essential_death_time_secs: Some(30.0),
        barter: Some(BarterModel {
            skill: "Speechcraft",
            skill_scale: 100.0,
            // "skill levels over 100 have no effect".
            skill_cap: Some(100.0),
            barter_max: 2.4,
            barter_min: 1.4,
            buy_floor_setting: "fBarterBuyMin",
            buy_floor: 1.05,
            sell_ceiling: 0.95,
            perk_hooks: Some((SKYRIM_MOD_BUY_PRICES, SKYRIM_MOD_SELL_PRICES)),
        }),
        perk_entry_points: PerkEntryPointDialect::Skyrim,
        level_cap_raises: &[],
        crime: None,
    };

    pub const FALLOUT4: Self = Self {
        name: "Fallout 4",
        skills: SkillSet::NONE,
        npc_stats: NpcStatModel::Stored,
        creature_stats: NpcStatModel::None,
        body_condition_base: None,
        vital_pools: &[("HP", "Health"), ("AP", "ActionPoints")],
        // FO4 / FO76 radiation lowers max HP instead of filling a meter.
        radiation_gauge: None,
        ruleset: RulesetBuilder::Fallout4,
        // No sourced FO4 over-encumbrance effect is wired (#5921 capture
        // note: intended, not an oversight).
        over_encumbered: OverEncumberedRule::None,
        kill_experience: false,
        // Fallout4.exe's setting table.
        essential_death_time_secs: Some(30.0),
        barter: Some(BarterModel {
            skill: "Charisma",
            skill_scale: 10.0,
            skill_cap: None,
            barter_max: 2.5,
            barter_min: 1.5,
            buy_floor_setting: "fBarterBuyMax",
            buy_floor: 1.05,
            sell_ceiling: 0.95,
            // FO4's PERK entry layout is not decoded.
            perk_hooks: None,
        }),
        perk_entry_points: PerkEntryPointDialect::None,
        level_cap_raises: &[],
        crime: None,
    };

    pub const FALLOUT76: Self = Self {
        name: "Fallout 76",
        skills: SkillSet::NONE,
        // #4453 — blocked, not forgotten: no capture line says FO76's
        // NPC_ records carry FO4's `Stored` PRPS/DNAM wire layout (FO76's
        // record formats restructure FO4's; the FO76 capture's NPC-stat
        // storage row reads "not researched"). Reading `Stored` here let
        // a FO76 master route NPCs through `derive_stored_actor_values`
        // on a presumption inherited from FO4's sourced row. `None` until
        // a source lands (xEdit wbDefinitionsFO76.pas NPC_ properties
        // layout).
        npc_stats: NpcStatModel::None,
        creature_stats: NpcStatModel::None,
        body_condition_base: None,
        vital_pools: &[("HP", "Health"), ("AP", "ActionPoints")],
        // FO4 / FO76 radiation lowers max HP instead of filling a meter.
        radiation_gauge: None,
        ruleset: RulesetBuilder::None,
        over_encumbered: OverEncumberedRule::None,
        kill_experience: false,
        // No executable capture for FO76.
        essential_death_time_secs: None,
        barter: None,
        perk_entry_points: PerkEntryPointDialect::None,
        level_cap_raises: &[],
        crime: None,
    };

    pub const STARFIELD: Self = Self {
        name: "Starfield",
        skills: SkillSet::NONE,
        // #4453 — same shape as FO76 above: the Starfield capture makes no
        // NPC-stat-storage claim at all, and Starfield's record formats
        // diverge from FO4's further than FO76's do. `None` until an
        // authoritative source for the NPC_ stat layout lands.
        npc_stats: NpcStatModel::None,
        creature_stats: NpcStatModel::None,
        body_condition_base: None,
        // #5044 — `Oxygen`, not `O2`: the AVIF EDID scan of `Starfield.esm`
        // (1,107 records, see `charal-starfield-ruleset.md`) gives
        // `Health` = 0x2D4 and `Oxygen` = 0x2D5 and no `O2` EDID at all —
        // the old row resolved nothing, so the O2 bar would have silently
        // dropped out of every candidate (`actor_value_form_id` drops
        // unknown editor ids rather than erroring).
        vital_pools: &[("HP", "Health"), ("O2", "Oxygen")],
        radiation_gauge: None,
        ruleset: RulesetBuilder::None,
        over_encumbered: OverEncumberedRule::None,
        kill_experience: false,
        // Starfield's executable carries no `fEssentialDeathTime`; its
        // recovery is data-driven (RACE `DAT2` bleedout Time Min/Max,
        // Recover Chance — captured on `RaceRecord::bleedout`, #6259) by a
        // rule no source states yet, so none is modelled.
        essential_death_time_secs: None,
        barter: None,
        perk_entry_points: PerkEntryPointDialect::None,
        level_cap_raises: &[],
        crime: None,
    };

    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// #4679 — the vital-pool roster: `(console label, AVIF editor id)`
    /// pairs in draw order. Empty for profiles with no pool model.
    #[must_use]
    pub const fn vital_pools(self) -> &'static [(&'static str, &'static str)] {
        self.vital_pools
    }

    /// See the field doc: `(HUD label, AVIF editor id)` of the rad meter.
    #[must_use]
    pub const fn radiation_gauge(self) -> Option<(&'static str, &'static str)> {
        self.radiation_gauge
    }

    #[must_use]
    pub const fn skills(self) -> SkillSet {
        self.skills
    }

    #[must_use]
    pub const fn npc_stat_model(self) -> NpcStatModel {
        self.npc_stats
    }

    /// How a `CREA`-family actor obtains its stats under this profile.
    ///
    /// [`NpcStatModel::None`] on every game that has no separate creature
    /// record class (Skyrim onward folded creatures into `NPC_`), and on
    /// Oblivion, whose `CREA` `DATA` is a different struct this has not
    /// been sourced for.
    #[must_use]
    pub const fn creature_stat_model(self) -> NpcStatModel {
        self.creature_stats
    }

    /// Base value body-condition actor values are seeded at on every
    /// populated actor (FO3/FNV: 100, per the GECK Stats List), or `None`
    /// on families that author no such value set — in which case the
    /// seeding no-ops entirely.
    #[must_use]
    pub const fn body_condition_base(self) -> Option<f32> {
        self.body_condition_base
    }

    /// #5921 — what over-encumbrance does under this profile.
    #[must_use]
    pub const fn over_encumbered(self) -> OverEncumberedRule {
        self.over_encumbered
    }

    /// #5921 — whether killing an actor awards experience.
    #[must_use]
    pub const fn awards_kill_experience(self) -> bool {
        self.kill_experience
    }

    /// #5921 — the engine's `fEssentialDeathTime` default, `None` where the
    /// game has no bleedout recovery.
    #[must_use]
    pub const fn essential_death_time_secs(self) -> Option<f32> {
        self.essential_death_time_secs
    }

    /// #5921 — the barter price model, `None` where none is sourced.
    #[must_use]
    pub const fn barter(self) -> Option<BarterModel> {
        self.barter
    }

    /// What a crime costs the criminal, `None` where no crime model is
    /// sourced and wired (every game but Oblivion).
    #[must_use]
    pub const fn crime(self) -> Option<CrimeModel> {
        self.crime
    }

    /// #5921 — the perk entry-point numbering the game's `PERK`s author.
    #[must_use]
    pub const fn perk_entry_points(self) -> PerkEntryPointDialect {
        self.perk_entry_points
    }

    /// #6227 — the levels the executable adds to the cap for the loaded
    /// plugins (file names, any case, with or without extension).
    #[must_use]
    pub fn level_cap_raise(self, plugin_files: &[String]) -> u16 {
        self.level_cap_raises
            .iter()
            .filter(|(stem, _)| {
                plugin_files.iter().any(|file| {
                    let file = file.rsplit(['/', '\\']).next().unwrap_or(file);
                    let loaded = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
                    loaded.eq_ignore_ascii_case(stem)
                })
            })
            .fold(0u16, |raise, (_, levels)| raise.saturating_add(*levels))
    }

    /// Build the canonical runtime ruleset with authored AVIF FormIDs, the
    /// authored GMSTs, and the loaded plugins' executable-applied cap raises.
    pub fn build_ruleset<F, G>(self, resolve: F, gmst: G, plugin_files: &[String]) -> Option<CharacterRuleset>
    where
        F: Fn(&str) -> Option<u32>,
        G: Fn(&str) -> Option<f32>,
    {
        let mut ruleset = match self.ruleset {
            RulesetBuilder::Fallout3 => fallout3_ruleset(resolve),
            RulesetBuilder::FalloutNewVegas => falloutnv_ruleset(resolve),
            RulesetBuilder::Fallout4 => fallout4_ruleset(resolve),
            RulesetBuilder::Skyrim => skyrim_ruleset(resolve),
            RulesetBuilder::None => return None,
        };
        ruleset.leveling = ruleset
            .leveling
            .with_gmst(gmst)
            .raise_cap(self.level_cap_raise(plugin_files));
        Some(ruleset)
    }
}

impl Default for CharacterRulesProfile {
    fn default() -> Self {
        Self::NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::LevelingModel;

    /// #5921 — the per-game rule rows that used to be consumer-side
    /// `GameKind` matches (encumbrance, kill XP, bleedout defaults, barter,
    /// perk entry points), pinned per profile. Each row is an exhaustive
    /// struct literal, so a new profile cannot compile without stating every
    /// one; this pin keeps the stated values from drifting.
    #[test]
    fn per_game_rule_rows_are_stated_on_every_profile() {
        use OverEncumberedRule as E;
        use PerkEntryPointDialect as P;
        let rows = [
            (CharacterRulesProfile::NONE, E::None, false, None, None, P::None),
            (CharacterRulesProfile::OBLIVION, E::None, false, Some(10.0), None, P::None),
            (CharacterRulesProfile::FALLOUT3, E::SpeedMult, true, Some(10.0), None, P::Fallout3NV),
            (
                CharacterRulesProfile::FALLOUT_NEW_VEGAS,
                E::SpeedMult,
                true,
                Some(10.0),
                None,
                P::Fallout3NV,
            ),
            (
                CharacterRulesProfile::SKYRIM,
                E::NoSprint,
                false,
                Some(30.0),
                Some("Speechcraft"),
                P::Skyrim,
            ),
            (CharacterRulesProfile::FALLOUT4, E::None, false, Some(30.0), Some("Charisma"), P::None),
            (CharacterRulesProfile::FALLOUT76, E::None, false, None, None, P::None),
            (CharacterRulesProfile::STARFIELD, E::None, false, None, None, P::None),
        ];
        for (profile, encumbered, kill_xp, death_time, barter_skill, perks) in rows {
            let name = profile.name();
            assert_eq!(profile.over_encumbered(), encumbered, "{name}");
            assert_eq!(profile.awards_kill_experience(), kill_xp, "{name}");
            assert_eq!(profile.essential_death_time_secs(), death_time, "{name}");
            assert_eq!(profile.barter().map(|b| b.skill), barter_skill, "{name}");
            assert_eq!(profile.perk_entry_points(), perks, "{name}");
        }
        // The price perk hooks exist only where the perk entries decode.
        let skyrim = CharacterRulesProfile::SKYRIM.barter().unwrap();
        assert_eq!(skyrim.perk_hooks, Some((8, 60)));
        assert_eq!(CharacterRulesProfile::FALLOUT4.barter().unwrap().perk_hooks, None);
    }

    #[test]
    fn fallout_profiles_keep_roster_health_and_ruleset_in_lockstep() {
        let fo3 = CharacterRulesProfile::FALLOUT3;
        let fnv = CharacterRulesProfile::FALLOUT_NEW_VEGAS;
        assert_eq!(fo3.skills(), SkillSet::FALLOUT3);
        assert_eq!(fnv.skills(), SkillSet::FALLOUT_NV);

        let NpcStatModel::ClassAutoCalc { health: fo3_health } = fo3.npc_stat_model() else {
            panic!("FO3 must class-auto-calculate NPC stats");
        };
        let NpcStatModel::ClassAutoCalc { health: fnv_health } = fnv.npc_stat_model() else {
            panic!("FNV must class-auto-calculate NPC stats");
        };
        assert_eq!(
            fo3_health, fnv_health,
            "both masters author the identical fAVDNPCHealth{{Endurance,Level}}Mult pair"
        );
        // #4453 — FO4 is the only sourced `Stored` profile; FO76 and
        // Starfield are `None` until their NPC_ stat wire layouts are
        // captured (pinned separately by
        // `fo76_and_starfield_claim_no_npc_stat_model_until_captured`).
        assert_eq!(
            CharacterRulesProfile::FALLOUT4.npc_stat_model(),
            NpcStatModel::Stored
        );
    }

    /// #5238 — the sourced curve reproduces the vanilla NPC Health table:
    /// `base + 5·(END−1) + 5·(L−1)` against the issue's 15 fandom infobox
    /// `hp` samples, five per representative slice (base/END/level from the
    /// records, measured with the probe).
    #[test]
    fn fo3_fnv_npc_health_curve_reproduces_the_vanilla_samples() {
        let curve = CharacterRulesProfile::FALLOUT_NEW_VEGAS
            .npc_stat_model()
            .health_curve();
        // (base, END, level, wiki hp) — FNV: Easy Pete, Sunny Smiles, Ringo,
        // Boone, Benny, Veronica, Cass, Raul (levels 1–14 span the curve's
        // reach). #5342 — the full 15 the doc claims: base/END are the
        // records' own DATA bytes (dump_record_subs over FalloutNV.esm);
        // levels are the fandom infobox values the hp column comes from.
        for &(base, end, level, hp) in &[
            (50.0, 4.0, 1.0, 65.0),
            (70.0, 4.0, 2.0, 90.0),
            (75.0, 6.0, 5.0, 120.0),
            (190.0, 5.0, 5.0, 230.0),
            (150.0, 5.0, 8.0, 205.0),
            (190.0, 6.0, 5.0, 235.0),
            (175.0, 4.0, 5.0, 210.0),
            (200.0, 4.0, 14.0, 280.0),
        ] {
            assert_eq!(
                curve.evaluate(base, end, level),
                hp,
                "FNV sample (base {base}, END {end}, L {level})"
            );
        }
        // FO3: Three Dog (base 10), Gob, Nova, Moira, Lucas Simms (base 10,
        // END 6, L 6), Charon, Clover — same coefficients, FO3's differing
        // *player* level mult does not touch the NPC curve.
        let fo3 = CharacterRulesProfile::FALLOUT3.npc_stat_model().health_curve();
        for &(base, end, level, hp) in &[
            (10.0, 4.0, 1.0, 25.0),
            (10.0, 4.0, 3.0, 35.0),
            (10.0, 4.0, 4.0, 40.0),
            (50.0, 4.0, 4.0, 80.0),
            (10.0, 6.0, 6.0, 60.0),
            (200.0, 5.0, 5.0, 240.0),
            (225.0, 4.0, 1.0, 240.0),
        ] {
            assert_eq!(
                fo3.evaluate(base, end, level),
                hp,
                "FO3 sample (base {base}, END {end}, L {level})"
            );
        }
    }

    /// #5238 — the GMST overlay retunes all three coefficients by editor id
    /// and leaves authored values in place when a setting is missing (the
    /// LevelingModel::with_gmst discipline; both masters author the mults,
    /// neither authors the offset).
    #[test]
    fn npc_health_curve_overlays_authored_gmsts() {
        let curve = CharacterRulesProfile::FALLOUT3
            .npc_stat_model()
            .health_curve();
        let gmst = |name: &str| match name {
            "fAVDNPCHealthEnduranceMult" => Some(6.0),
            "fAVDNPCHealthEnduranceOffset" => Some(-2.0),
            _ => None,
        };
        let tuned = curve.with_gmst(gmst);
        assert_eq!(tuned.endurance_multiplier, 6.0);
        assert_eq!(tuned.offset, -2.0);
        assert_eq!(
            tuned.level_multiplier, curve.level_multiplier,
            "a missing setting must retain the sourced constant"
        );
        // #5340 — retuning the Endurance offset moves ONLY the Endurance
        // term. Pre-split it dragged the level term by the same Δ, so a mod
        // overlaying fAVDNPCHealthEnduranceOffset shifted Health by
        // level_multiplier·Δ twice instead of once.
        let (endurance, level) = (5.0_f32, 10.0_f32);
        assert_eq!(
            tuned.evaluate(0.0, endurance, level) - curve.evaluate(0.0, endurance, level),
            tuned.endurance_multiplier * (endurance + tuned.offset)
                - curve.endurance_multiplier * (endurance + curve.offset),
            "the level term must be identical before and after the overlay"
        );
        // The vanilla authored pair resolves to the same curve the profile
        // carries, so a real master's overlay is a no-op.
        let from_master = curve.with_gmst(|name| match name {
            "fAVDNPCHealthEnduranceMult" | "fAVDNPCHealthLevelMult" => Some(5.0),
            _ => None,
        });
        assert_eq!(from_master, curve);
    }

    /// #5340 — no GMST can overlay the level term's anchor: a `strings`
    /// sweep of both exes finds no level-offset setting at all, so
    /// `with_gmst` must leave `level_anchor` at its engine-hardcoded −1.0
    /// (the player formula's `(Level − 1)`) no matter what it answers.
    #[test]
    fn the_level_anchor_is_engine_hardcoded_and_never_overlaid() {
        let curve = CharacterRulesProfile::FALLOUT_NEW_VEGAS
            .npc_stat_model()
            .health_curve();
        assert_eq!(curve.level_anchor, -1.0);
        let hostile = curve.with_gmst(|name| match name {
            "fAVDNPCHealthEnduranceOffset" => Some(9.0),
            "fAVDNPCHealthLevelMult" => Some(9.0),
            _ => None,
        });
        assert_eq!(
            hostile.level_anchor, -1.0,
            "no GMST name the overlay answers can move the level anchor"
        );
        // And the anchor is its own term: with the offset driven to +9 the
        // level half still reads 9·(L−1).
        assert_eq!(
            hostile.evaluate(0.0, 5.0, 6.0),
            5.0 * (5.0 + 9.0) + 9.0 * (6.0 - 1.0)
        );
    }

    /// #2941 — `esm_header_selects_one_canonical_character_profile`
    /// (`crates/plugin/src/esm/records/tests.rs`) already pins that a real
    /// FO3 HEDR resolves to `FALLOUT3`, not `FALLOUT_NEW_VEGAS`; this pins
    /// the other half — that the profile's `build_ruleset` actually routes
    /// to `fallout3_ruleset` (not `falloutnv_ruleset`) and so carries FO3's
    /// own `LevelingModel`, not FNV's. `fallout3_ruleset`/`LevelingModel::FO3`
    /// had no production call site before the profile-centralization
    /// refactor that fixed this (`b434e4c0`); this is the regression test
    /// the finding asked for so a future edit can't quietly re-collapse the
    /// two onto one leveling model without a test noticing.
    #[test]
    fn fo3_and_fnv_profiles_build_their_own_distinct_leveling_model() {
        let no_resolve = |_: &str| None;
        let no_gmst = |_: &str| None;

        let fo3 = CharacterRulesProfile::FALLOUT3
            .build_ruleset(no_resolve, no_gmst, &[])
            .expect("FO3 has a ruleset builder");
        let fnv = CharacterRulesProfile::FALLOUT_NEW_VEGAS
            .build_ruleset(no_resolve, no_gmst, &[])
            .expect("FNV has a ruleset builder");

        assert_eq!(
            fo3.leveling,
            LevelingModel::FO3,
            "FO3 must build fallout3_ruleset's LevelingModel::FO3, not FNV's"
        );
        assert_eq!(fnv.leveling, LevelingModel::FNV);
        assert_ne!(
            fo3.leveling, fnv.leveling,
            "the two capture-documented leveling models (level_cap 20/1.0/1 \
             vs 30/0.5/2) must not collapse onto one"
        );

        // The specific divergent constants the capture document locks:
        // level cap (20 vs 30) and perk cadence (every level vs every other).
        assert_eq!(fo3.leveling.level_cap(), 20);
        assert_eq!(fnv.leveling.level_cap(), 30);
        assert!(fo3.leveling.grants_perk_at(3), "FO3: every level");
        assert!(!fnv.leveling.grants_perk_at(3), "FNV: every other level");
        assert!(fnv.leveling.grants_perk_at(2));
    }

    /// #3170 / #3848 — before this fix, `CharacterRulesProfile::SKYRIM`
    /// carried `RulesetBuilder::None`, so `build_ruleset` returned `None`
    /// before ever reaching `LevelingModel::with_gmst`.
    /// `LevelingModel::SkillXp` (Skyrim's own variant, and the only one
    /// `with_gmst` overlays) was therefore never constructed on a real
    /// wired game — `with_gmst` executed only inside `leveling.rs`'s own
    /// isolated unit test.
    ///
    /// This pins actual production reach: `SKYRIM.build_ruleset` must both
    /// succeed AND actually invoke the `gmst` closure for the curve
    /// settings, not merely return a ruleset that happens to carry
    /// `LevelingModel::SkillXp`. Asserting the variant alone would pass on
    /// a build that skipped the GMST overlay entirely, which is the half
    /// #2942's seam exists for.
    #[test]
    fn skyrim_profile_builds_a_ruleset_and_actually_calls_gmst() {
        let no_resolve = |_: &str| None;
        let requested = std::cell::RefCell::new(Vec::new());
        let gmst = |name: &str| {
            requested.borrow_mut().push(name.to_owned());
            None
        };

        let skyrim = CharacterRulesProfile::SKYRIM
            .build_ruleset(no_resolve, gmst, &[])
            .expect("Skyrim must have a ruleset builder now that #3848 wires one");

        assert!(
            matches!(skyrim.leveling, LevelingModel::SkillXp { .. }),
            "Skyrim's ruleset must carry its own SkillXp leveling model"
        );
        let requested = requested.into_inner();
        assert!(
            requested.contains(&"fXPLevelUpBase".to_string()),
            "gmst must actually be invoked for the level-up base curve setting, \
             got {requested:?}"
        );
        assert!(
            requested.contains(&"fXPLevelUpMult".to_string()),
            "gmst must actually be invoked for the level-up mult curve setting, \
             got {requested:?}"
        );
    }

    /// #6227 — New Vegas's story add-ons each raise the cap by 5 on top of
    /// the authored `iMaxCharacterLevel`, keyed on the loaded file's stem in
    /// any case; the gear packs and other games raise nothing.
    #[test]
    fn new_vegas_add_ons_raise_the_level_cap_and_nothing_else_does() {
        let cap = |profile: CharacterRulesProfile, files: &[&str]| {
            let files: Vec<String> = files.iter().map(|file| (*file).to_owned()).collect();
            let gmst = |name: &str| (name == "iMaxCharacterLevel").then_some(30.0);
            profile.build_ruleset(|_| None, gmst, &files).unwrap().leveling.level_cap()
        };
        let fnv = CharacterRulesProfile::FALLOUT_NEW_VEGAS;
        assert_eq!(cap(fnv, &["falloutnv.esm"]), 30);
        assert_eq!(cap(fnv, &["falloutnv.esm", "deadmoney.esm", "GunRunnersArsenal.esm"]), 35);
        let all = ["FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm", "LonesomeRoad.esm"];
        assert_eq!(cap(fnv, &all), 50);
        assert_eq!(cap(CharacterRulesProfile::FALLOUT3, &all), 30, "FO3's cap is the GMST alone");
    }

    /// #4447 — body-condition seeding is profile data, not a consumer-side
    /// game-kind compare. FO3/FNV seed the seven GECK limb-condition AVs at
    /// 100; every other family authors none. Before this, the gate lived as
    /// a raw `GameKind::Fallout3NV` branch in `derive_npc_actor_values`,
    /// invisible to this table and unexercisable by the plugin crate's unit
    /// fixtures.
    #[test]
    fn body_condition_seeding_is_owned_by_the_fo3_fnv_profiles() {
        assert_eq!(
            CharacterRulesProfile::FALLOUT3.body_condition_base(),
            Some(100.0)
        );
        assert_eq!(
            CharacterRulesProfile::FALLOUT_NEW_VEGAS.body_condition_base(),
            Some(100.0)
        );
        for profile in [
            CharacterRulesProfile::NONE,
            CharacterRulesProfile::OBLIVION,
            CharacterRulesProfile::SKYRIM,
            CharacterRulesProfile::FALLOUT4,
            CharacterRulesProfile::FALLOUT76,
            CharacterRulesProfile::STARFIELD,
        ] {
            assert_eq!(
                profile.body_condition_base(),
                None,
                "{} authors no body-condition value set",
                profile.name()
            );
        }
    }

    /// #3848 — Oblivion's `RulesetBuilder::None` is a blocked arm, not a
    /// forgotten one, and the difference is worth pinning: the Skyrim arm
    /// was missing for five weeks precisely because a reader could not tell
    /// the two cases apart from the enum's shape. If Oblivion ever gains a
    /// legacy actor-value resolver (#3768), this test is the thing that
    /// should fail and be deleted.
    #[test]
    fn oblivion_still_has_no_runtime_ruleset_and_that_is_deliberate() {
        assert!(
            CharacterRulesProfile::OBLIVION
                .build_ruleset(|_| None, |_| None, &[])
                .is_none(),
            "Oblivion is blocked on a pre-AVIF actor-value resolver (#3768); if it \
             now builds one, wire it deliberately rather than letting this pass",
        );
    }

    /// #4453 — FO76 and Starfield claim no NPC stat model until a capture
    /// sources their `NPC_` stat-storage wire layout. `Stored` here was a
    /// presumption inherited from FO4's sourced row: it silently routed
    /// FO76/Starfield masters through the FO4 `PRPS`/`DNAM` decoder with
    /// no capture line saying their records carry that layout.
    #[test]
    fn fo76_and_starfield_claim_no_npc_stat_model_until_captured() {
        for profile in [CharacterRulesProfile::FALLOUT76, CharacterRulesProfile::STARFIELD] {
            assert_eq!(
                profile.npc_stat_model(),
                NpcStatModel::None,
                "{} NPC stat storage is uncaptured (#4453); wire it from a source, \
                 not by inheriting FO4's Stored row",
                profile.name()
            );
        }
    }

    /// #5044 — Starfield's oxygen pool must name the AVIF editor id the
    /// game actually authors. The `Starfield.esm` AVIF EDID scan (1,107
    /// records, `charal-starfield-ruleset.md`) gives `Oxygen` = 0x2D5 and
    /// no `O2` EDID, so the old `("O2", "O2")` row resolved nothing and
    /// `actor_value_form_id` would have silently dropped the bar.
    #[test]
    fn starfield_oxygen_pool_names_the_real_avif_editor_id() {
        assert_eq!(
            CharacterRulesProfile::STARFIELD.vital_pools(),
            &[("HP", "Health"), ("O2", "Oxygen")],
            "Starfield vital pools must resolve against the AVIF scan \
             (Health = 0x2D4, Oxygen = 0x2D5, no O2 EDID)"
        );
    }
}
