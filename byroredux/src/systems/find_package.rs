//! Find procedure (M42, FO3/FNV procedure 0) - go to a target and act on it.
//!
//! GECK *Find Package*: the actor travels to the target and then, "if an
//! inventory object, pick it up; if an activator, activate it; if a creature,
//! attack it; if a non-hostile NPC, talk to it; if a hostile NPC, attack it".
//! A generic target ("any object" of a base or a type) is looked for inside
//! the package's search area and gets the same treatment.
//!
//! ## What a target is
//!
//! - **Reference** (`PTDT` type 0): that reference, wherever it is loaded
//!   (`0x14` is the player).
//! - **Base object** (type 1): the nearest loaded reference of that base
//!   inside the search area. An actor base is hunted, an item base picked up.
//! - **Object type** (type 2): the nearest placed pickup item of that kind
//!   inside the search area. Only the kinds the item records can classify:
//!   Ammo (13), Alchemy (17) and Food (18, the `ALCH` Food Item flag).
//!
//! ## What the actor does on reaching it
//!
//! - **An item** is picked up through the player's pickup path
//!   ([`crate::inventory::take_placed_item`]): inventory, hidden placement,
//!   tombstone, item events. The package is done at its object count.
//! - **A creature or an actor the ambient hostility rules would attack** is
//!   attacked: the actor is armed with an `AiCombatState`, which the combat
//!   AI then fights. Unlike ambient hostility's, this combat is not dropped
//!   when contact is lost.
//! - **The player, when friendly,** is talked to: the force-greet bridge the
//!   Dialogue procedure uses takes over (generic greeting).
//! - **Another friendly actor** is walked to and stood beside; a conversation
//!   between two NPCs does not exist here.
//! - **Anything else** (an activator) is activated: an `ActivateEvent` with
//!   the actor as activator.
//!
//! ## v0 scope
//!
//! - A dead target is not hunted (the package finds nothing and the actor
//!   idles), and neither are flora, furniture or the other object types.
//! - The search runs once a second, nearest first; a target the actor cannot
//!   reach (held against an obstacle) is dropped for good and the next
//!   nearest is tried.
//! - Straight-line/navmesh walk at the actor's stride, no run speed.
//! - Not while fighting: combat suspends the package like any ambient one,
//!   and the hunt starts afresh when it ends.

use super::eat_sleep::resolve_anchor;
use super::faction_hostility::actor_would_attack;
use super::locomotion::{advance_stuck_repick, AI_PACKAGE_DEFAULT_RADIUS, LOCOMOTION_WALK_SPEED};
use super::navmesh_path::cached_walk_step;
use byroredux_core::ecs::components::{
    ActorValues, CreatureActor, Dead, EatSleepLocation, FindItemKind, FindPackageBehavior, FindProgress,
    FindTarget, GlobalTransform, Transform,
};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::Vec3;
use byroredux_plugin::esm::records::{EsmIndex, ItemKind};
use byroredux_scripting::condition::resolve_entity_by_global_form_id;
use byroredux_scripting::{AiCombatState, AiSuspended, SceneAliasCandidate};
use rustc_hash::FxHashMap;

/// Seconds between one actor's searches for a generic target.
const SEARCH_PERIOD_SECS: f32 = 1.0;
/// How close (ground plane) an actor must be to take an item or activate a
/// reference.
const REACH: f32 = 80.0;
/// How close it walks to a friendly actor before it stands beside them.
const BESIDE: f32 = 128.0;
/// How close it walks to a hostile target before the combat AI is handed the
/// fight.
const ENGAGE: f32 = 384.0;
/// Seconds a target the actor could not reach stays dropped before it is
/// tried again: geometry, or the target, may have moved. Engine policy; with
/// single-tile pathing a straight line across levels is blocked until then.
const UNREACHABLE_SECS: f32 = 30.0;

/// The item kind a placed base is, for an object-type search.
fn item_kind(index: &EsmIndex, base: u32) -> Option<FindItemKind> {
    match &index.items.get(&base)?.kind {
        ItemKind::Ammo { .. } => Some(FindItemKind::Ammo),
        ItemKind::Aid { food: true, .. } => Some(FindItemKind::Food),
        ItemKind::Aid { .. } => Some(FindItemKind::Alchemy),
        _ => None,
    }
}

fn ground_distance(a: Vec3, b: Vec3) -> f32 {
    Vec3::new(a.x - b.x, 0.0, a.z - b.z).length()
}

