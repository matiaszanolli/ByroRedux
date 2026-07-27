//! Platform-neutral input contract for Scaleform movies.
//!
//! Window-system events are translated into these values by the executable.
//! Keeping winit out of this crate lets archive/menu tests drive the same
//! AVM1 and AVM2 event path without constructing a native window.

use ruffle_core::events::{ImeEvent, MouseButton, MouseWheelDelta, PlayerEvent};

pub use ruffle_core::events::{
    KeyDescriptor as UiKeyDescriptor, KeyLocation as UiKeyLocation,
    LogicalKey as UiLogicalKey, NamedKey as UiNamedKey, PhysicalKey as UiPhysicalKey,
    TextControlCode as UiTextControlCode,
};

/// Mouse buttons understood by Bethesda Scaleform menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMouseButton {
    Unknown,
    Left,
    Right,
    Middle,
}

/// Scroll distance in the source window system's native unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UiMouseWheelDelta {
    Lines(f64),
    Pixels(f64),
}

/// Input-method-editor composition state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiImeEvent {
    Preedit(String, Option<(usize, usize)>),
    Commit(String),
}

/// Input accepted by an active Scaleform menu.
#[derive(Debug, Clone, PartialEq)]
pub enum UiInputEvent {
    KeyDown {
        key: UiKeyDescriptor,
    },
    KeyUp {
        key: UiKeyDescriptor,
    },
    MouseMove {
        x: f64,
        y: f64,
    },
    MouseUp {
        x: f64,
        y: f64,
        button: UiMouseButton,
    },
    MouseDown {
        x: f64,
        y: f64,
        button: UiMouseButton,
    },
    MouseLeave,
    MouseWheel {
        delta: UiMouseWheelDelta,
    },
    TextInput {
        codepoint: char,
    },
    TextControl {
        code: UiTextControlCode,
    },
    Ime(UiImeEvent),
    FocusGained,
    FocusLost,
}

impl From<UiMouseButton> for MouseButton {
    fn from(button: UiMouseButton) -> Self {
        match button {
            UiMouseButton::Unknown => Self::Unknown,
            UiMouseButton::Left => Self::Left,
            UiMouseButton::Right => Self::Right,
            UiMouseButton::Middle => Self::Middle,
        }
    }
}

impl From<UiMouseWheelDelta> for MouseWheelDelta {
    fn from(delta: UiMouseWheelDelta) -> Self {
        match delta {
            UiMouseWheelDelta::Lines(delta) => Self::Lines(delta),
            UiMouseWheelDelta::Pixels(delta) => Self::Pixels(delta),
        }
    }
}

impl From<UiImeEvent> for ImeEvent {
    fn from(event: UiImeEvent) -> Self {
        match event {
            UiImeEvent::Preedit(text, cursor) => Self::Preedit(text, cursor),
            UiImeEvent::Commit(text) => Self::Commit(text),
        }
    }
}

impl From<UiInputEvent> for PlayerEvent {
    fn from(event: UiInputEvent) -> Self {
        match event {
            UiInputEvent::KeyDown { key } => Self::KeyDown { key },
            UiInputEvent::KeyUp { key } => Self::KeyUp { key },
            UiInputEvent::MouseMove { x, y } => Self::MouseMove { x, y },
            UiInputEvent::MouseUp { x, y, button } => Self::MouseUp {
                x,
                y,
                button: button.into(),
            },
            UiInputEvent::MouseDown { x, y, button } => Self::MouseDown {
                x,
                y,
                button: button.into(),
                // winit does not currently expose click-count information.
                index: None,
            },
            UiInputEvent::MouseLeave => Self::MouseLeave,
            UiInputEvent::MouseWheel { delta } => Self::MouseWheel {
                delta: delta.into(),
            },
            UiInputEvent::TextInput { codepoint } => Self::TextInput { codepoint },
            UiInputEvent::TextControl { code } => Self::TextControl { code },
            UiInputEvent::Ime(event) => Self::Ime(event.into()),
            UiInputEvent::FocusGained => Self::FocusGained,
            UiInputEvent::FocusLost => Self::FocusLost,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_down_preserves_position_and_uses_unknown_click_index() {
        let event = PlayerEvent::from(UiInputEvent::MouseDown {
            x: 12.5,
            y: 24.0,
            button: UiMouseButton::Left,
        });

        assert!(matches!(
            event,
            PlayerEvent::MouseDown {
                x: 12.5,
                y: 24.0,
                button: MouseButton::Left,
                index: None,
            }
        ));
    }

    #[test]
    fn ime_preedit_preserves_selection_range() {
        let event = PlayerEvent::from(UiInputEvent::Ime(UiImeEvent::Preedit(
            "Dovah".to_string(),
            Some((2, 4)),
        )));

        assert!(matches!(
            event,
            PlayerEvent::Ime(ImeEvent::Preedit(text, Some((2, 4)))) if text == "Dovah"
        ));
    }
}
