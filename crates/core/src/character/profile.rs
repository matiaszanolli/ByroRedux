//! Data-driven per-game character-rule profiles.
//!
//! The ESM parser has a broad `GameKind` for binary-layout compatibility;
//! FO3 and New Vegas deliberately
//! share one such kind. Character rules are narrower and differ between those
//! games, so the parser translates its header once into this profile and all
//! downstream character consumers read the same table.

use super::{fallout3_ruleset, fallout4_ruleset, falloutnv_ruleset, CharacterRuleset, SkillSet};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RulesetBuilder {
    None,
    Fallout3,
    FalloutNewVegas,
    Fallout4,
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
    ruleset: RulesetBuilder,
}

impl CharacterRulesProfile {
    pub const NONE: Self = Self {
        name: "unsupported",
        skills: SkillSet::NONE,
        npc_stats: NpcStatModel::None,
        ruleset: RulesetBuilder::None,
    };

    pub const OBLIVION: Self = Self {
        name: "Oblivion",
        skills: SkillSet::OBLIVION,
        npc_stats: NpcStatModel::None,
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
        ruleset: RulesetBuilder::FalloutNewVegas,
    };

    pub const SKYRIM: Self = Self {
        name: "Skyrim",
        skills: SkillSet::SKYRIM,
        npc_stats: NpcStatModel::RaceBaseOffsets,
        ruleset: RulesetBuilder::None,
    };

    pub const FALLOUT4: Self = Self {
        name: "Fallout 4",
        skills: SkillSet::NONE,
        npc_stats: NpcStatModel::Stored,
        ruleset: RulesetBuilder::Fallout4,
    };

    pub const FALLOUT76: Self = Self {
        name: "Fallout 76",
        skills: SkillSet::NONE,
        npc_stats: NpcStatModel::Stored,
        ruleset: RulesetBuilder::None,
    };

    pub const STARFIELD: Self = Self {
        name: "Starfield",
        skills: SkillSet::NONE,
        npc_stats: NpcStatModel::Stored,
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
}
