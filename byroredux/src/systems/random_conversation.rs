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
//! Once a roll succeeds the pair is held (CK: "the AI code controls both
//! actors with interrupt behavior"): both packages are suspended, the
//! initiator walks to `iAISocialDistanceToTriggerEvent` of the partner while
//! the partner stops and turns to face it, `ADIA` is raised on arrival, and
//! after `fAISocialTimerToWaitForEvent` the hold either continues while a
//! scene runs for them or ends (no conversation started). When the scene is
//! over the hold ends and the actors' packages are re-selected.
//!
//! The *Actor Hello* event (`AHEL`, CK *Actor Hello Event*) is the same attempt made by an actor that is
//! **moving past** another: "the initiating actor must be moving, not in a scene, and have picked up a new
//! headtrack target; the valid target is not the player and not in a scene". The engine has no ambient
//! head-tracking, so the new target is the partner an attempt picks (the nearest valid actor inside the
//! social radius) when it differs from the one the actor last greeted; the timers, radius, chance and
//! same-pair cooldown are the conversation settings above. A passing hello takes nobody over: no hold, no
//! walk — the event is raised where the pair stands (`OnStoryHello(akLocation, akActor1, akActor2)`).
//! An actor counts as moving when it travelled [`MOVING_UNITS_PER_PASS`] since the last pass.
//!
//! The scene's Phase 0 "waits until the initiator reaches `fAIMinGreetingDistance`":
//! the event is raised at the nearer of that and the trigger distance, so the
//! scene begins once it holds. Not modelled: the *complex scene* flag and the
//! player's "important conversation" are approximated by the open dialogue
//! surface.

use byroredux_core::ecs::components::{EatBehavior, GlobalTransform, GuardBehavior, PatrolBehavior, SandboxBehavior, Transform};
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
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

/// The `SMEN` mnemonic of the Actor Hello Event (moving actors passing each other).
pub(crate) const AHEL: [u8; 4] = *b"AHEL";

/// How far an actor must have travelled since the previous one-second pass to be "moving". The CK gives
/// no speed; any real stride covers this, an idle actor's animation sway does not.
const MOVING_UNITS_PER_PASS: f32 = 10.0;

/// The CK's defaults (*Controlling Random Conversations*), used where the load
/// order authors no value. Skyrim.esm authors its own (radius 500 / 500, chance 10 % / 10 %,
/// timers 10-30 s, trigger distance 200; none for the same-NPC timer, the wait or the greeting
/// distance), so the values in play there are the master's, not these.
const DEFAULT_RADIUS_EXTERIOR: f32 = 2000.0;
const DEFAULT_RADIUS_INTERIOR: f32 = 1000.0;
const DEFAULT_CHANCE_EXTERIOR: f32 = 25.0;
const DEFAULT_CHANCE_INTERIOR: f32 = 25.0;
const DEFAULT_SAME_NPC_TIMER: f32 = 120.0;
const DEFAULT_TIMER_MIN: f32 = 30.0;
const DEFAULT_TIMER_MAX: f32 = 60.0;
const DEFAULT_TRIGGER_DISTANCE: f32 = 120.0;
const DEFAULT_MIN_GREETING_DISTANCE: f32 = 150.0;
const DEFAULT_WAIT_FOR_EVENT: f32 = 0.5;
/// A hold that has not reached its partner after this long ends (a pair that cannot path to each
/// other) rather than holding two actors for the rest of the session.
const MAX_APPROACH_SECS: f32 = 90.0;
/// The longest a conversation scene holds its pair. Vanilla conversation scenes run seconds; this
/// only bounds a scene that never finishes.
const MAX_TALK_SECS: f32 = 600.0;

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
    /// How close the initiator gets before the event is sent: `iAISocialDistanceToTriggerEvent`, or
    /// `fAIMinGreetingDistance` when that is nearer (the scene's Phase 0 waits for it).
    pub trigger_distance: f32,
    /// `fAISocialTimerToWaitForEvent`: how long it waits for a scene to start.
    pub wait_for_event: f32,
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
            // The CK holds the triggered scene in Phase 0 "until the initiating actor reaches
            // fAIMinGreetingDistance"; raising the event at the nearer of the two distances starts the
            // scene when that holds, which is the same moment its first line begins.
            trigger_distance: setting("iAISocialDistanceToTriggerEvent", DEFAULT_TRIGGER_DISTANCE)
                .min(setting("fAIMinGreetingDistance", DEFAULT_MIN_GREETING_DISTANCE))
                .max(1.0),
            wait_for_event: setting("fAISocialTimerToWaitForEvent", DEFAULT_WAIT_FOR_EVENT).max(0.0),
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
    /// Each actor's position at the previous pass, to tell who is moving.
    last_positions: HashMap<EntityId, Vec3>,
    /// The partner each actor last greeted in passing: a hello needs a *new* target.
    hello_targets: HashMap<EntityId, EntityId>,
}

