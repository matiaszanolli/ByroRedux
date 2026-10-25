//! M47.2 — `OnTrapHitStart` / `OnTrapHit` / `OnTrapHitStop` for trap bodies.
//!
//! CK *OnTrapHit - ObjectReference*: "Event called when the object reference is a
//! trap and has hit a live target"; *OnTrapHitStart*: "has started hitting a live
//! target"; *OnTrapHitStop(akTarget)*: "has stopped hitting". All but the last
//! carry `(akTarget, afXVel, afYVel, afZVel, afXPos, afYPos, afZPos, aeMaterial,
//! abInitialHit, aeMotionType)`: the trap's velocity and position "when it hit",
//! an opaque material number "not intended to be meaningful to script, just to be
//! passed to other native functions" (`ProcessTrapHit`), whether this is the first
//! hit, and the trap's motion type.
//!
//! A *trap* is a body on Havok layer 14 (`SkyrimLayer` Trap — the layer the
//! impulse multiplier table names too) that carries Papyrus scripts; a *live
//! target* is an actor that is not dead. Contact is a shape overlap of the trap's
//! colliders with the actor's. The scripts gate when contact matters themselves
//! (`TrapHitBase` stays in `CannotHit` until its trap springs).
//!
//! Not modelled, and said so: `OnTrapHit` "is sent every so often as long as a
//! trap object is in contact" with no stated cadence, so it is sent once, with
//! the start, as the initial hit; the material number is 0.

use rustc_hash::{FxHashMap, FxHashSet};

use byroredux_core::ecs::components::{ActorVitals, Dead, RigidBodyData};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::{EntityId, World};
use byroredux_scripting::translate::effects::VmStored;

/// Havok layer 14: Skyrim's Trap.
const LAYER_TRAP: u8 = 14;

/// How far up the parents a trap body looks for the entity that carries the scripts.
const MAX_PARENT_HOPS: usize = 8;

/// Which (trap, actor) pairs were in contact last tick.
#[derive(Debug, Default)]
pub(crate) struct TrapContacts(pub FxHashSet<(EntityId, EntityId)>, TrapOwnerCache);

/// The scripted-owner map for the Trap-layer bodies (`(owner, body)` per trap).
/// It depends only on the structure of the components in [`owner_key`], so it is
/// derived once and reused until one of their memberships changes (#6312);
/// before that the ancestor chains, script owners and the bare-collider
/// lookups were rebuilt, with fresh allocations, every tick.
#[derive(Debug, Default)]
struct TrapOwnerCache {
    key: Option<[u64; 6]>,
    traps: std::sync::Arc<Vec<(EntityId, byroredux_physics::RigidBodyHandle)>>,
}

impl Resource for TrapContacts {}

/// The script-side reference of `entity` (`0x14` for the player).
fn script_form(world: &World, entity: EntityId) -> Option<u32> {
    let player = byroredux_scripting::papyrus_demo::PapyrusPlayerEntity::get(world);
    if Some(entity) == player {
        return Some(byroredux_scripting::PLAYER_REFERENCE);
    }
    byroredux_scripting::condition::entity_global_form_id(world, entity)
}

/// Map only live actors' physics bodies to their owning actor. Trap contacts
/// need this small set; scanning every world body and resolving each one is
/// unnecessary.
pub(crate) fn live_actor_bodies(
    world: &World,
) -> FxHashMap<byroredux_physics::RigidBodyHandle, EntityId> {
    let mut owners = FxHashMap::default();
    if let Some(actors) = world.query::<ActorVitals>() {
        for (actor, _) in actors.iter() {
            if world.get::<Dead>(actor).is_none() {
                if let Some(handles) = world.get::<byroredux_physics::RapierHandles>(actor) {
                    owners.insert(handles.body, actor);
                }
            }
        }
    }
    if let Some(colliders) = world.query::<byroredux_physics::ActorColliderOwner>() {
        for (collider, owner) in colliders.iter() {
            if world.get::<ActorVitals>(owner.0).is_some()
                && world.get::<Dead>(owner.0).is_none()
            {
                if let Some(handles) = world.get::<byroredux_physics::RapierHandles>(collider) {
                    owners.insert(handles.body, owner.0);
                }
            }
        }
    }
    owners
}

