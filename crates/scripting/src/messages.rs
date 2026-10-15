//! Messages and text replacement (M47.2).
//!
//! The CK *Message* page: a `MESG` is shown as a **message box** ("a dialog
//! box that pauses the game and waits for a response") when its "Message Box"
//! flag is set, otherwise as a notification ("briefly displayed at the top
//! left of the screen"). *Show - Message* substitutes "the passed-in numbers
//! into the appropriate spots in the text" — `printf` conversions such as
//! `%.0f` (Skyrim.esm: "Current Bounty: %.0f", with `%%` for a literal
//! percent).
//!
//! *Text Replacement* tags are filled from the message's owning quest:
//! `<Alias=Name>` is "the full name of the ref filling alias Name" (`[...]`
//! when the alias is unfilled; `<Alias=Player>` needs no alias),
//! `<Alias.ShortName=Name>` falls back to the full name, and `<Global=Name>`
//! is the global's value ("Float globals are displayed to two decimal
//! places"). "If a tag fails to parse for any reason … the unparsed tag is
//! displayed in the text instead", which is what every other tag does here.

use std::collections::HashMap;

use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::records::{EsmIndex, SettingValue};

use crate::quest_stages::QuestFormId;

/// One `MESG` as the runtime shows it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MessageDefinition {
    pub title: String,
    pub text: String,
    pub message_box: bool,
    pub buttons: Vec<String>,
    /// The owning quest text replacement reads, `None` for none.
    pub owner_quest: Option<QuestFormId>,
}

/// The load order's messages, global display names and global editor IDs.
/// `NOT_SAVED_BY_DESIGN`: rebuilt from the records every boot.
#[derive(Debug, Default, Clone)]
pub struct MessageCatalog {
    messages: HashMap<u32, MessageDefinition>,
    /// Base / location FormID → `FULL` name.
    display_names: HashMap<u32, String>,
    /// Lowercased `GLOB` editor ID → (FormID, float-typed).
    globals: HashMap<String, (u32, bool)>,
}

impl Resource for MessageCatalog {}

impl MessageCatalog {
    pub fn from_index(index: &EsmIndex) -> Self {
        let messages = index
            .messages
            .values()
            .map(|message| {
                (
                    message.form_id,
                    MessageDefinition {
                        title: message.full_name.clone(),
                        text: message.description.clone(),
                        message_box: message.message_box,
                        buttons: message.buttons.clone(),
                        owner_quest: (message.owner_quest != 0)
                            .then_some(QuestFormId(message.owner_quest)),
                    },
                )
            })
            .collect();
        let mut display_names = HashMap::new();
        for npc in index.npcs.values().chain(index.creatures.values()) {
            if !npc.full_name.is_empty() {
                display_names.insert(npc.form_id, npc.full_name.clone());
            }
        }
        for item in index.items.values() {
            if !item.common.full_name.is_empty() {
                display_names.insert(item.form_id, item.common.full_name.clone());
            }
        }
        for location in index.locations.values() {
            if !location.full_name.is_empty() {
                display_names.insert(location.form_id, location.full_name.clone());
            }
        }
        let globals = index
            .globals
            .values()
            .map(|global| {
                (
                    global.editor_id.to_ascii_lowercase(),
                    (global.form_id, matches!(global.value, SettingValue::Float(_))),
                )
            })
            .collect();
        Self {
            messages,
            display_names,
            globals,
        }
    }

    pub fn message(&self, form_id: u32) -> Option<&MessageDefinition> {
        self.messages.get(&form_id)
    }

    pub fn display_name(&self, form_id: u32) -> Option<&str> {
        self.display_names.get(&form_id).map(String::as_str)
    }
}

/// Install the load order's [`MessageCatalog`]; returns how many messages.
pub fn install_message_catalog(world: &mut World, index: &EsmIndex) -> usize {
    let catalog = MessageCatalog::from_index(index);
    let count = catalog.messages.len();
    world.insert_resource(catalog);
    world.init_resource::<PendingScriptMessages>();
    count
}

