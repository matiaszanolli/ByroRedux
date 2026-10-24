//! UseWeapon procedure (M42, FO3/FNV procedure 16) — fire a weapon at a
//! target per the package's fire data.
//!
//! The firing itself is the ObScript `UseWeapon` command's runtime
//! ([`super::script_requests::use_weapon_system`]: face the target, launch
//! the weapon's projectile or land the shot, repeat on the weapon's attack
//! interval). This system is the PACK half: an actor with a
//! [`UseWeaponPackageBehavior`] has one [`UseWeaponRequest`] raised for it —
//! once per install, guarded by [`UseWeaponPackageStarted`] — sized from the
//! package's fire count and carrying its Do No Damage flag.
//!
//! ## Shots
//!
//! - **Repeat Fire** never finishes: the request is unbounded and ends when
//!   the target or the actor dies, or when the shot cannot be taken for the
//!   volley's patience window.
//! - **Number of Bursts** is `bursts × burst size`. A volley-fire burst is
//!   the midpoint of the authored shots-per-volley range (the game picks
//!   randomly in it; the midpoint is deterministic). An auto-fire burst is a
//!   full clip, from the weapon's record.
//!
//! ## v0 scope
//!
//! - The actor fires from where it stands: the `PLDT` place to stand and the
//!   `PLD2` target location (wait until the target is inside it) are not
//!   modelled, nor is Crouch To Reload, Hold Fire When Blocked or Always Hit.
//! - The pause between volleys is the weapon's attack interval, not the
//!   authored `PKW3` pause.
//! - The request waits for its target to be loaded; a package whose target
//!   never loads simply never fires.

use byroredux_core::ecs::components::{
    Dead, UseWeaponPackageBehavior, UseWeaponPackageStarted,
};
use byroredux_core::ecs::{EntityId, World};
use byroredux_plugin::esm::records::{EsmIndex, ItemKind};
use byroredux_scripting::condition::resolve_entity_by_global_form_id;
use byroredux_scripting::obscript_more::UseWeaponRequest;

/// The shots the package asks for: `u32::MAX` for Repeat Fire.
fn requested_shots(behavior: &UseWeaponPackageBehavior, index: &EsmIndex) -> u32 {
    let Some(bursts) = behavior.bursts else {
        return u32::MAX;
    };
    let per_burst = if behavior.volley_shots > 0 {
        behavior.volley_shots
    } else {
        match index.items.get(&behavior.weapon_form_id).map(|item| &item.kind) {
            Some(ItemKind::Weapon { clip_size, .. }) => u32::from(*clip_size).max(1),
            _ => 1,
        }
    };
    bursts.saturating_mul(per_burst)
}

