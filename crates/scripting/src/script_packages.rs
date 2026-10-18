//! Script packages — ObScript `AddScriptPackage` / `RemoveScriptPackage` (M47.3).
//!
//! The GECK *AddScriptPackage* page: "Add a script package to the calling
//! actor. This package takes precedence over all other packages until it is
//! done." Its notes: "An actor can have only one script package at a time. When
//! calling the function twice on the same actor, the second package replaces the
//! first one added" and "Unless the added package has some constraint on it
//! ('must complete' and/or 'must reach location'), it will be removed the next
//! time the actor reevaluates his package." *RemoveScriptPackage* "removes any
//! currently running script package ... Only necessary if you added a script
//! package which was set to run indefinitely."
//!
//! [`ScriptPackage`] is that one slot per actor. The ambient AI system
//! (`byroredux::npc_spawn::ai_package`) reads it ahead of the actor's own stack,
//! so the package wins without consulting its schedule or conditions, and tells
//! this module when it has installed it ([`mark_running`]).
//!
//! **What "the next time the actor reevaluates" means here.** The GECK
//! *How Are Packages Evaluated* page: "Packages seem to be re-evaluated every 10
//! seconds", on `EvaluatePackage`, and "when an existing package completes". The
//! ambient system re-selects once per in-game minute, which is not that cadence,
//! so the lifetime is counted on its own clock instead: an unconstrained script
//! package that has been running for [`SCRIPT_PACKAGE_REEVALUATE_SECS`] is gone.
//! A package that has not been installed yet is never dropped — the common
//! `AddScriptPackage X` followed by `EVP` idiom must start it, not discard it —
//! so an `EvaluatePackage` only drops a script package that is already running.
//! The constrained ones (the PKDT *Must reach location* / *Must complete* flags,
//! `0x2` / `0x4` in Oblivion's `PACK` and bit 1 / bit 2 of the FO3/FNV flag ids)
//! stay until removed, replaced, evaluated away, or completed.
//!
//! Saved: a package a script gave an actor is gameplay state.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Component, SparseSetStorage, World};

use crate::package::{queue_package_evaluation, PackageRegistry};

/// PKDT *Must reach location* (Oblivion `PACK`; FO3/FNV flag id 1).
const PKDT_MUST_REACH_LOCATION: u32 = 0x0000_0002;
/// PKDT *Must complete* (Oblivion `PACK`; FO3/FNV flag id 2).
const PKDT_MUST_COMPLETE: u32 = 0x0000_0004;

/// Seconds between package re-evaluations: the GECK *How Are Packages
/// Evaluated* page, "Packages seem to be re-evaluated every 10 seconds".
pub const SCRIPT_PACKAGE_REEVALUATE_SECS: f32 = 10.0;

/// The one script package an actor has.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ScriptPackage {
    /// The `PACK` FormID.
    pub package: u32,
    /// Whether the package is flagged *Must reach location* or *Must complete*.
    pub constrained: bool,
    /// Seconds the package has been the actor's running package; `None` until
    /// the ambient AI system has installed it.
    pub running_secs: Option<f32>,
}

impl Component for ScriptPackage {
    type Storage = SparseSetStorage<Self>;
}

impl ScriptPackage {
    pub fn new(package: u32, package_flags: u32) -> Self {
        Self {
            package,
            constrained: package_flags & (PKDT_MUST_REACH_LOCATION | PKDT_MUST_COMPLETE) != 0,
            running_secs: None,
        }
    }
}

/// `actor.AddScriptPackage <package>`: give `actor` the package, replacing any
/// script package it had, and have its packages re-evaluated. Declined (`false`)
/// for a package the load order does not define or an actor that cannot hold the
/// component — never an invented one.
pub fn add_script_package(world: &World, actor: EntityId, package: u32) -> bool {
    let flags = match world.try_resource::<PackageRegistry>() {
        Some(registry) => match registry.package(package) {
            Some(record) => record.package_flags,
            None => return false,
        },
        None => return false,
    };
    let Some(mut slots) = world.query_mut::<ScriptPackage>() else {
        return false;
    };
    slots.insert(actor, ScriptPackage::new(package, flags));
    drop(slots);
    queue_package_evaluation(world, actor)
}