/// A message ready to show: formatted, tags replaced.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptMessage {
    pub title: String,
    pub text: String,
    pub message_box: bool,
    pub buttons: Vec<String>,
    /// The quest whose script raised a box and reads its answer with
    /// `GetButtonPressed` (ObScript); `None` when nothing reads it.
    pub source: Option<QuestFormId>,
}

/// Each ObScript quest's last answered message-box button, consumed by its
/// `GetButtonPressed`. `NOT_SAVED_BY_DESIGN`: an answer the script has not
/// yet read is UI state, gone with the box (the CS wiki: it returns the
/// button "the first time this is called after a MessageBox button has
/// been pressed").
#[derive(Debug, Default)]
pub struct ButtonPresses(pub HashMap<QuestFormId, i32>);

impl Resource for ButtonPresses {}

/// The player answered `source`'s box with `button`.
pub fn record_button(world: &World, source: QuestFormId, button: usize) {
    if let Some(mut presses) = world.try_resource_mut::<ButtonPresses>() {
        presses.0.insert(source, button as i32);
    }
}

/// `GetButtonPressed`: the answer once, then -1.
pub fn take_button(world: &World, source: QuestFormId) -> i32 {
    world
        .try_resource_mut::<ButtonPresses>()
        .and_then(|mut presses| presses.0.remove(&source))
        .unwrap_or(-1)
}

/// Queue an ObScript `Message` / `MessageBox` for `quest`: numbers
/// substituted; a box without buttons gets the CS wiki's default "Done".
pub fn show_obscript_message(
    world: &World,
    quest: QuestFormId,
    message: crate::obscript_vm::ObScriptMessage,
) {
    let mut buttons = message.buttons;
    if message.message_box && buttons.is_empty() {
        buttons.push("Done".to_owned());
    }
    if let Some(mut pending) = world.try_resource_mut::<PendingScriptMessages>() {
        pending.0.push(ScriptMessage {
            title: String::new(),
            text: substitute_numbers(&message.text, &message.values),
            message_box: message.message_box,
            buttons,
            source: message.message_box.then_some(quest),
        });
    }
}

/// Script messages for the engine's presentation (notifications, the
/// message-box page), drained the frame they are raised;
/// `NOT_SAVED_BY_DESIGN`.
#[derive(Debug, Default)]
pub struct PendingScriptMessages(pub Vec<ScriptMessage>);

impl Resource for PendingScriptMessages {}

/// Substitute `args` into `text`'s `printf` conversions: `%[.N]f`, `%d` /
/// `%i`, and `%%`. A conversion past the last argument reads 0 (the
/// `Show` defaults); anything else after `%` stays as written.
pub fn substitute_numbers(text: &str, args: &[f32]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut next = 0usize;
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'%') {
            chars.next();
            out.push('%');
            continue;
        }
        // `%[.precision](f|d|i)`.
        let mut spec = String::new();
        while let Some(&d) = chars.peek() {
            if d == '.' || d.is_ascii_digit() {
                spec.push(d);
                chars.next();
            } else {
                break;
            }
        }
        let value = args.get(next).copied().unwrap_or(0.0);
        match chars.peek() {
            Some('f') => {
                chars.next();
                next += 1;
                let precision = spec
                    .strip_prefix('.')
                    .and_then(|p| p.parse::<usize>().ok())
                    .unwrap_or(6);
                out.push_str(&format!("{value:.precision$}"));
            }
            Some('d') | Some('i') => {
                chars.next();
                next += 1;
                out.push_str(&format!("{}", value as i64));
            }
            _ => {
                out.push('%');
                out.push_str(&spec);
            }
        }
    }
    out
}

