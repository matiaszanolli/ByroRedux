//! Fallout 4 actors stand in the idle their animation archetype names (M47.2).
//!
//! The clips come from `asset_provider::populate_archetype_idles`
//! ([`ArchetypeIdleClips`]); this system gives each actor the one its subgraph
//! entry resolves to, and swaps it when `Actor.ChangeAnimArchetype` changes the
//! entry. An actor something else already animates (a seat, a walk, a played
//! idle) keeps that: only an actor with no player, or one carrying this system's
//! [`ArchetypeIdle`] marker, is touched. The player and the dead are left alone.

use crate::asset_provider::ArchetypeIdleClips;
use crate::components::AnimationTarget;
use byroredux_core::animation::{AnimationPlayer, AnimationStack};
use byroredux_core::ecs::components::Dead;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::World;
use byroredux_plugin::esm::records::SubgraphRole;
use byroredux_scripting::AnimArchetypeOverrides;

/// The idle clip this system last gave an actor. `NOT_SAVED_BY_DESIGN`: derived
/// from the actor's keywords and the archives every time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ArchetypeIdle {
    pub(crate) clip: u32,
}

impl Component for ArchetypeIdle {
    type Storage = SparseSetStorage<Self>;
}

/// The archetype overrides the last pass saw: a change re-selects every actor.
/// `NOT_SAVED_BY_DESIGN`: a pass after a load simply runs once more.
#[derive(Default)]
pub(crate) struct ArchetypeIdleSeen {
    overrides: AnimArchetypeOverrides,
    /// Actors with no entry (or no clip for it) at the last selection: not asked
    /// again until the overrides change.
    unmatched: std::collections::HashSet<EntityId>,
}

impl Resource for ArchetypeIdleSeen {}

fn note_unmatched(world: &World, actor: EntityId) {
    if let Some(mut seen) = world.try_resource_mut::<ArchetypeIdleSeen>() {
        seen.unmatched.insert(actor);
    }
}

