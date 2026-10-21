//! Transient player feedback, independent of debug-panel visibility.
use byroredux_core::ecs::{Resource, World};
use std::collections::VecDeque;

const MAX_PENDING: usize = 8;

/// Script message boxes that may wait for the player at once. A box drains
/// only when the player answers it, and the text comes from mod scripts, so
/// an unconditional `MessageBox` in a polled quest block would otherwise
/// queue one (up to 64 KB) per tick without bound (#6012).
const MAX_PENDING_BOXES: usize = 16;

#[derive(Default)]
pub(crate) struct PlayerNotifications(VecDeque<String>);
impl Resource for PlayerNotifications {}

pub(crate) fn push(world: &World, message: impl Into<String>) {
    let Some(mut queue) = world.try_resource_mut::<PlayerNotifications>() else {
        return;
    };
    if queue.0.len() == MAX_PENDING {
        queue.0.pop_front();
    }
    queue.0.push_back(message.into());
}

pub(crate) fn drain(world: &World) -> Vec<String> {
    world
        .try_resource_mut::<PlayerNotifications>()
        .map(|mut queue| queue.0.drain(..).collect())
        .unwrap_or_default()
}

/// M47.2 — script message boxes waiting for the player's answer, shown one
/// at a time on the native message-box page. `NOT_SAVED_BY_DESIGN`: like a
/// notification, a box is presentation of a fragment that already ran.
#[derive(Default)]
pub(crate) struct MessageBoxQueue(pub(crate) VecDeque<byroredux_scripting::ScriptMessage>);
impl Resource for MessageBoxQueue {}

/// Route this frame's script messages: boxes queue for the message-box page,
/// everything else is a notification. Returns whether a box is waiting.
pub(crate) fn route_script_messages(world: &World) -> bool {
    let raised = world
        .try_resource_mut::<byroredux_scripting::PendingScriptMessages>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    for message in raised {
        if message.message_box {
            if let Some(mut boxes) = world.try_resource_mut::<MessageBoxQueue>() {
                // Refuse rather than drop the oldest: the front box is the one
                // the player is reading and the one whose answer is awaited.
                if boxes.0.len() >= MAX_PENDING_BOXES {
                    log::warn!(
                        "script message box dropped, {MAX_PENDING_BOXES} already waiting: {:?}",
                        message.text.chars().take(80).collect::<String>()
                    );
                } else {
                    boxes.0.push_back(message);
                }
            }
        } else {
            push(world, message.text);
        }
    }
    world
        .try_resource::<MessageBoxQueue>()
        .is_some_and(|boxes| !boxes.0.is_empty())
}

/// The box the message-box page shows.
pub(crate) fn front_message_box(world: &World) -> Option<byroredux_debug_ui::MessageBoxView> {
    let boxes = world.try_resource::<MessageBoxQueue>()?;
    let message = boxes.0.front()?;
    Some(byroredux_debug_ui::MessageBoxView {
        title: message.title.clone(),
        text: message.text.clone(),
        buttons: message.buttons.clone(),
    })
}

/// The front box is answered with `choice` (`None`: dismissed without one,
/// as with no UI); an ObScript box's answer reaches the first subsequent
/// `GetButtonPressed` call, regardless of its quest. Returns whether another
/// box is waiting.
pub(crate) fn answer_message_box(world: &World, choice: Option<usize>) -> bool {
    let (answered, more) = {
        let Some(mut boxes) = world.try_resource_mut::<MessageBoxQueue>() else {
            return false;
        };
        let answered = boxes.0.pop_front();
        (answered, !boxes.0.is_empty())
    };
    if let (Some(answered), Some(choice)) = (answered, choice) {
        if answered.source.is_some() {
            byroredux_scripting::messages::record_button(world, choice);
        }
    }
    more
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feedback_is_bounded_ordered_and_consumed_once() {
        let mut world = World::new();
        world.insert_resource(PlayerNotifications::default());
        for i in 0..12 {
            push(&world, format!("Message {i}"));
        }
        assert_eq!(
            drain(&world),
            (4..12).map(|i| format!("Message {i}")).collect::<Vec<_>>()
        );
        assert!(drain(&world).is_empty());
    }

    #[test]
    fn script_messages_split_into_notifications_and_boxes() {
        use byroredux_scripting::{PendingScriptMessages, ScriptMessage};
        let mut world = World::new();
        world.insert_resource(PlayerNotifications::default());
        world.insert_resource(MessageBoxQueue::default());
        let message = |text: &str, message_box: bool| ScriptMessage {
            title: String::new(),
            text: text.to_owned(),
            message_box,
            buttons: Vec::new(),
            source: None,
        };
        world.insert_resource(PendingScriptMessages(vec![
            message("Current Bounty: 40", false),
            message("Rest here?", true),
            message("Leave?", true),
        ]));
        assert!(route_script_messages(&world));
        assert_eq!(drain(&world), vec!["Current Bounty: 40".to_owned()]);
        assert_eq!(front_message_box(&world).unwrap().text, "Rest here?");
        assert!(answer_message_box(&world, Some(0)), "a second box waits");
        assert_eq!(front_message_box(&world).unwrap().text, "Leave?");
        assert!(!answer_message_box(&world, Some(0)));
        assert!(!route_script_messages(&world), "raised once");
    }

    #[test]
    fn message_box_queue_is_bounded_and_keeps_the_oldest() {
        use byroredux_scripting::{PendingScriptMessages, ScriptMessage};
        let mut world = World::new();
        world.insert_resource(PlayerNotifications::default());
        world.insert_resource(MessageBoxQueue::default());
        world.insert_resource(PendingScriptMessages(
            (0..MAX_PENDING_BOXES + 10)
                .map(|i| ScriptMessage {
                    title: String::new(),
                    text: format!("Box {i}"),
                    message_box: true,
                    buttons: Vec::new(),
                    source: None,
                })
                .collect(),
        ));
        assert!(route_script_messages(&world));
        assert_eq!(world.resource::<MessageBoxQueue>().0.len(), MAX_PENDING_BOXES);
        assert_eq!(front_message_box(&world).unwrap().text, "Box 0");
        // Answering frees room for a later box.
        answer_message_box(&world, None);
        world.insert_resource(PendingScriptMessages(vec![ScriptMessage {
            title: String::new(),
            text: "Late".to_owned(),
            message_box: true,
            buttons: Vec::new(),
            source: None,
        }]));
        route_script_messages(&world);
        assert_eq!(
            world.resource::<MessageBoxQueue>().0.back().unwrap().text,
            "Late"
        );
    }

    #[test]
    fn obscript_button_answer_is_consumed_globally_once() {
        use byroredux_scripting::messages::{take_button, ButtonPresses};
        use byroredux_scripting::{quest_stages::QuestFormId, ScriptMessage};

        let mut world = World::new();
        world.insert_resource(ButtonPresses::default());
        world.insert_resource(MessageBoxQueue(VecDeque::from([ScriptMessage {
            title: String::new(),
            text: "Continue?".to_owned(),
            message_box: true,
            buttons: vec!["Yes".to_owned(), "No".to_owned()],
            source: Some(QuestFormId(0x0001_C3D0)),
        }])));

        assert!(!answer_message_box(&world, Some(1)));
        assert_eq!(take_button(&world), 1);
        assert_eq!(take_button(&world), -1);
    }
}
