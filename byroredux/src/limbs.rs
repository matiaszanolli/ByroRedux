//! Crippled-limb effects on the actors of a Fallout 3 / New Vegas load order (`VATS-LIMB-006`, `-011`; the gun
//! spread term of `-010` has no consumer yet, see below).
//!
//! A limb is *crippled* when its condition actor value (`LeftAttackCondition`, `RightAttackCondition`,
//! `LeftMobilityCondition`, `RightMobilityCondition`) is present and at or below zero (GECK *GetVATSValue* /
//! *Cripple Part*: "condition changes from > 0 to <= 0"). The un-cripple threshold on FO3/FNV is a Phase 0
//! verify item (`VATS-LIMB-005`), so this reads the documented crippling condition only.
//!
//! The formulas and their sourced defaults are [`LimbEffectModel`]'s. This module is the world-facing half: it
//! resolves the four actor values from the load order once ([`build_limb_effects`], present only where the
//! profile seeds body conditions, i.e. FO3/FNV) and reads them per actor.
//!
//! * **Movement** - [`locomotion_mult`] multiplies the player's speed and every NPC walk/run stride.
//! * **Melee damage** - [`melee_arm_penalty`] is added to a melee swing's damage.
//! * **Gun spread** - [`LimbEffectModel::gun_spread_arm_penalty`] is built and pinned, but the engine has no
//!   gun-spread computation for it to widen (shots fly straight, and the GECK page's own spread formula is
//!   marked unvalidated), so it has no consumer until one exists.

use byroredux_core::character::{arm_condition, LimbEffectModel};
use byroredux_core::ecs::components::ActorValues;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::records::EsmIndex;

/// The crippled-limb settings and the four limb-condition actor values they read. `NOT_SAVED_BY_DESIGN`: rebuilt
/// from the load order on every boot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LimbEffects {
    pub(crate) model: LimbEffectModel,
    left_arm: Option<u32>,
    right_arm: Option<u32>,
    left_leg: Option<u32>,
    right_leg: Option<u32>,
}

impl Resource for LimbEffects {}

impl LimbEffects {
    /// A fixture for other modules' tests: the four limb-condition ids are consecutive, arms first.
    #[cfg(test)]
    pub(crate) fn for_tests(model: LimbEffectModel, left_arm: u32, right_arm: u32) -> Self {
        Self {
            model,
            left_arm: Some(left_arm),
            right_arm: Some(right_arm),
            left_leg: Some(right_arm + 1),
            right_leg: Some(right_arm + 2),
        }
    }
}

/// Build the limb effects for a load order: the Fallout defaults with the load order's settings over them, and
/// the condition actor values by name. `None` on every game whose profile seeds no body conditions.
pub(crate) fn build_limb_effects(index: &EsmIndex) -> Option<LimbEffects> {
    index.character_rules.body_condition_base()?;
    let model = LimbEffectModel::FALLOUT.with_gmst(|name| index.game_setting_float(name));
    let avif = |name: &str| index.actor_value_form_id(name);
    Some(LimbEffects {
        model,
        left_arm: avif("LeftAttackCondition"),
        right_arm: avif("RightAttackCondition"),
        left_leg: avif("LeftMobilityCondition"),
        right_leg: avif("RightMobilityCondition"),
    })
}

/// Whether the condition value `avif` is present on the actor and at or below zero. An actor that carries no such
/// value is not crippled (the absent-AV default of `0` must not read as a crippled limb).
fn crippled(values: &ActorValues, avif: Option<u32>) -> bool {
    avif.and_then(|avif| values.get(avif)).is_some_and(|value| value.current() <= 0.0)
}

/// How many of the actor's two legs are crippled.
pub(crate) fn crippled_legs(world: &World, actor: EntityId) -> u32 {
    let Some(effects) = world.try_resource::<LimbEffects>().map(|effects| *effects) else {
        return 0;
    };
    let Some(values) = world.get::<ActorValues>(actor) else {
        return 0;
    };
    u32::from(crippled(&values, effects.left_leg)) + u32::from(crippled(&values, effects.right_leg))
}

/// The movement speed multiplier of `actor`'s crippled legs: `1.0` with none, no limb effects (a load order that
/// seeds no body conditions) or no actor values.
pub(crate) fn locomotion_mult(world: &World, actor: EntityId) -> f32 {
    let Some(effects) = world.try_resource::<LimbEffects>().map(|effects| *effects) else {
        return 1.0;
    };
    effects.model.leg_speed_mult(crippled_legs(world, actor))
}

/// `MeleeArmConditionPenalty` for a swing of `base_weapon_damage` by `aggressor`: added to the damage, negative
/// when it reduces it, `0.0` with healthy arms or no limb effects. `two_handed` is the weapon's handedness.
pub(crate) fn melee_arm_penalty(world: &World, aggressor: EntityId, base_weapon_damage: f32, two_handed: bool) -> f32 {
    let Some(effects) = world.try_resource::<LimbEffects>().map(|effects| *effects) else {
        return 0.0;
    };
    let Some(values) = world.get::<ActorValues>(aggressor) else {
        return 0.0;
    };
    let condition = arm_condition(
        two_handed,
        crippled(&values, effects.left_arm),
        crippled(&values, effects.right_arm),
    );
    effects.model.melee_arm_penalty(base_weapon_damage, condition)
}

