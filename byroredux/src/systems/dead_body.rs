//! Dead-body discovery — the Story Manager's `DEAD` event (M43).
//!
//! CK *Dead Body*: "Event triggered whenever an actor discovers a dead body", with the discoverer, the dead
//! actor and the location as its data; `OnStoryDiscoverDeadBody(akActor, akDeadActor, akLocation)`. The quests
//! under the event read the corpse in `R2` (the slot census, `story-manager.md` §6.4); the discoverer rides
//! in `R1` and the session location in `L1`.
//!
//! No source gives a discovery distance, so the engine's one perception model is reused: a living, placed,
//! non-player actor discovers a body when it sees it as the ambient hostility pass sees a target — inside
//! `fSneakMaxDistance` (scaled outdoors) with nothing solid in the way
//! ([`actor_sees_corpse`](super::faction_hostility::actor_sees_corpse)). That is a sight test, not the view
//! cone or the detection meter, which the hostility pass does not model either. Each body is discovered
//! **once** (the nearest seeing actor wins); the player is not a discoverer, since the player's view cone
//! is not modelled and a body behind the camera would otherwise count.
//!
//! The pass runs every [`PASS_SECS`] and only when the Story Manager tree has a `DEAD` root, so a title
//! without one (Oblivion, Fallout 3, New Vegas) pays nothing.

use super::faction_hostility::{actor_sees_corpse, CombatDisposition};
use byroredux_core::ecs::components::{Dead, GlobalTransform};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_scripting::story_manager::{raise_story_event, SmTree, StoryEvent};
use std::collections::HashSet;

/// The `SMEN` mnemonic of the Discover Dead Body event.
pub(crate) const DEAD: [u8; 4] = *b"DEAD";

/// Seconds between discovery passes: the hostility pass runs twice a second, and a body does not need to
/// be found within the half second it takes to look.
const PASS_SECS: f32 = 1.0;

/// The bodies already found and the pass clock. `NOT_SAVED_BY_DESIGN`: a load re-arms the scan, so a body
/// found before the save can be found once more (the node's reset window bounds how often that starts a quest).
#[derive(Default)]
pub(crate) struct DeadBodyState {
    elapsed: f32,
    discovered: HashSet<EntityId>,
}

impl Resource for DeadBodyState {}

