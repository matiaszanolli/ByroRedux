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
//! - The actor waits until it is inside the package's `PLDT` location before the volley is raised
//!   (AIP-UW-001; it is not walked there), and the target until it is inside the `PLD2` location
//!   (AIP-UW-002): both are start gates, checked until the volley is raised, not per shot. A package with no
//!   `PTD2` fires along the actor's facing (AIP-UW-007). Always Hit (AIP-UW-004), Hold Fire When Blocked
//!   (AIP-UW-003) and the pause between volleys (AIP-UW-005) ride on the request's [`UseWeaponOptions`] and
//!   are honoured by [`super::script_requests::use_weapon_system`]. Crouch To Reload (AIP-UW-006) is not
//!   modelled: it waits on Phase 0 finding a crouched reload pose, and the flag stays unconsumed.
//! - Object type 12 (Weapons: Any) fires the actor's equipped weapon, else the first weapon in its inventory
//!   (AIP-UW-008; a Redux choice, PRD §6 row 14). Type 22 (Weapons: Melee) does the same restricted to melee animation
//!   types (AIP-UW-009, decision Q1); 21 and 23 stay declined as unsourced.
//! - The pause between volleys is the midpoint of the authored `PKW3` pause range (PRD §6 row 5), never
//!   shorter than the weapon's attack interval.
//! - The request waits for its target to be loaded; a package whose target
//!   never loads simply never fires.

use byroredux_core::ecs::components::{
    Dead, EquippedWeapon, GlobalTransform, Inventory, UseWeaponPackageBehavior, UseWeaponPackageStarted,
    USE_ACTORS_OWN_MELEE_WEAPON, USE_ACTORS_OWN_WEAPON, USE_FACING_DIRECTION,
};
use byroredux_core::ecs::{EntityId, World};
use byroredux_plugin::esm::records::{EsmIndex, ItemKind};
use byroredux_scripting::condition::resolve_entity_by_global_form_id;
use byroredux_scripting::obscript_more::{UseWeaponOptions, UseWeaponRequest};

/// Shots in one burst: the package's volley (the midpoint of its range), or for Auto Fire a full clip.
fn shots_per_burst(behavior: &UseWeaponPackageBehavior, index: &EsmIndex) -> u32 {
    if behavior.volley_shots > 0 {
        behavior.volley_shots
    } else {
        match index.items.get(&behavior.weapon_form_id).map(|item| &item.kind) {
            Some(ItemKind::Weapon { clip_size, .. }) => u32::from(*clip_size).max(1),
            _ => 1,
        }
    }
}

/// The shots the package asks for: `u32::MAX` for Repeat Fire.
fn requested_shots(behavior: &UseWeaponPackageBehavior, index: &EsmIndex) -> u32 {
    match behavior.bursts {
        Some(bursts) => bursts.saturating_mul(shots_per_burst(behavior, index)),
        None => u32::MAX,
    }
}

/// `WEAP` animation types Melee (1 Hand) and Melee (2 Hand) (xEdit `wbWeaponAnimTypeEnum`); Hand to Hand (0)
/// is a Phase 0 verify item and does not count.
const MELEE_ANIM_TYPES: [u8; 2] = [1, 2];

/// The weapon a class package fires: the equipped weapon, else the first weapon in the inventory (AIP-UW-008);
/// for the melee class (object type 22, AIP-UW-009) only a weapon with a melee animation type.
fn own_weapon(world: &World, actor: EntityId, index: &EsmIndex, melee_only: bool) -> Option<u32> {
    let fits = |base: &u32| match index.items.get(base).map(|item| &item.kind) {
        Some(ItemKind::Weapon { anim_type, .. }) => !melee_only || MELEE_ANIM_TYPES.contains(anim_type),
        _ => false,
    };
    // An equipped weapon outside the class does not stop the actor drawing one of the class from its pack.
    if let Some(equipped) = world.get::<EquippedWeapon>(actor).map(|e| e.base_form_id) {
        if !melee_only || fits(&equipped) {
            return Some(equipped);
        }
    }
    let inventory = world.get::<Inventory>(actor)?;
    inventory.items.iter().filter(|stack| stack.count > 0).map(|stack| stack.base_form_id).find(fits)
}

