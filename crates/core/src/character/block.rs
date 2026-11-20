//! Oblivion's Block: how much of a hit a blocking defender stops (#7054, Oblivion skill-use leveling; sourced in UESP
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
//! [`CharacterRulesProfile`](super::CharacterRulesProfile) `block` row (`Some` on Oblivion).
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

/// The block rules (installed as a resource where the profile's `block` row is `Some`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockModel {
    /// Block skill above this adds nothing (UESP: "skill levels over 100 will not increase the blocked damage").
    pub skill_cap: f32,
    pub shield_per_level: f32,
    /// A shield never blocks more than this (UESP: "capped at skill level 75 with 75%").
    pub shield_cap: f32,
    pub weapon_per_level: f32,
    pub hands_per_level: f32,
    /// The biped-slot bit of a shield on the game's `BMDT` (Oblivion: 13, UESP *Oblivion Mod:Mod File Format/ARMO*).
    pub shield_bit: u8,
}

impl Resource for BlockModel {}

impl BlockModel {
    /// Oblivion's, as UESP documents it.
    pub const OBLIVION: Self = Self {
        skill_cap: 100.0,
        shield_per_level: 0.01,
        shield_cap: 0.75,
        weapon_per_level: 0.005,
        hands_per_level: 0.0025,
        shield_bit: 13,
    };

    /// The fraction of a hit a defender with Block `skill` stops using `guard`, or `None` where the block has no effect:
    /// bare hands against an armed attacker (UESP: "NPC's equipped with weapons will always deal full damage"). A
    /// `Some` is a successful block, whatever its size (UESP *Oblivion:Block*: "each time you successfully block an
    /// attack ... regardless of the amount of damage blocked").
    #[must_use]
    pub fn reduction(&self, skill: f32, guard: BlockGuard, attacker_armed: bool) -> Option<f32> {
        let levels = skill.clamp(0.0, self.skill_cap);
        match guard {
            BlockGuard::Shield => Some((self.shield_per_level * levels).min(self.shield_cap)),
            BlockGuard::Weapon => Some(self.weapon_per_level * levels),
            BlockGuard::Hands if attacker_armed => None,
            BlockGuard::Hands => Some(self.hands_per_level * levels),
        }
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
        let model = BlockModel::OBLIVION;
        near(model.reduction(100.0, BlockGuard::Shield, true), Some(0.75));
        near(model.reduction(100.0, BlockGuard::Weapon, true), Some(0.5));
        near(model.reduction(40.0, BlockGuard::Shield, true), Some(0.4));
        near(model.reduction(75.0, BlockGuard::Shield, true), Some(0.75));
        near(model.reduction(90.0, BlockGuard::Shield, true), Some(0.75)); // capped
        near(model.reduction(40.0, BlockGuard::Weapon, true), Some(0.2));
    }

    /// Skill above 100 adds nothing; hands block 0.25% a level of an unarmed attacker and nothing of an armed one.
    #[test]
    fn hands_block_only_the_unarmed_and_skill_stops_at_100() {
        let model = BlockModel::OBLIVION;
        assert_eq!(model.reduction(150.0, BlockGuard::Weapon, true), model.reduction(100.0, BlockGuard::Weapon, true));
        near(model.reduction(40.0, BlockGuard::Hands, false), Some(0.1));
        assert_eq!(model.reduction(40.0, BlockGuard::Hands, true), None, "an armed attacker always deals full damage");
        near(model.reduction(-5.0, BlockGuard::Shield, true), Some(0.0)); // a negative skill clamps
    }
}
