//! The script extenders' key, control and menu registrations (LMC-SCR-004 catalog growth,
//! `docs/engine/extender-catalog.md` §10): SKSE's `RegisterForKey(code)`, `RegisterForControl(name)`,
//! `RegisterForMenu(name)`, their unregisters and `UnregisterForAll{Keys,Controls,Menus}`, and F4SE's
//! `RegisterForKey` / `RegisterForControl` and their unregisters, on the registering script's own object, and the
//! events they deliver.
//!
//! SKSE64's `PapyrusEvents.cpp`, at the catalog's pinned commit:
//! - a key that goes down sends `OnKeyDown(code)`, and one that comes up `OnKeyUp(code, holdTime)`, to the scripts
//!   registered for that code (a DirectInput scan code; a mouse button is 256 + its index);
//! - the control the key is bound to sends `OnControlDown(control)` / `OnControlUp(control, holdTime)` to the scripts
//!   registered for that control;
//! - a menu that opens sends `OnMenuOpen(name)`, and one that closes `OnMenuClose(name)`;
//! - none of them waits for a menu to close: SKSE's input handler queues them as they happen;
//! - a registration is made on the object's handle (`Register<TESForm>(key, formType, thisForm)`), not on the script,
//!   so it is one per object and binding, any script on the object unregisters it, and the event reaches every script
//!   attached to the object that has the handler, as the game's VM sends a handle's events;
//! - F4SE's (`Hooks_Input.cpp`) are the same events with the same arguments, but registered per script: the event
//!   goes to the registering script only (`SendPapyrusEvent1(reg.handle, reg.scriptName, …)`), and its unregister
//!   removes that script's registration; F4SE's co-save keeps them too (`KEYR`, `CTLR`);
//! - `InternalSerialization.cpp` writes every registration into the co-save (`KEYR`, `CTLR`, `MENR`) and clears them on
//!   a revert, so they are saved here and survive a load. A quest stopping does not unregister them.
//!
//! An event is delivered while its owner stands (a running quest or alias, a loaded reference), as every VM event is.

use super::*;
use crate::translate::effects::VmStored;

/// What a registration listens to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum InputBinding {
    /// A DirectInput key code.
    Key(i32),
    /// A control name (`"Activate"`), compared case-insensitively.
    Control(String),
    /// A menu name (`"InventoryMenu"`), compared case-insensitively.
    Menu(String),
}

impl InputBinding {
    fn same(&self, other: &InputBinding) -> bool {
        match (self, other) {
            (Self::Key(a), Self::Key(b)) => a == b,
            (Self::Control(a), Self::Control(b)) | (Self::Menu(a), Self::Menu(b)) => a.eq_ignore_ascii_case(b),
            _ => false,
        }
    }

    fn channel(&self) -> InputChannel {
        match self {
            Self::Key(_) => InputChannel::Key,
            Self::Control(_) => InputChannel::Control,
            Self::Menu(_) => InputChannel::Menu,
        }
    }
}

/// The three kinds of registration, for the unregister-all calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputChannel {
    Key,
    Control,
    Menu,
}

/// One object's registration (SKSE), or one script's (F4SE).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct InputEventRegistration {
    /// The object the registering script is attached to (a quest, an alias's owner id, a reference).
    #[cfg_attr(feature = "save", serde(with = "byroredux_core::stable_form_id"))]
    pub owner: u32,
    /// F4SE's registering script, which alone receives the events; `None` for SKSE's, which every script on the
    /// object receives.
    pub script: Option<String>,
    pub binding: InputBinding,
}

/// A registration change a script run asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEventRequest {
    Register(InputBinding),
    Unregister(InputBinding),
    UnregisterAll(InputChannel),
}

