//! How much of a physical hit a blocking defender stops. Oblivion's rules (#7054) come from UESP
//! *Oblivion:Block*, "Blocked Melee Damage" and "Notes").
//!
//! **Provenance.** Every number is documented: a shield blocks 1% of the damage per level of Block, capped at 75%; a
//! blade, blunt weapon or bow in hand blocks 0.5% per level; bare hands block 0.25% per level, "but only against unarmed
//! opponents. NPC's equipped with weapons will always deal full damage"; "skill levels over 100 will not increase the
//! blocked damage any further"; and the reduction is "the same for melee and ranged attacks". Not modelled, and said so
//! in `combat.status`: the Novice's Fatigue drain, the weapon's or shield's wear, the recoil, knockback, stagger and
//! disarm of the higher ranks, and whether a block needs the defender to face the attacker (the page does not say).
//!
//! Pure data and functions: no ECS, no game branches. The per-game seam is the
//! [`CharacterRulesProfile`](super::CharacterRulesProfile) `block` row.
//!
//! ```text
//! shield  reduction = min(0.75, 0.01   * min(Block, 100))
//! weapon  reduction =           0.005  * min(Block, 100)
//! hands   reduction =           0.0025 * min(Block, 100)      (an unarmed attacker only; 0 against a weapon)
//! ```

use crate::ecs::resource::Resource;

/// What the defender blocks with, in the order the original prefers: a worn shield, else a wielded weapon, else bare hands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockGuard {
    Shield,
    Weapon,
    Hands,
}

/// The block rule selected by the character profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockModel {
    Oblivion,
    Skyrim(SkyrimBlockModel),
}

impl Resource for BlockModel {}

impl BlockModel {
    pub const OBLIVION: Self = Self::Oblivion;
    pub const SKYRIM: Self = Self::Skyrim(SkyrimBlockModel::DEFAULT);
    /// Oblivion's shield slot in BMDT (UESP *Oblivion Mod:Mod File Format/ARMO*).
    pub const OBLIVION_SHIELD_BIT: u8 = 13;

    pub const fn legacy_shield_bit(self) -> Option<u8> {
        match self {
            Self::Oblivion => Some(Self::OBLIVION_SHIELD_BIT),
            Self::Skyrim(_) => None,
        }
    }

    /// Apply the load order's Skyrim settings. Oblivion uses its fixed rule below.
    pub fn with_gmst(self, get: impl Fn(&str) -> Option<f32>) -> Self {
        match self {
            Self::Oblivion => self,
            Self::Skyrim(default) => {
                let value = |name, fallback| get(name).filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(fallback);
                Self::Skyrim(SkyrimBlockModel {
                    shield_base: value("fShieldBaseFactor", default.shield_base),
                    shield_scale: value("fShieldScalingFactor", default.shield_scale),
                    weapon_base: value("fBlockWeaponBase", default.weapon_base),
                    weapon_scale: value("fBlockWeaponScaling", default.weapon_scale),
                    power_multiplier: value("fBlockPowerAttackMult", default.power_multiplier),
                    cap: (value("fPlayerMaxResistance", default.cap * 100.0) / 100.0).min(1.0),
                    half_angle_degrees: value("fCombatHitConeAngle", default.half_angle_degrees).min(180.0),
                })
            }
        }
    }
}

/// Oblivion credits every successful block, including one of size zero. Bare hands only stop unarmed hits.
#[must_use]
pub fn oblivion_block_reduction(skill: f32, guard: BlockGuard, attacker_armed: bool) -> Option<f32> {
    let levels = skill.clamp(0.0, 100.0);
    match guard {
        BlockGuard::Shield => Some((0.01 * levels).min(0.75)),
        BlockGuard::Weapon => Some(0.005 * levels),
        BlockGuard::Hands if attacker_armed => None,
        BlockGuard::Hands => Some(0.0025 * levels),
    }
}

/// Skyrim's ordinary shield, weapon and torch blocks before perk and magic modifiers.
/// Authored GMSTs supply the base and scaling terms. In-game measurements give the skill
/// contribution of 1.5 at skill 100 and the vanilla 85% cap shared with resistance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyrimBlockModel {
    shield_base: f32,
    shield_scale: f32,
    weapon_base: f32,
    weapon_scale: f32,
    power_multiplier: f32,
    cap: f32,
    /// Maximum horizontal angle off the defender's forward axis.
    pub half_angle_degrees: f32,
}

