//! Crippled-limb effects (M-VATS Phase 2, `VATS-LIMB-006` / `-010` / `-011`).
//!
//! A limb whose condition actor value has fallen to zero or below is *crippled* (GECK *GetVATSValue* /
//! *Cripple Part*: "condition changes from > 0 to <= 0"). Three formulas read how many legs or arms are:
//!
//! * **Movement** (GECK *Movement Settings*): one crippled leg multiplies movement speed by
//!   `fMoveOneCrippledLegSpeedMult`, two by `fMoveTwoCrippledLegsSpeedMult`.
//! * **Gun spread** (GECK *Gun Spread Formula*): `ArmPenalty = fGunSpreadArmBase + fGunSpreadArmMult × ArmCondition`,
//!   added to the weapon's spread.
//! * **Melee damage** (GECK *Weapon Damage Formula*):
//!   `MeleeArmConditionPenalty = BaseWeaponDamage × (fDamageArmConditionBase − 1 + fDamageArmConditionMult × ArmCondition) / 2`,
//!   added to the damage.
//!
//! `ArmCondition` is 1 with a healthy right arm and 0 with a crippled one for a one-handed weapon; for a
//! two-handed weapon it is 1 with both arms healthy, 0.5 with one crippled and 0 with both.
//!
//! ## Where the numbers come from
//!
//! Load-order GMST first, then the compiled-in default ([`VatsModel`](super::VatsModel)'s contract). The leg
//! multipliers and the gun-spread arm pair are authored by both `Fallout3.esm` and `FalloutNV.esm` at the
//! values below (read from the masters). The two melee-arm settings are authored by **neither** master: their
//! defaults come from `Fallout3.exe`'s setting table (the value sits before a pointer to the name; the same
//! method reproduces `fMoveEncumEffect` = 0.4) and equal the GECK page's: 0.2 and 0.8.
//!
//! The GECK page's prose says the melee term costs "(up to) 10 % of base weapon damage" with crippled arms,
//! but its own formula and these defaults give `(0.2 − 1) / 2 = −0.4`, 40 %. The formula is implemented as
//! written; the prose is not. A capture of vanilla melee damage with a crippled arm would settle it.

/// The crippled-limb settings (`f*` GMSTs), resolved load-order-first.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "inspect", derive(serde::Serialize, serde::Deserialize))]
pub struct LimbEffectModel {
    /// `fMoveOneCrippledLegSpeedMult`.
    pub one_leg_speed_mult: f32,
    /// `fMoveTwoCrippledLegsSpeedMult`.
    pub two_legs_speed_mult: f32,
    /// `fGunSpreadArmBase`.
    pub gun_spread_arm_base: f32,
    /// `fGunSpreadArmMult`.
    pub gun_spread_arm_mult: f32,
    /// `fDamageArmConditionBase`.
    pub melee_arm_base: f32,
    /// `fDamageArmConditionMult`.
    pub melee_arm_mult: f32,
}

impl LimbEffectModel {
    /// The Fallout 3 / New Vegas defaults (see the module docs for each value's source).
    pub const FALLOUT: Self = Self {
        one_leg_speed_mult: 0.85,
        two_legs_speed_mult: 0.75,
        gun_spread_arm_base: 0.5,
        gun_spread_arm_mult: -0.5,
        melee_arm_base: 0.2,
        melee_arm_mult: 0.8,
    };

    /// The model with each setting the load order authors replacing its default. A non-finite authored value is
    /// ignored; negative values are kept for the two spread/damage multipliers that are negative by design
    /// (`fGunSpreadArmMult` is −0.5), and refused for the speed multipliers, which cannot reverse movement.
    #[must_use]
    pub fn with_gmst(self, gmst: impl Fn(&str) -> Option<f32>) -> Self {
        let finite = |name: &str, field: f32| gmst(name).filter(|v| v.is_finite()).unwrap_or(field);
        let speed = |name: &str, field: f32| gmst(name).filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(field);
        Self {
            one_leg_speed_mult: speed("fMoveOneCrippledLegSpeedMult", self.one_leg_speed_mult),
            two_legs_speed_mult: speed("fMoveTwoCrippledLegsSpeedMult", self.two_legs_speed_mult),
            gun_spread_arm_base: finite("fGunSpreadArmBase", self.gun_spread_arm_base),
            gun_spread_arm_mult: finite("fGunSpreadArmMult", self.gun_spread_arm_mult),
            melee_arm_base: finite("fDamageArmConditionBase", self.melee_arm_base),
            melee_arm_mult: finite("fDamageArmConditionMult", self.melee_arm_mult),
        }
    }

    /// The movement speed multiplier for an actor with `crippled_legs` crippled legs (0, 1 or 2; more reads as 2).
    #[must_use]
    pub fn leg_speed_mult(&self, crippled_legs: u32) -> f32 {
        match crippled_legs {
            0 => 1.0,
            1 => self.one_leg_speed_mult,
            _ => self.two_legs_speed_mult,
        }
    }

