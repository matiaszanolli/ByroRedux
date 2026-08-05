//! Canonical player/actor control state written by Papyrus fragments.
//!
//! Skyrim's startup scripts selectively disable control domains, restrain the
//! player actor, and hand movement to AI. Keeping those as explicit ECS state
//! lets the app input/controller layer consume the same state without hosting
//! Papyrus-specific logic.

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::world::World;

/// Control domains selected by `EnablePlayerControls` /
/// `DisablePlayerControls`. A selected domain is assigned the call's enabled
/// value; an unselected domain is left untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerControlSelection {
    pub movement: bool,
    pub fighting: bool,
    pub camera_switching: bool,
    pub looking: bool,
    pub sneaking: bool,
    pub menu: bool,
    pub activation: bool,
    pub journal_tabs: bool,
    pub pov_type: i32,
}

impl PlayerControlSelection {
    /// Papyrus defaults shared by both global functions.
    pub const PAPYRUS_DEFAULT: Self = Self {
        movement: true,
        fighting: true,
        camera_switching: true,
        looking: true,
        sneaking: true,
        menu: true,
        activation: true,
        journal_tabs: false,
        pov_type: 0,
    };
}

/// Engine-facing state of every player input domain plus cinematic modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct PlayerControlState {
    pub movement_enabled: bool,
    pub fighting_enabled: bool,
    pub camera_switching_enabled: bool,
    pub looking_enabled: bool,
    pub sneaking_enabled: bool,
    pub menu_enabled: bool,
    pub activation_enabled: bool,
    pub journal_tabs_enabled: bool,
    pub disabled_pov_type: i32,
    pub ai_driven: bool,
    pub hud_cart_mode: bool,
}

impl Default for PlayerControlState {
    fn default() -> Self {
        Self {
            movement_enabled: true,
            fighting_enabled: true,
            camera_switching_enabled: true,
            looking_enabled: true,
            sneaking_enabled: true,
            menu_enabled: true,
            activation_enabled: true,
            journal_tabs_enabled: true,
            disabled_pov_type: 0,
            ai_driven: false,
            hud_cart_mode: false,
        }
    }
}

impl Resource for PlayerControlState {}

impl PlayerControlState {
    pub fn apply_selection(&mut self, enabled: bool, selection: PlayerControlSelection) {
        if selection.movement {
            self.movement_enabled = enabled;
        }
        if selection.fighting {
            self.fighting_enabled = enabled;
        }
        if selection.camera_switching {
            self.camera_switching_enabled = enabled;
        }
        if selection.looking {
            self.looking_enabled = enabled;
        }
        if selection.sneaking {
            self.sneaking_enabled = enabled;
        }
        if selection.menu {
            self.menu_enabled = enabled;
        }
        if selection.activation {
            self.activation_enabled = enabled;
        }
        if selection.journal_tabs {
            self.journal_tabs_enabled = enabled;
        }
        self.disabled_pov_type = selection.pov_type;
    }
}

/// Per-actor state written by `Actor.SetRestrained`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ActorControlState {
    pub restrained: bool,
}

impl Component for ActorControlState {
    type Storage = SparseSetStorage<Self>;
}

pub fn register(world: &mut World) {
    world.register::<ActorControlState>();
    if world.try_resource::<PlayerControlState>().is_none() {
        world.insert_resource(PlayerControlState::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selective_disable_leaves_unselected_domains_untouched() {
        let mut state = PlayerControlState::default();
        state.apply_selection(
            false,
            PlayerControlSelection {
                movement: true,
                fighting: false,
                looking: true,
                ..PlayerControlSelection::PAPYRUS_DEFAULT
            },
        );

        assert!(!state.movement_enabled);
        assert!(state.fighting_enabled);
        assert!(!state.looking_enabled);
        assert!(state.journal_tabs_enabled);
    }
}
