//! M47.2 — Papyrus distance events: `RegisterForDistanceLessThanEvent` /
//! `RegisterForDistanceGreaterThanEvent` / `UnregisterForDistanceEvents`,
//! delivering `OnDistanceLessThan` / `OnDistanceGreaterThan` to the script
//! that registered.
//!
//! The FO4 CK's *RegisterForDistanceLessThanEvent - ScriptObject*:
//! - the registering script gets a *single* event when two objects are closer
//!   than the distance, "sent immediately" if they already are;
//! - an object is an `ObjectReference` or a `ReferenceAlias` (the alias
//!   resolves to its reference when the check runs, so an alias filled later
//!   counts from then on);
//! - the two objects are interchangeable: re-registering a pair in either
//!   order only updates the distance, and the event carries the pair in the
//!   order it was first registered;
//! - a script holds one less-than and one greater-than registration per pair;
//! - the receiver need not be either object, and a quest's registrations end
//!   when the quest stops;
//! - `UnregisterForDistanceEvents(a, b)` removes both kinds for the pair.
//!
//! The distance is the one the `GetDistance` guard reads: between the
//! references' world positions. A reference with no placement is infinitely
//! far, so it triggers a greater-than event and never a less-than one.

use super::*;
use crate::translate::effects::VmStored;

/// An object a distance registration names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum DistanceTarget {
    Reference(u32),
    /// A `ReferenceAlias`, resolved when the distance is checked.
    Alias { quest: u32, alias: i16 },
}

/// One script instance's pending distance registration.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct DistanceWatch {
    /// The object the script is attached to: a quest or a reference.
    pub owner: u32,
    pub script: String,
    pub a: DistanceTarget,
    pub b: DistanceTarget,
    pub distance: f32,
    /// `OnDistanceLessThan` (otherwise `OnDistanceGreaterThan`).
    pub less: bool,
}

impl DistanceWatch {
    fn event(&self) -> &'static str {
        if self.less {
            "OnDistanceLessThan"
        } else {
            "OnDistanceGreaterThan"
        }
    }

    fn same_pair(&self, a: DistanceTarget, b: DistanceTarget) -> bool {
        (self.a == a && self.b == b) || (self.a == b && self.b == a)
    }
}

/// A distance registration change a script run asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DistanceRequest {
    Watch {
        a: DistanceTarget,
        b: DistanceTarget,
        distance: f32,
        less: bool,
    },
    Unwatch {
        a: DistanceTarget,
        b: DistanceTarget,
    },
}

impl DistanceRequest {
    /// The handler a call delivers to (`""` for the unregistration), by
    /// lowercase call name.
    pub(super) fn handler(name: &str) -> Option<&'static str> {
        match name {
            "registerfordistancelessthanevent" => Some("OnDistanceLessThan"),
            "registerfordistancegreaterthanevent" => Some("OnDistanceGreaterThan"),
            "unregisterfordistanceevents" => Some(""),
            _ => None,
        }
    }

    /// Whether a call's arguments are the shape the check can run: two
    /// objects, and for a registration a number.
    pub(super) fn arity(name: &str) -> usize {
        if name == "unregisterfordistanceevents" {
            2
        } else {
            3
        }
    }

    pub(super) fn of_call(name: &str, a: DistanceTarget, b: DistanceTarget, distance: Option<f32>) -> Option<Self> {
        match name {
            "registerfordistancelessthanevent" => Some(Self::Watch {
                a,
                b,
                distance: distance?,
                less: true,
            }),
            "registerfordistancegreaterthanevent" => Some(Self::Watch {
                a,
                b,
                distance: distance?,
                less: false,
            }),
            "unregisterfordistanceevents" => Some(Self::Unwatch { a, b }),
            _ => None,
        }
    }
}

/// Every pending distance registration. Saved.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct DistanceWatches {
    watches: Vec<DistanceWatch>,
}

impl Resource for DistanceWatches {}

impl DistanceWatches {
    pub fn watches(&self) -> &[DistanceWatch] {
        &self.watches
    }

    /// Apply `request` from `owner`'s instance of `script`.
    pub fn request(&mut self, owner: u32, script: &str, request: DistanceRequest) {
        let mine = |w: &DistanceWatch| w.owner == owner && w.script.eq_ignore_ascii_case(script);
        match request {
            DistanceRequest::Watch { a, b, distance, less } => {
                // One registration per pair and kind: a second only updates
                // the distance.
                if let Some(existing) = self
                    .watches
                    .iter_mut()
                    .find(|w| mine(w) && w.less == less && w.same_pair(a, b))
                {
                    existing.distance = distance;
                    return;
                }
                self.watches.push(DistanceWatch {
                    owner,
                    script: script.to_owned(),
                    a,
                    b,
                    distance,
                    less,
                });
            }
            DistanceRequest::Unwatch { a, b } => self.watches.retain(|w| !(mine(w) && w.same_pair(a, b))),
        }
    }
}

