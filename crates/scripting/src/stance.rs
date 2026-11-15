//! M65 (SNK-STN-001…004) — the player's sneak stance: the one path that
//! enters and leaves it (`docs/engine/sneak-detection.md` §2).
//!
//! The binary's key, the `stealth.sneak` console command and Papyrus
//! `Actor.StartSneaking` all call [`set_sneaking`] / [`toggle_sneaking`], so the
//! gate below is the same for each (SNK-DX-002). The stance itself is the
//! `Sneaking` marker in core.

use byroredux_core::ecs::components::Sneaking;
use byroredux_core::ecs::{EntityId, World};

/// Register the stance marker's storage (the scripting crate owns the one path
/// that writes it).
pub fn register(world: &mut World) {
    world.register::<Sneaking>();
}

/// Why the stance cannot be entered right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SneakRefusal {
    /// `DisablePlayerControls`' sneaking flag, or a `Control::Sneaking` input
    /// layer, disables sneaking ("Enables or disables the player's ability to
    /// sneak on this layer", FO4 CK *EnableSneaking - InputEnableLayer*).
    ControlDisabled,
}

impl SneakRefusal {
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::ControlDisabled => "sneaking is disabled (DisablePlayerControls or an input layer)",
        }
    }
}

/// Whether sneaking is enabled: neither `PlayerControlState.sneaking_enabled`
/// nor any input layer disables it. Absent resources mean enabled.
#[must_use]
pub fn sneaking_enabled(world: &World) -> bool {
    let controls = world
        .try_resource::<crate::PlayerControlState>()
        .is_none_or(|state| state.sneaking_enabled);
    let layers = world
        .try_resource::<crate::input_layers::InputLayers>()
        .is_none_or(|layers| layers.enabled(crate::input_layers::Control::Sneaking));
    controls && layers
}

/// Whether `actor` is in the sneak stance (`IsSneaking`, SNK-STN-002).
#[must_use]
pub fn is_sneaking(world: &World, actor: EntityId) -> bool {
    world.has::<Sneaking>(actor)
}

/// Put `actor` in the stance or take it out, and return whether it is sneaking
/// afterwards.
///
/// Entering is refused while sneaking is disabled (SNK-STN-004). Leaving is
/// never refused: whether disabling sneaking stands an already sneaking player
/// up is V1 (#6935), and until it is answered disabling leaves the player
/// where they are, the smaller change. Without component storage the call is a
/// no-op that reports the current state.
pub fn set_sneaking(world: &World, actor: EntityId, sneak: bool) -> Result<bool, SneakRefusal> {
    if sneak == is_sneaking(world, actor) {
        return Ok(sneak);
    }
    if sneak && !sneaking_enabled(world) {
        return Err(SneakRefusal::ControlDisabled);
    }
    let Some(mut storage) = world.query_mut::<Sneaking>() else {
        return Ok(is_sneaking(world, actor));
    };
    if sneak {
        storage.insert(actor, Sneaking);
    } else {
        storage.remove(actor);
    }
    Ok(sneak)
}

/// Flip `actor`'s stance through [`set_sneaking`]: the key's press, and
/// `StartSneaking` ("If the actor was already sneaking, cancels sneaking", CK
/// *StartSneaking - Actor*).
pub fn toggle_sneaking(world: &World, actor: EntityId) -> Result<bool, SneakRefusal> {
    set_sneaking(world, actor, !is_sneaking(world, actor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_layers::{Control, InputLayers};

    fn world_with_actor() -> (World, EntityId) {
        let mut world = World::new();
        let actor = world.spawn();
        world.register::<Sneaking>();
        (world, actor)
    }

    #[test]
    fn the_toggle_enters_then_leaves_the_stance() {
        let (world, actor) = world_with_actor();
        assert!(!is_sneaking(&world, actor));
        assert_eq!(toggle_sneaking(&world, actor), Ok(true));
        assert!(is_sneaking(&world, actor));
        assert_eq!(toggle_sneaking(&world, actor), Ok(false));
        assert!(!is_sneaking(&world, actor));
    }

    #[test]
    fn a_disabled_layer_or_control_refuses_entering_but_not_leaving() {
        let (mut world, actor) = world_with_actor();
        let mut layers = InputLayers::default();
        let layer = layers.create();
        layers.set_enabled(layer, Control::Sneaking, false);
        world.insert_resource(layers);
        assert_eq!(set_sneaking(&world, actor, true), Err(SneakRefusal::ControlDisabled));
        assert!(!is_sneaking(&world, actor));

        world.remove_resource::<InputLayers>();
        assert_eq!(set_sneaking(&world, actor, true), Ok(true));
        let mut controls = crate::PlayerControlState::default();
        controls.sneaking_enabled = false;
        world.insert_resource(controls);
        assert_eq!(set_sneaking(&world, actor, false), Ok(false), "leaving is never refused");
        assert_eq!(toggle_sneaking(&world, actor), Err(SneakRefusal::ControlDisabled));
    }
}
