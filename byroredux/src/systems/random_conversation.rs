//! NPC-to-NPC random conversations — the `ADIA` (Actor Dialogue) Story
//! Manager producer, M43.
//!
//! Skyrim and later start ambient chatter through the Story Manager: an NPC
//! whose package allows *Random conversations* looks for a partner, and the
//! `Actor Dialogue Event` it raises (`OnStoryDialogue(akLocation, akActor1,
//! akActor2)`) lets a conversation quest fill `Actor1` / `Actor2` from the
//! event and play its scene. `ADIA` gates 786 of Skyrim.esm's and 298 of
//! Fallout4.esm's Story-Manager-started quests (corpus census, 2026-10-30),
//! the largest event family by far, and nothing produced it.
//!
//! The rules are the CK's (*Scenes → Controlling Random Conversations*, local
//! `ck-uesp-wiki`), with every number a game setting:
//!
//! - an NPC does not send the event while it is in a scene, in combat or
//!   otherwise interrupted, and its package must allow random conversations
//!   (`PKDT` interrupt flag `0x2`); only Sandbox, Guard, Eat and Patrol
//!   packages initiate;
//! - it looks for valid targets inside the *social radius*
//!   (`fAISocialRadiusToTriggerConversation` exterior, `…Interior` indoors; a
//!   Sandbox package's own radius replaces it): the target's package allows
//!   random conversations too, it is not in a scene or interrupted, and the
//!   pair has not talked within `fAItalktosameNPCTimer`;
//! - with a target, it rolls `fAISocialchanceForConversation` (`…Interior`);
//! - each actor re-arms a timer between `fAISocialTimerForConversationsMin`
//!   and `…Max` after every attempt.
//!
//! Not modelled, and why: the CK has the initiator walk over to
//! `iAISocialDistanceToTriggerEvent` and the target stop; the ambient movers
//! (sandbox, patrol, guard, eat) do not yield to another driver, so the event
//! is raised from where the pair stands and the scene's own lines carry the
//! conversation. The *complex scene* flag and the player's "important
//! conversation" are approximated by the open dialogue surface.

use byroredux_core::ecs::components::{EatBehavior, GlobalTransform, GuardBehavior, PatrolBehavior, SandboxBehavior};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_core::math::Vec3;
use byroredux_scripting::cell_kind::CellKind;
use byroredux_scripting::story_manager::{raise_story_event, SmTree, StoryEvent, StoryManagerRng};
use std::collections::{HashMap, HashSet};

/// The `SMEN` mnemonic of the Actor Dialogue Event.
pub(crate) const ADIA: [u8; 4] = *b"ADIA";

/// Seconds between selection passes. The CK's own cadence is its 30–60 s
/// timers, so a one-second pass loses nothing and keeps the per-actor scan
/// off the other 59 frames.
const STEP_SECS: f32 = 1.0;

/// The CK's defaults (*Controlling Random Conversations*), used where the load
/// order authors no value.
const DEFAULT_RADIUS_EXTERIOR: f32 = 2000.0;
const DEFAULT_RADIUS_INTERIOR: f32 = 1000.0;
const DEFAULT_CHANCE_EXTERIOR: f32 = 25.0;
const DEFAULT_CHANCE_INTERIOR: f32 = 25.0;
const DEFAULT_SAME_NPC_TIMER: f32 = 120.0;
const DEFAULT_TIMER_MIN: f32 = 30.0;
const DEFAULT_TIMER_MAX: f32 = 60.0;

/// The game settings that tune random conversations, resolved once per pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SocialSettings {
    pub radius_exterior: f32,
    pub radius_interior: f32,
    /// Percent.
    pub chance_exterior: f32,
    pub chance_interior: f32,
    pub same_npc_timer: f32,
    pub timer_min: f32,
    pub timer_max: f32,
}