/// Whether `subject` (the actor for the `PLDT` location, AIP-UW-001; its target for `PLD2`, AIP-UW-002) stands
/// inside `place`. A location that cannot resolve yet (its reference is not loaded) is not inside.
fn inside_place(world: &World, subject: EntityId, place: (byroredux_core::ecs::components::EatSleepLocation, f32)) -> bool {
    use byroredux_core::ecs::components::EatSleepLocation;
    let (area, radius) = place;
    let Some(here) = world.get::<GlobalTransform>(subject).map(|t| t.translation) else {
        return false;
    };
    let Some(centre) = super::eat_sleep::resolve_anchor(world, subject, Some(here), area) else {
        return false;
    };
    let radius = match area {
        EatSleepLocation::InCell(_) => f32::INFINITY,
        _ if radius > 0.0 => radius,
        _ => super::locomotion::AI_PACKAGE_DEFAULT_RADIUS,
    };
    here.distance(centre) <= radius
}

pub(crate) fn use_weapon_package_system(world: &World, _dt: f32) {
    // An incapacitated actor does not start its volley (`use_weapon_system` would end it at once and
    // `UseWeaponPackageStarted` would stop the package asking again); it waits (#6467).
    let mut incapacitated = Vec::new();
    byroredux_scripting::collect_unconscious(world, &mut incapacitated);
    let pending: Vec<(EntityId, UseWeaponPackageBehavior)> = match world.query::<UseWeaponPackageBehavior>() {
        Some(behaviors) if !behaviors.is_empty() => behaviors
            .iter()
            .filter(|&(actor, _)| {
                !world.has::<UseWeaponPackageStarted>(actor)
                    && !world.has::<Dead>(actor)
                    && !incapacitated.contains(&actor)
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
        .try_resource::<crate::components::LoadedCellIndex>()
        .map(|index| index.0.clone())
    else {
        return;
    };
    for (actor, mut behavior) in pending {
        // AIP-UW-007: a package with no target (`USE_FACING_DIRECTION`) fires along the actor's facing and
        // waits for no one.
        let target = if behavior.target_form_id == USE_FACING_DIRECTION {
            None
        } else {
            match resolve_entity_by_global_form_id(world, behavior.target_form_id) {
                Some(target) => Some(target),
                None => continue,
            }
        };
        // AIP-UW-001: the volley is raised from inside the package's location.
        if behavior.place.is_some_and(|place| !inside_place(world, actor, place)) {
            continue;
        }
        // AIP-UW-002: and only once the target is inside the `PLD2` target location.
        if let (Some(target), Some(place)) = (target, behavior.target_place) {
            if !inside_place(world, target, place) {
                continue;
            }
        }
        // AIP-UW-008 / -009: a weapon class means the actor's own weapon, of that class.
        if behavior.weapon_form_id == USE_ACTORS_OWN_WEAPON || behavior.weapon_form_id == USE_ACTORS_OWN_MELEE_WEAPON {
            let Some(weapon) = own_weapon(world, actor, &index, behavior.weapon_form_id == USE_ACTORS_OWN_MELEE_WEAPON) else {
                continue;
            };
            behavior.weapon_form_id = weapon;
        }
        let request = UseWeaponRequest {
            weapon: behavior.weapon_form_id,
            target: behavior.target_form_id,
            shots: requested_shots(&behavior, &index),
            harmless: behavior.harmless,
            options: UseWeaponOptions {
                volley_shots: shots_per_burst(&behavior, &index),
                pause_secs: behavior.pause_secs,
                always_hit: behavior.always_hit,
                hold_fire_when_blocked: behavior.hold_fire_when_blocked,
            },
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
                kind: ItemKind::weapon_stub(3, clip_size).with_weapon_speed(1.0),
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
            place: None,
            target_place: None,
            always_hit: false,
            hold_fire_when_blocked: false,
            pause_secs: 0.0,
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
        world.insert_resource(crate::components::LoadedCellIndex(Arc::new(weapon_index(30))));
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
            Some(UseWeaponRequest { weapon: WEAPON, target: TARGET, shots: 10, harmless: true, options: UseWeaponOptions { volley_shots: 5, ..Default::default() } })
        );
        assert!(world.has::<UseWeaponPackageStarted>(actor));
        // The runtime consumes the request; the package must not re-raise it.
        world.query_mut::<UseWeaponRequest>().unwrap().remove(actor);
        use_weapon_package_system(&world, 0.0);
        assert_eq!(request_of(&world, actor), None, "one request per install");
    }

    /// AIP-UW-001 -- with a `PLDT` location the volley is raised only once the actor stands inside its radius.
    #[test]
    fn the_volley_waits_for_the_place_to_stand() {
        use byroredux_core::ecs::components::EatSleepLocation;
        let mut with_place = behavior(Some(1), 1, false);
        // The location is 500 units from the actor's current position; radius 100.
        with_place.place = Some((EatSleepLocation::NearCurrentLocation, 100.0));
        let (mut world, actor) = world_with(with_place, true);
        world.insert(actor, GlobalTransform::new(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY, 1.0));
        // NearCurrentLocation resolves to the actor's own position: always inside. Use a reference anchor for
        // the outside case instead.
        use_weapon_package_system(&world, 0.0);
        assert!(request_of(&world, actor).is_some(), "standing at the location: the volley is raised");

        let mut outside = behavior(Some(1), 1, false);
        outside.place = Some((EatSleepLocation::NearReference(0x7777), 100.0));
        let (mut world, actor) = world_with(outside, true);
        world.insert(actor, GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        use_weapon_package_system(&world, 0.0);
        assert_eq!(request_of(&world, actor), None, "the place cannot be resolved yet: the actor waits");
        assert!(!world.has::<UseWeaponPackageStarted>(actor));
    }

    /// AIP-UW-002 -- with a `PLD2` target location the volley is raised only once the target is inside it.
    #[test]
    fn the_volley_waits_for_the_target_location() {
        use byroredux_core::ecs::components::EatSleepLocation;
        // Target-relative NearCurrentLocation resolves to the target's own position: always inside.
        let mut inside = behavior(Some(1), 1, false);
        inside.target_place = Some((EatSleepLocation::NearCurrentLocation, 100.0));
        let (world, actor) = world_with(inside, true);
        use_weapon_package_system(&world, 0.0);
        assert!(request_of(&world, actor).is_some(), "the target is in its location: fire");

        // A location that cannot be resolved (its reference is not loaded) holds fire.
        let mut outside = behavior(Some(1), 1, false);
        outside.target_place = Some((EatSleepLocation::NearReference(0x7777), 100.0));
        let (world, actor) = world_with(outside, true);
        use_weapon_package_system(&world, 0.0);
        assert_eq!(request_of(&world, actor), None, "the target is not in its location: hold fire");
        assert!(!world.has::<UseWeaponPackageStarted>(actor), "and the package is still to start");
    }

    /// AIP-UW-007 -- a package with no target still raises its volley, aimed along the actor's facing, and does
    /// not wait for any reference to load.
    #[test]
    fn a_package_with_no_target_raises_its_volley_along_the_facing() {
        let mut facing = behavior(Some(1), 3, false);
        facing.target_form_id = USE_FACING_DIRECTION;
        // The world's target reference is not loaded; a facing shot must not wait for it.
        let (world, actor) = world_with(facing, false);
        use_weapon_package_system(&world, 0.0);
        let request = request_of(&world, actor).expect("fires without a target");
        assert_eq!(request.target, USE_FACING_DIRECTION);
        assert_eq!(request.shots, 3);
    }

    /// AIP-UW-003..005 -- the package's options ride on the request for the firing runtime.
    #[test]
    fn the_requests_carry_the_packages_options() {
        let mut options = behavior(Some(2), 5, false);
        options.always_hit = true;
        options.hold_fire_when_blocked = true;
        options.pause_secs = 4.0;
        let (world, actor) = world_with(options, true);
        use_weapon_package_system(&world, 0.0);
        assert_eq!(
            request_of(&world, actor).map(|r| r.options),
            Some(UseWeaponOptions { volley_shots: 5, pause_secs: 4.0, always_hit: true, hold_fire_when_blocked: true })
        );
    }

    /// AIP-UW-008 -- object type 12 fires the actor's equipped weapon, else its first inventory weapon; an actor
    /// with neither does not fire.
    #[test]
    fn a_weapon_class_package_fires_the_actors_own_weapon() {
        use byroredux_core::ecs::components::{Inventory, ItemStack};
        let mut own = behavior(Some(1), 1, false);
        own.weapon_form_id = USE_ACTORS_OWN_WEAPON;
        let (mut world, actor) = world_with(own, true);
        world.register::<Inventory>();
        world.register::<EquippedWeapon>();
        use_weapon_package_system(&world, 0.0);
        assert_eq!(request_of(&world, actor), None, "no weapon at all: nothing to fire");

        let mut inventory = Inventory::new();
        inventory.add(ItemStack::new(0xDEAD, 1));
        inventory.add(ItemStack::new(WEAPON, 1));
        world.insert(actor, inventory);
        use_weapon_package_system(&world, 0.0);
        assert_eq!(request_of(&world, actor).map(|r| r.weapon), Some(WEAPON), "the first inventory weapon (0xDEAD is no weapon)");
    }

    /// AIP-UW-009 (decision Q1) -- object type 22 fires only a melee-class weapon (WEAP animation type Melee 1
    /// Hand / 2 Hand): the equipped one when it fits, else the first inventory weapon that does. A rifle, a
    /// pistol, or Hand to Hand is not in the class.
    #[test]
    fn a_melee_class_package_fires_only_a_melee_weapon() {
        use byroredux_core::ecs::components::{Inventory, ItemStack};
        const RIFLE: u32 = 0xB1;
        const MELEE_2H: u32 = 0xB2;
        const FISTS: u32 = 0xB3;
        let (mut world, actor) = {
            let mut own = behavior(Some(1), 1, false);
            own.weapon_form_id = USE_ACTORS_OWN_MELEE_WEAPON;
            world_with(own, true)
        };
        // Replace the fixture's index with one holding the three class-test weapons.
        let mut index = weapon_index(1);
        for (base, anim) in [(RIFLE, 5u8), (MELEE_2H, 2), (FISTS, 0)] {
            let mut kind = ItemKind::weapon_stub(3, 1);
            if let ItemKind::Weapon { anim_type, .. } = &mut kind {
                *anim_type = anim;
            }
            index.items.insert(base, ItemRecord { form_id: base, common: CommonItemFields::default(), kind });
        }
        world.insert_resource(crate::components::LoadedCellIndex(Arc::new(index)));
        world.register::<Inventory>();
        world.register::<EquippedWeapon>();

        let mut inventory = Inventory::new();
        inventory.add(ItemStack::new(RIFLE, 1));
        inventory.add(ItemStack::new(FISTS, 1));
        world.insert(actor, inventory);
        use_weapon_package_system(&world, 0.0);
        assert_eq!(request_of(&world, actor), None, "a rifle and fists are not melee weapons: nothing to fire");

        let mut inventory = Inventory::new();
        inventory.add(ItemStack::new(RIFLE, 1));
        inventory.add(ItemStack::new(MELEE_2H, 1));
        world.insert(actor, inventory);
        use_weapon_package_system(&world, 0.0);
        assert_eq!(request_of(&world, actor).map(|r| r.weapon), Some(MELEE_2H), "the first melee weapon in the pack");
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

    /// #6467 — a knocked-down or fleeing actor (`AiSuspended`) does not start its volley, and the
    /// package is still to start, so it fires once the actor is back on its feet.
    #[test]
    fn an_incapacitated_actor_waits_to_start_its_volley() {
        let (mut world, actor) = world_with(behavior(None, 0, false), true);
        world.register::<byroredux_scripting::AiSuspended>();
        world.insert(actor, byroredux_scripting::AiSuspended);
        use_weapon_package_system(&world, 0.0);
        assert_eq!(request_of(&world, actor), None);
        assert!(!world.has::<UseWeaponPackageStarted>(actor));
        world.remove::<byroredux_scripting::AiSuspended>(actor);
        use_weapon_package_system(&world, 0.0);
        assert!(request_of(&world, actor).is_some(), "it starts once it is back on its feet");
    }
}
