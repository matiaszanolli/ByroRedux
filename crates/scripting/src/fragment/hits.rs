//! M47.2 — Papyrus hit events (Fallout 4, Starfield): `RegisterForHitEvent` /
//! `UnregisterForHitEvent` / `UnregisterForAllHitEvents`, delivering
//! `OnHit(akTarget, akAggressor, akSource, akProjectile, abPowerAttack,
//! abSneakAttack, abBashAttack, abHitBlocked, apMaterial)` to the script that
//! registered.
//!
//! The FO4 CK's *RegisterForHitEvent - ScriptObject* and *OnHit - ScriptObject*:
//! - "Registers this script to receive a single OnHit event matching the
//!   filters given to this function. Only the script that registers for an
//!   event will receive it": the registration is removed after the first
//!   matching hit, and a script re-registers to hear another;
//! - the target is "the ObjectReference, ReferenceAlias or RefCollectionAlias
//!   that is being hit" (a reference or an alias here);
//! - the aggressor filter is a reference, base object, keyword, faction, form
//!   list or alias, "None: Anything"; this runtime takes `None`, a reference
//!   ("Reference or actor: Exact match"), an alias ("Filling this alias") and a
//!   faction ("Actor in faction match"), and stops on the rest rather than match
//!   on a guess. The source and projectile filters must be `None`;
//! - the power / sneak / bash / block filters: "If 0, then it must NOT be a
//!   power attack; if >0, then it MUST be; if <0, then it matches both";
//! - `abMatch`: "If true, the filter must match the hit - if false, the filter
//!   must NOT match the hit (inverted filter)";
//! - "Aliases and quests will automatically unregister for this event when the
//!   quest stops".
//!
//! Skyrim's `OnHit(akAggressor, …)` takes no registration; that automatic
//! delivery is the reference and alias scripts' (`reference.rs`).

use super::*;
use crate::translate::effects::VmStored;

/// What a hit registration's aggressor filter names.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum HitAggressor {
    /// A reference (exact match) or an alias (the reference filling it).
    Target(DistanceTarget),
    /// Any actor in this faction.
    Faction(u32),
}

/// The filters of one hit registration.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct HitFilter {
    /// The aggressor that must (or, inverted, must not) have struck; `None`
    /// matches any.
    pub aggressor: Option<HitAggressor>,
    /// Power / sneak / bash / block: `<0` either, `0` must not be, `>0` must be.
    pub power: i32,
    pub sneak: i32,
    pub bash: i32,
    pub block: i32,
    /// `abMatch` false.
    pub invert: bool,
}

impl Default for HitFilter {
    fn default() -> Self {
        Self {
            aggressor: None,
            power: -1,
            sneak: -1,
            bash: -1,
            block: -1,
            invert: false,
        }
    }
}

impl HitFilter {
    /// Whether `hit` passes the filter, `aggressor_matches` saying whether its
    /// aggressor is the one the aggressor filter names.
    fn allows(&self, hit: &crate::HitEvent, aggressor_matches: bool) -> bool {
        let tri = |filter: i32, happened: bool| filter < 0 || (filter == 0) != happened;
        let matches = (self.aggressor.is_none() || aggressor_matches)
            && tri(self.power, hit.power_attack)
            && tri(self.sneak, hit.sneak_attack)
            && tri(self.bash, hit.bash_attack)
            && tri(self.block, hit.blocked);
        matches != self.invert
    }
}

/// One script instance's pending hit registration.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct HitRegistration {
    /// The object the script is attached to: a quest, a reference or an alias
    /// script's owner id.
    pub owner: u32,
    pub script: String,
    /// The object being hit.
    pub target: DistanceTarget,
    pub filter: HitFilter,
}

/// A hit registration change a script run asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum HitRequest {
    Register { target: DistanceTarget, filter: HitFilter },
    Unregister { target: DistanceTarget },
    UnregisterAll,
}

impl HitRequest {
    /// The handler a call delivers to (`""` for an unregistration), by
    /// lowercase call name.
    pub(super) fn handler(name: &str) -> Option<&'static str> {
        match name {
            "registerforhitevent" => Some("OnHit"),
            "unregisterforhitevent" | "unregisterforallhitevents" => Some(""),
            _ => None,
        }
    }
}