/// `WEAP` animation types of a melee weapon: Melee (1 Hand) and Melee (2 Hand) (xEdit `wbWeaponAnimTypeEnum`).
const MELEE_ANIM_TYPES: [u8; 2] = [1, 2];
/// The two-handed animation types: Melee (2 Hand), the rifles, Handle and Launcher.
const TWO_HANDED_ANIM_TYPES: [u8; 6] = [2, 5, 6, 7, 8, 9];

/// Whether a weapon of `anim_type` is a melee weapon.
pub(crate) fn is_melee_anim_type(anim_type: u8) -> bool {
    MELEE_ANIM_TYPES.contains(&anim_type)
}

/// Whether a weapon of `anim_type` is held in two hands.
pub(crate) fn is_two_handed_anim_type(anim_type: u8) -> bool {
    TWO_HANDED_ANIM_TYPES.contains(&anim_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT_ARM: u32 = 0x5D7;
    const RIGHT_ARM: u32 = 0x5D8;
    const LEFT_LEG: u32 = 0x5D9;
    const RIGHT_LEG: u32 = 0x5DA;

    fn world_with(conditions: [f32; 4]) -> (World, EntityId) {
        let mut world = World::new();
        world.register::<ActorValues>();
        world.insert_resource(LimbEffects {
            model: LimbEffectModel::FALLOUT,
            left_arm: Some(LEFT_ARM),
            right_arm: Some(RIGHT_ARM),
            left_leg: Some(LEFT_LEG),
            right_leg: Some(RIGHT_LEG),
        });
        let actor = world.spawn();
        let [la, ra, ll, rl] = conditions;
        world.insert(actor, ActorValues::from_pairs([(LEFT_ARM, la), (RIGHT_ARM, ra), (LEFT_LEG, ll), (RIGHT_LEG, rl)]));
        (world, actor)
    }

    /// VATS-LIMB-006 - the movement speed with zero, one and two crippled legs.
    #[test]
    fn movement_speed_follows_the_crippled_legs() {
        for (conditions, legs, want) in [
            ([100.0, 100.0, 100.0, 100.0], 0, 1.0),
            ([100.0, 100.0, 0.0, 100.0], 1, 0.85),
            ([100.0, 100.0, 100.0, -20.0], 1, 0.85),
            ([100.0, 100.0, 0.0, 0.0], 2, 0.75),
            // Arms do not slow the legs.
            ([0.0, 0.0, 100.0, 100.0], 0, 1.0),
        ] {
            let (world, actor) = world_with(conditions);
            assert_eq!(crippled_legs(&world, actor), legs, "{conditions:?}");
            assert_eq!(locomotion_mult(&world, actor), want, "{conditions:?}");
        }
    }

    /// An actor with no limb conditions, or a world with no limb effects, is never slowed: the absent-AV default
    /// of 0 must not read as a crippled limb.
    #[test]
    fn missing_conditions_are_not_crippled() {
        let (mut world, actor) = world_with([100.0; 4]);
        let bare = world.spawn();
        world.insert(bare, ActorValues::new());
        assert_eq!(locomotion_mult(&world, bare), 1.0);
        assert_eq!(melee_arm_penalty(&world, bare, 30.0, false), 0.0);
        let unplaced = world.spawn();
        assert_eq!(locomotion_mult(&world, unplaced), 1.0, "no ActorValues at all");
        world.remove_resource::<LimbEffects>();
        assert_eq!(locomotion_mult(&world, actor), 1.0, "no limb effects in this load order");
        assert_eq!(melee_arm_penalty(&world, actor, 30.0, true), 0.0);
    }

    /// VATS-LIMB-011 - the melee damage with and without the crippled arm, by weapon handedness.
    #[test]
    fn a_crippled_arm_reduces_melee_damage() {
        let (healthy, h) = world_with([100.0; 4]);
        assert_eq!(melee_arm_penalty(&healthy, h, 30.0, false), 0.0);
        assert_eq!(melee_arm_penalty(&healthy, h, 30.0, true), 0.0);
        // Right arm crippled: a one-handed weapon loses 0.4 of its base damage, a two-handed one half of that.
        let (right, r) = world_with([100.0, 0.0, 100.0, 100.0]);
        assert!((melee_arm_penalty(&right, r, 30.0, false) - -12.0).abs() < 1e-5);
        assert!((melee_arm_penalty(&right, r, 30.0, true) - -6.0).abs() < 1e-5);
        // Left arm crippled: a one-handed weapon does not care, a two-handed one does.
        let (left, l) = world_with([0.0, 100.0, 100.0, 100.0]);
        assert_eq!(melee_arm_penalty(&left, l, 30.0, false), 0.0);
        assert!((melee_arm_penalty(&left, l, 30.0, true) - -6.0).abs() < 1e-5);
    }

    #[test]
    fn weapon_animation_types_classify_melee_and_handedness() {
        assert!(is_melee_anim_type(1) && is_melee_anim_type(2));
        assert!(!is_melee_anim_type(0) && !is_melee_anim_type(3) && !is_melee_anim_type(5));
        assert!(is_two_handed_anim_type(2) && is_two_handed_anim_type(5) && is_two_handed_anim_type(9));
        assert!(!is_two_handed_anim_type(1) && !is_two_handed_anim_type(3) && !is_two_handed_anim_type(4));
    }
}