    /// `ArmPenalty` of the Gun Spread Formula: added to the spread. 0 with healthy arms at the default settings.
    #[must_use]
    pub fn gun_spread_arm_penalty(&self, arm_condition: f32) -> f32 {
        self.gun_spread_arm_base + self.gun_spread_arm_mult * arm_condition
    }

    /// `MeleeArmConditionPenalty` of the Weapon Damage Formula: added to the damage (negative when it reduces
    /// it). 0 with healthy arms at the default settings.
    #[must_use]
    pub fn melee_arm_penalty(&self, base_weapon_damage: f32, arm_condition: f32) -> f32 {
        base_weapon_damage * (self.melee_arm_base - 1.0 + self.melee_arm_mult * arm_condition) / 2.0
    }
}

/// `ArmCondition` for the Gun Spread and Weapon Damage formulas: with a one-handed weapon the right arm alone
/// decides (1 healthy, 0 crippled); with a two-handed weapon both arms do (1, 0.5, 0).
#[must_use]
pub fn arm_condition(two_handed: bool, left_arm_crippled: bool, right_arm_crippled: bool) -> f32 {
    if two_handed {
        1.0 - 0.5 * (u32::from(left_arm_crippled) + u32::from(right_arm_crippled)) as f32
    } else if right_arm_crippled {
        0.0
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-6;

    /// VATS-LIMB-006 — zero, one and two crippled legs.
    #[test]
    fn crippled_legs_slow_movement_by_the_settings() {
        let model = LimbEffectModel::FALLOUT;
        assert_eq!(model.leg_speed_mult(0), 1.0);
        assert_eq!(model.leg_speed_mult(1), 0.85);
        assert_eq!(model.leg_speed_mult(2), 0.75);
        assert_eq!(model.leg_speed_mult(4), 0.75, "a many-legged creature with every leg gone is the two-leg case");
    }

    /// VATS-LIMB-010 — the arm term widens the spread only when an arm is crippled.
    #[test]
    fn a_crippled_arm_widens_the_gun_spread() {
        let model = LimbEffectModel::FALLOUT;
        let healthy = model.gun_spread_arm_penalty(arm_condition(false, false, false));
        let crippled = model.gun_spread_arm_penalty(arm_condition(false, false, true));
        assert!(healthy.abs() < EPS, "healthy arms add nothing: {healthy}");
        assert!((crippled - 0.5).abs() < EPS, "a crippled right arm adds fArmBase: {crippled}");
        // Two-handed: half the penalty with one arm down, all of it with both.
        let one = model.gun_spread_arm_penalty(arm_condition(true, true, false));
        let both = model.gun_spread_arm_penalty(arm_condition(true, true, true));
        assert!((one - 0.25).abs() < EPS && (both - 0.5).abs() < EPS, "{one} {both}");
    }

    /// VATS-LIMB-011 — the melee arm term is zero with healthy arms and negative with a crippled one.
    #[test]
    fn a_crippled_arm_reduces_melee_damage() {
        let model = LimbEffectModel::FALLOUT;
        assert!(model.melee_arm_penalty(30.0, arm_condition(false, false, false)).abs() < EPS);
        // (0.2 - 1 + 0.8 * 0) / 2 = -0.4 of the base weapon damage (see the module docs on the page's "10 %").
        let crippled = model.melee_arm_penalty(30.0, arm_condition(false, false, true));
        assert!((crippled - (-12.0)).abs() < EPS, "{crippled}");
        let half = model.melee_arm_penalty(30.0, arm_condition(true, true, false));
        assert!((half - (-6.0)).abs() < EPS, "{half}");
    }

    /// A one-handed weapon ignores the left arm; a two-handed one counts both.
    #[test]
    fn arm_condition_follows_the_weapon_handedness() {
        assert_eq!(arm_condition(false, true, false), 1.0, "the left arm does not hold a one-handed weapon");
        assert_eq!(arm_condition(false, false, true), 0.0);
        assert_eq!(arm_condition(true, false, false), 1.0);
        assert_eq!(arm_condition(true, true, false), 0.5);
        assert_eq!(arm_condition(true, false, true), 0.5);
        assert_eq!(arm_condition(true, true, true), 0.0);
    }

    /// A mod that re-tunes a setting changes the result; a non-finite or (for speed) negative value is ignored,
    /// and the spread multiplier may be negative.
    #[test]
    fn the_load_order_retunes_the_effects() {
        let tuned = LimbEffectModel::FALLOUT.with_gmst(|name| match name {
            "fMoveOneCrippledLegSpeedMult" => Some(0.5),
            "fMoveTwoCrippledLegsSpeedMult" => Some(-1.0),
            "fGunSpreadArmMult" => Some(-1.5),
            "fDamageArmConditionBase" => Some(f32::NAN),
            _ => None,
        });
        assert_eq!(tuned.leg_speed_mult(1), 0.5);
        assert_eq!(tuned.leg_speed_mult(2), 0.75, "a negative speed multiplier is refused");
        assert_eq!(tuned.gun_spread_arm_mult, -1.5);
        assert_eq!(tuned.melee_arm_base, 0.2, "a non-finite value is refused");
        assert_eq!(LimbEffectModel::FALLOUT.with_gmst(|_| None), LimbEffectModel::FALLOUT);
    }
}