impl SocialSettings {
    pub(crate) fn from_world(world: &World) -> Self {
        let setting = |name: &str, default: f32| byroredux_scripting::obscript_state::game_setting(world, name).unwrap_or(default);
        let timer_min = setting("fAISocialTimerForConversationsMin", DEFAULT_TIMER_MIN).max(1.0);
        Self {
            radius_exterior: setting("fAISocialRadiusToTriggerConversation", DEFAULT_RADIUS_EXTERIOR),
            radius_interior: setting("fAISocialRadiusToTriggerConversationInterior", DEFAULT_RADIUS_INTERIOR),
            chance_exterior: setting("fAISocialchanceForConversation", DEFAULT_CHANCE_EXTERIOR),
            chance_interior: setting("fAISocialchanceForConversationInterior", DEFAULT_CHANCE_INTERIOR),
            same_npc_timer: setting("fAItalktosameNPCTimer", DEFAULT_SAME_NPC_TIMER),
            timer_min,
            timer_max: setting("fAISocialTimerForConversationsMax", DEFAULT_TIMER_MAX).max(timer_min),
        }
    }

    fn radius(&self, interior: bool) -> f32 {
        if interior {
            self.radius_interior
        } else {
            self.radius_exterior
        }
    }

    fn chance(&self, interior: bool) -> f32 {
        if interior {
            self.chance_interior
        } else {
            self.chance_exterior
        }
    }
}

/// One actor that may take part in a random conversation this pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Participant {
    pub entity: EntityId,
    pub position: Vec3,
    /// Its package is Sandbox, Guard, Eat or Patrol — the procedures that
    /// initiate (CK *Package Flags*).
    pub can_initiate: bool,
    /// A Sandbox package's own radius, which replaces the social radius.
    pub sandbox_radius: Option<f32>,
}

/// The nearest participant inside `radius` of `initiator` that has not talked
/// to it within the same-NPC timer. The CK names the validity rules but not
/// the choice among several valid targets; the nearest is the one a walk
/// would reach first.
pub(crate) fn pick_target(
    initiator: &Participant,
    others: &[Participant],
    radius: f32,
    talked_recently: impl Fn(EntityId, EntityId) -> bool,
) -> Option<EntityId> {
    let radius = initiator.sandbox_radius.filter(|radius| *radius > 0.0).unwrap_or(radius);
    others
        .iter()
        .filter(|other| other.entity != initiator.entity)
        .filter(|other| !talked_recently(initiator.entity, other.entity))
        .map(|other| (other, (other.position - initiator.position).length()))
        .filter(|(_, distance)| *distance <= radius)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(other, _)| other.entity)
}

/// Per-actor attempt timers and the pairs on cooldown. `NOT_SAVED_BY_DESIGN`:
/// the timers are 30–60 s of ambient scheduling that a load re-arms.
#[derive(Default)]
pub(crate) struct RandomConversationState {
    /// Seconds of the pass interval not yet consumed.
    pending_secs: f32,
    /// Seconds until each actor next looks for a partner.
    timers: HashMap<EntityId, f32>,
    /// Pairs (ordered by entity) and the seconds left on their cooldown.
    cooldowns: HashMap<(EntityId, EntityId), f32>,
}

impl Resource for RandomConversationState {}

fn pair(a: EntityId, b: EntityId) -> (EntityId, EntityId) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

impl RandomConversationState {
    fn talked_recently(&self, a: EntityId, b: EntityId) -> bool {
        self.cooldowns.contains_key(&pair(a, b))
    }

    /// Run `step_secs` of the clock: tick cooldowns, then every actor in
    /// `participants` that has no timer gets one rolled by `roll` and the
    /// rest count down. Returns the initiators whose timer ran out.
    fn advance(&mut self, step_secs: f32, participants: &[Participant], mut roll: impl FnMut() -> f32) -> Vec<EntityId> {
        self.cooldowns.retain(|_, remaining| {
            *remaining -= step_secs;
            *remaining > 0.0
        });
        let live: HashSet<EntityId> = participants.iter().map(|p| p.entity).collect();
        self.timers.retain(|entity, _| live.contains(entity));
        let mut due = Vec::new();
        for participant in participants {
            match self.timers.get_mut(&participant.entity) {
                None => {
                    self.timers.insert(participant.entity, roll());
                }
                Some(remaining) => {
                    *remaining -= step_secs;
                    if *remaining <= 0.0 && participant.can_initiate {
                        due.push(participant.entity);
                    }
                }
            }
        }
        due
    }
}