/// Every pending hit registration. Saved.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct HitRegistrations {
    entries: Vec<HitRegistration>,
}

impl Resource for HitRegistrations {}

impl HitRegistrations {
    pub fn entries(&self) -> &[HitRegistration] {
        &self.entries
    }

    /// Apply `request` from `owner`'s instance of `script`.
    pub fn request(&mut self, owner: u32, script: &str, request: &HitRequest) {
        let mine = |e: &HitRegistration| e.owner == owner && e.script.eq_ignore_ascii_case(script);
        match request {
            HitRequest::Register { target, filter } => {
                let exists = self.entries.iter().any(|e| mine(e) && e.target == *target && e.filter == *filter);
                if !exists {
                    self.entries.push(HitRegistration {
                        owner,
                        script: script.to_owned(),
                        target: *target,
                        filter: *filter,
                    });
                }
            }
            HitRequest::Unregister { target } => self.entries.retain(|e| !(mine(e) && e.target == *target)),
            HitRequest::UnregisterAll => self.entries.retain(|e| !mine(e)),
        }
    }

    /// Drop every registration the owners in `stopped` hold.
    pub(super) fn drop_owners(&mut self, stopped: &[u32]) {
        self.entries.retain(|e| !stopped.contains(&e.owner));
    }
}

