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

/// Default `fSkillUseCurve` — the global exponent in the Skyrim skill-XP
/// cost curve (UESP *Skyrim:Leveling*). Per-skill `SkillImproveMult` /
/// `SkillImproveOffset` are AUTHORED (each skill's AVIF record).
pub const SKYRIM_SKILL_USE_CURVE: f32 = 1.95;

/// Skyrim skill-XP required to advance a skill **from `current_level` to the
/// next** — the skill-internal half of the [`LevelingModel::SkillXp`] model
/// (§5 procedural leveling strategy).
///
/// `cost = improve_mult · current_level^use_curve + improve_offset`
///
/// `improve_mult` / `improve_offset` are the skill's AUTHORED AVIF values
/// (e.g. Lockpicking 0.25 / 300); `use_curve` is [`SKYRIM_SKILL_USE_CURVE`].
/// Source: UESP *Skyrim:Leveling* (Lockpicking 15→16 = `0.25·15^1.95 + 300`
/// ≈ 349.13). Distinct from [`LevelingModel::xp_from_skill_rank`], which is
/// the *character* XP a rank-up feeds into the level bar.
#[must_use]
pub fn skyrim_skill_xp_to_next(
    current_level: u16,
    improve_mult: f32,
    improve_offset: f32,
    use_curve: f32,
) -> f32 {
    improve_mult * f32::from(current_level).powf(use_curve) + improve_offset
}

/// Cumulative Skyrim skill-XP to raise a skill **from `from_level` to
/// `to_level`** — the sum of the per-step [`skyrim_skill_xp_to_next`] costs
/// for each intermediate level. `0.0` if `to_level <= from_level`.
#[must_use]
pub fn skyrim_skill_xp_between(
    from_level: u16,
    to_level: u16,
    improve_mult: f32,
    improve_offset: f32,
    use_curve: f32,
) -> f32 {
    (from_level..to_level)
        .map(|l| skyrim_skill_xp_to_next(l, improve_mult, improve_offset, use_curve))
        .sum()
}

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

    #[test]
    fn skill_xp_cost_matches_uesp_lockpicking() {
        // Lockpicking (mult 0.25, offset 300), curve 1.95.
        let c = SKYRIM_SKILL_USE_CURVE;
        // 15 → 16: 0.25·15^1.95 + 300 ≈ 349.13.
        let step = skyrim_skill_xp_to_next(15, 0.25, 300.0, c);
        assert!((step - 349.13).abs() < 0.5, "15→16 was {step}");
        // Cumulative 15 → 20 ≈ 1815.5.
        let cum = skyrim_skill_xp_between(15, 20, 0.25, 300.0, c);
        assert!((cum - 1815.54).abs() < 2.0, "15→20 was {cum}");
        // Empty / inverted range is zero.
        assert_eq!(skyrim_skill_xp_between(20, 15, 0.25, 300.0, c), 0.0);
        assert_eq!(skyrim_skill_xp_between(15, 15, 0.25, 300.0, c), 0.0);
    }

    #[test]
    fn skill_xp_between_is_sum_of_steps() {
        let c = SKYRIM_SKILL_USE_CURVE;
        let (m, o) = (0.5, 200.0);
        let manual: f32 = (15..18)
            .map(|l| skyrim_skill_xp_to_next(l, m, o, c))
            .sum();
        assert_eq!(skyrim_skill_xp_between(15, 18, m, o, c), manual);
    }
}