/// Every actor that may take part now: a live ambient package allowing random
/// conversations, outside a scene and not interrupted.
fn collect_participants(world: &World) -> Vec<Participant> {
    use crate::components::AmbientPackageRuntime;
    // The runtime query is released before the registry is taken: no two
    // guards are ever held together here.
    let active: Vec<(EntityId, u32)> = world
        .query::<AmbientPackageRuntime>()
        .map(|query| {
            query
                .iter()
                .filter_map(|(actor, runtime)| runtime.active_package_form_id.map(|package| (actor, package)))
                .collect()
        })
        .unwrap_or_default();
    let Some(registry) = world.try_resource::<byroredux_scripting::package::PackageRegistry>() else {
        return Vec::new();
    };
    let allowed: Vec<EntityId> = active
        .into_iter()
        .filter(|(_, package)| registry.package(*package).is_some_and(|pack| pack.allows_random_conversations()))
        .map(|(actor, _)| actor)
        .collect();
    drop(registry);
    if allowed.is_empty() {
        return Vec::new();
    }
    let in_scene = byroredux_scripting::actors_in_running_scenes(world);
    let player = crate::systems::PlayerEntity::get(world);
    allowed
        .into_iter()
        .filter(|actor| Some(*actor) != player && !in_scene.contains(actor))
        .filter(|actor| crate::systems::npc_dialogue::npc_refuses_dialogue(world, *actor).is_none())
        .filter_map(|actor| {
            let position = world.get::<GlobalTransform>(actor)?.translation;
            let sandbox = world.get::<SandboxBehavior>(actor).map(|sandbox| sandbox.search_radius);
            let can_initiate = sandbox.is_some()
                || world.has::<GuardBehavior>(actor)
                || world.has::<PatrolBehavior>(actor)
                || world.has::<EatBehavior>(actor);
            Some(Participant {
                entity: actor,
                position,
                can_initiate,
                sandbox_radius: sandbox.flatten(),
            })
        })
        .collect()
}

