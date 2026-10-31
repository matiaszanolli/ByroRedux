//! Attraction objects — the Story Manager's `AOBJ` event (M43).
//!
//! `OnStoryAttractionObject(akActor, akObject, akLocation, abCommanded)` (`Quest.pex`): "sent when the quest is
//! started by an actor detecting an attraction object" (CK *Quest Script*). Fallout 4's attraction system
//! (CK *AttractionRule*, *SetAttractionActive*): a **keyword** carries an *Attraction Type* — an `AORU` rule
//! with a radius, a minimum and maximum delay, *Requires Line of Sight* and *Combat Target* — and an object
//! and an actor that both carry the keyword react to each other: the object "send[s] attraction-object story
//! manager events to actors with that keyword" (`SetAttractionActive`). Fallout4.esm has 8 rules on 15
//! keywords, carried by activator, flora, furniture and container bases and by NPC bases (the vanilla comment
//! rules, the workshop alarm, the Bar, the egg clutch…).
//!
//! What runs here, with the engine-policy parts named:
//! - **Detection.** A live NPC and a placed object that share an attraction keyword come into the rule's
//!   radius (3-D distance between the placements) and, if the rule requires it, see each other through a clear
//!   physics line (the hostility pass's sight test, eye height on the actor).
//! - **Delay.** The rule gives a minimum and a maximum delay and no more: when a pair comes in range a delay is
//!   drawn uniformly between them, the event is raised when it runs out if the pair is still in range, and the
//!   next delay is drawn for as long as it stays. That reading of "delay" is engine policy.
//! - **Event.** `R1` the actor, `R2` the object, `L1` the session location, `V1` = `abCommanded` (always `0`:
//!   a settler *commanded* to an object is the workshop's command, not modelled).
//!
//! Not modelled: rules with *Combat Target* set (no source says what it changes, so they are left out rather
//! than guessed — the workshop-resource rule is one), `SetAttractionActive` switching an object off, and a
//! keyword an actor gets from its race. Idle on a title whose tree has no `AOBJ` root.

use byroredux_core::ecs::components::{Dead, GlobalTransform};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_core::math::Vec3;
use byroredux_plugin::esm::records::EsmIndex;
use byroredux_scripting::story_manager::{raise_story_event, ScriptedStoryEvent, SmTree, StoryEvent, StoryManagerRng};
use std::collections::HashMap;
use std::sync::Arc;

/// The `SMEN` mnemonic of the attraction-object event.
pub(crate) const AOBJ: [u8; 4] = *b"AOBJ";

/// Seconds between scans.
const SCAN_SECS: f32 = 0.5;

/// One attraction rule the system models.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rule {
    pub radius: f32,
    pub min_delay: f32,
    pub max_delay: f32,
    pub requires_line_of_sight: bool,
}

/// The attraction keywords, their rules, and which bases carry them. `NOT_SAVED_BY_DESIGN`: rebuilt from the
/// records every boot.
#[derive(Debug, Default, Clone)]
pub(crate) struct AttractionBook {
    rules: Arc<HashMap<u32, Rule>>,
    /// Object bases (activator / flora / furniture / container) → their attraction keywords.
    objects: Arc<HashMap<u32, Vec<u32>>>,
    /// NPC bases → their attraction keywords.
    actors: Arc<HashMap<u32, Vec<u32>>>,
}

impl Resource for AttractionBook {}

pub(crate) fn install_attraction_book(world: &mut World, index: &EsmIndex) -> usize {
    // Keyword → rule, for the rules that are fully documented (no Combat Target).
    let rules: HashMap<u32, Rule> = index
        .keywords
        .values()
        .filter(|keyword| keyword.attraction_rule != 0)
        .filter_map(|keyword| {
            let rule = index.attraction_rules.get(&keyword.attraction_rule)?;
            (!rule.combat_target && rule.radius > 0.0).then_some((
                keyword.form_id,
                Rule {
                    radius: rule.radius,
                    min_delay: rule.min_delay.max(0.0),
                    max_delay: rule.max_delay.max(rule.min_delay).max(0.0),
                    requires_line_of_sight: rule.requires_line_of_sight,
                },
            ))
        })
        .collect();
    let attraction_of = |keywords: &[u32]| -> Vec<u32> { keywords.iter().copied().filter(|k| rules.contains_key(k)).collect() };
    let objects: HashMap<u32, Vec<u32>> = index
        .placed_object_keywords
        .iter()
        .filter_map(|(base, keywords)| {
            let kept = attraction_of(keywords);
            (!kept.is_empty()).then_some((*base, kept))
        })
        .collect();
    let actors: HashMap<u32, Vec<u32>> = index
        .npcs
        .iter()
        .filter_map(|(base, npc)| {
            let kept = attraction_of(&npc.keywords);
            (!kept.is_empty()).then_some((*base, kept))
        })
        .collect();
    let count = rules.len();
    world.insert_resource(AttractionBook { rules: Arc::new(rules), objects: Arc::new(objects), actors: Arc::new(actors) });
    count
}

