//! M47.2 — Papyrus custom and remote events (Fallout 4): `RegisterForCustomEvent`
//! / `SendCustomEvent` and `RegisterForRemoteEvent`, with their unregister
//! calls.
//!
//! The FO4 CK's *RegisterForCustomEvent - ScriptObject* and *SendCustomEvent -
//! ScriptObject* pages:
//! - a script registers for a named event of a sender object; the event goes
//!   only to the registering script, and quests and aliases drop their
//!   registrations when the quest stops;
//! - `SendCustomEvent(name, args)` "returns immediately, the receivers
//!   receiving the event as if it was a game event and running their events in
//!   parallel": each receiver's handler `(akSender, akArgs)` runs from the
//!   continuation queue, not nested in the sender's run;
//! - `RegisterForRemoteEvent(sender, "OnActivate")` relays an event the sender
//!   *receives* — "Calling the event directly on the source object will not
//!   relay the event to this script" — to a handler that takes the sender
//!   first. Only the events the engine raises on a reference, and a quest's
//!   `OnStageSet(auiStageID, auiItemID)` ([`REMOTE_EVENTS`]), are accepted, so a
//!   registration is never one that would silently never fire.
//!
//! The compiler mangles the names: an event named `Ambush` in
//! `AmbushTriggerScript` is sent as the string `ambushtriggerscript_Ambush` and
//! handled by `Event AmbushTriggerScript_Ambush`; a remote `OnActivate` is
//! handled by `ObjectReference_OnActivate`. A custom registration's handler is
//! the event string; a remote one is the receiver's event ending
//! `_<event>`.

use super::*;
use crate::translate::effects::VmStored;

/// The events the engine raises on a reference that a remote registration may
/// ask for, lowercase.
pub const REMOTE_EVENTS: [&str; 5] = ["onactivate", "ondeath", "onhit", "ontriggerenter", "onstageset"];

/// One script's registration for another object's event.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct EventRegistration {
    /// The object the registering script is attached to.
    pub owner: u32,
    pub script: String,
    /// The object whose event it is.
    pub sender: super::DistanceTarget,
    /// The event as the script wrote it.
    pub event: String,
    /// `RegisterForRemoteEvent` rather than `RegisterForCustomEvent`.
    pub remote: bool,
}

/// A registration change a script run asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum EventRequest {
    Register {
        sender: super::DistanceTarget,
        event: String,
        remote: bool,
    },
    Unregister {
        sender: super::DistanceTarget,
        event: String,
        remote: bool,
    },
    /// `UnregisterForAllCustomEvents` / `…RemoteEvents` (`Some`) or
    /// `UnregisterForAllEvents` (`None`).
    UnregisterAll { remote: Option<bool> },
}

impl EventRequest {
    /// The handler family a call concerns (`None` for an unknown name).
    pub(super) fn of_call(name: &str, sender: Option<super::DistanceTarget>, event: Option<String>) -> Option<Self> {
        let both = |remote| Some((sender?, event?, remote));
        Some(match name {
            "registerforcustomevent" => {
                let (sender, event, remote) = both(false)?;
                Self::Register { sender, event, remote }
            }
            "registerforremoteevent" => {
                let (sender, event, remote) = both(true)?;
                Self::Register { sender, event, remote }
            }
            "unregisterforcustomevent" => {
                let (sender, event, remote) = both(false)?;
                Self::Unregister { sender, event, remote }
            }
            "unregisterforremoteevent" => {
                let (sender, event, remote) = both(true)?;
                Self::Unregister { sender, event, remote }
            }
            "unregisterforallcustomevents" => Self::UnregisterAll { remote: Some(false) },
            "unregisterforallremoteevents" => Self::UnregisterAll { remote: Some(true) },
            "unregisterforallevents" => Self::UnregisterAll { remote: None },
            _ => return None,
        })
    }