/// The entity a target stands for right now.
pub(super) fn target_entity(world: &World, target: DistanceTarget) -> Option<EntityId> {
    match target {
        DistanceTarget::Reference(form) => {
            if form == 0x14 {
                return crate::papyrus_demo::PapyrusPlayerEntity::get(world);
            }
            crate::condition::resolve_entity_by_global_form_id(world, form)
        }
        DistanceTarget::Alias { quest, alias } => world
            .try_resource::<crate::SceneActorBindings>()
            .and_then(|bindings| bindings.resolve(QuestFormId(quest), i32::from(alias))),
    }
}

/// The form a delivered event names an object by.
pub(super) fn entity_form(world: &World, entity: EntityId) -> Option<u32> {
    let player = crate::papyrus_demo::PapyrusPlayerEntity::get(world);
    if Some(entity) == player {
        return Some(0x14);
    }
    crate::condition::entity_global_form_id(world, entity)
}

/// Deliver the distance events now true. Checked every frame the registrations
/// exist; each delivers once and is removed.
pub fn distance_event_system(world: &World, _dt: f32) {
    let candidates: Vec<(usize, DistanceWatch)> = match world.try_resource::<DistanceWatches>() {
        Some(watches) if !watches.watches.is_empty() => {
            watches.watches.iter().cloned().enumerate().collect()
        }
        _ => return,
    };
    let owners = super::owners::ScriptOwners::new(world);
    // Quest standing first, released before anything else is read: a quest
    // delivers while it runs and drops its registrations when it stops.
    let standing: Vec<Option<bool>> = {
        let stages = world.resource::<QuestStageState>();
        candidates.iter().map(|(_, w)| owners.standing(world, &stages, w.owner)).collect()
    };
    let mut dropped = Vec::new();
    let mut fired: Vec<(usize, DistanceWatch, u32, u32, f32)> = Vec::new();
    let mut starts = super::budget::runs_startable(world);
    for ((index, watch), standing) in candidates.into_iter().zip(standing) {
        match standing {
            Some(false) => {
                dropped.push(index);
                continue;
            }
            Some(true) => {}
            // A reference's registration waits while it is unloaded.
            None if ReferenceScriptIndex::lookup(world, watch.owner).is_some() => {}
            None => continue,
        }
        let ends = (target_entity(world, watch.a), target_entity(world, watch.b));
        let distance = match ends {
            (Some(a), Some(b)) => {
                use byroredux_core::ecs::components::GlobalTransform;
                match (world.get::<GlobalTransform>(a), world.get::<GlobalTransform>(b)) {
                    (Some(a), Some(b)) => (a.translation - b.translation).length(),
                    _ => f32::MAX,
                }
            }
            // An unfilled alias or an unloaded reference: nothing to measure.
            _ => continue,
        };
        let met = if watch.less { distance < watch.distance } else { distance > watch.distance };
        if !met {
            continue;
        }
        let (Some(a), Some(b)) = ends else { continue };
        let (Some(form_a), Some(form_b)) = (entity_form(world, a), entity_form(world, b)) else {
            continue;
        };
        // A due registration the frame's interpreter allowance cannot start
        // stays registered for a later frame.
        if starts == 0 {
            continue;
        }
        starts -= 1;
        fired.push((index, watch, form_a, form_b, distance));
    }
    if !dropped.is_empty() || !fired.is_empty() {
        if let Some(mut watches) = world.try_resource_mut::<DistanceWatches>() {
            let mut remove: Vec<usize> = dropped;
            remove.extend(fired.iter().map(|(index, ..)| *index));
            remove.sort_unstable();
            let mut position = 0;
            watches.watches.retain(|_| {
                let keep = remove.binary_search(&position).is_err();
                position += 1;
                keep
            });
        }
    }
    let mut advances = Vec::new();
    for (_, watch, form_a, form_b, distance) in fired {
        let owner = QuestFormId(watch.owner);
        let event = watch.event();
        if !owners.handler_runs(watch.owner, &watch.script, event) {
            log::debug!("{:08X}: {}.{event} skipped: the Papyrus VM does not run it", watch.owner, watch.script);
            continue;
        }
        let vmad = owners.vmad(world, watch.owner, &watch.script);
        let effect = Effect::Interpret {
            script: watch.script.clone(),
            function: event.to_owned(),
            args: vec![VmStored::Form(form_a), VmStored::Form(form_b), VmStored::Float(distance)],
            resume: None,
        };
        log::debug!("{:08X}: {}.{event}({form_a:08X}, {form_b:08X}, {distance})", watch.owner, watch.script);
        advances.extend(run_papyrus_call(world, owner, vmad.as_ref(), effect));
    }
    flush_advances(world, advances);
}