/// Replace the *Text Replacement* tags the runtime can fill: `<Alias=…>`,
/// `<Alias.ShortName=…>` and `<Global=…>`. Unfillable tags stay as written.
pub fn replace_tags(world: &World, text: &str, quest: Option<QuestFormId>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let Some(end) = tail.find('>') else {
            out.push_str(tail);
            return out;
        };
        let tag = &tail[1..end];
        match fill_tag(world, tag, quest) {
            Some(filled) => out.push_str(&filled),
            None => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

fn fill_tag(world: &World, tag: &str, quest: Option<QuestFormId>) -> Option<String> {
    let (kind, name) = tag.split_once('=')?;
    match kind.to_ascii_lowercase().as_str() {
        // `ShortName` "the short name if it exists (otherwise just uses the
        // normal full name)"; short names are not parsed.
        "alias" | "alias.shortname" => alias_name(world, name.trim(), quest),
        "global" => global_text(world, name.trim()),
        _ => None,
    }
}

fn alias_name(world: &World, name: &str, quest: Option<QuestFormId>) -> Option<String> {
    let catalog = world.try_resource::<MessageCatalog>()?;
    if name.eq_ignore_ascii_case("player") {
        return catalog
            .display_name(crate::relationships::PLAYER_BASE)
            .map(str::to_owned);
    }
    let quest = quest?;
    let alias = world
        .try_resource::<crate::scene::SceneQuestAliasRegistry>()?
        .aliases_of(quest)?
        .iter()
        .find(|alias| alias.name.eq_ignore_ascii_case(name))
        .map(|alias| (alias.alias_id, alias.is_location))?;
    let form = if alias.1 {
        world
            .try_resource::<crate::LocationAliasBindings>()?
            .resolve(quest, alias.0)
    } else {
        let entity = world
            .try_resource::<crate::scene::SceneActorBindings>()?
            .resolve(quest, alias.0);
        match entity {
            Some(entity) => crate::relationships::actor_base(world, entity),
            None => None,
        }
    };
    // "If the resulting text is '[...]', then that means the alias wasn't
    // filled."
    let Some(form) = form else {
        return Some("[...]".to_owned());
    };
    catalog.display_name(form).map(str::to_owned)
}

fn global_text(world: &World, name: &str) -> Option<String> {
    let (form_id, float) = world
        .try_resource::<MessageCatalog>()?
        .globals
        .get(&name.to_ascii_lowercase())
        .copied()?;
    let value = world
        .try_resource::<crate::Globals>()
        .and_then(|globals| globals.get(form_id))?;
    Some(if float {
        format!("{value:.2}")
    } else {
        format!("{}", value as i64)
    })
}

/// Format message `form_id` with `args` and queue it for presentation.
/// `false` when the load order carries no such message.
pub fn show_message(world: &World, form_id: u32, args: &[f32]) -> bool {
    let Some(definition) = world
        .try_resource::<MessageCatalog>()
        .and_then(|catalog| catalog.message(form_id).cloned())
    else {
        return false;
    };
    let text = replace_tags(
        world,
        &substitute_numbers(&definition.text, args),
        definition.owner_quest,
    );
    let title = replace_tags(world, &definition.title, definition.owner_quest);
    let message = ScriptMessage {
        title,
        text,
        message_box: definition.message_box,
        buttons: definition.buttons,
        source: None,
    };
    match world.try_resource_mut::<PendingScriptMessages>() {
        Some(mut pending) => {
            pending.0.push(message);
            true
        }
        None => false,
    }
}

/// Queue a plain notification (`Debug.Notification`).
pub fn notify(world: &World, text: String) {
    if let Some(mut pending) = world.try_resource_mut::<PendingScriptMessages>() {
        pending.0.push(ScriptMessage {
            title: String::new(),
            text,
            message_box: false,
            buttons: Vec::new(),
            source: None,
        });
    }
}

pub fn register(world: &mut World) {
    world.init_resource::<PendingScriptMessages>();
    world.init_resource::<ButtonPresses>();
}

#[cfg(test)]
mod tests;
