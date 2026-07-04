//! Skyrim (TES V) [`CharacterRuleset`] builder (CHARAL).
//!
//! Skyrim broke from the classic Elder Scrolls shape modelled in
//! [`super::tes`]: it dropped attributes entirely, so there are **no
//! attribute-derived** pools. The three primary stats — Health, Magicka,
//! Stamina — each start at [`SKYRIM_POOL_BASE`] (100) and grow only by the
//! player's per-level pick (`+10` to one, carried by
//! [`LevelingModel::SKYRIM`](super::leveling::LevelingModel)); they are stored
//! character state, not a function of any attribute. Leveling is the skill-XP
//! model ([`LevelingModel::SkillXp`](super::leveling::LevelingModel)): raising
//! a skill feeds character XP, and each level grants a perk plus the pool pick.
//!
//! Sourced: Elder Scrolls Wiki *Health/Magicka/Stamina (Skyrim)* (base 100,
//! +10/level) and UESP *Skyrim:Leveling* (the XP formulae). The ruleset
//! carries the empty [`AttributeSet::SKYRIM`] roster, the 18 ungoverned skills
//! ([`SkillSet::SKYRIM`]), and the leveling model — plus one **skill-derived**
//! entry in the derived table (below), the first non-attribute-derived stat
//! CHARAL has populated for any game: Skyrim has no attributes to derive off
//! at all, but `DerivedStatFormula`'s `DerivedInput` already accepts any AVIF,
//! including a skill.

use super::attribute::AttributeSet;
use super::derived::{DerivedInput, DerivedStatFormula};
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

/// Light Armor's per-skill-point Armor Rating bonus for the **player**
/// (`1 + 0.004·LightArmor`). Source: UESP *Skyrim:Light Armor*
/// (`charal-skyrim-ruleset.md`). NPCs use a distinct, higher constant
/// (`0.015`) not modelled here — see [`skyrim_ruleset`].
pub const LIGHT_ARMOR_RATING_COEFF: f32 = 0.004;

/// Carry Weight's per-point-of-base-Stamina coefficient (`250 +
/// 0.5·BaseStamina`). Source: UESP *Skyrim:Carry Weight*
/// (`charal-skyrim-ruleset.md`) — every Stamina level-up pick adds `+10`
/// Stamina and `+5` Carry Weight together (`300 + 5n` vs. `100 + 10n`),
/// which reduces to this coefficient.
pub const CARRY_WEIGHT_STAMINA_COEFF: f32 = 0.5;
/// Carry Weight's bias term — see [`CARRY_WEIGHT_STAMINA_COEFF`].
pub const CARRY_WEIGHT_BIAS: f32 = 250.0;

