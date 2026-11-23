//! F4SE's external events (LMC-UI-002 slice 5): `RegisterForExternalEvent(asEventName, asCallbackName)` /
//! `UnregisterForExternalEvent(asEventName)` on any script, and the engine's sends.
//!
//! Fallout 4 MCM is their user: a script registers for `OnMCMSettingChange|<mod>` (that mod's changes) or
//! `OnMCMSettingChange` (every mod's), and its callback function receives `(modName, id)`; the wiki
//! (github.com/Neanka/MCM_0.1_AS3, *Advanced Papyrus Integration*) registers again after every load,
//! because F4SE keeps registrations for the session only. So do these: they are never saved, and a load
//! clears them.

use super::*;
use crate::translate::effects::VmStored;

/// One script's registration: whose run made it, and the function the event calls.
#[derive(Debug, Clone)]
pub struct ExternalEventRegistration {
    /// The object the script is attached to (`Self`: a quest, an alias owner, a reference).
    pub owner: u32,
    pub script: String,
    pub vmad: Option<Arc<ScriptInstanceData>>,
    /// The event as registered (`OnMCMSettingChange|MyMod`).
    pub event: String,
    pub callback: String,
}

/// The session's external-event registrations. Not saved (F4SE's are not).
#[derive(Debug, Clone, Default)]
pub struct ExternalEventRegistrations(pub Vec<ExternalEventRegistration>);

impl Resource for ExternalEventRegistrations {}

impl ExternalEventRegistrations {
    /// Register (or re-register: the callback is replaced) `script` on `owner` for `event`.
    pub fn register(&mut self, registration: ExternalEventRegistration) {
        self.0.retain(|r| !(r.owner == registration.owner && r.script.eq_ignore_ascii_case(&registration.script) && r.event.eq_ignore_ascii_case(&registration.event)));
        self.0.push(registration);
    }

    pub fn unregister(&mut self, owner: u32, script: &str, event: &str) {
        self.0.retain(|r| !(r.owner == owner && r.script.eq_ignore_ascii_case(script) && r.event.eq_ignore_ascii_case(event)));
    }

    /// The registrations an event named `name` reaches when sent for `filter` (a mod): those for the bare
    /// name and those for `name|filter`.
    fn receivers(&self, name: &str, filter: &str) -> Vec<ExternalEventRegistration> {
        self.0
            .iter()
            .filter(|r| match r.event.split_once('|') {
                None => r.event.eq_ignore_ascii_case(name),
                Some((event, only)) => event.eq_ignore_ascii_case(name) && only.eq_ignore_ascii_case(filter),
            })
            .cloned()
            .collect()
    }
}

/// Send external event `name` for `filter` with `args`: each registered callback runs now, in
/// registration order, under the frame budget.
pub fn send_external_event(world: &World, name: &str, filter: &str, args: &[VmStored]) {
    let receivers = match world.try_resource::<ExternalEventRegistrations>() {
        Some(registrations) => registrations.receivers(name, filter),
        None => return,
    };
    for receiver in receivers {
        log::debug!("external event {name}|{filter} → {}.{}", receiver.script, receiver.callback);
        let effect = Effect::interpret(receiver.script.clone(), receiver.callback.clone(), args.to_vec());
        let advances = run_papyrus_call(world, QuestFormId(receiver.owner), receiver.vmad.as_deref(), effect);
        flush_advances(world, advances);
    }
}

/// A load replaced the session: its registrations go, and the scripts register again (F4SE's rule).
pub fn reset_external_events(world: &World) {
    if let Some(mut registrations) = world.try_resource_mut::<ExternalEventRegistrations>() {
        registrations.0.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registration(owner: u32, event: &str) -> ExternalEventRegistration {
        ExternalEventRegistration { owner, script: "S".into(), vmad: None, event: event.into(), callback: "Cb".into() }
    }

    #[test]
    fn an_event_reaches_bare_and_filtered_registrations() {
        let mut registrations = ExternalEventRegistrations::default();
        registrations.register(registration(1, "OnMCMSettingChange|Frost"));
        registrations.register(registration(2, "OnMCMSettingChange"));
        registrations.register(registration(3, "OnMCMSettingChange|Other"));
        registrations.register(registration(4, "OnMCMMenuOpen"));
        let owners = |registrations: &ExternalEventRegistrations, name: &str, filter: &str| {
            registrations.receivers(name, filter).iter().map(|r| r.owner).collect::<Vec<_>>()
        };
        assert_eq!(owners(&registrations, "OnMCMSettingChange", "frost"), [1, 2]);
        assert_eq!(owners(&registrations, "OnMCMMenuOpen", "Frost"), [4]);
        registrations.register(registration(1, "onmcmsettingchange|FROST"));
        assert_eq!(registrations.0.len(), 4, "a repeat registration replaces the first");
        registrations.unregister(2, "s", "OnMCMSettingChange");
        assert_eq!(owners(&registrations, "OnMCMSettingChange", "Frost"), [1]);
    }
}
