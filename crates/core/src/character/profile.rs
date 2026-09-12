//! Data-driven per-game character-rule profiles.
//!
//! The ESM parser has a broad `GameKind` for binary-layout compatibility;
//! FO3 and New Vegas deliberately
//! share one such kind. Character rules are narrower and differ between those
//! games, so the parser translates its header once into this profile and all
//! downstream character consumers read the same table.

use super::{
    fallout3_ruleset, fallout4_ruleset, falloutnv_ruleset, skyrim_ruleset, CharacterRuleset,
    SkillSet,
};

/// A sourced linear END + level curve used to seed auto-calculated NPC Health.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NpcHealthCurve {
    pub bias: f32,
    pub endurance_multiplier: f32,
    pub level_multiplier: f32,
}

impl NpcHealthCurve {
    #[must_use]
    pub fn evaluate(self, endurance: f32, level: f32) -> f32 {
        self.bias + self.endurance_multiplier * endurance + self.level_multiplier * level
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
/// and the matching runtime ruleset builder as one coherent unit.
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
    ruleset: RulesetBuilder,
}

impl CharacterRulesProfile {
    pub const NONE: Self = Self {
        name: "unsupported",
        skills: SkillSet::NONE,
        npc_stats: NpcStatModel::None,
        creature_stats: NpcStatModel::None,
        ruleset: RulesetBuilder::None,
    };

    pub const OBLIVION: Self = Self {
        name: "Oblivion",
        skills: SkillSet::OBLIVION,
        npc_stats: NpcStatModel::None,
        creature_stats: NpcStatModel::None,
        ruleset: RulesetBuilder::None,
    };

    pub const FALLOUT3: Self = Self {
        name: "Fallout 3",
        skills: SkillSet::FALLOUT3,
        npc_stats: NpcStatModel::ClassAutoCalc {
            health: NpcHealthCurve {
                bias: 90.0,
                endurance_multiplier: 20.0,
                level_multiplier: 10.0,
            },
        },
        creature_stats: NpcStatModel::CreatureData,
        ruleset: RulesetBuilder::Fallout3,
    };

    pub const FALLOUT_NEW_VEGAS: Self = Self {
        name: "Fallout: New Vegas",
        skills: SkillSet::FALLOUT_NV,
        // 100 + 20·END + 5·(Level−1) = 95 + 20·END + 5·Level.
        npc_stats: NpcStatModel::ClassAutoCalc {
            health: NpcHealthCurve {
                bias: 95.0,
                endurance_multiplier: 20.0,
                level_multiplier: 5.0,
            },
        },
        creature_stats: NpcStatModel::CreatureData,
        ruleset: RulesetBuilder::FalloutNewVegas,
    };

    pub const SKYRIM: Self = Self {
        name: "Skyrim",
        skills: SkillSet::SKYRIM,
        npc_stats: NpcStatModel::RaceBaseOffsets,
        creature_stats: NpcStatModel::None,
        // #3170 / #3848 — the cheapest step that makes
        // `LevelingModel::with_gmst`'s one handled variant (`SkillXp`,
        // Skyrim's own) production-reachable at all: every other
        // `RulesetBuilder` arm carries `XpCurve` (Fallout) or falls through
        // `RulesetBuilder::None` (Oblivion), so without this arm `with_gmst`
        // executed only inside its own unit test.
        ruleset: RulesetBuilder::Skyrim,
    };

    pub const FALLOUT4: Self = Self {
        name: "Fallout 4",
        skills: SkillSet::NONE,
        npc_stats: NpcStatModel::Stored,
        creature_stats: NpcStatModel::None,
        ruleset: RulesetBuilder::Fallout4,
    };

    pub const FALLOUT76: Self = Self {
        name: "Fallout 76",
        skills: SkillSet::NONE,
        npc_stats: NpcStatModel::Stored,
        creature_stats: NpcStatModel::None,
        ruleset: RulesetBuilder::None,
    };

    pub const STARFIELD: Self = Self {
        name: "Starfield",
        skills: SkillSet::NONE,
        npc_stats: NpcStatModel::Stored,
        creature_stats: NpcStatModel::None,
        ruleset: RulesetBuilder::None,
    };

    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
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

    /// Build the canonical runtime ruleset with authored AVIF FormIDs.
    pub fn build_ruleset<F, G>(self, resolve: F, gmst: G) -> Option<CharacterRuleset>
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
        ruleset.leveling = ruleset.leveling.with_gmst(gmst);
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
        assert_eq!(fo3_health.evaluate(5.0, 2.0), 210.0);
        assert_eq!(fnv_health.evaluate(5.0, 2.0), 205.0);
        assert_eq!(
            CharacterRulesProfile::FALLOUT76.npc_stat_model(),
            NpcStatModel::Stored
        );
        assert_eq!(
            CharacterRulesProfile::STARFIELD.npc_stat_model(),
            NpcStatModel::Stored
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
            .build_ruleset(no_resolve, no_gmst)
            .expect("FO3 has a ruleset builder");
        let fnv = CharacterRulesProfile::FALLOUT_NEW_VEGAS
            .build_ruleset(no_resolve, no_gmst)
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
            .build_ruleset(no_resolve, gmst)
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
                .build_ruleset(|_| None, |_| None)
                .is_none(),
            "Oblivion is blocked on a pre-AVIF actor-value resolver (#3768); if it \
             now builds one, wire it deliberately rather than letting this pass",
        );
    }
}