/// Skyrim (TES V) [`CharacterRuleset`] builder.
///
/// Assembles the empty attribute roster ([`AttributeSet::SKYRIM`]), the 18
/// ungoverned skills ([`SkillSet::SKYRIM`]), and the skill-XP leveling model
/// ([`LevelingModel::SKYRIM`]). There are no attribute-derived pools — Health/
/// Magicka start at [`SKYRIM_POOL_BASE`] and advance via the level-up pick —
/// but the derived table has two entries:
/// - **Light Armor's Armor Rating multiplier** (`1 + 0.004·LightArmor`,
///   [`LIGHT_ARMOR_RATING_COEFF`]) is skill-derived rather than
///   attribute-derived, sourced from UESP *Skyrim:Light Armor*. It's
///   [`player_only`](DerivedStatFormula::player_only) because the source
///   gives a distinct, higher NPC constant (`0.015`) this builder doesn't
///   model — same "NPCs derive differently" reasoning `fallout.rs` already
///   applies to Health/Action Points.
/// - **Carry Weight** (`250 + 0.5·BaseStamina`, [`CARRY_WEIGHT_BIAS`] /
///   [`CARRY_WEIGHT_STAMINA_COEFF`]) derives off Stamina's **base** layer
///   only ([`DerivedStatFormula::a_from_base`]) — the source states
///   temporary Stamina changes (Fortify Stamina) do **not** affect Carry
///   Weight, unlike FO4 Health, which is confirmed to rescale dynamically
///   off *current* Endurance. Actor-general (NPCs derive Carry Weight the
///   same way, unlike Health/Action Points).
///
/// `resolve` maps an EditorID to its AVIF FormID (`EsmIndex::actor_value_form_id`
/// once wired to the loader); a stat whose EditorID doesn't resolve is
/// skipped, same resolve-or-skip contract as the Fallout builders.
#[must_use]
pub fn skyrim_ruleset<F: Fn(&str) -> Option<u32>>(resolve: F) -> CharacterRuleset {
    let mut rs = CharacterRuleset::new(LevelingModel::SKYRIM)
        .with_attributes(AttributeSet::SKYRIM)
        .with_skills(SkillSet::SKYRIM);
    // Armor Rating ×(1 + 0.004·LightArmor), player-only (NPCs use 0.015).
    if let (Some(out), Some(la)) = (resolve("DamageResist"), resolve("LightArmor")) {
        rs.push_derived(
            out,
            DerivedStatFormula::affine(
                DerivedInput::actor_value(la),
                LIGHT_ARMOR_RATING_COEFF,
                1.0,
            )
            .as_multiplier()
            .player_only(),
        );
    }
    // Carry Weight = 250 + 0.5·BaseStamina (base only — excludes Fortify Stamina).
    if let (Some(out), Some(stamina)) = (resolve("CarryWeight"), resolve("Stamina")) {
        rs.push_derived(
            out,
            DerivedStatFormula::affine(
                DerivedInput::actor_value(stamina),
                CARRY_WEIGHT_STAMINA_COEFF,
                CARRY_WEIGHT_BIAS,
            )
            .a_from_base(),
        );
    }
    rs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::LevelingModel as LM;

    #[test]
    fn skyrim_ruleset_has_no_attributes_and_eighteen_skills() {
        // A resolver that never resolves: no AVIF ids, so the Light Armor
        // derived entry is skipped too (resolve-or-skip contract).
        let rs = skyrim_ruleset(|_| None);
        assert!(rs.attributes.is_empty(), "Skyrim has no attributes");
        assert_eq!(rs.skills.len(), 18);
        assert_eq!(rs.leveling, LM::SKYRIM);
        assert_eq!(rs.derived_len(), 0, "unresolved AVIFs mean nothing populates");
    }

    fn full(id: &str) -> Option<u32> {
        Some(match id {
            "DamageResist" => 0x100,
            "LightArmor" => 0x101,
            "CarryWeight" => 0x102,
            "Stamina" => 0x103,
            _ => return None,
        })
    }

    #[test]
    fn light_armor_rating_bonus_matches_uesp() {
        use crate::character::DerivedScope;
        use crate::ecs::components::ActorValues;

        let rs = skyrim_ruleset(full);
        assert_eq!(rs.derived_len(), 2, "Light Armor + Carry Weight");
        // ×(1 + 0.004·LightArmor). Skill 50 → 1.20×.
        let avs = ActorValues::from_pairs([(0x101, 50.0)]);
        let mult = rs.derived_value(0x100, &avs, 1).unwrap();
        assert!((mult - 1.20).abs() < 1e-6, "got {mult}");
        assert_eq!(
            rs.derived_formula(0x100).unwrap().scope,
            DerivedScope::PlayerOnly,
            "NPCs use a different constant (0.015), not modelled here"
        );
    }

    #[test]
    fn light_armor_rating_bonus_skipped_when_unresolved() {
        let partial = |id: &str| match id {
            "LightArmor" => None,
            other => full(other),
        };
        let rs = skyrim_ruleset(partial);
        assert_eq!(rs.derived_len(), 1, "only Carry Weight resolves");
    }

    #[test]
    fn carry_weight_matches_uesp_and_ignores_temporary_stamina() {
        use crate::ecs::components::ActorValues;

        let rs = skyrim_ruleset(full);
        // 250 + 0.5·BaseStamina. 0 picks (Stamina 100) → 300; 1 pick (110) → 305.
        let base = ActorValues::from_pairs([(0x103, 100.0)]);
        assert_eq!(rs.derived_value(0x102, &base, 1), Some(300.0));
        let one_pick = ActorValues::from_pairs([(0x103, 110.0)]);
        assert_eq!(rs.derived_value(0x102, &one_pick, 1), Some(305.0));

        // A Fortify Stamina potion (temporary_mod) must not move Carry Weight.
        let mut fortified = ActorValues::from_pairs([(0x103, 100.0)]);
        fortified.mod_temporary(0x103, 50.0);
        assert_eq!(
            rs.derived_value(0x102, &fortified, 1),
            Some(300.0),
            "temporary Stamina must not affect Carry Weight"
        );
    }

    #[test]
    fn carry_weight_skipped_when_unresolved() {
        let partial = |id: &str| match id {
            "Stamina" => None,
            other => full(other),
        };
        let rs = skyrim_ruleset(partial);
        assert_eq!(rs.derived_len(), 1, "only Light Armor resolves");
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