/// The local form id of the REFR a bare collision entity came from.
fn source_reference(world: &World, entity: EntityId) -> Option<u32> {
    let source = world.get::<byroredux_core::ecs::components::PhysicsSourceForm>(entity)?.0;
    let pool = world.try_resource::<byroredux_core::form_id::FormIdPool>()?;
    pool.local_of(source)
}

/// Raise this tick's trap-contact events.
/// Contacts in `previous` that are gone from `now`, in a stable order.
/// #5873 — `HashSet::difference` iterates in a per-process-seeded order, so the
/// `OnTrapHitStop` scripts used to run in a different order every run.
fn ended_contacts(
    previous: &FxHashSet<(EntityId, EntityId)>,
    now: &FxHashSet<(EntityId, EntityId)>,
) -> Vec<(EntityId, EntityId)> {
    let mut ended: Vec<_> = previous.difference(now).copied().collect();
    ended.sort_unstable();
    ended
}

/// Membership generations of everything [`derive_traps`] reads: the Trap-layer
/// bodies, the parent links, the script holders, the physics handles, and the
/// bare-collider path's source form and form id. Each is a tracked component
/// (`TRACK_CHANGES`), so the generation moves on insert, overwrite and removal.
fn owner_key(world: &World) -> [u64; 6] {
    fn generation<T>(world: &World) -> u64
    where
        T: byroredux_core::ecs::Component<Storage = byroredux_core::ecs::SparseSetStorage<T>>,
    {
        world.query::<T>().map_or(0, |q| q.storage().structural_generation())
    }
    use byroredux_core::ecs::components::{FormIdComponent, Parent, PhysicsSourceForm};
    [
        generation::<RigidBodyData>(world),
        generation::<Parent>(world),
        generation::<byroredux_scripting::PapyrusReferenceScripts>(world),
        generation::<byroredux_physics::RapierHandles>(world),
        generation::<PhysicsSourceForm>(world),
        generation::<FormIdComponent>(world),
    ]
}

/// Resolve every Trap-layer body to `(scripted owner, physics body)`. `None`
/// when a storage it needs is not registered (nothing to do this tick).
fn derive_traps(world: &World) -> Option<Vec<(EntityId, byroredux_physics::RigidBodyHandle)>> {
    let trap_layer: Vec<EntityId> = world
        .query::<RigidBodyData>()?
        .iter()
        .filter(|(_, data)| data.havok_layer == LAYER_TRAP)
        .map(|(entity, _)| entity)
        .collect();
    // The scripts sit on the placement root, the colliders on the root or a child
    // of it: a trap body is owned by the nearest scripted entity up its parents.
    // (One guard at a time: the ancestor chains first, then the scripts.)
    let chains: Vec<(EntityId, Vec<EntityId>)> = {
        let parents = world.query::<byroredux_core::ecs::components::Parent>();
        trap_layer
            .iter()
            .copied()
            .map(|entity| {
                let mut chain = vec![entity];
                while chain.len() < MAX_PARENT_HOPS {
                    let Some(parent) = parents.as_ref().and_then(|p| p.get(*chain.last().unwrap())).map(|p| p.0) else {
                        break;
                    };
                    chain.push(parent);
                }
                (entity, chain)
            })
            .collect()
    };
    static LAST_CHAINS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(usize::MAX);
    if LAST_CHAINS.swap(chains.len(), std::sync::atomic::Ordering::Relaxed) != chains.len() {
        log::debug!("trap contacts: {} Trap-layer bodies; ancestor chains {:?}", chains.len(), chains);
    }
    let scripted: Vec<(EntityId, EntityId)> = {
        let scripts = world.query::<byroredux_scripting::PapyrusReferenceScripts>()?;
        chains
            .into_iter()
            .filter_map(|(entity, chain)| Some((entity, chain.into_iter().find(|&at| scripts.get(at).is_some())?)))
            .collect()
    };
    // A bhk collision shape can be a bare entity, decoupled from its placement's
    // hierarchy (the hierarchical import of an animated prop): its
    // `PhysicsSourceForm` names the REFR, whose entity carries the scripts.
    let scripted = {
        let mut owned: FxHashSet<EntityId> = scripted.iter().map(|(entity, _)| *entity).collect();
        let mut scripted = scripted;
        let bare: Vec<(EntityId, u32)> = trap_layer
            .iter()
            .filter(|entity| !owned.contains(entity))
            .filter_map(|&entity| Some((entity, source_reference(world, entity)?)))
            .collect();
        for (entity, reference) in bare {
            if let Some(owner) = byroredux_scripting::ReferenceScriptIndex::lookup(world, reference) {
                owned.insert(entity);
                scripted.push((entity, owner));
            }
        }
        scripted
    };
    let handles = world.query::<byroredux_physics::RapierHandles>()?;
    let traps: Vec<(EntityId, _)> = scripted
        .into_iter()
        .filter_map(|(body_entity, owner)| Some((owner, handles.get(body_entity)?.body)))
        .collect();
    static LAST_TRAP_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(usize::MAX);
    if LAST_TRAP_COUNT.swap(traps.len(), std::sync::atomic::Ordering::Relaxed) != traps.len() {
        log::debug!("trap contacts: {} scripted Trap-layer bodies", traps.len());
    }
    Some(traps)
}