pub(crate) fn use_weapon_package_system(world: &World, _dt: f32) {
    let pending: Vec<(EntityId, UseWeaponPackageBehavior)> = match world.query::<UseWeaponPackageBehavior>() {
        Some(behaviors) if !behaviors.is_empty() => behaviors
            .iter()
            .filter(|&(actor, _)| {
                !world.has::<UseWeaponPackageStarted>(actor)
                    && !world.has::<Dead>(actor)
                    // A scripted volley already in hand: this one waits.
                    && !world.has::<super::script_requests::UseWeaponRun>(actor)
            })
            .map(|(actor, behavior)| (actor, *behavior))
            .collect(),
        _ => return,
    };
    if pending.is_empty() {
        return;
    }
    let Some(index) = world
        .try_resource::<crate::cell_loader::LoadedCellIndex>()
        .map(|index| index.0.clone())
    else {
        return;
    };
    for (actor, behavior) in pending {
        if resolve_entity_by_global_form_id(world, behavior.target_form_id).is_none() {
            continue;
        }
        let request = UseWeaponRequest {
            weapon: behavior.weapon_form_id,
            target: behavior.target_form_id,
            shots: requested_shots(&behavior, &index),
            harmless: behavior.harmless,
        };
        if let Some(mut requests) = world.query_mut::<UseWeaponRequest>() {
            requests.insert(actor, request);
        }
        if let Some(mut started) = world.query_mut::<UseWeaponPackageStarted>() {
            started.insert(actor, UseWeaponPackageStarted);
        }
        log::info!(
            "UseWeapon package: actor {actor} fires {:08X} at {:08X} ({} shots{})",
            request.weapon,
            request.target,
            if request.shots == u32::MAX { "repeat".to_string() } else { request.shots.to_string() },
            if request.harmless { ", harmless" } else { "" },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::{FormIdComponent, GlobalTransform};
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    use byroredux_core::math::{Quat, Vec3};
    use byroredux_plugin::esm::records::{common::CommonItemFields, ItemRecord};
    use std::sync::Arc;

    const WEAPON: u32 = 0xA1;
    const TARGET: u32 = 0x0001_59E2;

    fn weapon_index(clip_size: u16) -> EsmIndex {
        let mut index = EsmIndex::default();
        index.items.insert(
            WEAPON,
            ItemRecord {
                form_id: WEAPON,
                common: CommonItemFields::default(),
                kind: ItemKind::Weapon {
                    ammo_form: 0,
                    damage: 3,
                    clip_size,
                    anim_type: 0,
                    ap_cost: 0.0,
                    skill_form: 0,
                    min_spread: 0.0,
                    spread: 0.0,
                    crit_mult: 1.0,
                    reach: 0.0,
                    speed: 1.0,
                    reload_anim: 0,
                    vats: None,
                },
            },
        );
        index
    }

    fn behavior(bursts: Option<u32>, volley_shots: u32, harmless: bool) -> UseWeaponPackageBehavior {
        UseWeaponPackageBehavior {
            weapon_form_id: WEAPON,
            target_form_id: TARGET,
            bursts,
            volley_shots,
            harmless,
        }
    }

    #[test]
    fn shots_follow_the_fire_count_and_burst_size() {
        let index = weapon_index(30);
        assert_eq!(requested_shots(&behavior(None, 0, false), &index), u32::MAX, "repeat fire is unbounded");
        assert_eq!(requested_shots(&behavior(Some(3), 4, false), &index), 12, "3 volleys of 4");
        assert_eq!(requested_shots(&behavior(Some(2), 0, false), &index), 60, "auto fire: 2 clips of 30");
        assert_eq!(requested_shots(&behavior(Some(2), 0, false), &weapon_index(0)), 2, "a clip-less weapon is one shot a burst");
    }

    fn world_with(actor_behavior: UseWeaponPackageBehavior, target_loaded: bool) -> (World, EntityId) {
        let mut world = World::new();
        world.register::<UseWeaponPackageBehavior>();
        world.register::<UseWeaponPackageStarted>();
        world.register::<UseWeaponRequest>();
        world.register::<super::super::script_requests::UseWeaponRun>();
        world.register::<Dead>();
        world.register::<FormIdComponent>();
        world.register::<GlobalTransform>();
        world.insert_resource(crate::cell_loader::LoadedCellIndex(Arc::new(weapon_index(30))));
        let mut pool = FormIdPool::new();
        let fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(TARGET),
        });
        world.insert_resource(pool);
        if target_loaded {
            let target = world.spawn();
            world.insert(target, FormIdComponent(fid));
            world.insert(target, GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        }
        let actor = world.spawn();
        world.insert(actor, actor_behavior);
        (world, actor)
    }

    fn request_of(world: &World, actor: EntityId) -> Option<UseWeaponRequest> {
        world.get::<UseWeaponRequest>(actor).map(|request| *request)
    }

    #[test]
    fn the_package_requests_its_volleys_once_with_its_flags() {
        let (world, actor) = world_with(behavior(Some(2), 5, true), true);
        use_weapon_package_system(&world, 0.0);
        assert_eq!(
            request_of(&world, actor),
            Some(UseWeaponRequest { weapon: WEAPON, target: TARGET, shots: 10, harmless: true })
        );
        assert!(world.has::<UseWeaponPackageStarted>(actor));
        // The runtime consumes the request; the package must not re-raise it.
        world.query_mut::<UseWeaponRequest>().unwrap().remove(actor);
        use_weapon_package_system(&world, 0.0);
        assert_eq!(request_of(&world, actor), None, "one request per install");
    }

    #[test]
    fn nothing_is_requested_before_the_target_loads_or_for_the_dead() {
        let (world, actor) = world_with(behavior(None, 0, false), false);
        use_weapon_package_system(&world, 0.0);
        assert_eq!(request_of(&world, actor), None, "the target is not loaded yet");
        assert!(!world.has::<UseWeaponPackageStarted>(actor), "so the package is still to start");

        let (mut world, actor) = world_with(behavior(None, 0, false), true);
        world.insert(actor, Dead);
        use_weapon_package_system(&world, 0.0);
        assert_eq!(request_of(&world, actor), None, "a dead actor does not fire");
    }
}