/// `actor.RemoveScriptPackage`: remove the package and have the actor's packages
/// re-evaluated. `true` when the actor had one.
pub fn remove_script_package(world: &World, actor: EntityId) -> bool {
    let removed = drop_script_package(world, actor, false);
    if removed {
        queue_package_evaluation(world, actor);
    }
    removed
}

/// Remove `actor`'s script package; with `only_running`, only if the ambient
/// system has installed it. `true` when one was removed. Queues nothing.
pub(crate) fn drop_script_package(world: &World, actor: EntityId, only_running: bool) -> bool {
    let Some(mut slots) = world.query_mut::<ScriptPackage>() else {
        return false;
    };
    let droppable = slots
        .get(actor)
        .is_some_and(|slot| !only_running || slot.running_secs.is_some());
    if droppable {
        slots.remove(actor);
    }
    droppable
}

/// The package `actor`'s script gave it, if any.
pub fn script_package_of(world: &World, actor: EntityId) -> Option<ScriptPackage> {
    world.get::<ScriptPackage>(actor).map(|slot| *slot)
}

/// The ambient system installed `actor`'s script package: start its clock.
pub fn mark_running(world: &World, actor: EntityId) {
    if let Some(mut slots) = world.query_mut::<ScriptPackage>() {
        if let Some(slot) = slots.get_mut(actor) {
            slot.running_secs.get_or_insert(0.0);
        }
    }
}

/// The actors whose script package is running, for the ambient system's
/// completion check.
pub fn running_script_package_actors(world: &World) -> Vec<EntityId> {
    world
        .query::<ScriptPackage>()
        .map(|slots| {
            slots
                .iter()
                .filter(|(_, slot)| slot.running_secs.is_some())
                .map(|(actor, _)| actor)
                .collect()
        })
        .unwrap_or_default()
}

/// A running script package finished (a Travel arrival): drop it and have the
/// actor's packages re-evaluated.
pub fn finish_script_package(world: &World, actor: EntityId) {
    if drop_script_package(world, actor, true) {
        queue_package_evaluation(world, actor);
    }
}