    /// Whether `name` is one of the registration calls.
    pub(super) fn is_call(name: &str) -> bool {
        matches!(
            name,
            "registerforcustomevent"
                | "registerforremoteevent"
                | "unregisterforcustomevent"
                | "unregisterforremoteevent"
                | "unregisterforallcustomevents"
                | "unregisterforallremoteevents"
                | "unregisterforallevents"
        )
    }

    /// How many arguments the call takes.
    pub(super) fn arity(name: &str) -> usize {
        if name.starts_with("unregisterforall") {
            0
        } else {
            2
        }
    }
}

/// Every script's event registrations. Saved.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct EventRegistrations {
    entries: Vec<EventRegistration>,
}

impl Resource for EventRegistrations {}

impl EventRegistrations {
    pub fn entries(&self) -> &[EventRegistration] {
        &self.entries
    }

    /// Apply `request` from `owner`'s instance of `script`.
    pub fn request(&mut self, owner: u32, script: &str, request: &EventRequest) {
        let mine = |e: &EventRegistration| e.owner == owner && e.script.eq_ignore_ascii_case(script);
        match request {
            EventRequest::Register { sender, event, remote } => {
                let exists = self.entries.iter().any(|e| {
                    mine(e) && e.sender == *sender && e.remote == *remote && e.event.eq_ignore_ascii_case(event)
                });
                if !exists {
                    self.entries.push(EventRegistration {
                        owner,
                        script: script.to_owned(),
                        sender: *sender,
                        event: event.clone(),
                        remote: *remote,
                    });
                }
            }
            EventRequest::Unregister { sender, event, remote } => self.entries.retain(|e| {
                !(mine(e) && e.sender == *sender && e.remote == *remote && e.event.eq_ignore_ascii_case(event))
            }),
            EventRequest::UnregisterAll { remote } => self
                .entries
                .retain(|e| !(mine(e) && remote.is_none_or(|remote| e.remote == remote))),
        }
    }

    /// The registrations for `event` of the object `sender` (a reference
    /// form), as stored.
    fn for_sender(&self, sender: u32, event: &str, remote: bool) -> Vec<EventRegistration> {
        self.entries
            .iter()
            .filter(|e| {
                e.remote == remote
                    && e.sender == super::DistanceTarget::Reference(sender)
                    && e.event.eq_ignore_ascii_case(event)
            })
            .cloned()
            .collect()
    }
}

/// The registrations `request`s leave, applied in order to `stored`: what a
/// run sees of its own earlier registrations.
pub(super) fn registrants(
    stored: &EventRegistrations,
    pending: &[(u32, String, EventRequest)],
    sender: u32,
    event: &str,
    remote: bool,
) -> Vec<EventRegistration> {
    let mut view = stored.clone();
    for (owner, script, request) in pending {
        view.request(*owner, script, request);
    }
    view.for_sender(sender, event, remote)
}

/// The handler `registration`'s script runs for `event`: the event string for
/// a custom registration, or the script's event ending `_<event>` for a
/// remote one.
pub(super) fn handler_name(
    scripts: &HashMap<String, Arc<Script>>,
    registration: &EventRegistration,
) -> Option<String> {
    let script = scripts.get(&registration.script.to_ascii_lowercase())?;
    handler_in(script, &registration.event, registration.remote)
}

/// [`handler_name`] over one script.
pub(super) fn handler_in(script: &Script, event: &str, remote: bool) -> Option<String> {
    use byroredux_papyrus::ast::{ScriptItem, StateItem};
    let wanted_suffix = format!("_{}", event.to_ascii_lowercase());
    let matches = |name: &str| {
        let lower = name.to_ascii_lowercase();
        if remote {
            lower.ends_with(&wanted_suffix)
        } else {
            lower == event.to_ascii_lowercase()
        }
    };
    script.body.iter().find_map(|item| match &item.node {
        ScriptItem::Function(f) if matches(&f.name.node.0) => Some(f.name.node.0.clone()),
        ScriptItem::Event(e) if matches(&e.name.node.0) => Some(e.name.node.0.clone()),
        ScriptItem::State(state) => state.body.iter().find_map(|item| match &item.node {
            StateItem::Function(f) if matches(&f.name.node.0) => Some(f.name.node.0.clone()),
            StateItem::Event(e) if matches(&e.name.node.0) => Some(e.name.node.0.clone()),
            _ => None,
        }),
        _ => None,
    })
}