/// System: arm the actors' social timers and raise `ADIA` when one finds a
/// partner. Does nothing on a title without an `ADIA` node in its Story
/// Manager tree (Oblivion, Fallout 3, New Vegas), nor with
/// `BYRO_NO_RANDOM_CONVERSATIONS=1`.
pub(crate) fn random_conversation_system(world: &World, dt: f32) {
    let has_adia = world
        .try_resource::<SmTree>()
        .is_some_and(|tree| tree.roots_by_mnemonic.contains_key(&ADIA));
    if !has_adia {
        return;
    }
    let Some(mut state) = world.try_resource_mut::<RandomConversationState>() else {
        return;
    };
    state.pending_secs += dt;
    if state.pending_secs < STEP_SECS {
        return;
    }
    let step = std::mem::take(&mut state.pending_secs);
    drop(state);

    // No ambient conversation starts under an open player dialogue.
    if world
        .try_resource::<crate::systems::npc_dialogue::DialogueSurfaceState>()
        .is_some_and(|surface| surface.npc.is_some())
    {
        return;
    }
    let location = crate::systems::story_events::derive_location(world);
    let interior = matches!(location.kind, Some(CellKind::Interior));
    let settings = SocialSettings::from_world(world);
    let participants = collect_participants(world);

    // One draw at a time, the RNG guard released between them, so no other
    // resource is taken under it. Without the Story Manager's RNG the draws
    // fall to the midpoint (timers) and to "always succeeds" (the roll).
    let unit = || world.try_resource_mut::<StoryManagerRng>().map(|mut rng| rng.unit_inclusive());
    let roll_timer = || settings.timer_min + unit().unwrap_or(0.5) * (settings.timer_max - settings.timer_min);
    let Some(mut state) = world.try_resource_mut::<RandomConversationState>() else {
        return;
    };
    let due = state.advance(step, &participants, roll_timer);
    let mut raised = Vec::new();
    for initiator in due {
        let Some(me) = participants.iter().find(|p| p.entity == initiator) else {
            continue;
        };
        // Re-arm first: every attempt, hit or miss, restarts the clock.
        state.timers.insert(initiator, roll_timer());
        let Some(target) = pick_target(me, &participants, settings.radius(interior), |a, b| state.talked_recently(a, b)) else {
            continue;
        };
        if unit().map_or(0.0, |unit| unit * 100.0) >= settings.chance(interior) {
            continue;
        }
        state.cooldowns.insert(pair(initiator, target), settings.same_npc_timer.max(STEP_SECS));
        raised.push((initiator, target));
    }
    drop(state);

    for (initiator, target) in raised {
        log::debug!("random conversation: {initiator:?} -> {target:?}");
        raise_story_event(
            world,
            initiator,
            StoryEvent {
                mnemonic: ADIA,
                reference_1: initiator,
                reference_2: Some(target),
                location_1: location.location,
                location_2: None,
                scripted: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::misc::PKDT_INTERRUPT_RANDOM_CONVERSATIONS;

    fn at(entity: u32, x: f32, can_initiate: bool) -> Participant {
        Participant {
            entity: entity,
            position: Vec3::new(x, 0.0, 0.0),
            can_initiate,
            sandbox_radius: None,
        }
    }

    #[test]
    fn the_nearest_valid_target_inside_the_radius_is_chosen() {
        let me = at(1, 0.0, true);
        let others = [at(1, 0.0, true), at(2, 900.0, false), at(3, 400.0, false), at(4, 1500.0, false)];
        assert_eq!(pick_target(&me, &others, 1000.0, |_, _| false), Some(3));
        assert_eq!(pick_target(&me, &others, 300.0, |_, _| false), None, "outside the radius");
    }

    #[test]
    fn a_pair_that_talked_recently_is_skipped_and_a_sandbox_radius_replaces_the_social_one() {
        let mut me = at(1, 0.0, true);
        let others = [at(2, 100.0, false), at(3, 800.0, false)];
        let talked = |a: EntityId, b: EntityId| pair(a, b) == pair(1, 2);
        assert_eq!(pick_target(&me, &others, 1000.0, talked), Some(3));
        me.sandbox_radius = Some(500.0);
        assert_eq!(pick_target(&me, &others, 1000.0, talked), None, "the Sandbox radius (500) excludes the far partner");
    }

    #[test]
    fn timers_arm_count_down_and_only_initiators_fire() {
        let participants = [at(1, 0.0, true), at(2, 10.0, false)];
        let mut state = RandomConversationState::default();
        assert!(state.advance(1.0, &participants, || 3.0).is_empty(), "first sight arms the timers");
        assert!(state.advance(1.0, &participants, || 3.0).is_empty());
        let due = state.advance(2.5, &participants, || 3.0);
        assert_eq!(due, vec![1], "the non-initiator's timer ran out too, but it does not initiate");
    }

    #[test]
    fn cooldowns_expire_and_departed_actors_drop_their_timers() {
        let participants = [at(1, 0.0, true), at(2, 10.0, true)];
        let mut state = RandomConversationState::default();
        state.advance(1.0, &participants, || 50.0);
        state.cooldowns.insert(pair(1, 2), 2.0);
        assert!(state.talked_recently(2, 1));
        state.advance(1.0, &participants[..1], || 50.0);
        assert!(state.talked_recently(1, 2));
        assert!(!state.timers.contains_key(&2), "actor 2 left");
        state.advance(1.0, &participants[..1], || 50.0);
        assert!(!state.talked_recently(1, 2), "cooldown elapsed");
    }

    /// The whole producer on a minimal world: two flagged sandboxing NPCs in
    /// range, an `ADIA` root in the tree. The first pass arms their timers, the
    /// later ones run them out and raise `ADIA` on the initiator with both actors
    /// in the event's slots; a package without the flag raises nothing.
    #[test]
    fn two_flagged_sandbox_npcs_raise_an_actor_dialogue_event() {
        use crate::components::AmbientPackageRuntime;
        use byroredux_plugin::esm::records::{GameSetting, PackRecord, SettingValue, SmNodeKind, SmNodeRecord};
        use byroredux_scripting::package::PackageRegistry;

        let run = |flag: u16| -> Option<StoryEvent> {
            let mut world = World::new();
            byroredux_scripting::register(&mut world);
            world.register::<AmbientPackageRuntime>();
            world.register::<GlobalTransform>();
            world.register::<SandboxBehavior>();
            world.insert_resource(RandomConversationState::default());
            // The social roll is a dice throw; a chance past 100 % makes it certain.
            let mut index = byroredux_plugin::esm::records::EsmIndex::default();
            index.insert_game_setting(GameSetting {
                form_id: 0x1,
                editor_id: "fAISocialchanceForConversation".into(),
                value: SettingValue::Float(101.0),
            });
            world.insert_resource(byroredux_scripting::obscript_state::GameSettings::from_index(&index));
            let records = HashMap::from([(
                0x10,
                SmNodeRecord {
                    form_id: 0x10,
                    kind: SmNodeKind::Event,
                    event_mnemonic: Some(ADIA),
                    ..Default::default()
                },
            )]);
            byroredux_scripting::story_manager::install_story_manager(&mut world, &records);
            world.insert_resource(PackageRegistry::from_records([PackRecord {
                form_id: 0x77,
                interrupt_flags: flag,
                ..Default::default()
            }]));
            let mut spawn = |x: f32| {
                let actor = world.spawn();
                world.insert(actor, GlobalTransform { translation: Vec3::new(x, 0.0, 0.0), ..Default::default() });
                world.insert(actor, SandboxBehavior { search_radius: None });
                world.insert(
                    actor,
                    AmbientPackageRuntime {
                        package_candidates: vec![0x77],
                        active_package_form_id: Some(0x77),
                        actor_form_id: 0x1000 + actor,
                        last_evaluated_game_minute: None,
                    },
                );
                actor
            };
            let (a, b) = (spawn(0.0), spawn(300.0));
            // The first step arms each timer at 30-60 s (random); two more
            // 50 s steps run every possible value out.
            for _ in 0..3 {
                random_conversation_system(&world, 50.0);
            }
            let event = world.query::<StoryEvent>().and_then(|events| events.iter().next().map(|(_, event)| *event));
            event.inspect(|event| {
                assert_eq!(event.mnemonic, ADIA);
                let pair = (event.reference_1, event.reference_2);
                assert!(pair == (a, Some(b)) || pair == (b, Some(a)), "the event names both actors: {pair:?}");
            })
        };
        assert!(run(PKDT_INTERRUPT_RANDOM_CONVERSATIONS).is_some(), "flagged pair converses");
        assert!(run(0x0001).is_none(), "hellos alone do not allow random conversations");
    }

    #[test]
    fn settings_fall_back_to_the_ck_defaults_and_keep_min_below_max() {
        let world = World::new();
        let settings = SocialSettings::from_world(&world);
        assert_eq!(settings.radius_exterior, 2000.0);
        assert_eq!(settings.radius_interior, 1000.0);
        assert_eq!(settings.chance_exterior, 25.0);
        assert_eq!(settings.same_npc_timer, 120.0);
        assert!(settings.timer_min <= settings.timer_max);
    }
}
