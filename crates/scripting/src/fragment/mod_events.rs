//! SKSE's mod events for the Papyrus VM (LMC-SCR-004 catalog growth, `docs/engine/extender-catalog.md` §11):
//! `RegisterForModEvent(eventName, callbackName)`, `UnregisterForModEvent(eventName)`, `UnregisterForAllModEvents()`
//! and `SendModEvent(eventName, strArg = "", numArg = 0.0)` on the script's own object, the handle builder
//! (`ModEvent.Create` / `Push*` / `Send` / `Release`), and the events that reach the registered callbacks.
//!
//! SKSE64 at the catalog's pinned commit:
//! - `Form.psc`: the callback is `Event Callback(string eventName, string strArg, float numArg, Form sender)`;
//!   `ModEvent.psc`: an event built with `ModEvent.Create` / `Push*` / `Send` passes the pushed values instead;
//! - `PapyrusForm.cpp`: a registration is made on the object's handle with its callback name
//!   (`g_modCallbackRegs.Register(eventName, handle, callbackName)`), so it is one per object and event, a second
//!   registration replaces its callback, and every script attached to the object with that function hears it;
//!   `SendModEvent` sends with the object as `sender`;
//! - `PapyrusModEvent.cpp`: `Create` stores the builder in SKSE's object storage (a global handle, `0` on failure),
//!   `Push*` appends a typed argument, `Send` takes the builder, queues its arguments to every registration and
//!   answers true, `Release` drops it; the storage is saved (`OBMG`), so a builder survives a save and load;
//! - `InternalSerialization.cpp` saves the registrations (`MCBR`) and loads them back, so they are saved here.
//!   `Form.psc`'s comment says they "have to be refreshed after each game load"; scripts that follow it register
//!   again, which changes nothing.
//!
//! There is one mod-event channel: the engine's shared bus (`byroredux_sdk::event`, `legacy.skse.mod-event.<name>`).
//! A VM script's send is published through the Papyrus provider runtime's publisher under its source's principal
//! ([`crate::script_sources`]); the host's dispatch copies every legacy mod event to the provider lane and to
//! [`VmModEventInbox`]. Without an extension host, a send is delivered to both lanes directly.

use super::*;
use crate::translate::effects::VmStored;
use byroredux_sdk::event::{
    legacy_skse_mod_event_name, CustomEvent, LegacySkseModEventPayload, LegacySkseModEventValue, LegacySkseVariadicModEventPayload,
};

/// The most events the VM inbox holds; more are dropped with a warning, as the provider lane's queue does.
pub const MAX_VM_MOD_EVENTS: usize = 256;

/// One object's registration for an event.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ModEventRegistration {
    /// The object the registering script is attached to (a quest, an alias's owner id, a reference).
    #[cfg_attr(feature = "save", serde(with = "byroredux_core::stable_form_id"))]
    pub owner: u32,
    pub event: String,
    pub callback: String,
}

/// A registration change a script run asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum ModEventRequest {
    Register { event: String, callback: String },
    Unregister { event: String },
    UnregisterAll,
}

/// Every object's mod-event registrations. Saved (SKSE's co-save keeps them).
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ModEventRegistrations {
    entries: Vec<ModEventRegistration>,
}

impl Resource for ModEventRegistrations {}

impl ModEventRegistrations {
    pub fn entries(&self) -> &[ModEventRegistration] {
        &self.entries
    }

    /// Apply `request` from a script on `owner`. Event names compare without case, as SKSE's pooled strings do.
    pub fn request(&mut self, owner: u32, request: &ModEventRequest) {
        match request {
            ModEventRequest::Register { event, callback } => {
                match self.entries.iter_mut().find(|e| e.owner == owner && e.event.eq_ignore_ascii_case(event)) {
                    Some(existing) => existing.callback = callback.clone(),
                    None => self.entries.push(ModEventRegistration { owner, event: event.clone(), callback: callback.clone() }),
                }
            }
            ModEventRequest::Unregister { event } => self.entries.retain(|e| !(e.owner == owner && e.event.eq_ignore_ascii_case(event))),
            ModEventRequest::UnregisterAll => self.entries.retain(|e| e.owner != owner),
        }
    }
}