/// Advance the running packages' clocks and drop the unconstrained ones that
/// have reached [`SCRIPT_PACKAGE_REEVALUATE_SECS`], re-evaluating their actors.
pub fn tick_script_packages(world: &World, dt: f32) {
    let expired: Vec<EntityId> = {
        let Some(mut slots) = world.query_mut::<ScriptPackage>() else {
            return;
        };
        let mut expired = Vec::new();
        for (actor, slot) in slots.iter_mut() {
            let Some(running) = slot.running_secs.as_mut() else {
                continue;
            };
            *running += dt;
            if !slot.constrained && *running >= SCRIPT_PACKAGE_REEVALUATE_SECS {
                expired.push(actor);
            }
        }
        for &actor in &expired {
            slots.remove(actor);
        }
        expired
    };
    for actor in expired {
        queue_package_evaluation(world, actor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EvaluatePackageRequest;
    use byroredux_plugin::esm::records::misc::pack::PackRecord;

    const PACKAGE: u32 = 0x0010_0001;
    const OTHER: u32 = 0x0010_0002;
    const CONSTRAINED: u32 = 0x0010_0003;

    fn world() -> (World, EntityId) {
        let mut world = World::new();
        crate::package::register(&mut world);
        world.register::<ScriptPackage>();
        let record = |form_id, package_flags| PackRecord { form_id, package_flags, ..Default::default() };
        world.insert_resource(PackageRegistry::from_records([
            record(PACKAGE, 0x0000_0400), // Once per day: not a constraint
            record(OTHER, 0),
            record(CONSTRAINED, PKDT_MUST_COMPLETE),
        ]));
        let actor = world.spawn();
        (world, actor)
    }

    fn requested(world: &World, actor: EntityId) -> bool {
        world.has::<EvaluatePackageRequest>(actor)
    }

    /// One slot per actor: the second package replaces the first (GECK note).
    #[test]
    fn a_second_script_package_replaces_the_first_and_queues_a_reevaluation() {
        let (world, actor) = world();
        assert!(add_script_package(&world, actor, PACKAGE));
        assert!(requested(&world, actor));
        assert!(add_script_package(&world, actor, OTHER));
        assert_eq!(script_package_of(&world, actor).map(|slot| slot.package), Some(OTHER));
    }

    /// A package the load order does not define is not invented.
    #[test]
    fn an_undefined_package_is_declined() {
        let (world, actor) = world();
        assert!(!add_script_package(&world, actor, 0x00FF_FFFF));
        assert_eq!(script_package_of(&world, actor), None);
        assert!(!requested(&world, actor));
    }

    #[test]
    fn the_must_flags_make_a_package_constrained() {
        assert!(!ScriptPackage::new(PACKAGE, 0x0000_0400).constrained);
        assert!(ScriptPackage::new(PACKAGE, PKDT_MUST_REACH_LOCATION).constrained);
        assert!(ScriptPackage::new(PACKAGE, PKDT_MUST_COMPLETE | 0x1).constrained);
    }

    #[test]
    fn remove_script_package_removes_it_and_reevaluates() {
        let (world, actor) = world();
        assert!(!remove_script_package(&world, actor), "nothing to remove");
        assert!(!requested(&world, actor));
        add_script_package(&world, actor, PACKAGE);
        world.query_mut::<EvaluatePackageRequest>().unwrap().remove(actor);
        assert!(remove_script_package(&world, actor));
        assert_eq!(script_package_of(&world, actor), None);
        assert!(requested(&world, actor));
    }

    /// An unconstrained package is dropped at the first re-evaluation after it
    /// began running — not before it was ever installed, and not for a
    /// constrained one.
    #[test]
    fn an_unconstrained_package_expires_only_once_running() {
        let (world, actor) = world();
        add_script_package(&world, actor, PACKAGE);
        tick_script_packages(&world, 60.0);
        assert!(script_package_of(&world, actor).is_some(), "never installed: it has not run yet");

        mark_running(&world, actor);
        world.query_mut::<EvaluatePackageRequest>().unwrap().remove(actor);
        tick_script_packages(&world, SCRIPT_PACKAGE_REEVALUATE_SECS - 1.0);
        assert!(script_package_of(&world, actor).is_some());
        tick_script_packages(&world, 1.0);
        assert_eq!(script_package_of(&world, actor), None);
        assert!(requested(&world, actor), "the actor re-evaluates when it lapses");
    }

    #[test]
    fn a_constrained_package_outlives_the_reevaluation_interval() {
        let (world, actor) = world();
        add_script_package(&world, actor, CONSTRAINED);
        mark_running(&world, actor);
        tick_script_packages(&world, 600.0);
        assert!(script_package_of(&world, actor).is_some());
    }

    /// Completion and `EvaluatePackage` only end a package that is running.
    #[test]
    fn evaluate_package_drops_a_running_package_but_not_a_pending_one() {
        let (world, actor) = world();
        add_script_package(&world, actor, CONSTRAINED);
        crate::package::request_package_evaluation(&world, actor);
        assert!(script_package_of(&world, actor).is_some(), "AddScriptPackage then EVP must start it");
        mark_running(&world, actor);
        assert_eq!(running_script_package_actors(&world), vec![actor]);
        crate::package::request_package_evaluation(&world, actor);
        assert_eq!(script_package_of(&world, actor), None, "EVP drops even a must-complete package");
    }

    #[test]
    fn finishing_a_running_package_drops_it_and_reevaluates() {
        let (world, actor) = world();
        add_script_package(&world, actor, CONSTRAINED);
        finish_script_package(&world, actor);
        assert!(script_package_of(&world, actor).is_some(), "not running: nothing finished");
        mark_running(&world, actor);
        world.query_mut::<EvaluatePackageRequest>().unwrap().remove(actor);
        finish_script_package(&world, actor);
        assert_eq!(script_package_of(&world, actor), None);
        assert!(requested(&world, actor));
    }
}
