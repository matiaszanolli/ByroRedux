//! winit → platform-neutral Scaleform input translation.
//!
//! The key mapping follows the pinned Ruffle desktop frontend. Keeping it at
//! this boundary prevents `byroredux-ui` from depending on a window system.

use byroredux_core::ecs::World;
use byroredux_ui::{
    UiImeEvent, UiInputEvent, UiKeyDescriptor, UiKeyLocation, UiLogicalKey, UiMouseButton,
    UiMouseWheelDelta, UiNamedKey, UiPhysicalKey, UiTextControlCode,
};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key, KeyCode, KeyLocation, ModifiersState, NamedKey, PhysicalKey};

use crate::components::InputState;

#[derive(Debug, Default)]
pub(crate) struct UiInputState {
    cursor_position: PhysicalPosition<f64>,
    modifiers: ModifiersState,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct UiWindowDispatch {
    pub(crate) captured: bool,
    pub(crate) mouse_in_stage: Option<bool>,
}

/// Release platform-facing world input when a modal UI owns the event.
///
/// Returns whether the cursor was captured so the window layer can mirror the
/// state change with the corresponding native cursor-grab operation.
pub(crate) fn release_world_input(world: &World) -> bool {
    let mut input = world.resource_mut::<InputState>();
    input.keys_held.clear();
    std::mem::replace(&mut input.mouse_captured, false)
}

/// Translate and dispatch one winit event.
///
/// The result identifies window input that belongs to a focused menu and
/// carries Ruffle's separate native-pointer stage state when it changes.
pub(crate) fn dispatch_window_event(
    event: &WindowEvent,
    state: &mut UiInputState,
    window_size: PhysicalSize<u32>,
    ui_size: (u32, u32),
    mut dispatch: impl FnMut(UiInputEvent),
) -> UiWindowDispatch {
    let mut mouse_in_stage = None;
    match event {
        WindowEvent::CursorMoved { position, .. } => {
            state.cursor_position = *position;
            let (x, y) = cursor_to_ui(*position, window_size, ui_size);
            dispatch(UiInputEvent::MouseMove { x, y });
        }
        WindowEvent::CursorEntered { .. } => mouse_in_stage = Some(true),
        WindowEvent::CursorLeft { .. } => {
            mouse_in_stage = Some(false);
            dispatch(UiInputEvent::MouseLeave);
        }
        WindowEvent::Focused(true) => dispatch(UiInputEvent::FocusGained),
        WindowEvent::Focused(false) => dispatch(UiInputEvent::FocusLost),
        WindowEvent::MouseInput {
            button,
            state: button_state,
            ..
        } => {
            let (x, y) = cursor_to_ui(state.cursor_position, window_size, ui_size);
            let button = mouse_button(*button);
            let event = match button_state {
                ElementState::Pressed => UiInputEvent::MouseDown { x, y, button },
                ElementState::Released => UiInputEvent::MouseUp { x, y, button },
            };
            dispatch(event);
        }
        WindowEvent::MouseWheel { delta, .. } => {
            let delta = match delta {
                MouseScrollDelta::LineDelta(_, y) => UiMouseWheelDelta::Lines((*y).into()),
                MouseScrollDelta::PixelDelta(position) => UiMouseWheelDelta::Pixels(position.y),
            };
            dispatch(UiInputEvent::MouseWheel { delta });
        }
        WindowEvent::ModifiersChanged(modifiers) => state.modifiers = modifiers.state(),
        WindowEvent::KeyboardInput { event, .. } => {
            let key = key_descriptor(event);
            match event.state {
                ElementState::Pressed => {
                    dispatch(UiInputEvent::KeyDown { key });
                    if let Some(code) = text_control(event.logical_key.as_ref(), state.modifiers) {
                        dispatch(UiInputEvent::TextControl { code });
                    } else if let Some(text) = event.text.as_ref() {
                        for codepoint in text.chars() {
                            dispatch(UiInputEvent::TextInput { codepoint });
                        }
                    }
                }
                ElementState::Released => dispatch(UiInputEvent::KeyUp { key }),
            }
        }
        WindowEvent::Ime(ime) => match ime {
            Ime::Enabled | Ime::Disabled => {}
            Ime::Preedit(text, cursor) => dispatch(UiInputEvent::Ime(UiImeEvent::Preedit(
                text.clone(),
                *cursor,
            ))),
            Ime::Commit(text) => dispatch(UiInputEvent::Ime(UiImeEvent::Commit(text.clone()))),
        },
        _ => return UiWindowDispatch::default(),
    }
    UiWindowDispatch {
        captured: true,
        mouse_in_stage,
    }
}

pub(crate) fn is_debug_overlay_key(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::KeyboardInput {
            event: KeyEvent {
                physical_key: PhysicalKey::Code(KeyCode::F3),
                ..
            },
            ..
        }
    )
}