impl InputEventRequest {
    /// The handlers a registration call delivers to (empty for an unregistration), by lowercase call name.
    pub(super) fn handlers(name: &str) -> Option<&'static [&'static str]> {
        Some(match name {
            "registerforkey" => &["onkeydown", "onkeyup"],
            "registerforcontrol" => &["oncontroldown", "oncontrolup"],
            "registerformenu" => &["onmenuopen", "onmenuclose"],
            "unregisterforkey" | "unregisterforallkeys" | "unregisterforcontrol" | "unregisterforallcontrols"
            | "unregisterformenu" | "unregisterforallmenus" => &[],
            _ => return None,
        })
    }

    /// Whether a call takes a key code (`true`) or a name (`false`); `None` for the unregister-all calls.
    pub(super) fn takes_key(name: &str) -> Option<bool> {
        match name {
            "registerforkey" | "unregisterforkey" => Some(true),
            "registerforcontrol" | "unregisterforcontrol" | "registerformenu" | "unregisterformenu" => Some(false),
            _ => None,
        }
    }

    /// The request a call makes, from its evaluated argument (a key code or a name).
    pub(super) fn of_call(name: &str, code: Option<i32>, text: Option<String>) -> Option<Self> {
        Some(match name {
            "registerforkey" => Self::Register(InputBinding::Key(code?)),
            "unregisterforkey" => Self::Unregister(InputBinding::Key(code?)),
            "registerforcontrol" => Self::Register(InputBinding::Control(text?)),
            "unregisterforcontrol" => Self::Unregister(InputBinding::Control(text?)),
            "registerformenu" => Self::Register(InputBinding::Menu(text?)),
            "unregisterformenu" => Self::Unregister(InputBinding::Menu(text?)),
            "unregisterforallkeys" => Self::UnregisterAll(InputChannel::Key),
            "unregisterforallcontrols" => Self::UnregisterAll(InputChannel::Control),
            "unregisterforallmenus" => Self::UnregisterAll(InputChannel::Menu),
            _ => return None,
        })
    }
}

/// Every object's SKSE key, control and menu registrations. Saved (SKSE's co-save keeps them).
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct InputEventRegistrations {
    entries: Vec<InputEventRegistration>,
}

impl Resource for InputEventRegistrations {}

impl InputEventRegistrations {
    pub fn entries(&self) -> &[InputEventRegistration] {
        &self.entries
    }

    /// Apply `request` from a script on `owner`: `script` names it for an F4SE registration, `None` for SKSE's
    /// object-wide one. Registering twice is one registration.
    pub fn request(&mut self, owner: u32, script: Option<&str>, request: &InputEventRequest) {
        let mine = |e: &InputEventRegistration| {
            e.owner == owner
                && match (&e.script, script) {
                    (None, None) => true,
                    (Some(theirs), Some(ours)) => theirs.eq_ignore_ascii_case(ours),
                    _ => false,
                }
        };
        match request {
            InputEventRequest::Register(binding) => {
                if !self.entries.iter().any(|e| mine(e) && e.binding.same(binding)) {
                    self.entries.push(InputEventRegistration { owner, script: script.map(str::to_owned), binding: binding.clone() });
                }
            }
            InputEventRequest::Unregister(binding) => self.entries.retain(|e| !(mine(e) && e.binding.same(binding))),
            InputEventRequest::UnregisterAll(channel) => self.entries.retain(|e| !(mine(e) && e.binding.channel() == *channel)),
        }
    }

    /// The registrations for `binding`: the owner, and the one script an F4SE registration names.
    fn registered(&self, binding: &InputBinding) -> Vec<(u32, Option<String>)> {
        self.entries.iter().filter(|e| e.binding.same(binding)).map(|e| (e.owner, e.script.clone())).collect()
    }
}

/// Send `handler(args)` to the scripts each registration reaches: its own script (F4SE), or every script on its
/// object (SKSE).
fn send(world: &World, owners: &super::owners::ScriptOwners, registered: &[(u32, Option<String>)], handler: &str, args: &[VmStored]) {
    for (owner, script) in registered {
        let scripts = match script {
            Some(script) => vec![script.clone()],
            None => owners.attached_scripts(world, *owner),
        };
        for script in scripts {
            if owners.handler_runs(*owner, &script, handler) {
                super::menu_events::deliver_to_standing(world, owners, *owner, &script, handler, args.to_vec());
            }
        }
    }
}

/// The keys held now, by DirectInput code in ascending order (`Input.IsKeyPressed` / `GetNumKeysPressed` /
/// `GetNthKeyPressed`), published by the host each frame. Transient input state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeldKeys(pub Vec<i32>);

impl Resource for HeldKeys {}

/// One frame's input transitions, as the host saw them: the codes that went down, those that came up with how long
/// they were held, and the controls those keys are bound to.
#[derive(Debug, Clone, Default)]
pub struct InputFrame {
    pub keys_down: Vec<i32>,
    pub keys_up: Vec<(i32, f32)>,
    pub controls_down: Vec<String>,
    pub controls_up: Vec<(String, f32)>,
}

