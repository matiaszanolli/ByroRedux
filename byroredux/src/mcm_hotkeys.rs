//! MCM Helper's hotkeys, and the script extenders' key events, from the player's keys (LMC-UI-002 slice 4b,
//! `docs/engine/mcm-host.md` §5.1; LMC-SCR-004, `docs/engine/extender-catalog.md` §10). For the hotkeys:
//! each frame the keys and mouse buttons the world holds are compared with the last frame's, and every
//! key that went down or came up goes to [`byroredux_scripting::mcm::helper::hotkey_events`] as its
//! DirectInput code, with how long a released key was held.
//!
//! The held set is the world's: a key a menu, the console or egui took never enters it, and while an
//! open menu pauses the game nothing is sent (MCM Helper's `KeybindEventHandler` skips input while
//! `GameIsPaused`); a key released meanwhile comes up on the first frame after.

use std::collections::{HashMap, HashSet};

use byroredux_core::ecs::resources::MenuRegistry;
use byroredux_core::ecs::{Resource, World};

/// The keys down at the last frame a reader saw, by DirectInput code, with how long each has been down.
#[derive(Debug, Default)]
pub(crate) struct KeyTracker(HashMap<i32, f32>);

/// The keys the MCM Helper hotkeys last saw held (they stop while a menu pauses the game).
#[derive(Debug, Default)]
pub(crate) struct HotkeyKeys(KeyTracker);

impl Resource for HotkeyKeys {}

/// The keys the script extenders' key and control events last saw held (LMC-SCR-004): SKSE64's input handler
/// queues its events whatever menu is open, so these do not stop.
#[derive(Debug, Default)]
pub(crate) struct ExtenderInputKeys(KeyTracker);

impl Resource for ExtenderInputKeys {}

impl KeyTracker {
    /// Advance by `dt` to the keys `now` held: the codes that went down, and those that came up with the
    /// seconds they were held. Both sorted, so a frame's events run in one order.
    fn advance(&mut self, now: &HashSet<i32>, dt: f32) -> (Vec<i32>, Vec<(i32, f32)>) {
        for held in self.0.values_mut() {
            *held += dt;
        }
        let mut ups: Vec<(i32, f32)> = self.0.iter().filter(|(code, _)| !now.contains(code)).map(|(code, held)| (*code, *held)).collect();
        let mut downs: Vec<i32> = now.iter().copied().filter(|code| !self.0.contains_key(code)).collect();
        for (code, _) in &ups {
            self.0.remove(code);
        }
        for code in &downs {
            self.0.insert(*code, 0.0);
        }
        ups.sort_unstable_by_key(|(code, _)| *code);
        downs.sort_unstable();
        (downs, ups)
    }
}

/// The keys and mouse buttons the world holds, by DirectInput code.
fn held_codes(world: &World) -> Option<HashSet<i32>> {
    world.try_resource::<crate::components::InputState>().map(|input| {
        let keys = input.keys_held.iter().filter_map(|key| crate::interaction::papyrus_keycode(*key));
        let buttons = input.mouse_buttons_held.iter().filter_map(|button| crate::interaction::papyrus_mouse_keycode(*button));
        keys.chain(buttons).collect::<HashSet<i32>>()
    })
}

/// The frame's hotkey presses and releases, sent to the MCM Helper hotkeys.
pub(crate) fn mcm_hotkey_system(world: &World, dt: f32) {
    if world.try_resource::<MenuRegistry>().is_some_and(|registry| registry.paused()) {
        return;
    }
    let Some(now) = held_codes(world) else {
        return;
    };
    let (downs, ups) = {
        let Some(mut keys) = world.try_resource_mut::<HotkeyKeys>() else { return };
        keys.0.advance(&now, dt)
    };
    if !downs.is_empty() || !ups.is_empty() {
        byroredux_scripting::mcm::helper::hotkey_events(world, &downs, &ups);
    }
}

