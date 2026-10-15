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

use std::collections::HashSet;

use byroredux_core::ecs::components::{Dead, RigidBodyData};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::{EntityId, World};
use byroredux_scripting::translate::effects::VmStored;

/// Havok layer 14: Skyrim's Trap.
const LAYER_TRAP: u8 = 14;

/// How far up the parents a trap body looks for the entity that carries the scripts.
const MAX_PARENT_HOPS: usize = 8;

/// Which (trap, actor) pairs were in contact last tick.
#[derive(Debug, Default)]
pub(crate) struct TrapContacts(pub HashSet<(EntityId, EntityId)>);

impl Resource for TrapContacts {}

/// The script-side reference of `entity` (`0x14` for the player).
fn script_form(world: &World, entity: EntityId) -> Option<u32> {
    let player = byroredux_scripting::papyrus_demo::PapyrusPlayerEntity::get(world);
    if Some(entity) == player {
        return Some(byroredux_scripting::PLAYER_REFERENCE);
    }
    byroredux_scripting::condition::entity_global_form_id(world, entity)
}

/// The actor a collider belongs to, if it is a live one.
fn live_actor(world: &World, collider_entity: EntityId) -> Option<EntityId> {
    let actor = world
        .get::<byroredux_physics::ActorColliderOwner>(collider_entity)
        .map(|owner| owner.0)
        .unwrap_or(collider_entity);
    world.get::<byroredux_core::ecs::components::ActorVitals>(actor)?;
    world.get::<Dead>(actor).is_none().then_some(actor)
}

/// The local form id of the REFR a bare collision entity came from.
fn source_reference(world: &World, entity: EntityId) -> Option<u32> {
    let source = world.get::<byroredux_core::ecs::components::PhysicsSourceForm>(entity)?.0;
    let pool = world.try_resource::<byroredux_core::form_id::FormIdPool>()?;
    pool.resolve(source).map(|pair| pair.local.0)
}

/// Raise this tick's trap-contact events.
pub(crate) fn trap_contact_system(world: &World, _dt: f32) {
    // The scripted trap bodies: a Trap-layer rigid body that carries scripts.
    // One storage guard at a time (a snapshot each), so no lock order is implied.
    let trap_layer: Vec<EntityId> = match world.query::<RigidBodyData>() {
        Some(bodies) => bodies
            .iter()
            .filter(|(_, data)| data.havok_layer == LAYER_TRAP)
            .map(|(entity, _)| entity)
            .collect(),
        None => return,
    };
    // The scripts sit on the placement root, the colliders on the root or a child
    // of it: a trap body is owned by the nearest scripted entity up its parents.
    // (One guard at a time: the ancestor chains first, then the scripts.)
    let trap_layer_all = trap_layer.clone();
    let chains: Vec<(EntityId, Vec<EntityId>)> = {
        let parents = world.query::<byroredux_core::ecs::components::Parent>();
        trap_layer
            .into_iter()
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
        let Some(scripts) = world.query::<byroredux_scripting::PapyrusReferenceScripts>() else {
            return;
        };
        chains
            .into_iter()
            .filter_map(|(entity, chain)| Some((entity, chain.into_iter().find(|&at| scripts.get(at).is_some())?)))
            .collect()
    };
    // A bhk collision shape can be a bare entity, decoupled from its placement's
    // hierarchy (the hierarchical import of an animated prop): its
    // `PhysicsSourceForm` names the REFR, whose entity carries the scripts.
    let scripted = {
        let mut owned: std::collections::HashSet<EntityId> = scripted.iter().map(|(entity, _)| *entity).collect();
        let mut scripted = scripted;
        let bare: Vec<(EntityId, u32)> = trap_layer_all
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
    let traps: Vec<(EntityId, _)> = match world.query::<byroredux_physics::RapierHandles>() {
        Some(handles) => scripted
            .into_iter()
            .filter_map(|(body_entity, owner)| Some((owner, handles.get(body_entity)?.body)))
            .collect(),
        None => return,
    };
    static LAST_TRAP_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(usize::MAX);
    if LAST_TRAP_COUNT.swap(traps.len(), std::sync::atomic::Ordering::Relaxed) != traps.len() {
        log::debug!("trap contacts: {} scripted Trap-layer bodies", traps.len());
    }
    let previous = world
        .try_resource_mut::<TrapContacts>()
        .map(|mut contacts| std::mem::take(&mut contacts.0))
        .unwrap_or_default();
    if traps.is_empty() && previous.is_empty() {
        return;
    }
    // Body -> entity, resolved before the physics world is taken (the melee
    // sweep's lock order).
    let owners: std::collections::HashMap<_, _> = world
        .query::<byroredux_physics::RapierHandles>()
        .map(|handles| handles.iter().map(|(entity, handles)| (handles.body, entity)).collect())
        .unwrap_or_default();
    // The physics world is held only to gather the overlaps; the actors are
    // resolved after it is released (component reads under it would order
    // ActorVitals after PhysicsWorld, against the character systems' order).
    let overlaps: Vec<(EntityId, byroredux_physics::BodyMotion, Vec<_>)> = {
        let Some(physics) = world.try_resource::<byroredux_physics::PhysicsWorld>() else {
            return;
        };
        traps
            .iter()
            .filter_map(|&(trap, body)| Some((trap, physics.body_motion(body)?, physics.bodies_overlapping_body(body))))
            .collect()
    };
    let mut now: HashSet<(EntityId, EntityId)> = HashSet::new();
    let mut starts: Vec<(EntityId, EntityId, byroredux_physics::BodyMotion)> = Vec::new();
    for (trap, motion, others) in overlaps {
        for other in others {
            let Some(actor) = owners.get(&other).and_then(|&collider| live_actor(world, collider)) else {
                continue;
            };
            if actor == trap || !now.insert((trap, actor)) {
                continue;
            }
            if !previous.contains(&(trap, actor)) {
                starts.push((trap, actor, motion));
            }
        }
    }
    let stops: Vec<(EntityId, EntityId)> = previous.difference(&now).copied().collect();
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
        (world, trap, near)
    }

    /// A live actor whose collider overlaps a scripted Trap-layer body is a new
    /// contact; one out of reach is not; and when the actor dies the contact ends.
    #[test]
    fn contact_starts_with_an_overlap_and_ends_with_the_actor() {
        let (world, trap, near) = trap_world();
        trap_contact_system(&world, 0.0);
        assert_eq!(world.resource::<TrapContacts>().0, HashSet::from([(trap, near)]), "only the overlapping actor");
        trap_contact_system(&world, 0.0);
        assert_eq!(world.resource::<TrapContacts>().0.len(), 1, "a standing contact is not a new one");
        world.query_mut::<Dead>().unwrap().insert(near, Dead);
        trap_contact_system(&world, 0.0);
        assert!(world.resource::<TrapContacts>().0.is_empty(), "a dead actor is not a live target");
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
        assert_eq!(world.resource::<TrapContacts>().0, HashSet::from([(root, near)]));
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