/// The builders `ModEvent.Create` made and `Send` or `Release` has not taken yet, one store for the whole session as
/// SKSE's object storage is (the SDK's [`LegacySkseModEventBuilders`] bounds them). Saved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct VmModEventBuilders(pub byroredux_sdk::event::LegacySkseModEventBuilders);

impl Resource for VmModEventBuilders {}

/// The mod events waiting for the VM's registrations. Transient: a load starts empty.
#[derive(Debug, Clone, Default)]
pub struct VmModEventInbox(pub Vec<CustomEvent>);

impl Resource for VmModEventInbox {}

/// Queue a legacy mod event for the VM's registrations.
pub fn queue_vm_mod_event(world: &World, event: CustomEvent) {
    let Some(mut inbox) = world.try_resource_mut::<VmModEventInbox>() else {
        return;
    };
    if inbox.0.len() < MAX_VM_MOD_EVENTS {
        inbox.0.push(event);
    } else {
        log::warn!("Papyrus VM mod-event inbox limit of {MAX_VM_MOD_EVENTS} exceeded");
    }
}

/// The callback's arguments for `event`: SKSE's `(eventName, strArg, numArg, sender)` for a `SendModEvent`, or the
/// pushed values for a `ModEvent.Send`. A form outside the load order arrives as `None`.
fn callback_args(world: &World, name: &str, payload: &[u8]) -> Option<Vec<VmStored>> {
    let identity = world.try_resource::<crate::LoadOrderIdentity>();
    let form = |form: Option<byroredux_sdk::identity::FormRef>| {
        form.and_then(|form| identity.as_ref().and_then(|identity| identity.global_form_id(form))).map_or(VmStored::None, VmStored::Form)
    };
    if let Some(fixed) = LegacySkseModEventPayload::decode(payload) {
        return Some(vec![
            VmStored::Str(name.to_owned()),
            VmStored::Str(fixed.string_arg),
            VmStored::Float(f32::from_bits(fixed.number_arg_bits)),
            form(fixed.sender),
        ]);
    }
    let variadic = LegacySkseVariadicModEventPayload::decode(payload)?;
    Some(
        variadic
            .arguments
            .into_iter()
            .map(|argument| match argument {
                LegacySkseModEventValue::Bool(value) => VmStored::Bool(value),
                LegacySkseModEventValue::Int(value) => VmStored::Int(value),
                LegacySkseModEventValue::FloatBits(bits) => VmStored::Float(f32::from_bits(bits)),
                LegacySkseModEventValue::String(value) => VmStored::Str(value),
                LegacySkseModEventValue::Form(value) => form(value),
            })
            .collect(),
    )
}

/// Deliver the inbox's mod events to the VM's registrations: each registered object's scripts with the callback.
pub fn vm_mod_event_system(world: &World, _dt: f32) {
    let events = match world.try_resource_mut::<VmModEventInbox>() {
        Some(mut inbox) if !inbox.0.is_empty() => std::mem::take(&mut inbox.0),
        _ => return,
    };
    let registrations = match world.try_resource::<ModEventRegistrations>() {
        Some(registrations) if !registrations.entries.is_empty() => registrations.entries.clone(),
        _ => return,
    };
    let owners = super::owners::ScriptOwners::new(world);
    for event in events {
        let Some(name) = legacy_skse_mod_event_name(&event.event) else {
            continue;
        };
        let receivers: Vec<&ModEventRegistration> = registrations.iter().filter(|r| r.event.eq_ignore_ascii_case(&name)).collect();
        if receivers.is_empty() {
            continue;
        }
        let Some(args) = callback_args(world, &name, &event.payload) else {
            log::warn!("mod event {name}: a payload neither SendModEvent nor ModEvent shape, dropped");
            continue;
        };
        for receiver in receivers {
            super::input_events::send(world, &owners, &[(receiver.owner, None)], &receiver.callback, &args);
        }
    }
}