/// The script extenders' key state and events (LMC-SCR-004, `docs/engine/extender-catalog.md` §10): the held keys
/// for `Input.IsKeyPressed` and kin, and the frame's key and control transitions for `OnKeyDown` / `OnKeyUp` /
/// `OnControlDown` / `OnControlUp`. A key's control is the one the engine's bindings give it, under SKSE's control
/// names (`ActionBindings::papyrus_bindings`, keyboard only for now).
pub(crate) fn extender_input_system(world: &World, dt: f32) {
    let Some(now) = held_codes(world) else {
        return;
    };
    let mut sorted: Vec<i32> = now.iter().copied().collect();
    sorted.sort_unstable();
    if let Some(mut held) = world.try_resource_mut::<byroredux_scripting::HeldKeys>()
        && held.0 != sorted
    {
        held.0 = sorted;
    }
    let (downs, ups) = {
        let Some(mut keys) = world.try_resource_mut::<ExtenderInputKeys>() else { return };
        keys.0.advance(&now, dt)
    };
    if downs.is_empty() && ups.is_empty() {
        return;
    }
    let bindings = world.try_resource::<crate::interaction::ActionBindings>().map(|bindings| bindings.papyrus_bindings()).unwrap_or_default();
    let control = |code: i32| bindings.iter().find(|binding| binding.keycode == code).map(|binding| binding.control.clone());
    let frame = byroredux_scripting::InputFrame {
        controls_down: downs.iter().filter_map(|code| control(*code)).collect(),
        controls_up: ups.iter().filter_map(|(code, held)| control(*code).map(|name| (name, *held))).collect(),
        keys_down: downs,
        keys_up: ups,
    };
    byroredux_scripting::deliver_input_events(world, &frame);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presses_and_releases_come_from_the_held_set_with_their_time() {
        let mut keys = KeyTracker::default();
        let held = |codes: &[i32]| codes.iter().copied().collect::<HashSet<i32>>();
        assert_eq!(keys.advance(&held(&[0x22, 256]), 0.016), (vec![0x22, 256], Vec::new()));
        assert_eq!(keys.advance(&held(&[0x22, 256]), 0.5), (Vec::new(), Vec::new()), "held, no event");
        let (downs, ups) = keys.advance(&held(&[256, 0x2A]), 0.25);
        assert_eq!(downs, [0x2A]);
        assert_eq!(ups.len(), 1);
        assert_eq!(ups[0].0, 0x22);
        assert!((ups[0].1 - 0.75).abs() < 1e-6, "held 0.5 + 0.25 s");
    }

    /// LMC-SCR-004 — the extenders' input: the held keys are published in code order, and a key's binding names
    /// its control (E is Activate by default), for a key that went down and for one that came up.
    #[test]
    fn extender_input_publishes_held_keys_and_maps_keys_to_controls() {
        use winit::keyboard::KeyCode;
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.insert_resource(crate::components::InputState::default());
        world.insert_resource(crate::interaction::ActionBindings::default());
        world.init_resource::<ExtenderInputKeys>();
        world.resource_mut::<crate::components::InputState>().keys_held.extend([KeyCode::KeyE, KeyCode::KeyA]);
        extender_input_system(&world, 0.0);
        assert_eq!(world.resource::<byroredux_scripting::HeldKeys>().0, [0x12, 0x1E], "E (0x12) before A (0x1E)");
        let bindings = world.resource::<crate::interaction::ActionBindings>().papyrus_bindings();
        assert!(bindings.iter().any(|binding| binding.keycode == 0x12 && binding.control == "Activate"));
        world.resource_mut::<crate::components::InputState>().keys_held.clear();
        extender_input_system(&world, 0.5);
        assert!(world.resource::<byroredux_scripting::HeldKeys>().0.is_empty());
    }

    #[test]
    fn keys_and_mouse_buttons_map_to_directinput_codes() {
        use winit::event::MouseButton;
        use winit::keyboard::KeyCode;
        assert_eq!(crate::interaction::papyrus_keycode(KeyCode::KeyG), Some(0x22));
        assert_eq!(crate::interaction::papyrus_keycode(KeyCode::ArrowUp), Some(0xC8));
        assert_eq!(crate::interaction::papyrus_keycode(KeyCode::NumpadEnter), Some(0x9C));
        assert_eq!(crate::interaction::papyrus_mouse_keycode(MouseButton::Left), Some(256));
        assert_eq!(crate::interaction::papyrus_mouse_keycode(MouseButton::Middle), Some(258));
        assert_eq!(crate::interaction::papyrus_mouse_keycode(MouseButton::Other(9)), None);
    }
}