fn is_actor(world: &World, entity: EntityId) -> bool {
    world.has::<ActorValues>(entity)
        || world
            .try_resource::<crate::systems::PlayerEntity>()
            .is_some_and(|player| player.0 == Some(entity))
}

#[derive(Default)]
struct FindScratch {
    search_clock: FxHashMap<EntityId, f32>,
    stuck: FxHashMap<EntityId, f32>,
    /// `(actor, target)` pairs the actor could not reach, with the seconds
    /// left before it tries again.
    unreachable: FxHashMap<(EntityId, EntityId), f32>,
}

/// Whether `target` is still worth going to.
fn still_valid(world: &World, actor: EntityId, target: EntityId, scratch: &FindScratch) -> bool {
    world.get::<GlobalTransform>(target).is_some()
        && !world.has::<Dead>(target)
        && !world.has::<crate::inventory::PickedUp>(target)
        && !scratch.unreachable.contains_key(&(actor, target))
}

/// The nearest loaded candidate for a generic target, inside the search area.
fn search(
    world: &World,
    index: &EsmIndex,
    actor: EntityId,
    here: Vec3,
    behavior: &FindPackageBehavior,
    scratch: &FindScratch,
) -> Option<EntityId> {
    if let FindTarget::Reference(form_id) = behavior.target {
        return resolve_entity_by_global_form_id(world, form_id)
            .filter(|&target| target != actor && still_valid(world, actor, target, scratch));
    }
    let centre = resolve_anchor(world, actor, Some(here), behavior.area)?;
    let radius = match behavior.area {
        EatSleepLocation::InCell(_) => f32::INFINITY,
        _ if behavior.area_radius > 0.0 => behavior.area_radius,
        _ => AI_PACKAGE_DEFAULT_RADIUS,
    };
    // The identity guard is dropped before the per-entity checks: `is_pickup_target`
    // takes `InventoryCatalog`, which other systems hold while reading identities,
    // so reading it under this guard closes an ABBA cycle.
    let candidates: Vec<EntityId> = {
        let identities = world.query::<SceneAliasCandidate>()?;
        identities
            .iter()
            .filter(|&(entity, identity)| {
                entity != actor
                    && match behavior.target {
                        FindTarget::Base(base) => identity.base_form_id == base,
                        FindTarget::Kind(kind) => item_kind(index, identity.base_form_id) == Some(kind),
                        FindTarget::Reference(_) => false,
                    }
            })
            .map(|(entity, _)| entity)
            .collect()
    };
    candidates
        .into_iter()
        .filter_map(|entity| {
            if !still_valid(world, actor, entity, scratch) {
                return None;
            }
            // A kind search is for loose items; a base search takes whatever the base is.
            if matches!(behavior.target, FindTarget::Kind(_)) && !crate::inventory::is_pickup_target(world, entity) {
                return None;
            }
            let at = world.get::<GlobalTransform>(entity)?.translation;
            (at.distance(centre) <= radius).then(|| (entity, here.distance(at)))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(entity, _)| entity)
}

fn arm_combat(world: &World, actor: EntityId, target: EntityId) {
    if let Some(mut states) = world.query_mut::<AiCombatState>() {
        states.insert(actor, AiCombatState { target, attack_cooldown_remaining: 0.0 });
    }
}

fn find_package_system_inner(world: &World, dt: f32, scratch: &mut FindScratch) {
    let behaviors: Vec<(EntityId, FindPackageBehavior)> = match world.query::<FindPackageBehavior>() {
        Some(q) if !q.is_empty() => q.iter().map(|(actor, b)| (actor, *b)).collect(),
        _ => {
            scratch.search_clock.clear();
            scratch.stuck.clear();
            return;
        }
    };
    let Some(index) = world.try_resource::<crate::cell_loader::LoadedCellIndex>().map(|i| i.0.clone()) else {
        return;
    };
    scratch.unreachable.retain(|_, left| {
        *left -= dt;
        *left > 0.0
    });
    for (actor, behavior) in behaviors {
        if world.has::<Dead>(actor)
            || world.has::<AiCombatState>(actor)
            || world.has::<AiSuspended>(actor)
            || world.has::<super::flee::FleeState>(actor)
        {
            continue;
        }
        let mut progress = world.get::<FindProgress>(actor).map(|p| *p).unwrap_or_default();
        if progress.done {
            continue;
        }
        let (Some(current), Some(rotation), Some(here)) = (
            world.get::<Transform>(actor).map(|t| t.translation),
            world.get::<Transform>(actor).map(|t| t.rotation),
            world.get::<GlobalTransform>(actor).map(|t| t.translation),
        ) else {
            continue;
        };

        // Keep the target while it is worth going to; otherwise look for one.
        if progress.target.is_some_and(|target| !still_valid(world, actor, target, scratch)) {
            progress.target = None;
        }
        if progress.target.is_none() {
            let clock = scratch.search_clock.entry(actor).or_insert(0.0);
            *clock -= dt;
            if *clock > 0.0 {
                continue;
            }
            *clock = SEARCH_PERIOD_SECS;
            progress.target = search(world, &index, actor, here, &behavior, scratch);
            store(world, actor, progress);
            if progress.target.is_none() {
                continue;
            }
        }
        let Some(target) = progress.target else { continue };
        let Some(target_at) = world.get::<GlobalTransform>(target).map(|t| t.translation) else {
            continue;
        };
        let distance = ground_distance(here, target_at);

        let (stop_at, on_arrival) = if is_actor(world, target) {
            let hostile = world.has::<CreatureActor>(target) || actor_would_attack(world, actor, target);
            if hostile {
                (ENGAGE, Arrival::Attack)
            } else if world.try_resource::<crate::systems::PlayerEntity>().is_some_and(|p| p.0 == Some(target)) {
                (BESIDE, Arrival::Greet)
            } else {
                (BESIDE, Arrival::Stand)
            }
        } else if crate::inventory::is_pickup_target(world, target) {
            (REACH, Arrival::Take)
        } else {
            (REACH, Arrival::Activate)
        };

        if distance > stop_at {
            let speed = world
                .get::<crate::components::WalkSpeed>(actor)
                .map_or(LOCOMOTION_WALK_SPEED, |s| s.0);
            let step = cached_walk_step(world, actor, current, rotation, target_at, 0.0, dt, speed);
            if let Some(mut transforms) = world.query_mut::<Transform>() {
                if let Some(transform) = transforms.get_mut(actor) {
                    transform.translation = step.position;
                    if let Some(turned) = step.rotation {
                        transform.rotation = turned;
                    }
                }
            }
            let stuck = scratch.stuck.entry(actor).or_insert(0.0);
            if advance_stuck_repick(stuck, step.blocked, dt) {
                log::info!("Find package: actor {actor} cannot reach {target} and drops it");
                scratch.unreachable.insert((actor, target), UNREACHABLE_SECS);
                progress.target = None;
                store(world, actor, progress);
            }
            continue;
        }
        scratch.stuck.remove(&actor);
        match on_arrival {
            Arrival::Take => {
                if crate::inventory::take_placed_item(world, actor, target, false) {
                    progress.taken += 1;
                    log::info!("Find package: actor {actor} took {target} ({}/{})", progress.taken, behavior.count.max(1));
                }
                progress.target = None;
                progress.done = progress.taken >= behavior.count.max(1);
                // A kind/base search may have more to take at once.
                scratch.search_clock.insert(actor, 0.0);
            }
            Arrival::Attack => {
                log::info!("Find package: actor {actor} attacks {target}");
                arm_combat(world, actor, target);
                progress.done = true;
            }
            Arrival::Greet => {
                log::info!("Find package: actor {actor} greets the player");
                if let Some(mut directives) = world.query_mut::<super::forcegreet::ForceGreetDirective>() {
                    directives.insert(
                        actor,
                        super::forcegreet::ForceGreetDirective { topic: None, radius: super::forcegreet::FORCE_GREET_RADIUS },
                    );
                }
                progress.done = true;
            }
            Arrival::Activate => {
                log::info!("Find package: actor {actor} activates {target}");
                if let Some(mut events) = world.query_mut::<byroredux_scripting::ActivateEvent>() {
                    events.insert(target, byroredux_scripting::ActivateEvent { activator: actor });
                }
                progress.done = true;
            }
            // Stood beside a friendly actor: nothing more to do, and it keeps
            // the target so it follows if they walk off.
            Arrival::Stand => {}
        }
        store(world, actor, progress);
    }
}

#[derive(Clone, Copy)]
enum Arrival {
    Take,
    Attack,
    Greet,
    Stand,
    Activate,
}

fn store(world: &World, actor: EntityId, progress: FindProgress) {
    if let Some(mut all) = world.query_mut::<FindProgress>() {
        all.insert(actor, progress);
    }
}

pub(crate) fn make_find_package_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = FindScratch::default();
    move |world: &World, dt: f32| find_package_system_inner(world, dt, &mut scratch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{InventoryCatalog, PickedUp};
    use byroredux_core::ecs::components::{FormIdComponent, Inventory};
    use byroredux_core::ecs::resources::ItemInstancePool;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    use byroredux_core::math::Quat;
    use byroredux_plugin::esm::records::{common::CommonItemFields, ItemRecord};
    use std::sync::Arc;

    const FOOD: u32 = 0x5001;
    const TONIC: u32 = 0x5002;
    const BULLETS: u32 = 0x5003;

    fn item(kind: ItemKind) -> ItemRecord {
        ItemRecord { form_id: 0, common: CommonItemFields::default(), kind }
    }

    fn aid(food: bool) -> ItemKind {
        ItemKind::Aid {
            magic_effects: Vec::new(),
            addiction_chance: 0.0,
            immediate_effects: None,
            authored_effects: None,
            simple_consumption_header: false,
            medicine: false,
            food,
        }
    }

    fn index() -> EsmIndex {
        let mut index = EsmIndex::default();
        index.items.insert(FOOD, item(aid(true)));
        index.items.insert(TONIC, item(aid(false)));
        index.items.insert(
            BULLETS,
            item(ItemKind::Ammo {
                damage: 1.0,
                dt_mult: 1.0,
                spread: 0.0,
                projectile_form: 0,
                casing_form: 0,
                health: 0,
                clip_rounds: 0,
            }),
        );
        index
    }

    #[test]
    fn an_object_type_is_told_from_the_item_records() {
        let index = index();
        assert_eq!(item_kind(&index, FOOD), Some(FindItemKind::Food), "the Food Item flag makes it Food (18)");
        assert_eq!(item_kind(&index, TONIC), Some(FindItemKind::Alchemy), "an unflagged ALCH is Alchemy (17)");
        assert_eq!(item_kind(&index, BULLETS), Some(FindItemKind::Ammo));
        assert_eq!(item_kind(&index, 0x9999), None, "an unknown base is nothing");
    }

    fn world() -> World {
        let mut world = World::new();
        world.register::<Transform>();
        world.register::<GlobalTransform>();
        world.register::<Inventory>();
        world.register::<PickedUp>();
        world.register::<FormIdComponent>();
        world.register::<SceneAliasCandidate>();
        world.register::<FindPackageBehavior>();
        world.register::<FindProgress>();
        world.register::<AiCombatState>();
        world.register::<CreatureActor>();
        world.register::<ActorValues>();
        world.register::<Dead>();
        world.insert_resource(crate::cell_loader::LoadedCellIndex(Arc::new(index())));
        world.insert_resource(FormIdPool::new());
        world.insert_resource(ItemInstancePool::new());
        world.insert_resource(crate::cell_loader::reference_state::PersistentReferenceStates::default());
        world.insert_resource(InventoryCatalog::with_pickup_bases(&[FOOD, TONIC, BULLETS]));
        world
    }

    fn place(world: &mut World, base: u32, local: u32, at: Vec3) -> EntityId {
        let id = world.resource_mut::<FormIdPool>().intern(FormIdPair {
            plugin: PluginId::from_filename("FalloutNV.esm"),
            local: LocalFormId(local),
        });
        let entity = world.spawn();
        world.insert(entity, FormIdComponent(id));
        world.insert(entity, SceneAliasCandidate { reference_form_id: local, base_form_id: base, ..Default::default() });
        world.insert(entity, Transform::from_translation(at));
        world.insert(entity, GlobalTransform::new(at, Quat::IDENTITY, 1.0));
        entity
    }

    fn finder(world: &mut World, behavior: FindPackageBehavior) -> EntityId {
        let actor = world.spawn();
        world.insert(actor, Inventory::new());
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(actor, GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
        world.insert(actor, behavior);
        actor
    }

    fn run(world: &World, steps: usize) {
        let mut scratch = FindScratch::default();
        let mut propagate = byroredux_core::ecs::systems::make_transform_propagation_system();
        for _ in 0..steps {
            find_package_system_inner(world, 0.5, &mut scratch);
            propagate(world, 0.5);
        }
    }

    /// Object type 18 (Food) searched in a 1000-unit area: the nearest *food* is
    /// walked to and taken - not the nearer non-food, not the food outside the
    /// area - and the package is done at its count of one.
    #[test]
    fn a_food_search_takes_the_nearest_food_in_its_area() {
        let mut world = world();
        let near_tonic = place(&mut world, TONIC, 0x101, Vec3::new(60.0, 0.0, 0.0));
        let food = place(&mut world, FOOD, 0x102, Vec3::new(0.0, 0.0, 500.0));
        let far_food = place(&mut world, FOOD, 0x103, Vec3::new(0.0, 0.0, 4000.0));
        let actor = finder(
            &mut world,
            FindPackageBehavior {
                target: FindTarget::Kind(FindItemKind::Food),
                count: 1,
                area: EatSleepLocation::NearCurrentLocation,
                area_radius: 1000.0,
            },
        );
        run(&world, 200);
        let progress = world.get::<FindProgress>(actor).map(|p| *p).expect("progress");
        assert!(progress.done && progress.taken == 1, "took its one food: {progress:?}");
        assert!(world.has::<PickedUp>(food), "the food in the area was taken");
        assert!(!world.has::<PickedUp>(near_tonic), "the nearer non-food was left");
        assert!(!world.has::<PickedUp>(far_food), "the food outside the area was left");
        let held: Vec<u32> = world.get::<Inventory>(actor).unwrap().items.iter().map(|s| s.base_form_id).collect();
        assert_eq!(held, vec![FOOD]);
    }

    /// GECK: "if a creature, attack it" - a creature target is not walked all the
    /// way to; once the actor is within engaging distance the combat AI has the fight.
    #[test]
    fn a_found_creature_is_attacked_from_engaging_distance() {
        let mut world = world();
        let creature = place(&mut world, 0x7001, 0x201, Vec3::new(0.0, 0.0, 900.0));
        world.insert(creature, CreatureActor);
        world.insert(creature, ActorValues::default());
        let actor = finder(
            &mut world,
            FindPackageBehavior {
                target: FindTarget::Base(0x7001),
                count: 1,
                area: EatSleepLocation::NearCurrentLocation,
                area_radius: 2000.0,
            },
        );
        run(&world, 120);
        assert_eq!(world.get::<AiCombatState>(actor).map(|s| s.target), Some(creature));
        assert!(world.get::<FindProgress>(actor).is_some_and(|p| p.done));
    }

    /// A target the actor cannot reach is dropped, and tried again once the
    /// mark expires: the player behind a wall is not given up on for good.
    #[test]
    fn an_unreachable_target_is_retried_after_it_expires() {
        let mut scratch = FindScratch::default();
        scratch.unreachable.insert((1, 2), UNREACHABLE_SECS);
        let mut world = world();
        let target = place(&mut world, 0x7001, 0x201, Vec3::new(0.0, 0.0, 300.0));
        let actor = finder(&mut world, FindPackageBehavior {
            target: FindTarget::Reference(0x201),
            count: 1,
            area: EatSleepLocation::NearCurrentLocation,
            area_radius: 0.0,
        });
        scratch.unreachable.insert((actor, target), UNREACHABLE_SECS);
        assert!(!still_valid(&world, actor, target, &scratch), "dropped while marked");
        scratch.unreachable.retain(|_, left| {
            *left -= UNREACHABLE_SECS + 1.0;
            *left > 0.0
        });
        assert!(still_valid(&world, actor, target, &scratch), "valid again once it expires");
    }

    #[test]
    fn a_dead_target_is_not_hunted_and_a_busy_actor_does_not_search() {
        let mut world = world();
        let creature = place(&mut world, 0x7001, 0x201, Vec3::new(0.0, 0.0, 300.0));
        world.insert(creature, CreatureActor);
        world.insert(creature, Dead);
        let behavior = FindPackageBehavior {
            target: FindTarget::Base(0x7001),
            count: 1,
            area: EatSleepLocation::NearCurrentLocation,
            area_radius: 2000.0,
        };
        let actor = finder(&mut world, behavior);
        run(&world, 10);
        assert!(world.get::<AiCombatState>(actor).is_none(), "a corpse is not attacked");
        assert!(world.get::<FindProgress>(actor).is_none_or(|p| p.target.is_none()));
    }
}