/// Per-pair reaction timers and the scan clock. `NOT_SAVED_BY_DESIGN`: ambient reactions a load re-arms.
#[derive(Debug, Default)]
pub(crate) struct AttractionState {
    elapsed: f32,
    /// (actor, object, keyword) → seconds left before the next reaction.
    pairs: HashMap<(EntityId, EntityId, u32), f32>,
}

impl Resource for AttractionState {}

/// The first attraction keyword an actor and an object share.
fn shared_keywords<'a>(actor: &'a [u32], object: &'a [u32]) -> impl Iterator<Item = u32> + 'a {
    actor.iter().copied().filter(move |keyword| object.contains(keyword))
}

fn draw_delay(world: &World, rule: &Rule) -> f32 {
    let unit = world.try_resource_mut::<StoryManagerRng>().map_or(0.5, |mut rng| rng.unit_inclusive());
    rule.min_delay + unit * (rule.max_delay - rule.min_delay)
}

/// System: raise `AOBJ` when an actor has been near an attraction object for its delay.
pub(crate) fn attraction_system(world: &World, dt: f32) {
    if !world.try_resource::<SmTree>().is_some_and(|tree| tree.roots_by_mnemonic.contains_key(&AOBJ)) {
        return;
    }
    let book = match world.try_resource::<AttractionBook>() {
        Some(book) if !book.rules.is_empty() => book.clone(),
        _ => return,
    };
    let Some(mut state) = world.try_resource_mut::<AttractionState>() else {
        return;
    };
    state.elapsed += dt.max(0.0);
    if state.elapsed < SCAN_SECS {
        return;
    }
    let step = std::mem::take(&mut state.elapsed);
    drop(state);

    let mut objects: Vec<(EntityId, Vec3, &Vec<u32>)> = Vec::new();
    let mut actors: Vec<(EntityId, Vec3, &Vec<u32>)> = Vec::new();
    if let Some(candidates) = world.query::<byroredux_scripting::SceneAliasCandidate>() {
        for (entity, candidate) in candidates.iter() {
            let is_object = book.objects.get(&candidate.base_form_id);
            let is_actor = book.actors.get(&candidate.base_form_id);
            if is_object.is_none() && is_actor.is_none() {
                continue;
            }
            let Some(at) = world.get::<GlobalTransform>(entity).map(|t| t.translation) else {
                continue;
            };
            if let Some(keywords) = is_object {
                objects.push((entity, at, keywords));
            }
            if let Some(keywords) = is_actor {
                if world.get::<Dead>(entity).is_none() {
                    actors.push((entity, at, keywords));
                }
            }
        }
    }
    let eye = Vec3::Y * crate::npc_spawn::FALLBACK_ACTOR_CAPSULE_CENTRE_HEIGHT;
    let mut due: Vec<(EntityId, EntityId)> = Vec::new();
    let mut seen: Vec<(EntityId, EntityId, u32)> = Vec::new();
    for (actor, actor_at, actor_keywords) in &actors {
        for (object, object_at, object_keywords) in &objects {
            if actor == object {
                continue;
            }
            for keyword in shared_keywords(actor_keywords, object_keywords) {
                let rule = book.rules[&keyword];
                if actor_at.distance(*object_at) > rule.radius {
                    continue;
                }
                if rule.requires_line_of_sight
                    && !crate::systems::faction_hostility::sight_is_clear(world, *actor_at + eye, *object_at + eye)
                {
                    continue;
                }
                seen.push((*actor, *object, keyword));
            }
        }
    }
    // Advance the timers of the pairs still in range, draw one for each new pair, forget the rest.
    let fresh_delays: HashMap<(EntityId, EntityId, u32), f32> = seen
        .iter()
        .filter(|key| !world.try_resource::<AttractionState>().is_some_and(|state| state.pairs.contains_key(*key)))
        .map(|key| (*key, draw_delay(world, &book.rules[&key.2])))
        .collect();
    let Some(mut state) = world.try_resource_mut::<AttractionState>() else {
        return;
    };
    let mut next = HashMap::with_capacity(seen.len());
    let mut redraw = Vec::new();
    for key in &seen {
        let left = match state.pairs.get(key) {
            Some(left) => left - step,
            None => fresh_delays[key] - step,
        };
        if left <= 0.0 {
            due.push((key.0, key.1));
            redraw.push(*key);
        } else {
            next.insert(*key, left);
        }
    }
    state.pairs = next;
    drop(state);
    // A pair that reacted draws its next delay.
    let redrawn: Vec<((EntityId, EntityId, u32), f32)> = redraw.iter().map(|key| (*key, draw_delay(world, &book.rules[&key.2]))).collect();
    if let Some(mut state) = world.try_resource_mut::<AttractionState>() {
        for (key, delay) in redrawn {
            state.pairs.insert(key, delay);
        }
    }
    if due.is_empty() {
        return;
    }
    // #5372 — the location is resolved before the StoryEvent write guard.
    let location = crate::systems::story_events::derive_location(world).location;
    for (actor, object) in due {
        raise_story_event(
            world,
            actor,
            StoryEvent {
                mnemonic: AOBJ,
                reference_1: actor,
                reference_2: Some(object),
                location_1: location,
                location_2: None,
                // `abCommanded` is false: nothing commanded the actor to the object.
                scripted: Some(ScriptedStoryEvent { keyword: 0, values: [0, 0], has_reference_1: false }),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::{AttractionRuleRecord, KywdRecord, SmNodeKind, SmNodeRecord};

    const KEYWORD: u32 = 0x100;
    const NPC_BASE: u32 = 0x200;
    const OBJECT_BASE: u32 = 0x300;

    fn index(combat_target: bool) -> EsmIndex {
        let mut index = EsmIndex::default();
        index.attraction_rules.insert(
            0x900,
            AttractionRuleRecord { form_id: 0x900, editor_id: "Rule".into(), radius: 256.0, min_delay: 5.0, max_delay: 5.0, requires_line_of_sight: false, combat_target },
        );
        index.keywords.insert(KEYWORD, KywdRecord { form_id: KEYWORD, editor_id: "AO_Type_Test".into(), attraction_rule: 0x900 });
        index.placed_object_keywords.insert(OBJECT_BASE, vec![KEYWORD, 0x777]);
        index.npcs.insert(NPC_BASE, byroredux_plugin::esm::records::NpcRecord { keywords: vec![KEYWORD], ..Default::default() });
        index
    }

    fn setup(combat_target: bool) -> (World, EntityId, EntityId) {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<GlobalTransform>();
        world.register::<Dead>();
        world.insert_resource(AttractionState::default());
        let records = HashMap::from([(0x10, SmNodeRecord { form_id: 0x10, kind: SmNodeKind::Event, event_mnemonic: Some(AOBJ), ..Default::default() })]);
        byroredux_scripting::story_manager::install_story_manager(&mut world, &records);
        install_attraction_book(&mut world, &index(combat_target));
        let object = world.spawn();
        world.insert(object, GlobalTransform { translation: Vec3::ZERO, ..Default::default() });
        world.insert(object, byroredux_scripting::SceneAliasCandidate { base_form_id: OBJECT_BASE, ..Default::default() });
        let actor = world.spawn();
        world.insert(actor, GlobalTransform { translation: Vec3::new(100.0, 0.0, 0.0), ..Default::default() });
        world.insert(actor, byroredux_scripting::SceneAliasCandidate { base_form_id: NPC_BASE, ..Default::default() });
        (world, actor, object)
    }

    fn events(world: &World) -> usize {
        let markers = world.query::<StoryEvent>().map(|e| e.iter().filter(|(_, e)| e.mnemonic == AOBJ).count()).unwrap_or(0);
        let queued = world.try_resource::<byroredux_scripting::story_manager::PendingScriptStoryEvents>().map_or(0, |p| p.0.iter().filter(|e| e.mnemonic == AOBJ).count());
        markers + queued
    }

    #[test]
    fn the_book_keeps_only_documented_rules_and_the_keywords_both_sides_share() {
        let (world, _actor, _object) = setup(false);
        let book = world.resource::<AttractionBook>();
        assert_eq!(book.rules.len(), 1);
        assert_eq!(book.objects[&OBJECT_BASE], vec![KEYWORD], "the non-attraction keyword is dropped");
        assert_eq!(book.actors[&NPC_BASE], vec![KEYWORD]);
        let (world, _, _) = setup(true);
        assert!(world.resource::<AttractionBook>().rules.is_empty(), "a Combat Target rule is not modelled");
    }

    /// An actor inside the radius reacts once its delay runs out, with itself in R1 and the object in R2;
    /// outside the radius it does not.
    #[test]
    fn an_actor_near_an_attraction_object_reacts_after_the_rules_delay() {
        let (mut world, actor, object) = setup(false);
        for _ in 0..4 {
            attraction_system(&world, 1.0);
        }
        assert_eq!(events(&world), 0, "4 s in: the 5 s delay has not run out");
        attraction_system(&world, 1.0);
        assert_eq!(events(&world), 1);
        let marker = world.get::<StoryEvent>(actor).map(|e| *e).expect("raised on the actor");
        assert_eq!((marker.mnemonic, marker.reference_1, marker.reference_2), (AOBJ, actor, Some(object)));
        assert_eq!(marker.scripted.map(|s| s.values[0]), Some(0), "not commanded");

        world.insert(actor, GlobalTransform { translation: Vec3::new(900.0, 0.0, 0.0), ..Default::default() });
        world.query_mut::<StoryEvent>().unwrap().remove(actor);
        for _ in 0..20 {
            attraction_system(&world, 1.0);
        }
        assert_eq!(events(&world), 0, "outside 256 units nothing is detected");
    }
}