pub(crate) fn trap_contact_system(world: &World, _dt: f32) {
    // The owner map is invariant between structural changes (Havok layer is
    // import-time data; runtime motion edits do not change membership).
    let key = owner_key(world);
    let cached = world.try_resource::<TrapContacts>().and_then(|contacts| {
        (contacts.1.key == Some(key)).then(|| std::sync::Arc::clone(&contacts.1.traps))
    });
    let traps = match cached {
        Some(traps) => traps,
        None => {
            let Some(traps) = derive_traps(world) else { return };
            let traps = std::sync::Arc::new(traps);
            if let Some(mut contacts) = world.try_resource_mut::<TrapContacts>() {
                contacts.1 = TrapOwnerCache { key: Some(key), traps: std::sync::Arc::clone(&traps) };
            }
            traps
        }
    };
    let previous = world
        .try_resource_mut::<TrapContacts>()
        .map(|mut contacts| std::mem::take(&mut contacts.0))
        .unwrap_or_default();
    if traps.is_empty() && previous.is_empty() {
        return;
    }
    // Resolve actor bodies before the physics world is taken (the melee
    // sweep's lock order). The overlap query uses this small set to skip world
    // geometry before shape intersection.
    let actor_bodies = live_actor_bodies(world);
    // The physics world is held only to gather the overlaps; the actors are
    // resolved after it is released (component reads under it would order
    // ActorVitals after PhysicsWorld, against the character systems' order).
    let overlaps: Vec<(EntityId, byroredux_physics::BodyMotion, Vec<_>)> = {
        let Some(physics) = world.try_resource::<byroredux_physics::PhysicsWorld>() else {
            return;
        };
        traps
            .iter()
            .filter_map(|&(trap, body)| {
                Some((
                    trap,
                    physics.body_motion(body)?,
                    physics.bodies_overlapping_body_where(body, &|other| actor_bodies.contains_key(&other)),
                ))
            })
            .collect()
    };
    let mut now: FxHashSet<(EntityId, EntityId)> = FxHashSet::default();
    let mut starts: Vec<(EntityId, EntityId, byroredux_physics::BodyMotion)> = Vec::new();
    for (trap, motion, others) in overlaps {
        for other in others {
            let Some(&actor) = actor_bodies.get(&other) else {
                continue;
            };
            if world.get::<Dead>(actor).is_some() || actor == trap || !now.insert((trap, actor)) {
                continue;
            }
            if !previous.contains(&(trap, actor)) {
                starts.push((trap, actor, motion));
            }
        }
    }
    let stops = ended_contacts(&previous, &now);
    if let Some(mut contacts) = world.try_resource_mut::<TrapContacts>() {
        contacts.0 = now;
    }
    for (trap, actor, motion) in starts {
        let Some(target) = script_form(world, actor) else { continue };
        let velocity = byroredux_core::math::coord::yup_to_zup_pos(motion.velocity.to_array());
        let position = byroredux_core::math::coord::yup_to_zup_pos(motion.position.to_array());
        let args = vec![
            VmStored::Form(target),
            VmStored::Float(velocity[0]),
            VmStored::Float(velocity[1]),
            VmStored::Float(velocity[2]),
            VmStored::Float(position[0]),
            VmStored::Float(position[1]),
            VmStored::Float(position[2]),
            VmStored::Int(0),
            VmStored::Bool(true),
            VmStored::Int(motion.motion_type),
        ];
        for event in ["OnTrapHitStart", "OnTrapHit"] {
            byroredux_scripting::raise_reference_event(world, trap, event, args.clone());
        }
    }
    for (trap, actor) in stops {
        let Some(target) = script_form(world, actor) else { continue };
        byroredux_scripting::raise_reference_event(world, trap, "OnTrapHitStop", vec![VmStored::Form(target)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::ActorVitals;
    use byroredux_core::math::Vec3;
    use byroredux_physics::PhysicsWorld;
    use byroredux_plugin::esm::records::script_instance::ScriptInstanceData;
    use byroredux_scripting::{PapyrusReferenceScripts, ReferenceScript};

    fn trap_world() -> (World, EntityId, EntityId) {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<RigidBodyData>();
        world.register::<byroredux_physics::RapierHandles>();
        world.register::<byroredux_physics::ActorColliderOwner>();
        world.register::<ActorVitals>();
        world.register::<Dead>();
        world.insert_resource(TrapContacts::default());
        let mut physics = PhysicsWorld::new();
        let trap_handles = physics.spawn_fixed_cuboid_handles(Vec3::splat(20.0), Vec3::ZERO);
        let actor_handles = physics.spawn_fixed_cuboid_handles(Vec3::splat(20.0), Vec3::new(30.0, 0.0, 0.0));
        let wall_handles = physics.spawn_fixed_cuboid_handles(Vec3::splat(20.0), Vec3::ZERO);
        let far_handles = physics.spawn_fixed_cuboid_handles(Vec3::splat(20.0), Vec3::new(500.0, 0.0, 0.0));
        world.insert_resource(physics);
        let trap = world.spawn();
        world.insert(trap, trap_handles);
        world.insert(trap, RigidBodyData { havok_layer: LAYER_TRAP, ..RigidBodyData::static_body() });
        world.insert(
            trap,
            PapyrusReferenceScripts(vec![ReferenceScript {
                script: "TrapHitBase".into(),
                vmad: std::sync::Arc::new(ScriptInstanceData { version: 5, object_format: 2, scripts: Vec::new() }),
                events: vec!["OnTrapHitStart".into()],
            }]),
        );
        let near = world.spawn();
        world.insert(near, actor_handles);
        world.insert(near, ActorVitals { health: 0x3E8 });
        let far = world.spawn();
        world.insert(far, far_handles);
        world.insert(far, ActorVitals { health: 0x3E8 });
        let _wall = world.spawn();
        world.insert(_wall, wall_handles);
        (world, trap, near)
    }

    /// A live actor whose collider overlaps a scripted Trap-layer body is a new
    /// contact; one out of reach is not; and when the actor dies the contact ends.
    #[test]
    fn contact_starts_with_an_overlap_and_ends_with_the_actor() {
        let (world, trap, near) = trap_world();
        let actor_bodies = live_actor_bodies(&world);
        let trap_body = world.get::<byroredux_physics::RapierHandles>(trap).unwrap().body;
        let near_body = world.get::<byroredux_physics::RapierHandles>(near).unwrap().body;
        let overlaps = world
            .resource::<PhysicsWorld>()
            .bodies_overlapping_body_where(trap_body, &|body| actor_bodies.contains_key(&body));
        assert_eq!(overlaps, vec![near_body], "the broad wall overlap is filtered before shape intersection");

        trap_contact_system(&world, 0.0);
        assert_eq!(world.resource::<TrapContacts>().0, [(trap, near)].into_iter().collect::<FxHashSet<_>>(), "only the overlapping actor");
        trap_contact_system(&world, 0.0);
        assert_eq!(world.resource::<TrapContacts>().0.len(), 1, "a standing contact is not a new one");
        world.query_mut::<Dead>().unwrap().insert(near, Dead);
        trap_contact_system(&world, 0.0);
        assert!(world.resource::<TrapContacts>().0.is_empty(), "a dead actor is not a live target");
    }

    #[test]
    fn trap_layer_cache_rebuilds_after_a_body_is_inserted() {
        let (mut world, trap, near) = trap_world();
        trap_contact_system(&world, 0.0);

        let handles = world
            .resource_mut::<PhysicsWorld>()
            .spawn_fixed_cuboid_handles(Vec3::splat(20.0), Vec3::ZERO);
        let added = world.spawn();
        world.insert(added, handles);
        world.insert(added, RigidBodyData { havok_layer: LAYER_TRAP, ..RigidBodyData::static_body() });
        world.insert(
            added,
            PapyrusReferenceScripts(vec![ReferenceScript {
                script: "TrapHitBase".into(),
                vmad: std::sync::Arc::new(ScriptInstanceData { version: 5, object_format: 2, scripts: Vec::new() }),
                events: vec!["OnTrapHitStart".into()],
            }]),
        );

        trap_contact_system(&world, 0.0);
        assert_eq!(
            world.resource::<TrapContacts>().0,
            [(trap, near), (added, near)].into_iter().collect::<FxHashSet<_>>(),
            "a structural body change refreshes the cached Trap-layer subset"
        );
    }

    /// #6312 — the trap → owner map is cached between ticks, so it must follow
    /// the structure it was derived from: losing the scripts ends the contact,
    /// regaining them (a cell reload re-attaches) starts it again, and a trap
    /// reparented under a scripted root is owned by that root.
    #[test]
    fn the_cached_owner_map_follows_script_and_parent_changes() {
        use byroredux_core::ecs::components::Parent;
        let (mut world, trap, near) = trap_world();
        world.register::<Parent>();
        trap_contact_system(&world, 0.0);
        assert_eq!(
            world.resource::<TrapContacts>().0,
            [(trap, near)].into_iter().collect::<FxHashSet<_>>()
        );

        let scripts = world.remove::<PapyrusReferenceScripts>(trap).expect("the trap had scripts");
        trap_contact_system(&world, 0.0);
        assert!(world.resource::<TrapContacts>().0.is_empty(), "no scripts, no trap");

        let root = world.spawn();
        world.insert(root, scripts);
        world.insert(trap, Parent(root));
        trap_contact_system(&world, 0.0);
        assert_eq!(
            world.resource::<TrapContacts>().0,
            [(root, near)].into_iter().collect::<FxHashSet<_>>(),
            "the scripted root now owns the body"
        );
    }

    /// #5873 — stop events come out in (trap, actor) order, whatever the
    /// set's iteration order is.
    #[test]
    fn ended_contacts_are_ordered() {
        let mut world = World::new();
        let ids: Vec<EntityId> = (0..16).map(|_| world.spawn()).collect();
        let previous: FxHashSet<_> = ids.iter().rev().flat_map(|&t| ids.iter().rev().map(move |&a| (t, a))).collect();
        let now: FxHashSet<_> = [(ids[0], ids[0])].into_iter().collect();
        let ended = ended_contacts(&previous, &now);
        assert_eq!(ended.len(), previous.len() - 1);
        assert!(ended.windows(2).all(|w| w[0] < w[1]), "sorted and unique");
        assert!(!ended.contains(&(ids[0], ids[0])));
    }

    /// The scripts sit on the placement root and the collider on a child of it:
    /// the root is the trap that is hit.
    #[test]
    fn a_child_collider_belongs_to_its_scripted_root() {
        use byroredux_core::ecs::components::Parent;
        let (mut world, trap, near) = trap_world();
        world.register::<Parent>();
        let scripts = world.remove::<PapyrusReferenceScripts>(trap).expect("the trap had scripts");
        let root = world.spawn();
        world.insert(root, scripts);
        world.insert(trap, Parent(root));
        trap_contact_system(&world, 0.0);
        assert_eq!(world.resource::<TrapContacts>().0, [(root, near)].into_iter().collect::<FxHashSet<_>>());
    }

    /// A body that is not on the Trap layer, or carries no scripts, is not a trap.
    #[test]
    fn only_scripted_trap_layer_bodies_are_traps() {
        let (world, trap, _) = trap_world();
        world.query_mut::<RigidBodyData>().unwrap().get_mut(trap).unwrap().havok_layer = 10;
        trap_contact_system(&world, 0.0);
        assert!(world.resource::<TrapContacts>().0.is_empty());
    }
}