/// System: raise `DEAD` for each newly seen corpse.
pub(crate) fn dead_body_discovery_system(world: &World, dt: f32) {
    if !world
        .try_resource::<SmTree>()
        .is_some_and(|tree| tree.roots_by_mnemonic.contains_key(&DEAD))
    {
        return;
    }
    let Some(mut state) = world.try_resource_mut::<DeadBodyState>() else {
        return;
    };
    state.elapsed += dt.max(0.0);
    if state.elapsed < PASS_SECS {
        return;
    }
    state.elapsed = 0.0;
    let bodies: Vec<EntityId> = world
        .query::<Dead>()
        .map(|dead| dead.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    state.discovered.retain(|body| bodies.contains(body));
    let fresh: Vec<EntityId> = bodies.into_iter().filter(|body| !state.discovered.contains(body)).collect();
    drop(state);
    if fresh.is_empty() {
        return;
    }
    let player = crate::systems::PlayerEntity::get(world);
    let observers: Vec<EntityId> = world
        .query::<CombatDisposition>()
        .map(|actors| actors.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    let observers: Vec<EntityId> = observers
        .into_iter()
        .filter(|actor| Some(*actor) != player && world.get::<Dead>(*actor).is_none())
        .collect();
    if observers.is_empty() {
        return;
    }
    let mut found = Vec::new();
    for body in fresh {
        let Some(at) = world.get::<GlobalTransform>(body).map(|t| t.translation) else {
            continue;
        };
        // The nearest actor is asked first, so the first that sees is the nearest that does.
        let mut by_distance: Vec<(f32, EntityId)> = observers
            .iter()
            .filter_map(|&actor| Some((world.get::<GlobalTransform>(actor)?.translation.distance(at), actor)))
            .collect();
        by_distance.sort_by(|a, b| a.0.total_cmp(&b.0));
        if let Some(&(_, discoverer)) = by_distance.iter().find(|(_, actor)| actor_sees_corpse(world, *actor, body)) {
            found.push((discoverer, body));
        }
    }
    if found.is_empty() {
        return;
    }
    // #5372 — the location is resolved before the StoryEvent write guard.
    let location = crate::systems::story_events::derive_location(world).location;
    for (discoverer, body) in found {
        log::debug!("dead body discovered: {discoverer:?} found {body:?}");
        if let Some(mut state) = world.try_resource_mut::<DeadBodyState>() {
            state.discovered.insert(body);
        }
        raise_story_event(
            world,
            discoverer,
            StoryEvent {
                mnemonic: DEAD,
                reference_1: discoverer,
                reference_2: Some(body),
                location_1: location,
                location_2: None,
                scripted: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems::faction_hostility::DetectionConfig;
    use byroredux_core::math::Vec3;
    use byroredux_plugin::esm::records::{ActorAiData, Aggression, Confidence, SmNodeKind, SmNodeRecord};
    use std::collections::HashMap;

    fn world_with_dead_root(with_root: bool) -> World {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<Dead>();
        world.register::<GlobalTransform>();
        world.register::<CombatDisposition>();
        world.insert_resource(DeadBodyState::default());
        world.insert_resource(DetectionConfig { max_distance: 1000.0, exterior_mult: 1.0 });
        let records = if with_root {
            HashMap::from([(0x10, SmNodeRecord { form_id: 0x10, kind: SmNodeKind::Event, event_mnemonic: Some(DEAD), ..Default::default() })])
        } else {
            HashMap::new()
        };
        byroredux_scripting::story_manager::install_story_manager(&mut world, &records);
        world
    }

    fn spawn_at(world: &mut World, x: f32) -> EntityId {
        let actor = world.spawn();
        world.insert(actor, GlobalTransform { translation: Vec3::new(x, 0.0, 0.0), ..Default::default() });
        actor
    }

    fn watcher(world: &mut World, x: f32) -> EntityId {
        let actor = spawn_at(world, x);
        world.insert(
            actor,
            CombatDisposition(ActorAiData { aggression: Aggression::Unaggressive, confidence: Confidence::Average, responsibility: None, attack_radius: None }),
        );
        actor
    }

    fn raised(world: &World) -> Vec<StoryEvent> {
        world.query::<StoryEvent>().map(|events| events.iter().map(|(_, e)| *e).collect()).unwrap_or_default()
    }

    /// The nearest actor inside `fSneakMaxDistance` discovers the body once, with itself in R1, the body in R2.
    #[test]
    fn the_nearest_seeing_actor_discovers_a_body_once() {
        let mut world = world_with_dead_root(true);
        let near = watcher(&mut world, 300.0);
        let _far = watcher(&mut world, 800.0);
        let _out_of_range = watcher(&mut world, 5000.0);
        let body = spawn_at(&mut world, 0.0);
        world.insert(body, Dead);
        dead_body_discovery_system(&world, 1.5);
        let events = raised(&world);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].mnemonic, DEAD);
        assert_eq!((events[0].reference_1, events[0].reference_2), (near, Some(body)));
        dead_body_discovery_system(&world, 1.5);
        assert_eq!(raised(&world).len(), 1, "a body is discovered once");
    }

    /// A body beyond everyone's sight range is not found; neither is anything on a title with no `DEAD` root.
    #[test]
    fn nothing_is_found_out_of_range_or_without_a_dead_root() {
        let mut world = world_with_dead_root(true);
        let _far = watcher(&mut world, 5000.0);
        let body = spawn_at(&mut world, 0.0);
        world.insert(body, Dead);
        dead_body_discovery_system(&world, 1.5);
        assert!(raised(&world).is_empty());

        let mut world = world_with_dead_root(false);
        let _near = watcher(&mut world, 100.0);
        let body = spawn_at(&mut world, 0.0);
        world.insert(body, Dead);
        dead_body_discovery_system(&world, 1.5);
        assert!(raised(&world).is_empty());
    }
}