/// Give each actor its archetype idle; see the module docs.
pub(crate) fn archetype_idle_system(world: &World, _dt: f32) {
    if world.try_resource::<ArchetypeIdleClips>().is_none() {
        return;
    }
    let overrides = world.try_resource::<AnimArchetypeOverrides>().map(|o| o.clone()).unwrap_or_default();
    let changed = match world.try_resource_mut::<ArchetypeIdleSeen>() {
        Some(mut seen) if seen.overrides != overrides => {
            seen.overrides = overrides;
            seen.unmatched.clear();
            true
        }
        Some(_) => false,
        None => false,
    };
    if changed {
        log::debug!("archetype idle: the archetype overrides changed");
    }
    let actors: Vec<(EntityId, EntityId)> = match world.query::<AnimationTarget>() {
        Some(targets) => targets.iter().map(|(entity, target)| (entity, target.skeleton_root)).collect(),
        None => return,
    };
    let player = byroredux_scripting::papyrus_demo::PapyrusPlayerEntity::get(world);
    for (actor, skeleton_root) in actors {
        if Some(actor) == player || world.get::<Dead>(actor).is_some() {
            continue;
        }
        let marker = world.get::<ArchetypeIdle>(actor).map(|marker| *marker);
        if marker.is_none() && world.try_resource::<ArchetypeIdleSeen>().is_some_and(|seen| seen.unmatched.contains(&actor)) {
            continue;
        }
        let animated = world.get::<AnimationPlayer>(actor).is_some();
        if (animated && marker.is_none()) || (marker.is_some() && !changed) {
            continue;
        }
        let Some(entry) = byroredux_scripting::anim_subgraph::select_subgraph(world, actor, SubgraphRole::Mt, &[]) else {
            log::debug!("archetype idle: {actor} matches no MT subgraph entry");
            note_unmatched(world, actor);
            continue;
        };
        let Some(clip) = world.try_resource::<ArchetypeIdleClips>().and_then(|clips| clips.clip_for(&entry)) else {
            log::debug!("archetype idle: {actor}: no idle clip for {:?}", entry.animation_paths.first());
            note_unmatched(world, actor);
            continue;
        };
        if marker.is_some_and(|marker| marker.clip == clip) {
            log::debug!("archetype idle: {actor} keeps clip {clip}");
            continue;
        }
        let base = byroredux_scripting::relationships::actor_base(world, actor).unwrap_or(actor);
        let player = crate::npc_spawn::desynced_idle_player(world, clip, base, skeleton_root);
        log::debug!("archetype idle: {actor} -> clip {clip} ({:?})", entry.animation_paths.first());
        // The actor's behavior graph for this entry takes over its animation: events sent
        // to the actor (`PlayAnimation`) move it (`asset_provider::actor_graph`).
        if let Some(mut pending) = world.try_resource_mut::<crate::asset_provider::PendingActorGraphs>() {
            pending.0.push((actor, entry.clone()));
        }
        // A graph that was blending this actor (a stack) gives way to the new player.
        if let Some(mut stacks) = world.query_mut::<AnimationStack>() {
            stacks.remove(actor);
        }
        if let Some(mut players) = world.query_mut::<AnimationPlayer>() {
            players.insert(actor, player);
        }
        if let Some(mut markers) = world.query_mut::<ArchetypeIdle>() {
            markers.insert(actor, ArchetypeIdle { clip });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
    use byroredux_plugin::esm::records::{EsmIndex, KywdRecord, NpcRecord, RaceRecord, SubgraphData};

    const HUMAN: u32 = 0x0001_3746;
    const SETTLER: u32 = 0x0002_0001;
    const CONFIDENT: u32 = 0x0002_2E49;
    const SCARED: u32 = 0x0005_F763;

    fn entry(keys: &[u32], folder: &str) -> SubgraphData {
        SubgraphData {
            actor_keywords: keys.to_vec(),
            behavior_graph: r"Actors\Character\Behaviors\MTBehavior.hkx".to_owned(),
            animation_paths: vec![folder.to_owned()],
            target_keywords: Vec::new(),
            role: SubgraphRole::Mt,
            first_person: false,
        }
    }

    /// A confident settler (authored archetype) with a body that has a skeleton:
    /// `(world, actor, the actor's override key)`.
    fn world_with_settler() -> (World, EntityId, u32) {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<AnimationTarget>();
        world.register::<AnimationPlayer>();
        world.register::<ArchetypeIdle>();
        world.register::<Dead>();
        world.insert_resource(byroredux_core::animation::AnimationClipRegistry::new());
        world.insert_resource(ArchetypeIdleSeen::default());
        let mut index = EsmIndex::default();
        for (form_id, name) in [(CONFIDENT, "AnimArchetypeConfident"), (SCARED, "AnimArchetypeScared")] {
            index.keywords.insert(form_id, KywdRecord { form_id, editor_id: name.to_owned() });
        }
        index.races.insert(
            HUMAN,
            RaceRecord {
                form_id: HUMAN,
                subgraph_data: vec![entry(&[], "Neutral"), entry(&[CONFIDENT], "Confident"), entry(&[SCARED], "Scared")],
                ..Default::default()
            },
        );
        index.npcs.insert(
            SETTLER,
            NpcRecord { form_id: SETTLER, race_form_id: HUMAN, keywords: vec![CONFIDENT], ..Default::default() },
        );
        byroredux_scripting::keywords::install_keyword_table(&mut world, &index);
        byroredux_scripting::anim_subgraph::install_subgraph_table(&mut world, &index);
        world.insert_resource(AnimArchetypeOverrides::default());
        world.insert_resource(ArchetypeIdleClips::for_test(&[
            (&entry(&[], "Neutral"), 10),
            (&entry(&[CONFIDENT], "Confident"), 11),
            (&entry(&[SCARED], "Scared"), 12),
        ]));
        world.insert_resource(FormIdPool::new());
        let actor = world.spawn();
        let skeleton = world.spawn();
        world.insert(actor, byroredux_scripting::scene::SceneAliasCandidate { base_form_id: SETTLER, ..Default::default() });
        world.insert(actor, AnimationTarget { skeleton_root: skeleton, consumed_idle_serial: 0 });
        let id = world.resource_mut::<FormIdPool>().intern(FormIdPair {
            plugin: PluginId::from_filename("Fallout4.esm"),
            local: LocalFormId(0x0002_0777),
        });
        world.insert(actor, byroredux_core::ecs::components::FormIdComponent(id));
        let key = byroredux_scripting::condition::entity_global_form_id(&world, actor).unwrap();
        (world, actor, key)
    }

    fn clip_of(world: &World, actor: EntityId) -> Option<u32> {
        world.get::<AnimationPlayer>(actor).map(|player| player.clip_handle)
    }

    /// An actor idles in its authored archetype's clip, and a script's
    /// `ChangeAnimArchetype` swaps it - to the new archetype's, and with none to
    /// Neutral's.
    #[test]
    fn an_actor_idles_in_its_archetype_and_change_anim_archetype_swaps_it() {
        let (world, actor, key) = world_with_settler();
        archetype_idle_system(&world, 0.0);
        assert_eq!(clip_of(&world, actor), Some(11), "the authored Confident idle");
        assert_eq!(world.get::<ArchetypeIdle>(actor).map(|marker| marker.clip), Some(11));
        let skeleton = world.get::<AnimationTarget>(actor).unwrap().skeleton_root;
        assert_eq!(world.get::<AnimationPlayer>(actor).unwrap().root_entity, Some(skeleton));

        // Nothing changed: the player is left running (no restart).
        world.query_mut::<AnimationPlayer>().unwrap().get_mut(actor).unwrap().local_time = 0.7;
        archetype_idle_system(&world, 0.0);
        assert_eq!(world.get::<AnimationPlayer>(actor).unwrap().local_time, 0.7);

        world.resource_mut::<AnimArchetypeOverrides>().set(key, SCARED);
        archetype_idle_system(&world, 0.0);
        assert_eq!(clip_of(&world, actor), Some(12), "ChangeAnimArchetype(Scared)");

        world.resource_mut::<AnimArchetypeOverrides>().set(key, 0);
        archetype_idle_system(&world, 0.0);
        assert_eq!(clip_of(&world, actor), Some(10), "no archetype: Neutral");
    }

    /// An actor something else animates keeps that, and the dead are left alone.
    #[test]
    fn an_actor_another_system_animates_or_a_corpse_is_left_alone() {
        let (world, actor, _) = world_with_settler();
        world.query_mut::<AnimationPlayer>().unwrap().insert(actor, AnimationPlayer::new(99));
        archetype_idle_system(&world, 0.0);
        assert_eq!(clip_of(&world, actor), Some(99), "a seat, walk or played idle is not replaced");
        assert!(world.get::<ArchetypeIdle>(actor).is_none());

        let (world, corpse, _) = world_with_settler();
        world.query_mut::<Dead>().unwrap().insert(corpse, Dead);
        archetype_idle_system(&world, 0.0);
        assert_eq!(clip_of(&world, corpse), None);
    }

    /// The marker component and the pass's state must exist in the engine's world:
    /// a component that is only registered by a unit test is never stored live, and
    /// the idle then never swaps (found live on Magnolia).
    #[test]
    fn the_engine_world_registers_the_marker_and_the_pass_state() {
        let world = crate::boot::build_world(false, &[]);
        assert!(world.query::<ArchetypeIdle>().is_some(), "ArchetypeIdle is registered");
        assert!(
            world.query::<AnimationStack>().is_some(),
            "AnimationStack is registered: the graph's blended pose is stored through query_mut"
        );
        assert!(world.try_resource::<ArchetypeIdleSeen>().is_some());
    }
}