impl Resource for RandomConversationState {}

/// Where a held pair's conversation stands. Only the initiator's phase drives the hold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum HoldPhase {
    /// The initiator is walking to its partner (seconds so far).
    Approaching(f32),
    /// `ADIA` has been raised; waiting for a scene to start (seconds so far).
    Waiting(f32),
    /// A scene holds the pair (seconds so far).
    Talking(f32),
}

/// Marks an actor taken over by a random conversation: the ambient package system skips it, and
/// [`conversation_hold_system`] walks / releases the pair. `NOT_SAVED_BY_DESIGN`: a load drops the
/// conversation and re-selects the packages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ConversationHold {
    pub partner: EntityId,
    pub initiator: bool,
    pub phase: HoldPhase,
}

impl Component for ConversationHold {
    type Storage = SparseSetStorage<Self>;
}

fn pair(a: EntityId, b: EntityId) -> (EntityId, EntityId) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

impl RandomConversationState {
    /// The actors that travelled [`MOVING_UNITS_PER_PASS`] since the previous pass; records the new
    /// positions and forgets the actors gone from `participants`.
    fn moving(&mut self, participants: &[Participant]) -> HashSet<EntityId> {
        let moving = participants
            .iter()
            .filter(|p| self.last_positions.get(&p.entity).is_some_and(|before| (p.position - *before).length() >= MOVING_UNITS_PER_PASS))
            .map(|p| p.entity)
            .collect();
        self.last_positions = participants.iter().map(|p| (p.entity, p.position)).collect();
        let live: HashSet<EntityId> = participants.iter().map(|p| p.entity).collect();
        self.hello_targets.retain(|actor, _| live.contains(actor));
        moving
    }

    fn talked_recently(&self, a: EntityId, b: EntityId) -> bool {
        self.cooldowns.contains_key(&pair(a, b))
    }

    /// Run `step_secs` of the clock: tick cooldowns, then every actor in
    /// `participants` that has no timer gets one rolled by `roll` and the
    /// rest count down. Returns the initiators whose timer ran out.
    #[cfg(test)]
    fn advance(&mut self, step_secs: f32, participants: &[Participant], roll: impl FnMut() -> f32) -> Vec<EntityId> {
        self.advance_where(step_secs, participants, roll, |p| p.can_initiate)
    }

