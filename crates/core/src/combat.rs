//! Classic Oblivion combat-damage math (CHARAL-adjacent, not CHARAL itself).
//!
//! Sibling of [`crate::stealth`], not a submodule of [`crate::character`]:
//! these formulas read SPECIAL/skill actor values as *inputs* but are
//! evaluated at combat-resolution time against transient per-hit state
//! (weapon condition, base weapon damage) that never lives in `ActorValues`
//! — the same "AV in, mechanic-formula out" boundary already drawn for
//! Sneak Detection (`crate::stealth`) and documented for Lockpicking/
//! Acrobatics in `docs/engine/charal-oblivion-ruleset.md`. No combat/attack-
//! resolution consumer system exists yet in the engine; this module is the
//! reusable, tested piece built ahead of that consumer, mirroring how
//! `stealth::detection_score` and `character::resistance` were built ahead
//! of their consumers.
//!
//! Source: UESP *Oblivion:The Complete Damage Formula*, *Oblivion:Blade*,
//! *Oblivion:Blunt* (`docs/engine/charal-oblivion-ruleset.md`). Scope is
//! **classic Oblivion (2006, Gamebryo)** — Oblivion Remastered's diverging
//! damage/regen math (noted throughout that doc) is out of scope, matching
//! the precedent already set in `character::tes::oblivion_health_gain_per_level`.

/// Oblivion's combat-time **Luck-chained effective skill**:
/// `Skill + 0.4 × (Luck − 50)`. Used at combat/haggling resolution, distinct
/// from Luck's role in a stat's base value — confirmed reused verbatim
/// across weapon damage, Hand-to-Hand damage, and Mercantile haggling
/// (`docs/engine/charal-oblivion-ruleset.md`), so it's a general-purpose
/// quantity, not a combat-specific one-off.
#[must_use]
pub fn modified_skill(skill: f32, luck: f32) -> f32 {
    skill + 0.4 * (luck - 50.0)
}

/// The shared Blade/Blunt/Marksman weapon-damage multiplier:
/// `0.5 × (0.75 + 0.005·Attribute) × (0.2 + 0.015·ModifiedSkill)`.
///
/// `Attribute` is Strength for Blade/Blunt, Agility for Marksman;
/// `weapon_skill` is the governing weapon-type skill (Blade/Blunt/Marksman).
/// Both `Attribute` and the Luck-chained [`modified_skill`] are clamped to
/// `[0, 100]` before use — UESP states this explicitly ("Attribute and
/// ModifiedSkill are constrained between 0 and 100"). The caller multiplies
/// this by `BaseWeaponDamage` and the weapon-condition ratio (equipment-layer
/// inputs this module deliberately does not own — see the module docs).
#[must_use]
pub fn oblivion_weapon_damage_multiplier(attribute: f32, weapon_skill: f32, luck: f32) -> f32 {
    let attribute = attribute.clamp(0.0, 100.0);
    let skill = modified_skill(weapon_skill, luck).clamp(0.0, 100.0);
    0.5 * (0.75 + 0.005 * attribute) * (0.2 + 0.015 * skill)
}

/// Hand-to-Hand damage per hit: `Health = 1 + 10.5 × (Strength/100) ×
/// (ModifiedSkill/100)`, `Fatigue = 1 + 0.5 × Health`.
///
/// A **pure cross-term** formula (no separate additive Strength or Skill
/// term, unlike the weapon-damage multiplier above) — confirmed structurally
/// divergent from Blade/Blunt/Marksman, per UESP *The Complete Damage
/// Formula*. `Fatigue` chains a second hop off `Health`. Neither term is
/// clamped here — UESP's `[0,100]` clamp note appears only in the Weapon
/// Damage section, not repeated for Hand-to-Hand, so this function doesn't
/// assume it applies (no-guessing).
#[must_use]
pub fn oblivion_hand_to_hand_damage(strength: f32, hand_to_hand_skill: f32, luck: f32) -> (f32, f32) {
    let skill = modified_skill(hand_to_hand_skill, luck);
    let health = 1.0 + 10.5 * (strength / 100.0) * (skill / 100.0);
    let fatigue = 1.0 + 0.5 * health;
    (health, fatigue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modified_skill_is_skill_at_luck_fifty() {
        // Luck 50 is the formula's neutral point — no adjustment either way.
        assert_eq!(modified_skill(40.0, 50.0), 40.0);
        // Luck 100 → +20; Luck 0 → −20.
        assert_eq!(modified_skill(40.0, 100.0), 60.0);
        assert_eq!(modified_skill(40.0, 0.0), 20.0);
    }

    #[test]
    fn weapon_damage_multiplier_matches_hand_derivation() {
        // STR 0, Skill 0, Luck 50 → 0.5×0.75×0.2 = 0.075 (the bias term
        // recorded in charal-oblivion-ruleset.md's bilinear expansion).
        assert!((oblivion_weapon_damage_multiplier(0.0, 0.0, 50.0) - 0.075).abs() < 1e-6);
        // STR 100, Skill 100, Luck 50 → 0.5×1.25×1.7 = 1.0625.
        assert!((oblivion_weapon_damage_multiplier(100.0, 100.0, 50.0) - 1.0625).abs() < 1e-6);
    }

    #[test]
    fn weapon_damage_multiplier_luck_chains_into_skill() {
        // Skill 50 with Luck 100 (ModifiedSkill 70) must out-damage the same
        // Skill 50 at Luck 50 (ModifiedSkill 50) — Luck strictly helps here.
        let low_luck = oblivion_weapon_damage_multiplier(50.0, 50.0, 50.0);
        let high_luck = oblivion_weapon_damage_multiplier(50.0, 50.0, 100.0);
        assert!(high_luck > low_luck);
    }

    #[test]
    fn weapon_damage_multiplier_clamps_inputs_past_100() {
        // Attribute/ModifiedSkill are stated to be constrained to [0,100];
        // pushing either past 100 must not out-scale the capped value.
        let capped = oblivion_weapon_damage_multiplier(100.0, 100.0, 100.0);
        let overdriven = oblivion_weapon_damage_multiplier(200.0, 200.0, 100.0);
        assert_eq!(capped, overdriven);
    }

    #[test]
    fn hand_to_hand_damage_matches_formula() {
        // STR 0, Skill 0 → Health 1, Fatigue 1.5.
        let (health, fatigue) = oblivion_hand_to_hand_damage(0.0, 0.0, 50.0);
        assert_eq!(health, 1.0);
        assert_eq!(fatigue, 1.5);
        // STR 100, Skill 100, Luck 50 → Health 1+10.5=11.5, Fatigue 1+5.75=6.75.
        let (health, fatigue) = oblivion_hand_to_hand_damage(100.0, 100.0, 50.0);
        assert!((health - 11.5).abs() < 1e-6);
        assert!((fatigue - 6.75).abs() < 1e-6);
    }

    #[test]
    fn hand_to_hand_fatigue_always_chains_off_health() {
        let (health, fatigue) = oblivion_hand_to_hand_damage(73.0, 61.0, 80.0);
        assert!((fatigue - (1.0 + 0.5 * health)).abs() < 1e-6);
    }
}