fn cursor_to_ui(
    position: PhysicalPosition<f64>,
    window_size: PhysicalSize<u32>,
    ui_size: (u32, u32),
) -> (f64, f64) {
    if window_size.width == 0 || window_size.height == 0 {
        return (0.0, 0.0);
    }
    (
        position.x * f64::from(ui_size.0) / f64::from(window_size.width),
        position.y * f64::from(ui_size.1) / f64::from(window_size.height),
    )
}

fn mouse_button(button: MouseButton) -> UiMouseButton {
    match button {
        MouseButton::Left => UiMouseButton::Left,
        MouseButton::Right => UiMouseButton::Right,
        MouseButton::Middle => UiMouseButton::Middle,
        _ => UiMouseButton::Unknown,
    }
}

fn key_descriptor(event: &KeyEvent) -> UiKeyDescriptor {
    UiKeyDescriptor {
        physical_key: physical_key(event.physical_key),
        logical_key: logical_key(event.logical_key.as_ref()),
        key_location: key_location(event.location),
    }
}

fn text_control(logical_key: Key<&str>, modifiers: ModifiersState) -> Option<UiTextControlCode> {
    let shift = modifiers.shift_key();
    let ctrl_cmd = modifiers.control_key() || (modifiers.super_key() && cfg!(target_os = "macos"));
    match logical_key {
        Key::Named(NamedKey::Enter) => Some(UiTextControlCode::Enter),
        Key::Character("a") if ctrl_cmd => Some(UiTextControlCode::SelectAll),
        Key::Character("c") if ctrl_cmd => Some(UiTextControlCode::Copy),
        Key::Character("v") if ctrl_cmd => Some(UiTextControlCode::Paste),
        Key::Character("x") if ctrl_cmd => Some(UiTextControlCode::Cut),
        Key::Named(NamedKey::Backspace) if ctrl_cmd => Some(UiTextControlCode::BackspaceWord),
        Key::Named(NamedKey::Backspace) => Some(UiTextControlCode::Backspace),
        Key::Named(NamedKey::Delete) if ctrl_cmd => Some(UiTextControlCode::DeleteWord),
        Key::Named(NamedKey::Delete) => Some(UiTextControlCode::Delete),
        Key::Named(NamedKey::ArrowLeft) if ctrl_cmd && shift => {
            Some(UiTextControlCode::SelectLeftWord)
        }
        Key::Named(NamedKey::ArrowLeft) if ctrl_cmd => Some(UiTextControlCode::MoveLeftWord),
        Key::Named(NamedKey::ArrowLeft) if shift => Some(UiTextControlCode::SelectLeft),
        Key::Named(NamedKey::ArrowLeft) => Some(UiTextControlCode::MoveLeft),
        Key::Named(NamedKey::ArrowRight) if ctrl_cmd && shift => {
            Some(UiTextControlCode::SelectRightWord)
        }
        Key::Named(NamedKey::ArrowRight) if ctrl_cmd => Some(UiTextControlCode::MoveRightWord),
        Key::Named(NamedKey::ArrowRight) if shift => Some(UiTextControlCode::SelectRight),
        Key::Named(NamedKey::ArrowRight) => Some(UiTextControlCode::MoveRight),
        Key::Named(NamedKey::Home) if ctrl_cmd && shift => {
            Some(UiTextControlCode::SelectLeftDocument)
        }
        Key::Named(NamedKey::Home) if ctrl_cmd => Some(UiTextControlCode::MoveLeftDocument),
        Key::Named(NamedKey::Home) if shift => Some(UiTextControlCode::SelectLeftLine),
        Key::Named(NamedKey::Home) => Some(UiTextControlCode::MoveLeftLine),
        Key::Named(NamedKey::End) if ctrl_cmd && shift => {
            Some(UiTextControlCode::SelectRightDocument)
        }
        Key::Named(NamedKey::End) if ctrl_cmd => Some(UiTextControlCode::MoveRightDocument),
        Key::Named(NamedKey::End) if shift => Some(UiTextControlCode::SelectRightLine),
        Key::Named(NamedKey::End) => Some(UiTextControlCode::MoveRightLine),
        _ => None,
    }
}

