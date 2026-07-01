//! Skyrim (TES V) [`CharacterRuleset`] builder (CHARAL).
//!
//! Skyrim broke from the classic Elder Scrolls shape modelled in
//! [`super::tes`]: it dropped attributes entirely, so there are **no
//! attribute-derived pools**. The three primary stats — Health, Magicka,
//! Stamina — each start at [`SKYRIM_POOL_BASE`] (100) and grow only by the
//! player's per-level pick (`+10` to one, carried by
//! [`LevelingModel::SKYRIM`](super::leveling::LevelingModel)); they are stored
//! character state, not a function of any attribute. Leveling is the skill-XP
//! model ([`LevelingModel::SkillXp`](super::leveling::LevelingModel)): raising
//! a skill feeds character XP, and each level grants a perk plus the pool pick.
//!
//! Sourced: Elder Scrolls Wiki *Health/Magicka/Stamina (Skyrim)* (base 100,
//! +10/level) and UESP *Skyrim:Leveling* (the XP formulae). The ruleset
//! therefore carries an **empty derived table** and the empty
//! [`AttributeSet::SKYRIM`] roster — the 18 ungoverned skills
//! ([`SkillSet::SKYRIM`]) and the leveling model are the substance.

use super::attribute::AttributeSet;
use super::leveling::LevelingModel;
use super::ruleset::CharacterRuleset;
use super::skill::SkillSet;

/// Base value of each Skyrim primary pool (Health / Magicka / Stamina) at
/// character creation, before any per-level `+10` picks. Elder Scrolls Wiki
/// *Health/Magicka/Stamina (Skyrim)*.
pub const SKYRIM_POOL_BASE: f32 = 100.0;

/// Skyrim (TES V) [`CharacterRuleset`] builder.
///
/// Assembles the empty attribute roster ([`AttributeSet::SKYRIM`]), the 18
/// ungoverned skills ([`SkillSet::SKYRIM`]), and the skill-XP leveling model
/// ([`LevelingModel::SKYRIM`]). There are no attribute-derived pools, so the
/// derived table stays empty — Health/Magicka/Stamina start at
/// [`SKYRIM_POOL_BASE`] and advance via the level-up pick. The `resolve`
/// parameter is accepted for signature parity with the other family builders
/// (and future use, e.g. resolving skill AVIF ids for population); nothing is
/// resolved today.
#[must_use]
pub fn skyrim_ruleset<F: Fn(&str) -> Option<u32>>(_resolve: F) -> CharacterRuleset {
    CharacterRuleset::new(LevelingModel::SKYRIM)
        .with_attributes(AttributeSet::SKYRIM)
        .with_skills(SkillSet::SKYRIM)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::LevelingModel as LM;

    #[test]
    fn skyrim_ruleset_has_no_attributes_and_eighteen_skills() {
        let rs = skyrim_ruleset(|_| None);
        assert!(rs.attributes.is_empty(), "Skyrim has no attributes");
        assert_eq!(rs.skills.len(), 18);
        assert_eq!(rs.leveling, LM::SKYRIM);
        assert_eq!(rs.derived_len(), 0, "no attribute-derived pools in Skyrim");
    }

    #[test]
    fn pool_base_and_per_level_pick() {
        assert_eq!(SKYRIM_POOL_BASE, 100.0);
        // A pool maxed via picks: base 100 + 10 per level chosen into it.
        let after_5_picks = SKYRIM_POOL_BASE + 5.0 * LM::SKYRIM.pool_pick_gain().unwrap();
        assert_eq!(after_5_picks, 150.0);
    }
}