/// Deliver this frame's hits to the scripts registered for them, each
/// registration once.
pub fn hit_event_system(world: &World, _dt: f32) {
    let candidates: Vec<(usize, HitRegistration)> = match world.try_resource::<HitRegistrations>() {
        Some(registrations) if !registrations.entries.is_empty() => {
            registrations.entries.iter().cloned().enumerate().collect()
        }
        _ => return,
    };
    let hits: Vec<(EntityId, crate::HitEvent)> = match world.query::<crate::HitEvent>() {
        Some(events) => events.iter().map(|(entity, hit)| (entity, *hit)).collect(),
        None => return,
    };
    if hits.is_empty() {
        return;
    }
    let owners = super::owners::ScriptOwners::new(world);
    let standing: Vec<Option<bool>> = {
        let stages = world.resource::<QuestStageState>();
        candidates.iter().map(|(_, r)| owners.standing(world, &stages, r.owner)).collect()
    };
    let weapons = world.query::<byroredux_core::ecs::components::EquippedWeapon>();
    let mut fired: Vec<(usize, HitRegistration, EntityId, crate::HitEvent, bool)> = Vec::new();
    let mut starts = super::budget::runs_startable(world);
    for ((index, registration), standing) in candidates.into_iter().zip(standing) {
        // A quest's (or alias's) registration delivers while the quest runs; a
        // reference's while it is loaded.
        match standing {
            Some(false) => continue,
            Some(true) => {}
            None if ReferenceScriptIndex::lookup(world, registration.owner).is_some() => {}
            None => continue,
        }
        let Some(victim) = super::distance::target_entity(world, registration.target) else {
            continue;
        };
        let aggressor_filter = match registration.filter.aggressor {
            Some(HitAggressor::Target(target)) => super::distance::target_entity(world, target),
            _ => None,
        };
        let Some(&(_, hit)) = hits.iter().find(|(entity, hit)| {
            let aggressor_matches = match registration.filter.aggressor {
                None => true,
                Some(HitAggressor::Target(_)) => aggressor_filter == Some(hit.aggressor),
                Some(HitAggressor::Faction(faction)) => world
                    .get::<byroredux_core::ecs::components::FactionRanks>(hit.aggressor)
                    .is_some_and(|ranks| ranks.is_member(faction)),
            };
            *entity == victim && registration.filter.allows(hit, aggressor_matches)
        }) else {
            continue;
        };
        // #5987 — a hit is a one-shot marker (drained at the end of the frame) and
        // `RegisterForHitEvent` is itself one-shot, so a hit the frame cannot
        // afford to start must not be skipped: the registration fires either way
        // and the run is queued for a later frame instead of started now.
        let defer = starts == 0;
        starts = starts.saturating_sub(1);
        fired.push((index, registration, victim, hit, defer));
    }
    if fired.is_empty() {
        return;
    }
    if let Some(mut registrations) = world.try_resource_mut::<HitRegistrations>() {
        let mut remove: Vec<usize> = fired.iter().map(|(index, ..)| *index).collect();
        remove.sort_unstable();
        let mut position = 0;
        registrations.entries.retain(|_| {
            let keep = remove.binary_search(&position).is_err();
            position += 1;
            keep
        });
    }
    let mut advances = Vec::new();
    for (_, registration, victim, hit, defer) in fired {
        let owner = QuestFormId(registration.owner);
        if !owners.handler_runs(registration.owner, &registration.script, "OnHit") {
            log::debug!(
                "{:08X}: {}.OnHit skipped: the Papyrus VM does not run it",
                registration.owner,
                registration.script
            );
            continue;
        }
        let vmad = owners.vmad(world, registration.owner, &registration.script);
        let form = |entity: EntityId| VmStored::Form(super::distance::entity_form(world, entity).unwrap_or(0));
        let source = weapons
            .as_ref()
            .and_then(|q| q.get(hit.aggressor))
            .map_or(VmStored::None, |weapon| VmStored::Form(weapon.base_form_id));
        let effect = Effect::Interpret {
            script: registration.script.clone(),
            function: "OnHit".to_owned(),
            args: vec![
                form(victim),
                form(hit.aggressor),
                source,
                // Projectiles are not modelled.
                VmStored::None,
                VmStored::Bool(hit.power_attack),
                VmStored::Bool(hit.sneak_attack),
                VmStored::Bool(hit.bash_attack),
                VmStored::Bool(hit.blocked),
                // The material the hit landed on is not modelled.
                VmStored::Str(String::new()),
            ],
            resume: None,
        };
        log::debug!("{:08X}: {}.OnHit", registration.owner, registration.script);
        let effect = if defer {
            match super::budget::defer(world, owner, vmad.as_ref(), vec![effect]) {
                Ok(()) => continue,
                // No continuation queue: run it now, exactly as before the budget.
                Err(mut effects) => effects.remove(0),
            }
        } else {
            effect
        };
        advances.extend(run_papyrus_call(world, owner, vmad.as_ref(), effect));
    }
    flush_advances(world, advances);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(power: bool, sneak: bool) -> crate::HitEvent {
        crate::HitEvent {
            aggressor: 7,
            source: 0,
            projectile: 0,
            damage: 1.0,
            power_attack: power,
            sneak_attack: sneak,
            bash_attack: false,
            blocked: false,
        }
    }

    #[test]
    fn filters_follow_the_creation_kit_tri_state_and_match_flag() {
        let any = HitFilter::default();
        assert!(any.allows(&hit(true, false), false));
        let must_power = HitFilter { power: 1, ..any };
        assert!(must_power.allows(&hit(true, false), false));
        assert!(!must_power.allows(&hit(false, false), false));
        let no_sneak = HitFilter { sneak: 0, ..any };
        assert!(no_sneak.allows(&hit(false, false), false));
        assert!(!no_sneak.allows(&hit(false, true), false));
        let named = HitFilter {
            aggressor: Some(HitAggressor::Target(DistanceTarget::Reference(0x14))),
            ..any
        };
        assert!(named.allows(&hit(false, false), true));
        assert!(!named.allows(&hit(false, false), false), "not the named aggressor");
        let inverted = HitFilter { invert: true, ..named };
        assert!(inverted.allows(&hit(false, false), false), "abMatch false: anyone but the filter");
        assert!(!inverted.allows(&hit(false, false), true));
    }

    #[test]
    fn registrations_dedupe_unregister_and_drop_with_their_owner() {
        let mut registrations = HitRegistrations::default();
        let target = DistanceTarget::Alias { quest: 1, alias: 2 };
        let filter = HitFilter::default();
        for _ in 0..2 {
            registrations.request(0xFC00_0000, "S", &HitRequest::Register { target, filter });
        }
        registrations.request(0xFC00_0001, "S", &HitRequest::Register { target, filter });
        assert_eq!(registrations.entries().len(), 2, "one per owner, target and filter");
        registrations.request(0xFC00_0000, "s", &HitRequest::Unregister { target });
        assert_eq!(registrations.entries().len(), 1);
        registrations.drop_owners(&[0xFC00_0001]);
        assert!(registrations.entries().is_empty());
    }
}