fn physical_key(key: PhysicalKey) -> UiPhysicalKey {
    match key {
        PhysicalKey::Code(code) => match code {
            KeyCode::Backquote => UiPhysicalKey::Backquote,
            KeyCode::Backslash => UiPhysicalKey::Backslash,
            KeyCode::BracketLeft => UiPhysicalKey::BracketLeft,
            KeyCode::BracketRight => UiPhysicalKey::BracketRight,
            KeyCode::Comma => UiPhysicalKey::Comma,
            KeyCode::Digit0 => UiPhysicalKey::Digit0,
            KeyCode::Digit1 => UiPhysicalKey::Digit1,
            KeyCode::Digit2 => UiPhysicalKey::Digit2,
            KeyCode::Digit3 => UiPhysicalKey::Digit3,
            KeyCode::Digit4 => UiPhysicalKey::Digit4,
            KeyCode::Digit5 => UiPhysicalKey::Digit5,
            KeyCode::Digit6 => UiPhysicalKey::Digit6,
            KeyCode::Digit7 => UiPhysicalKey::Digit7,
            KeyCode::Digit8 => UiPhysicalKey::Digit8,
            KeyCode::Digit9 => UiPhysicalKey::Digit9,
            KeyCode::Equal => UiPhysicalKey::Equal,
            KeyCode::IntlBackslash => UiPhysicalKey::IntlBackslash,
            KeyCode::IntlRo => UiPhysicalKey::IntlRo,
            KeyCode::IntlYen => UiPhysicalKey::IntlYen,
            KeyCode::KeyA => UiPhysicalKey::KeyA,
            KeyCode::KeyB => UiPhysicalKey::KeyB,
            KeyCode::KeyC => UiPhysicalKey::KeyC,
            KeyCode::KeyD => UiPhysicalKey::KeyD,
            KeyCode::KeyE => UiPhysicalKey::KeyE,
            KeyCode::KeyF => UiPhysicalKey::KeyF,
            KeyCode::KeyG => UiPhysicalKey::KeyG,
            KeyCode::KeyH => UiPhysicalKey::KeyH,
            KeyCode::KeyI => UiPhysicalKey::KeyI,
            KeyCode::KeyJ => UiPhysicalKey::KeyJ,
            KeyCode::KeyK => UiPhysicalKey::KeyK,
            KeyCode::KeyL => UiPhysicalKey::KeyL,
            KeyCode::KeyM => UiPhysicalKey::KeyM,
            KeyCode::KeyN => UiPhysicalKey::KeyN,
            KeyCode::KeyO => UiPhysicalKey::KeyO,
            KeyCode::KeyP => UiPhysicalKey::KeyP,
            KeyCode::KeyQ => UiPhysicalKey::KeyQ,
            KeyCode::KeyR => UiPhysicalKey::KeyR,
            KeyCode::KeyS => UiPhysicalKey::KeyS,
            KeyCode::KeyT => UiPhysicalKey::KeyT,
            KeyCode::KeyU => UiPhysicalKey::KeyU,
            KeyCode::KeyV => UiPhysicalKey::KeyV,
            KeyCode::KeyW => UiPhysicalKey::KeyW,
            KeyCode::KeyX => UiPhysicalKey::KeyX,
            KeyCode::KeyY => UiPhysicalKey::KeyY,
            KeyCode::KeyZ => UiPhysicalKey::KeyZ,
            KeyCode::Minus => UiPhysicalKey::Minus,
            KeyCode::Period => UiPhysicalKey::Period,
            KeyCode::Quote => UiPhysicalKey::Quote,
            KeyCode::Semicolon => UiPhysicalKey::Semicolon,
            KeyCode::Slash => UiPhysicalKey::Slash,
            KeyCode::AltLeft => UiPhysicalKey::AltLeft,
            KeyCode::AltRight => UiPhysicalKey::AltRight,
            KeyCode::Backspace => UiPhysicalKey::Backspace,
            KeyCode::CapsLock => UiPhysicalKey::CapsLock,
            KeyCode::ContextMenu => UiPhysicalKey::ContextMenu,
            KeyCode::ControlLeft => UiPhysicalKey::ControlLeft,
            KeyCode::ControlRight => UiPhysicalKey::ControlRight,
            KeyCode::Enter => UiPhysicalKey::Enter,
            KeyCode::SuperLeft => UiPhysicalKey::SuperLeft,
            KeyCode::SuperRight => UiPhysicalKey::SuperRight,
            KeyCode::ShiftLeft => UiPhysicalKey::ShiftLeft,
            KeyCode::ShiftRight => UiPhysicalKey::ShiftRight,
            KeyCode::Space => UiPhysicalKey::Space,
            KeyCode::Tab => UiPhysicalKey::Tab,
            KeyCode::Delete => UiPhysicalKey::Delete,
            KeyCode::End => UiPhysicalKey::End,
            KeyCode::Home => UiPhysicalKey::Home,
            KeyCode::Insert => UiPhysicalKey::Insert,
            KeyCode::PageDown => UiPhysicalKey::PageDown,
            KeyCode::PageUp => UiPhysicalKey::PageUp,
            KeyCode::ArrowDown => UiPhysicalKey::ArrowDown,
            KeyCode::ArrowLeft => UiPhysicalKey::ArrowLeft,
            KeyCode::ArrowRight => UiPhysicalKey::ArrowRight,
            KeyCode::ArrowUp => UiPhysicalKey::ArrowUp,
            KeyCode::NumLock => UiPhysicalKey::NumLock,
            KeyCode::Numpad0 => UiPhysicalKey::Numpad0,
            KeyCode::Numpad1 => UiPhysicalKey::Numpad1,
            KeyCode::Numpad2 => UiPhysicalKey::Numpad2,
            KeyCode::Numpad3 => UiPhysicalKey::Numpad3,
            KeyCode::Numpad4 => UiPhysicalKey::Numpad4,
            KeyCode::Numpad5 => UiPhysicalKey::Numpad5,
            KeyCode::Numpad6 => UiPhysicalKey::Numpad6,
            KeyCode::Numpad7 => UiPhysicalKey::Numpad7,
            KeyCode::Numpad8 => UiPhysicalKey::Numpad8,
            KeyCode::Numpad9 => UiPhysicalKey::Numpad9,
            KeyCode::NumpadAdd => UiPhysicalKey::NumpadAdd,
            KeyCode::NumpadComma => UiPhysicalKey::NumpadComma,
            KeyCode::NumpadDecimal => UiPhysicalKey::NumpadDecimal,
            KeyCode::NumpadDivide => UiPhysicalKey::NumpadDivide,
            KeyCode::NumpadEnter => UiPhysicalKey::NumpadEnter,
            KeyCode::NumpadMultiply => UiPhysicalKey::NumpadMultiply,
            KeyCode::NumpadSubtract => UiPhysicalKey::NumpadSubtract,
            KeyCode::Escape => UiPhysicalKey::Escape,
            KeyCode::Fn => UiPhysicalKey::Fn,
            KeyCode::FnLock => UiPhysicalKey::FnLock,
            KeyCode::PrintScreen => UiPhysicalKey::PrintScreen,
            KeyCode::ScrollLock => UiPhysicalKey::ScrollLock,
            KeyCode::Pause => UiPhysicalKey::Pause,
            KeyCode::F1 => UiPhysicalKey::F1,
            KeyCode::F2 => UiPhysicalKey::F2,
            KeyCode::F3 => UiPhysicalKey::F3,
            KeyCode::F4 => UiPhysicalKey::F4,
            KeyCode::F5 => UiPhysicalKey::F5,
            KeyCode::F6 => UiPhysicalKey::F6,
            KeyCode::F7 => UiPhysicalKey::F7,
            KeyCode::F8 => UiPhysicalKey::F8,
            KeyCode::F9 => UiPhysicalKey::F9,
            KeyCode::F10 => UiPhysicalKey::F10,
            KeyCode::F11 => UiPhysicalKey::F11,
            KeyCode::F12 => UiPhysicalKey::F12,
            KeyCode::F13 => UiPhysicalKey::F13,
            KeyCode::F14 => UiPhysicalKey::F14,
            KeyCode::F15 => UiPhysicalKey::F15,
            KeyCode::F16 => UiPhysicalKey::F16,
            KeyCode::F17 => UiPhysicalKey::F17,
            KeyCode::F18 => UiPhysicalKey::F18,
            KeyCode::F19 => UiPhysicalKey::F19,
            KeyCode::F20 => UiPhysicalKey::F20,
            KeyCode::F21 => UiPhysicalKey::F21,
            KeyCode::F22 => UiPhysicalKey::F22,
            KeyCode::F23 => UiPhysicalKey::F23,
            KeyCode::F24 => UiPhysicalKey::F24,
            KeyCode::F25 => UiPhysicalKey::F25,
            KeyCode::F26 => UiPhysicalKey::F26,
            KeyCode::F27 => UiPhysicalKey::F27,
            KeyCode::F28 => UiPhysicalKey::F28,
            KeyCode::F29 => UiPhysicalKey::F29,
            KeyCode::F30 => UiPhysicalKey::F30,
            KeyCode::F31 => UiPhysicalKey::F31,
            KeyCode::F32 => UiPhysicalKey::F32,
            KeyCode::F33 => UiPhysicalKey::F33,
            KeyCode::F34 => UiPhysicalKey::F34,
            KeyCode::F35 => UiPhysicalKey::F35,
            _ => UiPhysicalKey::Unknown,
        },
        PhysicalKey::Unidentified(_) => UiPhysicalKey::Unknown,
    }
}

