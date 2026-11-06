//! Input enable layers (M47.2): Fallout 4 / Starfield `InputEnableLayer`.
//!
//! CK *InputEnableLayer Script*: "A single layer used for disabling/enabling
//! player input." A script `Create`s a layer ("these are a limited resource, so
//! you should not hold onto them longer then you need to have the player's
//! controls disabled"), disables or enables controls on it, and `Delete`s it —
//! "any script variables or properties looking at it will be invalidated, and of
//! course any disables it may have held are cleared"; `Reset` "clears any control
//! disables that this layer may have". A control is enabled for the player while
//! no layer holds it disabled. The layers are saved (a script holds the handle
//! across a save); [`InputLayers::sync_state`] folds them into the
//! [`PlayerControlState`] domains the engine reads. The controls that state has
//! no domain for (VATS, favorites, running, jumping, sprinting, the Z key, fast
//! travel) are tracked and answer the `Is…Enabled` queries; nothing reads them yet.

use std::collections::{BTreeMap, BTreeSet};

use byroredux_core::ecs::resource::Resource;

use crate::player_control::PlayerControlState;

/// A player control an input layer can disable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum Control {
    Movement,
    Fighting,
    CamSwitch,
    Looking,
    Sneaking,
    Menu,
    Activate,
    /// The journal tabs (`abJournalTabs` / `EnableJournal`).
    Journal,
    Vats,
    Favorites,
    Running,
    Jumping,
    Sprinting,
    ZKey,
    FastTravel,
}

impl Control {
    /// The controls `EnablePlayerControls` / `DisablePlayerControls` take, in
    /// parameter order.
    pub const PARAMETER_ORDER: [Control; 11] = [
        Control::Movement,
        Control::Fighting,
        Control::CamSwitch,
        Control::Looking,
        Control::Sneaking,
        Control::Menu,
        Control::Activate,
        Control::Journal,
        Control::Vats,
        Control::Favorites,
        Control::Running,
    ];

    /// `DisablePlayerControls`' defaults, in parameter order (CK).
    pub const DISABLE_DEFAULTS: [bool; 11] = [true, true, false, false, false, true, true, false, true, true, false];

    /// The control `Enable<X>` / `Is<X>Enabled` names, lowercase without the
    /// `enable` / `is` / `enabled` affixes (`"movement"`, `"zkey"`).
    pub fn of_name(name: &str) -> Option<Control> {
        Some(match name {
            "movement" => Control::Movement,
            "fighting" => Control::Fighting,
            "camswitch" => Control::CamSwitch,
            "looking" => Control::Looking,
            "sneaking" => Control::Sneaking,
            "menu" => Control::Menu,
            "activate" => Control::Activate,
            "journal" => Control::Journal,
            "vats" => Control::Vats,
            "favorites" => Control::Favorites,
            "running" => Control::Running,
            "jumping" => Control::Jumping,
            "sprinting" => Control::Sprinting,
            "zkey" => Control::ZKey,
            "fasttravel" => Control::FastTravel,
            _ => return None,
        })
    }
}

/// Every live layer, by handle, with the controls it holds disabled. Saved.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct InputLayers {
    next: u32,
    layers: BTreeMap<u32, BTreeSet<Control>>,
}

impl Resource for InputLayers {}

impl InputLayers {
    /// A new layer's handle (never 0, never reused).
    pub fn create(&mut self) -> u32 {
        self.next += 1;
        self.layers.insert(self.next, BTreeSet::new());
        self.next
    }

    /// Whether `layer` is live.
    pub fn exists(&self, layer: u32) -> bool {
        self.layers.contains_key(&layer)
    }

    /// Delete the layer: its disables are cleared.
    pub fn delete(&mut self, layer: u32) {
        self.layers.remove(&layer);
    }

    /// Clear the layer's disables.
    pub fn reset(&mut self, layer: u32) {
        if let Some(disabled) = self.layers.get_mut(&layer) {
            disabled.clear();
        }
    }

    /// Enable or disable `control` on the layer (a dead layer ignores it).
    pub fn set_enabled(&mut self, layer: u32, control: Control, enabled: bool) {
        if let Some(disabled) = self.layers.get_mut(&layer) {
            if enabled {
                disabled.remove(&control);
            } else {
                disabled.insert(control);
            }
        }
    }

    /// Whether the layer leaves `control` enabled (true for a dead layer).
    pub fn layer_enabled(&self, layer: u32, control: Control) -> bool {
        self.layers.get(&layer).is_none_or(|disabled| !disabled.contains(&control))
    }

    /// Whether no layer holds `control` disabled.
    pub fn enabled(&self, control: Control) -> bool {
        !self.layers.values().any(|disabled| disabled.contains(&control))
    }

    /// Fold the layers into the engine's control domains.
    pub fn sync_state(&self, state: &mut PlayerControlState) {
        state.movement_enabled = self.enabled(Control::Movement);
        state.fighting_enabled = self.enabled(Control::Fighting);
        state.camera_switching_enabled = self.enabled(Control::CamSwitch);
        state.looking_enabled = self.enabled(Control::Looking);
        state.sneaking_enabled = self.enabled(Control::Sneaking);
        state.menu_enabled = self.enabled(Control::Menu);
        state.activation_enabled = self.enabled(Control::Activate);
        state.journal_tabs_enabled = self.enabled(Control::Journal);
    }
}

/// #6705 — this subsystem's saved column(s). Called by the binary's `build_save_registry` in its
/// row order (the schema fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    // M47.2 — InputEnableLayer handles and the controls they disable.
    registry.register_resource::<InputLayers>("InputLayers");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_control_is_enabled_while_no_layer_disables_it() {
        let mut layers = InputLayers::default();
        let (a, b) = (layers.create(), layers.create());
        assert_ne!(a, b);
        layers.set_enabled(a, Control::Movement, false);
        layers.set_enabled(b, Control::Movement, false);
        layers.set_enabled(b, Control::Fighting, false);
        let mut state = PlayerControlState::default();
        layers.sync_state(&mut state);
        assert!(!state.movement_enabled && !state.fighting_enabled && state.looking_enabled);
        layers.delete(a);
        layers.sync_state(&mut state);
        assert!(!state.movement_enabled, "layer b still holds movement");
        layers.reset(b);
        layers.sync_state(&mut state);
        assert!(state.movement_enabled && state.fighting_enabled);
        assert!(!layers.layer_enabled(a, Control::Movement) || !layers.exists(a));
        assert!(layers.layer_enabled(a, Control::Movement), "a dead layer enables everything");
    }

    #[test]
    fn handles_are_never_reused() {
        let mut layers = InputLayers::default();
        let first = layers.create();
        layers.delete(first);
        assert_ne!(layers.create(), first);
    }

    #[test]
    fn control_names_cover_the_native_family() {
        assert_eq!(Control::of_name("zkey"), Some(Control::ZKey));
        assert_eq!(Control::of_name("nonsense"), None);
        assert_eq!(Control::PARAMETER_ORDER.len(), Control::DISABLE_DEFAULTS.len());
    }
}
