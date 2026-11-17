//! M48 Phase 4 (#6666, Q1) — Skyrim script message boxes on the game's own movie (S1),
//! `interface\messagebox.swf` (`MessageBoxMenu`).
//!
//! Read off the movie's own code (`MessageBox`, the `_root.MessageMenu` instance), which
//! registers its native-code entry points as GameDelegate callbacks:
//!
//! - **Engine → movie** (through the path service when the front box changes):
//!   `setMessageText` → `SetMessage(aText, abHTML)`; `setButtons` → `setupButtons(abFocusFirst,
//!   label…)` (a label of `" "` is skipped; button *i* is the *i*-th label); `setIsCancellable`
//!   → `SetIsCancellable(aIsCancellable, aCancelOptionIndex)`. A box with no authored buttons gets
//!   the game's "Ok" (`sOk`), as on the native page; a script box is not cancellable.
//! - **Movie → engine** (H1, registered with the bridge): `buttonPress [index]` answers the box
//!   with that button (`answer_message_box`, the native page's path).
//!
//! The movie shows the message text only: Skyrim's message box has no title line.

use byroredux_native_ui::MessageBoxView;
use byroredux_ui::{PathOp, ScaleformHostCall, ScaleformValue, UiLayer, WhenAbsent};

/// The movie's message box clip.
const ROOT: &str = "_root.MessageMenu";

/// The host method this driver serves (H1).
pub(crate) const SERVED_METHODS: [&str; 1] = ["buttonPress"];

/// The open message box movie.
#[derive(Debug)]
pub(crate) struct MessageBoxMovie {
    /// The movie's layer in the `UiManager` stack.
    pub(crate) layer: String,
    /// The last box fed, so an unchanged box is not re-sent.
    signature: Option<u64>,
}

/// The button a drained host call answers with, if it is a `buttonPress`.
pub(crate) fn decode(call: &ScaleformHostCall) -> Option<usize> {
    match (call.method.as_str(), call.arguments.first()) {
        ("buttonPress", Some(ScaleformValue::Number(index))) if *index >= 0.0 => Some(*index as usize),
        _ => None,
    }
}

/// The calls that present `message` on the movie, in the order native code makes them; `ok` is
/// the label of the implicit button of a box that authors none.
pub(crate) fn feed_ops(message: &MessageBoxView, ok: &str) -> Vec<PathOp> {
    let call = |method: &str, args: Vec<ScaleformValue>| PathOp::Call(format!("{ROOT}.{method}"), args);
    let text = |value: &str| ScaleformValue::String(value.to_owned());
    let mut buttons = vec![ScaleformValue::Bool(true)];
    if message.buttons.is_empty() {
        buttons.push(text(ok));
    } else {
        buttons.extend(message.buttons.iter().map(|label| text(label)));
    }
    vec![
        call("SetMessage", vec![text(&message.text), ScaleformValue::Bool(false)]),
        call("SetIsCancellable", vec![ScaleformValue::Bool(false), ScaleformValue::Number(0.0)]),
        call("setupButtons", buttons),
    ]
}

fn signature(message: &MessageBoxView) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    message.title.hash(&mut hasher);
    message.text.hash(&mut hasher);
    message.buttons.hash(&mut hasher);
    hasher.finish()
}

impl MessageBoxMovie {
    pub(crate) fn new(layer: String) -> Self {
        Self { layer, signature: None }
    }

    /// Present `message` when it changed (the next box of a queue replaces the answered one).
    pub(crate) fn feed(&mut self, layer: &mut UiLayer, message: &MessageBoxView, ok: &str) {
        let signature = signature(message);
        if self.signature == Some(signature) {
            return;
        }
        self.signature = Some(signature);
        for op in feed_ops(message, ok) {
            layer.path_op(op, WhenAbsent::Queue);
        }
    }

    /// Forget the fed box, so the next feed re-sends even an identical one (a queue may hold the
    /// same box twice).
    pub(crate) fn answered(&mut self) {
        self.signature = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_ui::{HostClass, ScaleformHostDispatch, ScaleformProfile};

    #[test]
    fn the_feed_follows_the_movies_callback_shapes() {
        let text = |s: &str| ScaleformValue::String(s.into());
        let message = MessageBoxView { title: "Ignored".into(), text: "Wait here?".into(), buttons: vec!["Yes".into(), "No".into()] };
        let ops = feed_ops(&message, "Ok");
        assert_eq!(ops[0], PathOp::Call(format!("{ROOT}.SetMessage"), vec![text("Wait here?"), ScaleformValue::Bool(false)]));
        assert_eq!(ops[1], PathOp::Call(format!("{ROOT}.SetIsCancellable"), vec![ScaleformValue::Bool(false), ScaleformValue::Number(0.0)]));
        assert_eq!(ops[2], PathOp::Call(format!("{ROOT}.setupButtons"), vec![ScaleformValue::Bool(true), text("Yes"), text("No")]));
        let bare = MessageBoxView { buttons: Vec::new(), ..message };
        assert_eq!(feed_ops(&bare, "Ok")[2], PathOp::Call(format!("{ROOT}.setupButtons"), vec![ScaleformValue::Bool(true), text("Ok")]));
    }

    #[test]
    fn button_presses_decode_to_answers() {
        let call = |method: &str, arguments: Vec<ScaleformValue>| ScaleformHostCall {
            sequence: 0,
            profile: ScaleformProfile::SkyrimAvm1,
            transport_method: method.into(),
            method: method.into(),
            host_object: None,
            request_id: Some(1),
            arguments,
            dispatch: ScaleformHostDispatch::Queued,
            class: HostClass::Served,
        };
        assert_eq!(decode(&call("buttonPress", vec![ScaleformValue::Number(1.0)])), Some(1));
        assert_eq!(decode(&call("buttonPress", Vec::new())), None);
        assert_eq!(decode(&call("PlaySound", vec![ScaleformValue::String("UIMenuFocus".into())])), None);
        let catalog = byroredux_ui::ScaleformHostCatalog::for_profile(ScaleformProfile::SkyrimAvm1);
        for method in SERVED_METHODS {
            assert_eq!(catalog.behaviour(method).map(|b| b.target), Some(HostClass::Served), "{method}");
        }
    }
}