fn logical_key(key: Key<&str>) -> UiLogicalKey {
    match key {
        Key::Named(NamedKey::Alt) => UiLogicalKey::Named(UiNamedKey::Alt),
        Key::Named(NamedKey::AltGraph) => UiLogicalKey::Named(UiNamedKey::AltGraph),
        Key::Named(NamedKey::CapsLock) => UiLogicalKey::Named(UiNamedKey::CapsLock),
        Key::Named(NamedKey::Control) => UiLogicalKey::Named(UiNamedKey::Control),
        Key::Named(NamedKey::Fn) => UiLogicalKey::Named(UiNamedKey::Fn),
        Key::Named(NamedKey::FnLock) => UiLogicalKey::Named(UiNamedKey::FnLock),
        Key::Named(NamedKey::NumLock) => UiLogicalKey::Named(UiNamedKey::NumLock),
        Key::Named(NamedKey::ScrollLock) => UiLogicalKey::Named(UiNamedKey::ScrollLock),
        Key::Named(NamedKey::Shift) => UiLogicalKey::Named(UiNamedKey::Shift),
        Key::Named(NamedKey::Symbol) => UiLogicalKey::Named(UiNamedKey::Symbol),
        Key::Named(NamedKey::SymbolLock) => UiLogicalKey::Named(UiNamedKey::SymbolLock),
        Key::Named(NamedKey::Super) => UiLogicalKey::Named(UiNamedKey::Super),
        Key::Named(NamedKey::Enter) => UiLogicalKey::Named(UiNamedKey::Enter),
        Key::Named(NamedKey::Tab) => UiLogicalKey::Named(UiNamedKey::Tab),
        Key::Named(NamedKey::Space) => UiLogicalKey::Character(' '),
        Key::Named(NamedKey::ArrowDown) => UiLogicalKey::Named(UiNamedKey::ArrowDown),
        Key::Named(NamedKey::ArrowLeft) => UiLogicalKey::Named(UiNamedKey::ArrowLeft),
        Key::Named(NamedKey::ArrowRight) => UiLogicalKey::Named(UiNamedKey::ArrowRight),
        Key::Named(NamedKey::ArrowUp) => UiLogicalKey::Named(UiNamedKey::ArrowUp),
        Key::Named(NamedKey::End) => UiLogicalKey::Named(UiNamedKey::End),
        Key::Named(NamedKey::Home) => UiLogicalKey::Named(UiNamedKey::Home),
        Key::Named(NamedKey::PageDown) => UiLogicalKey::Named(UiNamedKey::PageDown),
        Key::Named(NamedKey::PageUp) => UiLogicalKey::Named(UiNamedKey::PageUp),
        Key::Named(NamedKey::Backspace) => UiLogicalKey::Named(UiNamedKey::Backspace),
        Key::Named(NamedKey::Clear) => UiLogicalKey::Named(UiNamedKey::Clear),
        Key::Named(NamedKey::Copy) => UiLogicalKey::Named(UiNamedKey::Copy),
        Key::Named(NamedKey::CrSel) => UiLogicalKey::Named(UiNamedKey::CrSel),
        Key::Named(NamedKey::Cut) => UiLogicalKey::Named(UiNamedKey::Cut),
        Key::Named(NamedKey::Delete) => UiLogicalKey::Named(UiNamedKey::Delete),
        Key::Named(NamedKey::EraseEof) => UiLogicalKey::Named(UiNamedKey::EraseEof),
        Key::Named(NamedKey::ExSel) => UiLogicalKey::Named(UiNamedKey::ExSel),
        Key::Named(NamedKey::Insert) => UiLogicalKey::Named(UiNamedKey::Insert),
        Key::Named(NamedKey::Paste) => UiLogicalKey::Named(UiNamedKey::Paste),
        Key::Named(NamedKey::Redo) => UiLogicalKey::Named(UiNamedKey::Redo),
        Key::Named(NamedKey::Undo) => UiLogicalKey::Named(UiNamedKey::Undo),
        Key::Named(NamedKey::ContextMenu) => UiLogicalKey::Named(UiNamedKey::ContextMenu),
        Key::Named(NamedKey::Escape) => UiLogicalKey::Named(UiNamedKey::Escape),
        Key::Named(NamedKey::Pause) => UiLogicalKey::Named(UiNamedKey::Pause),
        Key::Named(NamedKey::Play) => UiLogicalKey::Named(UiNamedKey::Play),
        Key::Named(NamedKey::Select) => UiLogicalKey::Named(UiNamedKey::Select),
        Key::Named(NamedKey::ZoomIn) => UiLogicalKey::Named(UiNamedKey::ZoomIn),
        Key::Named(NamedKey::ZoomOut) => UiLogicalKey::Named(UiNamedKey::ZoomOut),
        Key::Named(NamedKey::PrintScreen) => UiLogicalKey::Named(UiNamedKey::PrintScreen),
        Key::Named(NamedKey::F1) => UiLogicalKey::Named(UiNamedKey::F1),
        Key::Named(NamedKey::F2) => UiLogicalKey::Named(UiNamedKey::F2),
        Key::Named(NamedKey::F3) => UiLogicalKey::Named(UiNamedKey::F3),
        Key::Named(NamedKey::F4) => UiLogicalKey::Named(UiNamedKey::F4),
        Key::Named(NamedKey::F5) => UiLogicalKey::Named(UiNamedKey::F5),
        Key::Named(NamedKey::F6) => UiLogicalKey::Named(UiNamedKey::F6),
        Key::Named(NamedKey::F7) => UiLogicalKey::Named(UiNamedKey::F7),
        Key::Named(NamedKey::F8) => UiLogicalKey::Named(UiNamedKey::F8),
        Key::Named(NamedKey::F9) => UiLogicalKey::Named(UiNamedKey::F9),
        Key::Named(NamedKey::F10) => UiLogicalKey::Named(UiNamedKey::F10),
        Key::Named(NamedKey::F11) => UiLogicalKey::Named(UiNamedKey::F11),
        Key::Named(NamedKey::F12) => UiLogicalKey::Named(UiNamedKey::F12),
        Key::Named(NamedKey::F13) => UiLogicalKey::Named(UiNamedKey::F13),
        Key::Named(NamedKey::F14) => UiLogicalKey::Named(UiNamedKey::F14),
        Key::Named(NamedKey::F15) => UiLogicalKey::Named(UiNamedKey::F15),
        Key::Named(NamedKey::F16) => UiLogicalKey::Named(UiNamedKey::F16),
        Key::Named(NamedKey::F17) => UiLogicalKey::Named(UiNamedKey::F17),
        Key::Named(NamedKey::F18) => UiLogicalKey::Named(UiNamedKey::F18),
        Key::Named(NamedKey::F19) => UiLogicalKey::Named(UiNamedKey::F19),
        Key::Named(NamedKey::F20) => UiLogicalKey::Named(UiNamedKey::F20),
        Key::Named(NamedKey::F21) => UiLogicalKey::Named(UiNamedKey::F21),
        Key::Named(NamedKey::F22) => UiLogicalKey::Named(UiNamedKey::F22),
        Key::Named(NamedKey::F23) => UiLogicalKey::Named(UiNamedKey::F23),
        Key::Named(NamedKey::F24) => UiLogicalKey::Named(UiNamedKey::F24),
        Key::Named(NamedKey::F25) => UiLogicalKey::Named(UiNamedKey::F25),
        Key::Named(NamedKey::F26) => UiLogicalKey::Named(UiNamedKey::F26),
        Key::Named(NamedKey::F27) => UiLogicalKey::Named(UiNamedKey::F27),
        Key::Named(NamedKey::F28) => UiLogicalKey::Named(UiNamedKey::F28),
        Key::Named(NamedKey::F29) => UiLogicalKey::Named(UiNamedKey::F29),
        Key::Named(NamedKey::F30) => UiLogicalKey::Named(UiNamedKey::F30),
        Key::Named(NamedKey::F31) => UiLogicalKey::Named(UiNamedKey::F31),
        Key::Named(NamedKey::F32) => UiLogicalKey::Named(UiNamedKey::F32),
        Key::Named(NamedKey::F33) => UiLogicalKey::Named(UiNamedKey::F33),
        Key::Named(NamedKey::F34) => UiLogicalKey::Named(UiNamedKey::F34),
        Key::Named(NamedKey::F35) => UiLogicalKey::Named(UiNamedKey::F35),
        Key::Character(characters) => characters
            .chars()
            .last()
            .map(UiLogicalKey::Character)
            .unwrap_or(UiLogicalKey::Unknown),
        _ => UiLogicalKey::Unknown,
    }
}

