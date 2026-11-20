//! Oblivion's armor rating and Resist Normal Weapons: how much of a weapon hit the target's worn armor and its resistance
//! stop (UESP *Oblivion:The Complete Damage Formula*, "Weapon Damage"; the Oblivion ruleset's "Armor Rating" section).
//! The Skyrim sibling, a different model, is [`ArmorModel`](super::ArmorModel).
//!
//! **Provenance.** Documented: a worn piece is worth `BaseArmorRating * (0.35 + 0.0065 * ArmorSkill) * ArmorHealth /
//! MaxArmorHealth`; the pieces add up and the sum is capped at `fMaxArmorRating` (85); the hit is scaled by `(100 -
//! CombinedArmorRating) / 100`, which is 1 for an attacker who is "successfully sneaking and has the Master Sneak perk";
//! and by `(100 - NormalWeaponResistance) / 100`, which is 1 for "Silver/Daedric/Enchanted weapons". Not modelled, and
//! said so in `bow.status`: the armor's condition (the engine tracks none: `ArmorHealth / MaxArmorHealth` reads 1) and
//! "boosting effects like Shield". The `ARMO` rating is stored x100 (the iron cuirass reads 1000 for a displayed 10).
//!
//! Pure data and functions: no ECS, no game branches. The per-game seam is the
//! [`CharacterRulesProfile`](super::CharacterRulesProfile) `armor_rating` row (`Some` on Oblivion).
//!
//! ```text
//! piece     = base * (0.35 + 0.0065 * skill)                     skill = Light or Heavy Armor by the piece
//! combined  = min(sum of pieces, fMaxArmorRating)
//! hit      *= (100 - combined) / 100                              (1 for a Master Sneak attacker)
//! hit      *= (100 - Resist Normal Weapons) / 100                 (1 for a silver, daedric or enchanted weapon)
//! ```

use crate::ecs::resource::Resource;

/// The armor-rating rules (installed as a resource where the profile's `armor_rating` row is `Some`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArmorRatingModel {
    /// A piece's rating at skill 0, as a fraction of its base (0.35).
    pub skill_base: f32,
    /// What each point of the wearer's armor skill adds (0.0065).
    pub skill_per_level: f32,
    /// `fMaxArmorRating`: the cap on the combined rating.
    pub max_rating: f32,
    /// The Sneak skill that carries the Master perk (100: UESP *Oblivion:Sneak*, Master).
    pub master_sneak_skill: f32,
    /// The `ARMO` rating is stored this many times the displayed one.
    pub stored_scale: f32,
}

impl Resource for ArmorRatingModel {}

impl ArmorRatingModel {
    /// Oblivion's, as documented.
    pub const OBLIVION: Self = Self { skill_base: 0.35, skill_per_level: 0.0065, max_rating: 85.0, master_sneak_skill: 100.0, stored_scale: 100.0 };

    /// The model with each setting the load order authors replacing its default (`fMaxArmorRating`). A non-finite value is
    /// ignored.
    #[must_use]
    pub fn with_gmst(self, gmst: impl Fn(&str) -> Option<f32>) -> Self {
        Self { max_rating: gmst("fMaxArmorRating").filter(|value| value.is_finite()).unwrap_or(self.max_rating), ..self }
    }

    /// What one worn piece is worth to a wearer with `armor_skill` in its kind (Light or Heavy Armor): the `ARMO`'s stored
    /// rating, descaled, times the skill term. The piece's condition is taken as full.
    #[must_use]
    pub fn piece_rating(&self, stored_rating: u32, armor_skill: f32) -> f32 {
        stored_rating as f32 / self.stored_scale * (self.skill_base + self.skill_per_level * armor_skill.max(0.0))
    }

    /// The share of a hit that gets through `combined` armor rating (the sum of the pieces): `(100 - min(combined, cap)) /
    /// 100`. A `master_sneak` attacker (a sneak attack with the Master Sneak perk) ignores armor: 1.
    #[must_use]
    pub fn armor_factor(&self, combined: f32, master_sneak: bool) -> f32 {
        if master_sneak {
            return 1.0;
        }
        (100.0 - combined.clamp(0.0, self.max_rating)) / 100.0
    }

    /// Whether a sneak attack from an attacker with `sneak_skill` carries the Master Sneak perk.
    #[must_use]
    pub fn is_master_sneak(&self, sneak_skill: f32) -> bool {
        sneak_skill >= self.master_sneak_skill
    }
}

/// The share of a weapon hit that gets through a target's `resist_normal_weapons` (a percentage): `(100 - resist) / 100`,
/// 1 for a weapon that ignores it, never below 0 (a resistance over 100 stops the hit, it does not heal).
#[must_use]
pub fn weapon_resistance_factor(resist_normal_weapons: f32, ignores_resistance: bool) -> f32 {
    if ignores_resistance {
        return 1.0;
    }
    ((100.0 - resist_normal_weapons) / 100.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The iron cuirass (stored 1000, a displayed 10) on a wearer at Heavy Armor 50: `10 * (0.35 + 0.325) = 6.75`; the
    /// combined rating caps at 85; and the share through is `(100 - rating) / 100`.
    #[test]
    fn the_documented_rating() {
        let model = ArmorRatingModel::OBLIVION;
        assert!((model.piece_rating(1000, 50.0) - 6.75).abs() < 1e-4);
        assert!((model.piece_rating(1000, 0.0) - 3.5).abs() < 1e-4, "skill 0: 35% of the base");
        assert!((model.armor_factor(6.75, false) - 0.9325).abs() < 1e-5);
        assert!((model.armor_factor(200.0, false) - 0.15).abs() < 1e-6, "capped at 85");
        assert_eq!(model.armor_factor(80.0, true), 1.0, "a Master Sneak attack ignores armor");
        assert!(model.is_master_sneak(100.0) && !model.is_master_sneak(99.0));
        assert!((model.with_gmst(|name| (name == "fMaxArmorRating").then_some(50.0)).armor_factor(90.0, false) - 0.5).abs() < 1e-6);
    }

    /// UESP's worked case for Block: 85% armor leaves 15%; and Resist Normal Weapons scales by `(100 - R) / 100` unless
    /// the weapon ignores it.
    #[test]
    fn the_documented_resistance() {
        assert_eq!(weapon_resistance_factor(0.0, false), 1.0);
        assert!((weapon_resistance_factor(50.0, false) - 0.5).abs() < 1e-6);
        assert_eq!(weapon_resistance_factor(100.0, false), 0.0, "a ghost");
        assert_eq!(weapon_resistance_factor(150.0, false), 0.0, "never heals");
        assert_eq!(weapon_resistance_factor(100.0, true), 1.0, "a silver, daedric or enchanted weapon");
        assert_eq!(weapon_resistance_factor(-20.0, false), 1.0, "a weakness is not modelled by this term");
    }
}
