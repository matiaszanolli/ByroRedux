//! M48 GUI-EVT-001/002 — Fallout 4's menu open / close events:
//! `RegisterForMenuOpenCloseEvent(asMenuName)`, `UnregisterForMenuOpenCloseEvent(asMenuName)`,
//! `UnregisterForAllMenuOpenCloseEvents()` and the `OnMenuOpenCloseEvent(asMenuName, abOpening)`
//! they deliver.
//!
//! The FO4 CK's *RegisterForMenuOpenCloseEvent - ScriptObject* and *OnMenuOpenCloseEvent -
//! ScriptObject* pages:
//! - "Only the script that registers for an event will receive it. Other scripts attached to the
//!   same form, alias, or magic effect will not receive the event unless they also register";
//!   the registration stands until unregistered (it is not one-shot, unlike `RegisterForHitEvent`);
//! - the event is "called when menus are opened or closed", with `asMenuName` the menu's name
//!   ("LockpickingMenu", "ContainerMenu") and `abOpening` "true if the menu is opening, false if
//!   the menu is closing";
//! - "Aliases and quests will automatically unregister for this event when the quest stops"
//!   ([`super::events`]' quest-stop prune). "Active magic effects will automatically unregister
//!   when they are removed": the VM runs no `ActiveMagicEffect` script (its instances belong to
//!   quests, aliases and references), so no effect can hold one yet; when effect scripts run,
//!   their removal drops their owner through [`MenuEventRegistrations::drop_owners`].
//!
//! The menus are the registry's ([`byroredux_core::ecs::resources::MenuRegistry`]): every opening and
//! closing it records is drained by [`menu_open_close_event_system`], which queues the handler
//! of each registration for that name (compared case-insensitively, as Papyrus compares strings)
//! as its own run, like a custom event's receivers.

use super::*;
use crate::translate::effects::VmStored;

/// The handler the registrations deliver to.
pub const MENU_OPEN_CLOSE_EVENT: &str = "OnMenuOpenCloseEvent";

/// One script's registration for a menu's open / close events.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct MenuEventRegistration {
    /// The object the registering script is attached to.
    #[cfg_attr(feature = "save", serde(with = "byroredux_core::stable_form_id"))]
    pub owner: u32,
    pub script: String,
    /// The menu name as the script wrote it.
    pub menu: String,
}

/// A registration change a script run asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuEventRequest {
    Register(String),
    Unregister(String),
    UnregisterAll,
}

impl MenuEventRequest {
    /// The handler a call delivers to (`""` for an unregistration), by lowercase call name.
    pub(super) fn handler(name: &str) -> Option<&'static str> {
        match name {
            "registerformenuopencloseevent" => Some(MENU_OPEN_CLOSE_EVENT),
            "unregisterformenuopencloseevent" | "unregisterforallmenuopencloseevents" => Some(""),
            _ => None,
        }
    }

    /// The request a call makes, from its evaluated menu name.
    pub(super) fn of_call(name: &str, menu: Option<String>) -> Option<Self> {
        Some(match name {
            "registerformenuopencloseevent" => Self::Register(menu?),
            "unregisterformenuopencloseevent" => Self::Unregister(menu?),
            "unregisterforallmenuopencloseevents" => Self::UnregisterAll,
            _ => return None,
        })
    }

    /// How many arguments the call takes.
    pub(super) fn arity(name: &str) -> usize {
        usize::from(name != "unregisterforallmenuopencloseevents")
    }
}

/// Every script's menu open / close registrations. Saved.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct MenuEventRegistrations {
    entries: Vec<MenuEventRegistration>,
}

impl Resource for MenuEventRegistrations {}

impl MenuEventRegistrations {
    pub fn entries(&self) -> &[MenuEventRegistration] {
        &self.entries
    }

    /// Apply `request` from `owner`'s instance of `script`.
    pub fn request(&mut self, owner: u32, script: &str, request: &MenuEventRequest) {
        let mine = |e: &MenuEventRegistration| e.owner == owner && e.script.eq_ignore_ascii_case(script);
        match request {
            MenuEventRequest::Register(menu) => {
                if !self.entries.iter().any(|e| mine(e) && e.menu.eq_ignore_ascii_case(menu)) {
                    self.entries.push(MenuEventRegistration { owner, script: script.to_owned(), menu: menu.clone() });
                }
            }
            MenuEventRequest::Unregister(menu) => self.entries.retain(|e| !(mine(e) && e.menu.eq_ignore_ascii_case(menu))),
            MenuEventRequest::UnregisterAll => self.entries.retain(|e| !mine(e)),
        }
    }