    /// [`Self::advance`] with the test of who may act when their timer runs out.
    fn advance_where(
        &mut self,
        step_secs: f32,
        participants: &[Participant],
        mut roll: impl FnMut() -> f32,
        may_act: impl Fn(&Participant) -> bool,
    ) -> Vec<EntityId> {
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
                    if *remaining <= 0.0 && may_act(participant) {
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
    let held: HashSet<EntityId> = world
        .query::<ConversationHold>()
        .map(|query| query.iter().map(|(actor, _)| actor).collect())
        .unwrap_or_default();
    allowed
        .into_iter()
        .filter(|actor| Some(*actor) != player && !in_scene.contains(actor) && !held.contains(actor))
        .filter(|actor| crate::systems::npc_dialogue::npc_refuses_dialogue(world, *actor).is_none())
        .filter_map(|actor| {
            let position = world.get::<GlobalTransform>(actor)?.translation;
            let sandbox = world.get::<SandboxBehavior>(actor).map(|sandbox| sandbox.search_radius);
            let can_initiate = sandbox.is_some()
                || world.has::<GuardBehavior>(actor)
                || world.has::<PatrolBehavior>(actor)
                || world.has::<EatBehavior>(actor);
            // Any actor allowed to chat takes part in the pass; a mover also initiates a hello.
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
    let (has_adia, has_ahel) = world
        .try_resource::<SmTree>()
        .map(|tree| (tree.roots_by_mnemonic.contains_key(&ADIA), tree.roots_by_mnemonic.contains_key(&AHEL)))
        .unwrap_or_default();
    if !has_adia && !has_ahel {
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
    let movers = state.moving(&participants);
    let due = state.advance_where(step, &participants, roll_timer, |p| {
        (p.can_initiate && has_adia) || (has_ahel && movers.contains(&p.entity))
    });
    let mut raised = Vec::new();
    let mut hellos = Vec::new();
    for initiator in due {
        let Some(me) = participants.iter().find(|p| p.entity == initiator) else {
            continue;
        };
        // Re-arm first: every attempt, hit or miss, restarts the clock.
        state.timers.insert(initiator, roll_timer());
        // An actor that is already going somewhere says hello in passing; one that stands at its
        // sandbox / guard post starts a conversation.
        let hello = has_ahel && movers.contains(&initiator) && !(me.can_initiate && has_adia);
        let Some(target) = pick_target(me, &participants, settings.radius(interior), |a, b| state.talked_recently(a, b)) else {
            continue;
        };
        if hello && state.hello_targets.get(&initiator) == Some(&target) {
            continue;
        }
        if unit().map_or(0.0, |unit| unit * 100.0) >= settings.chance(interior) {
            continue;
        }
        state.cooldowns.insert(pair(initiator, target), settings.same_npc_timer.max(STEP_SECS));
        if hello {
            state.hello_targets.insert(initiator, target);
            hellos.push((initiator, target));
        } else {
            raised.push((initiator, target));
        }
    }
    drop(state);

    for (initiator, target) in raised {
        log::debug!("random conversation: {initiator:?} -> {target:?}");
        begin_conversation(world, initiator, target);
    }
    for (initiator, target) in hellos {
        log::debug!("actor hello: {initiator:?} -> {target:?}");
        raise_pair_event(world, AHEL, initiator, target);
    }
}

/// Take the pair over: suspend both ambient packages (the package system skips a held actor), and
/// let [`conversation_hold_system`] walk the initiator over and raise `ADIA`.
fn begin_conversation(world: &World, initiator: EntityId, target: EntityId) {
    crate::npc_spawn::ai_package::suspend_ambient_behavior_for_combat(world, initiator);
    crate::npc_spawn::ai_package::suspend_ambient_behavior_for_combat(world, target);
    if let Some(mut holds) = world.query_mut::<ConversationHold>() {
        holds.insert(initiator, ConversationHold { partner: target, initiator: true, phase: HoldPhase::Approaching(0.0) });
        holds.insert(target, ConversationHold { partner: initiator, initiator: false, phase: HoldPhase::Waiting(0.0) });
    }
}

/// End a hold on both actors; the ambient package system re-selects their packages on its next tick
/// (suspension left their evaluation clock reset).
fn release_pair(world: &World, a: EntityId, b: EntityId) {
    if let Some(mut holds) = world.query_mut::<ConversationHold>() {
        holds.remove(a);
        holds.remove(b);
    }
}

fn raise_actor_dialogue(world: &World, initiator: EntityId, target: EntityId) {
    raise_pair_event(world, ADIA, initiator, target);
}

/// Raise `mnemonic` for a pair of actors: `R1` the initiator, `R2` the partner, `L1` the session location
/// (`OnStoryDialogue` / `OnStoryHello(akLocation, akActor1, akActor2)`).
fn raise_pair_event(world: &World, mnemonic: [u8; 4], initiator: EntityId, target: EntityId) {
    // #5372 — the location is resolved before the StoryEvent write guard.
    let location = crate::systems::story_events::derive_location(world).location;
    raise_story_event(
        world,
        initiator,
        StoryEvent {
            mnemonic,
            reference_1: initiator,
            reference_2: Some(target),
            location_1: location,
            location_2: None,
            scripted: None,
        },
    );
}

/// What one held initiator does this frame.
enum HoldStep {
    Release,
    Walk { new_pos: Vec3, rotation: Option<byroredux_core::math::Quat> },
    Arrived,
    Phase(HoldPhase),
}

/// System: walk each conversation initiator to its partner, turn the partner to face it, raise `ADIA`
/// on arrival and release the pair when no scene takes them (or the scene ends). Production registers
/// [`make_conversation_hold_system`]; the kill switch `BYRO_NO_AI_LOCOMOTION` makes the initiator
/// raise the event from where it stands, like the force-greet's arrival-in-radius path.
#[cfg(test)]
pub(crate) fn conversation_hold_system(world: &World, dt: f32) {
    conversation_hold_inner(world, dt, true, &mut HashMap::new());
}

pub(crate) fn make_conversation_hold_system(walk_enabled: bool) -> impl FnMut(&World, f32) + Send + Sync {
    let mut stuck_secs: HashMap<EntityId, f32> = HashMap::new();
    move |world: &World, dt: f32| conversation_hold_inner(world, dt, walk_enabled, &mut stuck_secs)
}

fn conversation_hold_inner(world: &World, dt: f32, walk_enabled: bool, stuck_secs: &mut HashMap<EntityId, f32>) {
    let initiators: Vec<(EntityId, ConversationHold)> = world
        .query::<ConversationHold>()
        .map(|query| query.iter().filter(|(_, hold)| hold.initiator).map(|(actor, hold)| (actor, *hold)).collect())
        .unwrap_or_default();
    if initiators.is_empty() {
        stuck_secs.clear();
        return;
    }
    stuck_secs.retain(|actor, _| initiators.iter().any(|(initiator, _)| initiator == actor));
    let settings = SocialSettings::from_world(world);
    let in_scene = byroredux_scripting::actors_in_running_scenes(world);

    // Pass 1 — decide each step under storage guards only (the physics resource is taken alone,
    // inside the walk helper, as in the other movers).
    let mut steps: Vec<(EntityId, ConversationHold, HoldStep)> = Vec::with_capacity(initiators.len());
    for (initiator, hold) in initiators {
        let partner = hold.partner;
        let step = (|| {
            let usable = |actor| {
                world.get::<GlobalTransform>(actor).is_some()
                    && crate::systems::npc_dialogue::npc_refuses_dialogue(world, actor).is_none()
            };
            if !usable(initiator) || !usable(partner) || !world.has::<ConversationHold>(partner) {
                return HoldStep::Release;
            }
            match hold.phase {
                HoldPhase::Approaching(elapsed) => {
                    let (Some(me), Some(them)) = (
                        world.get::<GlobalTransform>(initiator).map(|t| t.translation),
                        world.get::<GlobalTransform>(partner).map(|t| t.translation),
                    ) else {
                        return HoldStep::Release;
                    };
                    let flat = Vec3::new(them.x - me.x, 0.0, them.z - me.z).length();
                    if flat <= settings.trigger_distance || !walk_enabled {
                        return HoldStep::Arrived;
                    }
                    if elapsed >= MAX_APPROACH_SECS {
                        return HoldStep::Release;
                    }
                    let speed = world
                        .get::<crate::components::WalkSpeed>(initiator)
                        .map(|speed| speed.0)
                        .unwrap_or(crate::systems::locomotion::LOCOMOTION_WALK_SPEED);
                    let rotation = world.get::<Transform>(initiator).map(|t| t.rotation).unwrap_or_default();
                    let walk = crate::systems::navmesh_path::cached_walk_step(
                        world,
                        initiator,
                        me,
                        rotation,
                        them,
                        crate::systems::follow::FOLLOW_REPATH_THRESHOLD,
                        dt,
                        speed,
                    );
                    let timer = stuck_secs.entry(initiator).or_insert(0.0);
                    if crate::systems::locomotion::advance_stuck_repick(timer, walk.blocked, dt) {
                        // Held against architecture: no closer approach exists, so start from here.
                        return HoldStep::Arrived;
                    }
                    HoldStep::Walk { new_pos: walk.position, rotation: walk.rotation }
                }
                HoldPhase::Waiting(waited) => {
                    let waited = waited + dt;
                    if waited < settings.wait_for_event {
                        HoldStep::Phase(HoldPhase::Waiting(waited))
                    } else if in_scene.contains(&initiator) || in_scene.contains(&partner) {
                        HoldStep::Phase(HoldPhase::Talking(0.0))
                    } else {
                        // No conversation quest took the pair: back to what they were doing.
                        HoldStep::Release
                    }
                }
                HoldPhase::Talking(talked) => {
                    let talked = talked + dt;
                    if talked >= MAX_TALK_SECS || !(in_scene.contains(&initiator) || in_scene.contains(&partner)) {
                        HoldStep::Release
                    } else {
                        HoldStep::Phase(HoldPhase::Talking(talked))
                    }
                }
            }
        })();
        steps.push((initiator, hold, step));
    }

    // Pass 2 — apply.
    if let Some(mut transforms) = world.query_mut::<Transform>() {
        for (initiator, _, step) in &steps {
            if let HoldStep::Walk { new_pos, rotation } = step {
                if let Some(t) = transforms.get_mut(*initiator) {
                    t.translation = *new_pos;
                    if let Some(rotation) = rotation {
                        t.rotation = *rotation;
                    }
                }
            }
        }
    }
    // The partner stops and turns to face the initiator while it is held.
    let faces: Vec<(EntityId, f32, f32)> = steps
        .iter()
        .filter(|(_, _, step)| !matches!(step, HoldStep::Release))
        .filter_map(|(initiator, hold, _)| {
            let me = world.get::<GlobalTransform>(*initiator)?.translation;
            let them = world.get::<GlobalTransform>(hold.partner)?.translation;
            Some((hold.partner, me.x - them.x, me.z - them.z))
        })
        .collect();
    if let Some(mut transforms) = world.query_mut::<Transform>() {
        for (partner, dx, dz) in faces {
            if let Some(t) = transforms.get_mut(partner) {
                if dx != 0.0 || dz != 0.0 {
                    t.rotation = crate::systems::script_requests::facing(dx, dz);
                }
            }
        }
    }
    for (initiator, hold, step) in steps {
        match step {
            HoldStep::Release => {
                stuck_secs.remove(&initiator);
                release_pair(world, initiator, hold.partner);
            }
            HoldStep::Walk { .. } => {
                if let Some(mut holds) = world.query_mut::<ConversationHold>() {
                    if let Some(entry) = holds.get_mut(initiator) {
                        if let HoldPhase::Approaching(elapsed) = entry.phase {
                            entry.phase = HoldPhase::Approaching(elapsed + dt);
                        }
                    }
                }
            }
            HoldStep::Arrived => {
                stuck_secs.remove(&initiator);
                log::debug!("random conversation: {initiator:?} reached {:?}", hold.partner);
                raise_actor_dialogue(world, initiator, hold.partner);
                if let Some(mut holds) = world.query_mut::<ConversationHold>() {
                    if let Some(entry) = holds.get_mut(initiator) {
                        entry.phase = HoldPhase::Waiting(0.0);
                    }
                }
            }
            HoldStep::Phase(phase) => {
                if let Some(mut holds) = world.query_mut::<ConversationHold>() {
                    if let Some(entry) = holds.get_mut(initiator) {
                        entry.phase = phase;
                    }
                }
            }
        }
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
            world.register::<ConversationHold>();
            world.register::<Transform>();
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
            let (a, b) = (spawn(0.0), spawn(100.0));
            // The first step arms each timer at 30-60 s (random); two more
            // 50 s steps run every possible value out.
            for _ in 0..3 {
                random_conversation_system(&world, 50.0);
            }
            // A successful roll holds the pair; the initiator is already in range, so the hold
            // system raises the event on its first step.
            conversation_hold_system(&world, 0.1);
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

    /// CK *Actor Hello Event*: a **moving** actor with a valid target raises `AHEL` in passing (no hold,
    /// no walk), once per new target; an actor that stands still raises nothing.
    #[test]
    fn a_moving_actor_says_hello_to_a_new_target_once() {
        use crate::components::AmbientPackageRuntime;
        use byroredux_plugin::esm::records::{GameSetting, PackRecord, SettingValue, SmNodeKind, SmNodeRecord};
        use byroredux_scripting::package::PackageRegistry;

        let run = |walk: bool| -> Vec<StoryEvent> {
            let mut world = World::new();
            byroredux_scripting::register(&mut world);
            world.register::<AmbientPackageRuntime>();
            world.register::<GlobalTransform>();
            world.register::<ConversationHold>();
            world.register::<Transform>();
            world.insert_resource(RandomConversationState::default());
            let mut index = byroredux_plugin::esm::records::EsmIndex::default();
            index.insert_game_setting(GameSetting {
                form_id: 0x1,
                editor_id: "fAISocialchanceForConversation".into(),
                value: SettingValue::Float(101.0),
            });
            world.insert_resource(byroredux_scripting::obscript_state::GameSettings::from_index(&index));
            let records = HashMap::from([(
                0x10,
                SmNodeRecord { form_id: 0x10, kind: SmNodeKind::Event, event_mnemonic: Some(AHEL), ..Default::default() },
            )]);
            byroredux_scripting::story_manager::install_story_manager(&mut world, &records);
            world.insert_resource(PackageRegistry::from_records([PackRecord {
                form_id: 0x77,
                interrupt_flags: PKDT_INTERRUPT_RANDOM_CONVERSATIONS,
                ..Default::default()
            }]));
            let mut spawn = |x: f32| {
                let actor = world.spawn();
                world.insert(actor, GlobalTransform { translation: Vec3::new(x, 0.0, 0.0), ..Default::default() });
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
            let (mover, _still) = (spawn(0.0), spawn(100.0));
            for step in 0..6 {
                if walk {
                    // Walking along the pair's own axis, always inside the 500-unit radius.
                    world.get_mut::<GlobalTransform>(mover).unwrap().translation = Vec3::new(step as f32 * 20.0, 0.0, 0.0);
                }
                random_conversation_system(&world, 50.0);
            }
            assert!(world.query::<ConversationHold>().is_none_or(|holds| holds.iter().next().is_none()), "a hello holds nobody");
            let events = world.query::<StoryEvent>().map(|events| events.iter().map(|(_, e)| *e).collect()).unwrap_or_default();
            events
        };
        let walked = run(true);
        assert_eq!(walked.len(), 1, "one hello for the one new target: {walked:?}");
        assert_eq!(walked[0].mnemonic, AHEL);
        assert!(walked[0].reference_2.is_some());
        assert!(run(false).is_empty(), "nobody moved");
    }

    /// A pair beyond the trigger distance: the initiator is held and walks (no event yet), the partner
    /// is held and faces it; once in range `ADIA` is raised, and with no scene to take the pair after
    /// the wait both are released (CK: "wait ... to see if a Scene is initiated").
    #[test]
    fn a_far_pair_is_held_while_the_initiator_walks_then_released_without_a_scene() {
        use crate::components::AmbientPackageRuntime;
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.register::<AmbientPackageRuntime>();
        world.register::<GlobalTransform>();
        world.register::<Transform>();
        world.register::<ConversationHold>();
        let mut spawn = |x: f32| {
            let actor = world.spawn();
            world.insert(actor, GlobalTransform { translation: Vec3::new(x, 0.0, 0.0), ..Default::default() });
            world.insert(actor, Transform { translation: Vec3::new(x, 0.0, 0.0), ..Default::default() });
            actor
        };
        let (a, b) = (spawn(0.0), spawn(1000.0));
        begin_conversation(&world, a, b);
        assert_eq!(world.get::<ConversationHold>(a).map(|h| h.initiator), Some(true));
        assert_eq!(world.get::<ConversationHold>(b).map(|h| h.partner), Some(a));

        conversation_hold_system(&world, 0.1);
        assert!(
            world.query::<StoryEvent>().is_none_or(|events| events.iter().next().is_none()),
            "out of range: no event yet"
        );
        assert!(matches!(world.get::<ConversationHold>(a).map(|h| h.phase), Some(HoldPhase::Approaching(_))));

        // Teleport the initiator into range (the walk itself rides the shared KCC step).
        world.query_mut::<GlobalTransform>().unwrap().get_mut(a).unwrap().translation = Vec3::new(950.0, 0.0, 0.0);
        conversation_hold_system(&world, 0.1);
        let raised = world.query::<StoryEvent>().and_then(|events| events.iter().next().map(|(_, e)| *e));
        let event = raised.expect("in range: ADIA raised");
        assert_eq!((event.reference_1, event.reference_2), (a, Some(b)));
        assert!(matches!(world.get::<ConversationHold>(a).map(|h| h.phase), Some(HoldPhase::Waiting(_))));

        // No scene starts: after fAISocialTimerToWaitForEvent (0.5 s) both are released.
        conversation_hold_system(&world, 1.0);
        assert!(world.get::<ConversationHold>(a).is_none() && world.get::<ConversationHold>(b).is_none());
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