/// What a VM run sends.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum ModEventPayload {
    /// `SendModEvent(event, strArg, numArg)`: the sender is the sending object.
    Fixed { event: String, string_arg: String, number_arg: f32 },
    /// A `ModEvent` builder, taken by `Send`.
    Built(byroredux_sdk::event::PublishEventCommand),
}

/// One send a VM run made, published after the run's guards drop.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ModEventSend {
    pub(super) script: String,
    pub(super) owner: u32,
    pub(super) payload: ModEventPayload,
}

/// Send what VM runs asked for: through the extension host's bus when there is one, else to both lanes directly.
pub(super) fn send_mod_events(world: &World, sends: Vec<ModEventSend>) {
    for send in sends {
        let principal = world.try_resource::<crate::script_sources::ScriptSourceIndex>().and_then(|sources| sources.0.principal_of(world, &send.script));
        let Some(principal) = principal else {
            log::warn!("a mod event from {}: a script no source holds has no principal, not sent", send.script);
            continue;
        };
        let command = match send.payload {
            ModEventPayload::Built(command) => command,
            ModEventPayload::Fixed { event, string_arg, number_arg } => {
                let sender = world.try_resource::<crate::LoadOrderIdentity>().and_then(|identity| identity.form_ref(send.owner));
                match byroredux_sdk::compatibility::adapt_legacy_send_mod_event(&event, string_arg, number_arg, sender) {
                    Ok(command) => command,
                    Err(error) => {
                        log::warn!("SendModEvent(\"{event}\") from {}: {error}", send.script);
                        continue;
                    }
                }
            }
        };
        let publisher = world.try_resource::<crate::PapyrusProviderRuntime>().and_then(|runtime| runtime.mod_event_publisher());
        match publisher {
            Some(publisher) => {
                if let Err(error) = publisher(&principal, command) {
                    log::warn!("a mod event from {}: {error}", send.script);
                }
            }
            None => {
                let event = CustomEvent { event: command.event, sender: principal, payload: command.payload };
                crate::queue_papyrus_mod_event(world, event.clone());
                queue_vm_mod_event(world, event);
            }
        }
    }
}

/// This subsystem's saved column. Called by the binary's `build_save_registry` in its row order (the schema
/// fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub(super) fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    // LMC-SCR-004 — SKSE's mod-event registrations (SKSE's co-save keeps them).
    registry.register_resource::<ModEventRegistrations>("ModEventRegistrations");
    registry.register_resource::<VmModEventBuilders>("VmModEventBuilders");
}

/// This subsystem's world setup.
pub(super) fn install_mod_events(world: &mut byroredux_core::ecs::World) {
    world.init_resource::<ModEventRegistrations>();
    world.init_resource::<VmModEventInbox>();
    world.insert_resource(VmModEventBuilders(byroredux_sdk::event::LegacySkseModEventBuilders::new()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_registration_is_one_per_object_and_event_and_replaces_its_callback() {
        let mut registrations = ModEventRegistrations::default();
        let register = |event: &str, callback: &str| ModEventRequest::Register { event: event.into(), callback: callback.into() };
        registrations.request(0x10, &register("MyMod_Ready", "OnReady"));
        registrations.request(0x10, &register("mymod_ready", "OnReadyAgain"));
        registrations.request(0x10, &register("Other", "OnOther"));
        registrations.request(0x11, &register("MyMod_Ready", "OnReady"));
        assert_eq!(registrations.entries().len(), 3);
        assert_eq!(registrations.entries()[0].callback, "OnReadyAgain", "the second registration replaces the callback");
        registrations.request(0x10, &ModEventRequest::Unregister { event: "MYMOD_READY".into() });
        assert_eq!(registrations.entries().len(), 2);
        registrations.request(0x10, &ModEventRequest::UnregisterAll);
        assert_eq!(registrations.entries().iter().map(|e| e.owner).collect::<Vec<_>>(), [0x11]);
    }
}