impl SkyrimBlockModel {
    const DEFAULT: Self = Self {
        shield_base: 0.45,
        shield_scale: 0.2,
        weapon_base: 0.30,
        weapon_scale: 0.2,
        power_multiplier: 0.66,
        cap: 0.85,
        half_angle_degrees: 35.0,
    };

    /// `base_rating` is the shield's base ARMO rating or the attacker's base WEAP damage, by guard.
    #[must_use]
    pub fn reduction(self, skill: f32, guard: BlockGuard, base_rating: f32, power_attack: bool) -> Option<f32> {
        let (base, scale) = match guard {
            BlockGuard::Shield => (self.shield_base, self.shield_scale),
            BlockGuard::Weapon => (self.weapon_base, self.weapon_scale),
            BlockGuard::Hands => return None,
        };
        let skill_factor = 1.0 + 0.015 * skill.max(0.0);
        let power = if power_attack { self.power_multiplier } else { 1.0 };
        let stopped = (base + scale * 0.01 * base_rating.max(0.0) * skill_factor) * power;
        Some(stopped.min(self.cap))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(got: Option<f32>, want: Option<f32>) {
        match (got, want) {
            (Some(got), Some(want)) => assert!((got - want).abs() < 1e-5, "{got} vs {want}"),
            (got, want) => assert_eq!(got, want),
        }
    }

    /// UESP's worked cases: a Master (100) with a shield blocks 75%, with a weapon 50%; a shield caps at skill 75.
    #[test]
    fn the_documented_reductions() {
        near(oblivion_block_reduction(100.0, BlockGuard::Shield, true), Some(0.75));
        near(oblivion_block_reduction(100.0, BlockGuard::Weapon, true), Some(0.5));
        near(oblivion_block_reduction(40.0, BlockGuard::Shield, true), Some(0.4));
        near(oblivion_block_reduction(75.0, BlockGuard::Shield, true), Some(0.75));
        near(oblivion_block_reduction(90.0, BlockGuard::Shield, true), Some(0.75)); // capped
        near(oblivion_block_reduction(40.0, BlockGuard::Weapon, true), Some(0.2));
    }

    /// Skill above 100 adds nothing; hands block 0.25% a level of an unarmed attacker and nothing of an armed one.
    #[test]
    fn hands_block_only_the_unarmed_and_skill_stops_at_100() {
        near(oblivion_block_reduction(150.0, BlockGuard::Weapon, true), Some(0.5));
        near(oblivion_block_reduction(40.0, BlockGuard::Hands, false), Some(0.1));
        assert_eq!(oblivion_block_reduction(40.0, BlockGuard::Hands, true), None, "an armed attacker always deals full damage");
        near(oblivion_block_reduction(-5.0, BlockGuard::Shield, true), Some(0.0));
    }

    #[test]
    fn skyrim_blocks_scale_with_base_ratings_and_apply_the_cap_after_power_attacks() {
        let model = SkyrimBlockModel::DEFAULT;
        near(model.reduction(0.0, BlockGuard::Shield, 15.0, false), Some(0.48));
        near(model.reduction(100.0, BlockGuard::Shield, 15.0, false), Some(0.525));
        near(model.reduction(100.0, BlockGuard::Weapon, 17.0, false), Some(0.385));
        near(model.reduction(100.0, BlockGuard::Weapon, 0.0, false), Some(0.30));
        near(model.reduction(100.0, BlockGuard::Hands, 17.0, false), None);
        near(model.reduction(100.0, BlockGuard::Shield, 100.0, false), Some(0.85));
        near(model.reduction(100.0, BlockGuard::Shield, 100.0, true), Some(0.627));
    }

    #[test]
    fn skyrim_reads_the_resistance_cap_and_block_tuning_by_editor_id() {
        let BlockModel::Skyrim(model) = BlockModel::SKYRIM.with_gmst(|name| match name {
            "fBlockWeaponBase" => Some(0.50),
            "fBlockPowerAttackMult" => Some(0.5),
            "fPlayerMaxResistance" => Some(40.0),
            "fCombatHitConeAngle" => Some(60.0),
            _ => None,
        }) else { unreachable!() };
        near(model.reduction(100.0, BlockGuard::Weapon, 17.0, false), Some(0.4));
        near(model.reduction(100.0, BlockGuard::Weapon, 0.0, true), Some(0.25));
        assert_eq!(model.half_angle_degrees, 60.0);
    }
}