/// Deliver a frame's key and control transitions to the scripts registered for them.
pub fn deliver_input_events(world: &World, frame: &InputFrame) {
    let Some(registrations) = world.try_resource::<InputEventRegistrations>().map(|r| r.clone()) else {
        return;
    };
    if registrations.entries.is_empty() {
        return;
    }
    let owners = super::owners::ScriptOwners::new(world);
    for code in &frame.keys_down {
        send(world, &owners, &registrations.registered(&InputBinding::Key(*code)), "OnKeyDown", &[VmStored::Int(*code)]);
    }
    for (code, held) in &frame.keys_up {
        send(world, &owners, &registrations.registered(&InputBinding::Key(*code)), "OnKeyUp", &[VmStored::Int(*code), VmStored::Float(*held)]);
    }
    for control in &frame.controls_down {
        let registered = registrations.registered(&InputBinding::Control(control.clone()));
        send(world, &owners, &registered, "OnControlDown", &[VmStored::Str(control.clone())]);
    }
    for (control, held) in &frame.controls_up {
        let registered = registrations.registered(&InputBinding::Control(control.clone()));
        send(world, &owners, &registered, "OnControlUp", &[VmStored::Str(control.clone()), VmStored::Float(*held)]);
    }
}

/// Deliver a menu's opening or closing to the scripts registered for it (from the menu system's one drain).
pub(super) fn deliver_menu_events(world: &World, owners: &super::owners::ScriptOwners, name: &str, opening: bool) {
    let Some(registered) = world.try_resource::<InputEventRegistrations>().map(|r| r.registered(&InputBinding::Menu(name.to_owned())))
    else {
        return;
    };
    let handler = if opening { "OnMenuOpen" } else { "OnMenuClose" };
    send(world, owners, &registered, handler, &[VmStored::Str(name.to_owned())]);
}

/// This subsystem's saved column. Called by the binary's `build_save_registry` in its row order (the schema
/// fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub(super) fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    // LMC-SCR-004 — SKSE's key, control and menu registrations (SKSE's co-save keeps them).
    registry.register_resource::<InputEventRegistrations>("InputEventRegistrations");
}

/// This subsystem's world setup.
pub(super) fn install_input_events(world: &mut byroredux_core::ecs::World) {
    world.init_resource::<InputEventRegistrations>();
    world.init_resource::<HeldKeys>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrations_dedupe_and_unregister_per_channel() {
        let mut registrations = InputEventRegistrations::default();
        let register = |binding: InputBinding| InputEventRequest::Register(binding);
        registrations.request(0x10, None, &register(InputBinding::Key(57)));
        registrations.request(0x10, None, &register(InputBinding::Key(57)));
        registrations.request(0x10, None, &register(InputBinding::Key(28)));
        registrations.request(0x10, None, &register(InputBinding::Control("Activate".into())));
        registrations.request(0x10, None, &register(InputBinding::Menu("InventoryMenu".into())));
        registrations.request(0x11, None, &register(InputBinding::Key(57)));
        assert_eq!(registrations.entries().len(), 5, "one per object and binding");
        assert_eq!(registrations.registered(&InputBinding::Key(57)), [(0x10, None), (0x11, None)]);
        assert_eq!(registrations.registered(&InputBinding::Control("activate".into())), [(0x10, None)], "names compare without case");
        registrations.request(0x10, None, &InputEventRequest::UnregisterAll(InputChannel::Key));
        assert_eq!(registrations.entries().len(), 3, "the object's keys go, its control and menu stay");
        registrations.request(0x10, None, &InputEventRequest::Unregister(InputBinding::Menu("INVENTORYMENU".into())));
        assert_eq!(registrations.entries().len(), 2);
    }

    #[test]
    fn f4se_registrations_are_per_script() {
        let mut registrations = InputEventRegistrations::default();
        let key = InputEventRequest::Register(InputBinding::Key(57));
        registrations.request(0x10, Some("A"), &key);
        registrations.request(0x10, Some("B"), &key);
        registrations.request(0x10, Some("a"), &key);
        assert_eq!(registrations.registered(&InputBinding::Key(57)), [(0x10, Some("A".into())), (0x10, Some("B".into()))]);
        registrations.request(0x10, Some("B"), &InputEventRequest::Unregister(InputBinding::Key(57)));
        assert_eq!(registrations.registered(&InputBinding::Key(57)), [(0x10, Some("A".into()))], "B's alone goes");
    }

    #[test]
    fn calls_map_to_requests_and_handlers() {
        assert_eq!(InputEventRequest::handlers("registerforkey"), Some(&["onkeydown", "onkeyup"][..]));
        assert_eq!(InputEventRequest::handlers("unregisterforallmenus"), Some(&[][..]));
        assert_eq!(InputEventRequest::handlers("registerformenuopencloseevent"), None, "Fallout 4's call is not this one");
        assert_eq!(InputEventRequest::of_call("registerforkey", Some(57), None), Some(InputEventRequest::Register(InputBinding::Key(57))));
        assert_eq!(InputEventRequest::of_call("registerformenu", None, None), None);
        assert_eq!(InputEventRequest::takes_key("unregisterforcontrol"), Some(false));
        assert_eq!(InputEventRequest::takes_key("unregisterforallkeys"), None);
    }
}