/// Queue each of `registrants`' handlers for `event` on the continuation queue
/// with `args` after the sender (`sender_form` first): the receivers run as
/// separate runs, not nested in the sender's.
pub(super) fn enqueue(
    world: &World,
    registrants: &[EventRegistration],
    sender_form: u32,
    args: &[VmStored],
) {
    if registrants.is_empty() {
        return;
    }
    let owners = super::owners::ScriptOwners::new(world);
    let scripts = owners.scripts();
    for registration in registrants {
        let Some(handler) = handler_name(&scripts, registration) else {
            continue;
        };
        let owner = QuestFormId(registration.owner);
        if !owners.handler_runs(registration.owner, &registration.script, &handler) {
            log::debug!(
                "{:08X}: {}.{handler} skipped: the Papyrus VM does not run it",
                registration.owner,
                registration.script
            );
            continue;
        }
        let vmad = owners.vmad(world, registration.owner, &registration.script);
        let mut handler_args = vec![VmStored::Form(sender_form)];
        handler_args.extend_from_slice(args);
        let effect = Effect::interpret(registration.script.clone(), handler, handler_args);
        if super::budget::defer(world, owner, vmad.as_ref(), vec![effect]).is_err() {
            log::debug!("event delivery dropped: FragmentExecutionQueue is unavailable");
        }
    }
}

/// Drop the registrations of quests that stopped (the CK: "Aliases and quests
/// will automatically unregister for this event when the quest stops"): the
/// custom / remote event registrations and the hit event registrations of the
/// quests and of the alias scripts they own.
pub(super) fn prune_stopped_quests(world: &World) {
    let mut registered: Vec<u32> = world
        .try_resource::<EventRegistrations>()
        .map(|registrations| registrations.entries.iter().map(|e| e.owner).collect())
        .unwrap_or_default();
    if let Some(hits) = world.try_resource::<super::HitRegistrations>() {
        registered.extend(hits.entries().iter().map(|e| e.owner));
    }
    if let Some(animation) = world.try_resource::<super::AnimationEventRegistrations>() {
        registered.extend(animation.entries().iter().map(|e| e.owner));
    }
    if registered.is_empty() {
        return;
    }
    registered.sort_unstable();
    registered.dedup();
    let owners = super::owners::ScriptOwners::new(world);
    let stopped: Vec<u32> = {
        let stages = world.resource::<QuestStageState>();
        registered
            .into_iter()
            .filter(|owner| owners.standing(world, &stages, *owner) == Some(false))
            .collect()
    };
    if stopped.is_empty() {
        return;
    }
    if let Some(mut registrations) = world.try_resource_mut::<EventRegistrations>() {
        registrations.entries.retain(|e| !stopped.contains(&e.owner));
    }
    if let Some(mut hits) = world.try_resource_mut::<super::HitRegistrations>() {
        hits.drop_owners(&stopped);
    }
    if let Some(mut animation) = world.try_resource_mut::<super::AnimationEventRegistrations>() {
        animation.drop_owners(&stopped);
    }
}

/// Relay an engine event a reference received to the scripts registered for it
/// remotely: `event` (`OnActivate`) of the reference `sender`, with the
/// event's own `args`.
pub(super) fn relay_remote(world: &World, sender: u32, event: &str, args: &[VmStored]) {
    let registrants = match world.try_resource::<EventRegistrations>() {
        Some(registrations) if !registrations.entries.is_empty() => registrations.for_sender(sender, event, true),
        _ => return,
    };
    enqueue(world, &registrants, sender, args);
}
