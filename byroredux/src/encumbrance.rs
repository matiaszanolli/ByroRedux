//! Encumbrance (M47.2): the player's carried weight against Carry Weight.
//!
//! Capacity is the CarryWeight actor value through the Get Max Carry Weight
//! perk entry point. The GECK *Stats List*: "InventoryWeight — how much the actor is carrying.
//! If InventoryWeight > CarryWeight, the actor is over-encumbered." What
//! over-encumbrance does is per game:
//!
//! - **Fallout 3 / New Vegas** — the GECK *Movement Settings*: "movement speed
//!   is multiplied by `fMoveEncumEffect` when the actor's InventoryWeight AV
//!   exceeds the CarryWeight AV" (0.4 in Fallout3.exe's setting table, the
//!   GECK's default). Its `fMoveEncumEffectNoWea` companion ("possibly when
//!   the actor has their weapon holstered") is not split out: the player has
//!   no drawn / holstered state yet.
//! - **Skyrim** — UESP *Carry Weight*: "you will be limited to walking", so
//!   the player cannot sprint.
//!
//! Oblivion's encumbrance ("will not be able to move") is a hard-coded actor
//! value with no `AVIF`, and Fallout 4 / Starfield are not documented in the
//! local references, so they keep only the InventoryWeight upkeep where the
//! `AVIF` exists.

use byroredux_core::ecs::components::{ActorValues, Inventory};
use byroredux_core::ecs::{Resource, World};
use byroredux_core::character::OverEncumberedRule;
use byroredux_plugin::esm::records::EsmIndex;
use byroredux_scripting::condition::{evaluate_function, ConditionFunction};

/// What over-encumbrance does to the player's movement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum OverEncumbered {
    /// Movement speed is multiplied by this.
    SpeedMult(f32),
    /// No faster than the base (walking) pace.
    NoSprint,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EncumbranceRules {
    carry_weight: u32,
    inventory_weight: Option<u32>,
    effect: Option<OverEncumbered>,
}

impl Resource for EncumbranceRules {}

/// The player's encumbrance this frame. Re-derived every frame from the
/// inventory, so not saved.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct PlayerEncumbrance {
    pub(crate) weight: f32,
    pub(crate) capacity: f32,
    pub(crate) effect: Option<OverEncumbered>,
}

impl Resource for PlayerEncumbrance {}

impl PlayerEncumbrance {
    pub(crate) fn over(&self) -> bool {
        self.weight > self.capacity
    }

    /// The movement speed multiplier and whether sprinting is allowed.
    pub(crate) fn movement(&self) -> (f32, bool) {
        match (self.over(), self.effect) {
            (true, Some(OverEncumbered::SpeedMult(mult))) => {
                (fallout_move_encumbrance_multiplier(Some(mult)), true)
            }
            (true, Some(OverEncumbered::NoSprint)) => (1.0, false),
            _ => (1.0, true),
        }
    }
}

/// `fMoveEncumEffect`'s engine default, from Fallout3.exe's setting table
/// (the value precedes a pointer to the name) and the GECK page. FalloutNV.exe
/// is encrypted on disk; New Vegas runs the same engine.
const FALLOUT_MOVE_ENCUM_EFFECT: f32 = 0.4;
/// Encumbrance slows movement; it never grants a speed increase.
const MAX_FALLOUT_MOVE_ENCUM_EFFECT: f32 = 1.0;

fn fallout_move_encumbrance_multiplier(authored: Option<f32>) -> f32 {
    authored
        .filter(|mult| mult.is_finite() && *mult >= 0.0)
        .unwrap_or(FALLOUT_MOVE_ENCUM_EFFECT)
        .min(MAX_FALLOUT_MOVE_ENCUM_EFFECT)
}

/// Install (or remove) [`EncumbranceRules`]: needs the load order's
/// `CarryWeight` actor value.
pub(crate) fn install_encumbrance_rules(world: &mut World, index: &EsmIndex) -> bool {
    let Some(carry_weight) = index.actor_value_form_id("CarryWeight") else {
        world.remove_resource::<EncumbranceRules>();
        return false;
    };
    // #5921 — the rule is the character profile's row, not a game match.
    let effect = match index.character_rules.over_encumbered() {
        OverEncumberedRule::SpeedMult => Some(OverEncumbered::SpeedMult(
            fallout_move_encumbrance_multiplier(index.game_setting_float("fMoveEncumEffect")),
        )),
        OverEncumberedRule::NoSprint => Some(OverEncumbered::NoSprint),
        OverEncumberedRule::None => None,
    };
    world.insert_resource(EncumbranceRules {
        carry_weight,
        inventory_weight: index.actor_value_form_id("InventoryWeight"),
        effect,
    });
    true
}

/// Weigh the player's inventory, keep its `InventoryWeight` actor value
/// current (so conditions and scripts read it), and publish
/// [`PlayerEncumbrance`].
pub(crate) fn encumbrance_system(world: &World, _dt: f32) {
    let Some(rules) = world.try_resource::<EncumbranceRules>().map(|rules| rules.clone()) else {
        return;
    };
    let Some(player) = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|player| player.0)
    else {
        return;
    };
    // Weigh the stacks under the read guards rather than cloning the inventory
    // each frame (#6313). Safe to nest: this is an exclusive system and both are
    // read locks (its declared access is `reads` for each).
    let weight = {
        let Some(catalog) = world.try_resource::<crate::inventory::InventoryCatalog>() else {
            return;
        };
        let Some(inventory) = world.get::<Inventory>(player) else {
            return;
        };
        catalog.inventory_weight(&inventory)
    };
    if let Some(inventory_weight) = rules.inventory_weight {
        if let Some(mut values) = world.query_mut::<ActorValues>() {
            if let Some(values) = values.get_mut(player) {
                values.set_base(inventory_weight, weight);
            }
        }
    }
    let carry_weight = evaluate_function(
        ConditionFunction::GetActorValue,
        &byroredux_plugin::esm::records::condition::Condition {
            param_1: rules.carry_weight,
            ..Default::default()
        },
        player,
        world,
    );
    let capacity = byroredux_scripting::perk_entry_points::compose_max_carry_weight(
        world,
        player,
        carry_weight,
    );
    let encumbrance = PlayerEncumbrance {
        weight,
        capacity,
        effect: rules.effect,
    };
    if let Some(mut published) = world.try_resource_mut::<PlayerEncumbrance>() {
        if published.over() != encumbrance.over() {
            let state = if encumbrance.over() { "is" } else { "is no longer" };
            log::info!("Player {state} over-encumbered: carrying {weight:.1} of {capacity:.1}");
        }
        *published = encumbrance;
    }
}

#[cfg(test)]
mod tests;