    /// Drop every registration the owners in `gone` hold: quests that stopped (and their
    /// aliases), and — once the VM runs them — removed active magic effects.
    pub fn drop_owners(&mut self, gone: &[u32]) {
        self.entries.retain(|e| !gone.contains(&e.owner));
    }
}

/// Deliver the menus that opened and closed since the last frame to the scripts registered for
/// them (GUI-EVT-001).
pub fn menu_open_close_event_system(world: &World, _dt: f32) {
    // Always drain, so the registry's queue holds one frame's transitions.
    let transitions = match world.try_resource_mut::<byroredux_core::ecs::resources::MenuRegistry>() {
        Some(mut registry) => registry.drain_transitions(),
        None => return,
    };
    if transitions.is_empty() {
        return;
    }
    let registered: Vec<MenuEventRegistration> = match world.try_resource::<MenuEventRegistrations>() {
        Some(registrations) if !registrations.entries.is_empty() => registrations.entries.clone(),
        _ => return,
    };
    let owners = super::owners::ScriptOwners::new(world);
    for transition in transitions {
        for registration in registered.iter().filter(|r| r.menu.eq_ignore_ascii_case(&transition.name)) {
            // A quest's (or alias's) registration delivers while the quest runs; a reference's
            // while it is loaded.
            let standing = {
                let stages = world.resource::<QuestStageState>();
                owners.standing(world, &stages, registration.owner)
            };
            let delivers = match standing {
                Some(standing) => standing,
                None => ReferenceScriptIndex::lookup(world, registration.owner).is_some(),
            };
            if !delivers {
                continue;
            }
            log::debug!(
                "{:08X}: {}.{MENU_OPEN_CLOSE_EVENT}(\"{}\", {})",
                registration.owner,
                registration.script,
                transition.name,
                transition.opening
            );
            events::queue_handler(
                world,
                &owners,
                registration.owner,
                &registration.script,
                MENU_OPEN_CLOSE_EVENT.to_owned(),
                vec![VmStored::Str(transition.name.clone()), VmStored::Bool(transition.opening)],
            );
        }
    }
}

/// This subsystem's saved column. Called by the binary's `build_save_registry` in its row order
/// (the schema fingerprint is order-sensitive).
#[cfg(feature = "save")]
pub(super) fn register_saved(registry: &mut byroredux_save::SaveRegistry) {
    // M48 GUI-EVT-001 — Papyrus menu open / close event registrations.
    registry.register_resource::<MenuEventRegistrations>("MenuEventRegistrations");
}

/// This subsystem's world setup.
pub(super) fn install_menu_events(world: &mut byroredux_core::ecs::World) {
    // M48 GUI-EVT-001 — Papyrus menu open / close event registrations (saved).
    world.init_resource::<MenuEventRegistrations>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrations_dedupe_per_script_and_menu_unregister_and_drop_with_their_owner() {
        let mut registrations = MenuEventRegistrations::default();
        let register = |menu: &str| MenuEventRequest::Register(menu.to_owned());
        registrations.request(0x10, "S", &register("ContainerMenu"));
        registrations.request(0x10, "s", &register("containermenu"));
        registrations.request(0x10, "S", &register("LooksMenu"));
        registrations.request(0x10, "Other", &register("ContainerMenu"));
        registrations.request(0x11, "S", &register("ContainerMenu"));
        assert_eq!(registrations.entries().len(), 4, "one per owner, script and menu");
        registrations.request(0x10, "S", &MenuEventRequest::Unregister("CONTAINERMENU".into()));
        assert_eq!(registrations.entries().len(), 3, "only this script's registration goes");
        registrations.request(0x10, "S", &MenuEventRequest::UnregisterAll);
        assert_eq!(registrations.entries().len(), 2);
        registrations.drop_owners(&[0x11]);
        assert_eq!(registrations.entries(), [MenuEventRegistration { owner: 0x10, script: "Other".into(), menu: "ContainerMenu".into() }]);
    }

    #[test]
    fn calls_map_to_requests_with_their_arity() {
        assert_eq!(MenuEventRequest::handler("registerformenuopencloseevent"), Some(MENU_OPEN_CLOSE_EVENT));
        assert_eq!(MenuEventRequest::handler("unregisterforallmenuopencloseevents"), Some(""));
        assert_eq!(MenuEventRequest::handler("registerformenu"), None, "SKSE's Skyrim call is not this one");
        assert_eq!(MenuEventRequest::of_call("registerformenuopencloseevent", None), None);
        assert_eq!(MenuEventRequest::arity("unregisterformenuopencloseevent"), 1);
        assert_eq!(MenuEventRequest::arity("unregisterforallmenuopencloseevents"), 0);
    }
}
