//! MCM Helper's hotkeys from the player's keys (LMC-UI-002 slice 4b, `docs/engine/mcm-host.md` §5.1):
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

/// The keys down at the last frame the hotkeys saw, by DirectInput code, with how long each has been down.
#[derive(Debug, Default)]
pub(crate) struct HotkeyKeys(HashMap<i32, f32>);

impl Resource for HotkeyKeys {}

impl HotkeyKeys {
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

/// The frame's hotkey presses and releases, sent to the MCM Helper hotkeys.
pub(crate) fn mcm_hotkey_system(world: &World, dt: f32) {
    if world.try_resource::<MenuRegistry>().is_some_and(|registry| registry.paused()) {
        return;
    }
    let Some(now) = world.try_resource::<crate::components::InputState>().map(|input| {
        let keys = input.keys_held.iter().filter_map(|key| crate::interaction::papyrus_keycode(*key));
        let buttons = input.mouse_buttons_held.iter().filter_map(|button| crate::interaction::papyrus_mouse_keycode(*button));
        keys.chain(buttons).collect::<HashSet<i32>>()
    }) else {
        return;
    };
    let (downs, ups) = {
        let Some(mut keys) = world.try_resource_mut::<HotkeyKeys>() else { return };
        keys.advance(&now, dt)
    };
    if !downs.is_empty() || !ups.is_empty() {
        byroredux_scripting::mcm::helper::hotkey_events(world, &downs, &ups);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presses_and_releases_come_from_the_held_set_with_their_time() {
        let mut keys = HotkeyKeys::default();
        let held = |codes: &[i32]| codes.iter().copied().collect::<HashSet<i32>>();
        assert_eq!(keys.advance(&held(&[0x22, 256]), 0.016), (vec![0x22, 256], Vec::new()));
        assert_eq!(keys.advance(&held(&[0x22, 256]), 0.5), (Vec::new(), Vec::new()), "held, no event");
        let (downs, ups) = keys.advance(&held(&[256, 0x2A]), 0.25);
        assert_eq!(downs, [0x2A]);
        assert_eq!(ups.len(), 1);
        assert_eq!(ups[0].0, 0x22);
        assert!((ups[0].1 - 0.75).abs() < 1e-6, "held 0.5 + 0.25 s");
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