fn key_location(location: KeyLocation) -> UiKeyLocation {
    match location {
        KeyLocation::Standard => UiKeyLocation::Standard,
        KeyLocation::Left => UiKeyLocation::Left,
        KeyLocation::Right => UiKeyLocation::Right,
        KeyLocation::Numpad => UiKeyLocation::Numpad,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_focus_transfer_clears_world_keys_and_mouse_capture() {
        let mut world = World::new();
        let mut input = InputState::default();
        input.keys_held.extend([KeyCode::KeyW, KeyCode::KeyE]);
        input.mouse_captured = true;
        world.insert_resource(input);

        assert!(release_world_input(&world));
        let input = world.resource::<InputState>();
        assert!(input.keys_held.is_empty());
        assert!(!input.mouse_captured);
    }

    #[test]
    fn cursor_coordinates_scale_to_the_persistent_movie_viewport() {
        assert_eq!(
            cursor_to_ui(
                PhysicalPosition::new(640.0, 360.0),
                PhysicalSize::new(1280, 720),
                (1920, 1080),
            ),
            (960.0, 540.0)
        );
        assert_eq!(
            cursor_to_ui(
                PhysicalPosition::new(10.0, 10.0),
                PhysicalSize::new(0, 0),
                (1920, 1080),
            ),
            (0.0, 0.0)
        );
    }

    #[test]
    fn key_mapping_preserves_physical_logical_and_numpad_identity() {
        assert_eq!(
            physical_key(PhysicalKey::Code(KeyCode::KeyA)),
            UiPhysicalKey::KeyA
        );
        assert_eq!(
            logical_key(Key::Character("å")),
            UiLogicalKey::Character('å')
        );
        assert_eq!(key_location(KeyLocation::Numpad), UiKeyLocation::Numpad);
    }

    #[test]
    fn text_controls_respect_selection_and_word_modifiers() {
        assert_eq!(
            text_control(
                Key::Named(NamedKey::ArrowLeft),
                ModifiersState::SHIFT | ModifiersState::CONTROL,
            ),
            Some(UiTextControlCode::SelectLeftWord)
        );
        assert_eq!(
            text_control(Key::Character("a"), ModifiersState::CONTROL),
            Some(UiTextControlCode::SelectAll)
        );
    }
}
