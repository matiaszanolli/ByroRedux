//! Quest-stage *fragment* dispatch — the runtime half of the b2 lever
//! ([`docs/engine/m47-2-recognizer-scaling.md`]).
//!
//! A Papyrus quest carries one compiled `Fragment_N` function per stage;
//! the runtime runs stage N's fragment when the quest is `SetStage`-d to
//! N. The [`crate::translate::effects`] lowerer turns each fragment body
//! into a `Vec<Effect>`; this module stores those per `(quest, stage)`
//! and applies them when a [`QuestStageAdvanced`] marker says the stage
//! was set.
//!
//! ## What ships here
//!
//! - **Engine + contract + dispatch:** the [`QuestStageFragments`]
//!   resource, [`apply_effects`] against the canonical [`QuestStageState`]
//!   / [`QuestObjectiveState`], and [`quest_fragment_dispatch_system`]
//!   which consumes `QuestStageAdvanced` and cascades chained `SetStage`s
//!   (bounded).
//! - **Population (shipped, #1739 / `8a70b81a`):** [`QuestStageFragments`]
//!   is filled from real game data via the QUST `VMAD` fragment-section
//!   decoder
//!   (`byroredux_plugin::esm::records::script_instance::parse_quest_fragments`)
//!   feeding [`populate_quest_fragments_from_pex`], wired live from the
//!   cell loader. Validated end-to-end on real Skyrim data.
//!
//! ## Quest-ref resolution
//!
//! A fragment's effect targets `Self` / `Self.GetOwningQuest()` (→ the
//! advancing quest, known at dispatch) or a `Quest Property` (→ a *different*
//! quest bound by the QUST's own VMAD). The former always resolves; the
//! latter needs the quest's VMAD scripts-section registered via
//! [`QuestStageFragments::insert_vmad`] (the same bytes the fragment-binding
//! decoder reads, decoded a second time for its property table) — a
//! `Property`-targeted effect with no VMAD on hand, or naming a property the
//! VMAD doesn't carry, is skipped (logged), never guessed.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use byroredux_core::ecs::components::{
    EquipmentSlots, GlobalTransform, Inventory, InventoryIndex, ItemStack, Transform,
};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::script_instance::{
    PropertyValue, SceneFragmentEvent, ScriptInstanceData,
};

use crate::quest_stages::{
    QuestFormId, QuestObjectiveState, QuestStageAdvanced, QuestStageAdvancedBatch, QuestStageState,
    FRAGMENT_QUEST_EVENT_SUBSCRIBER,
};
use byroredux_papyrus::ast::{Script, ScriptItem, StateItem, Stmt, Type};
use byroredux_papyrus::span::Spanned;

use crate::translate::compose::{ObjectRef, QuestRef};
use crate::translate::effects::{lower_fragment_with_quest_properties, ActorRef, Effect};

/// Persistent enable/disable state for placed references targeted by
/// Papyrus fragments. Form IDs keep the state valid while the reference's
/// cell is unloaded; cell streaming can consult this resource when spawning
/// enable-parent chains.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ReferenceEnableState {
    disabled: HashSet<u32>,
}

impl Resource for ReferenceEnableState {}

impl ReferenceEnableState {
    pub fn is_enabled(&self, form_id: u32) -> bool {
        !self.disabled.contains(&form_id)
    }

    pub fn set_enabled(&mut self, form_id: u32, enabled: bool) {
        if enabled {
            self.disabled.remove(&form_id);
        } else {
            self.disabled.insert(form_id);
        }
    }
}

/// Lowered quest-stage fragments, keyed by `(quest, stage)`. Populated at
/// cell load by [`populate_quest_fragments_from_pex`] from the QUST `VMAD`
/// fragment bindings the decoder recovers; consumed by
/// [`quest_fragment_dispatch_system`].
#[derive(Debug, Clone, Default)]
pub struct QuestStageFragments {
    map: Arc<HashMap<(QuestFormId, u16), Vec<Effect>>>,
    /// The QF_ script's own property table per quest — the same VMAD
    /// scripts-section bytes the QUST record's fragment section is read
    /// alongside. Lets a fragment's cross-quest `Property`-targeted
    /// effect (`SomeOtherQuest.SetStage(..)` via a `Quest Property`)
    /// resolve at dispatch time instead of always skipping.
    vmad: Arc<HashMap<QuestFormId, ScriptInstanceData>>,
    /// Whether the QUST walk that fills the two maps above has already run
    /// for this session.
    ///
    /// #3161 — callers used to gate the walk on `is_empty()`, which reads
    /// only `map`. But the walk fills `map` and `vmad` independently:
    /// `insert_vmad` runs for every scripted quest before any `.pex` is
    /// resolved, while `insert` runs only on a successful lowering. A
    /// session where the VMAD side populates but no `QF_` `.pex` resolves —
    /// a wrong or missing `--scripts-bsa`, exactly what the smoke harness's
    /// WARN text anticipates — leaves `map` empty forever, so the full
    /// 845-quest walk, with a per-quest `HashMap` build and an archive
    /// `extract_pex` per script name, re-ran on every exterior cell
    /// `begin` for the rest of the session. A dedicated latch also stays
    /// correct in the legitimately-empty cases (pre-Papyrus game, no script
    /// archive) where both maps end empty and `is_empty()` would re-walk
    /// just the same.
    populated: bool,
}

impl Resource for QuestStageFragments {}

impl QuestStageFragments {
    /// Register a stage's lowered fragment effects.
    pub fn insert(&mut self, quest: QuestFormId, stage: u16, effects: Vec<Effect>) {
        Arc::make_mut(&mut self.map).insert((quest, stage), effects);
    }

    /// The lowered effects for a `(quest, stage)`, if any.
    pub fn get(&self, quest: QuestFormId, stage: u16) -> Option<&[Effect]> {
        self.map.get(&(quest, stage)).map(Vec::as_slice)
    }

    /// Register a quest's own VMAD scripts section (its declared
    /// property bindings), so `Property`-targeted effects in its
    /// fragments can resolve. A no-op for a VMAD with no attached
    /// scripts — nothing a `Property` lookup could ever match.
    pub fn insert_vmad(&mut self, quest: QuestFormId, vmad: ScriptInstanceData) {
        if vmad.has_script() {
            Arc::make_mut(&mut self.vmad).insert(quest, vmad);
        }
    }

    /// The registered VMAD for `quest`, if any.
    pub fn vmad(&self, quest: QuestFormId) -> Option<&ScriptInstanceData> {
        self.vmad.get(&quest)
    }

    /// Number of registered stage fragments.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Whether the QUST fragment walk has already run this session — see
    /// [`Self::populated`]. Callers deciding whether to walk must use this
    /// rather than [`Self::is_empty`].
    pub fn is_populated(&self) -> bool {
        self.populated
    }

    /// Latch the walk as done, whatever it found. Idempotent.
    pub fn mark_populated(&mut self) {
        self.populated = true;
    }
}

/// Lowered Papyrus fragments attached to authored `SCEN` lifecycle events.
///
/// A scene event has at most one fragment binding in the VMAD format, so the
/// canonical key is `(scene FormID, event)`. The value retains the owning
/// quest and scene VMAD property table needed by the shared effect executor.
#[derive(Debug, Clone, Default)]
pub struct SceneFragments {
    map: Arc<HashMap<(u32, SceneFragmentEvent), SceneFragmentEffects>>,
}

impl Resource for SceneFragments {}

#[derive(Debug, Clone)]
struct SceneFragmentEffects {
    context: QuestFormId,
    vmad: Option<ScriptInstanceData>,
    effects: Vec<Effect>,
}

impl SceneFragments {
    pub fn insert(
        &mut self,
        scene_form_id: u32,
        event: SceneFragmentEvent,
        context: QuestFormId,
        vmad: Option<ScriptInstanceData>,
        effects: Vec<Effect>,
    ) {
        Arc::make_mut(&mut self.map).insert(
            (scene_form_id, event),
            SceneFragmentEffects {
                context,
                vmad,
                effects,
            },
        );
    }

    fn get(&self, scene_form_id: u32, event: SceneFragmentEvent) -> Option<&SceneFragmentEffects> {
        self.map.get(&(scene_form_id, event))
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// One suspended latent fragment tail. The VMAD snapshot is retained so a
/// continuation resolves properties exactly as the original dispatch did,
/// even if the installed fragment table changes before the wait expires.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
struct PendingFragmentExecution {
    context: QuestFormId,
    vmad: Option<ScriptInstanceData>,
    effects: Vec<Effect>,
    remaining_seconds: f32,
    resume_when: FragmentResumeCondition,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
enum FragmentResumeCondition {
    DelayElapsed,
    Actors3DLoaded {
        actors: Vec<ActorRef>,
        poll_seconds: f32,
        /// Total seconds this entry has spent re-polling with the actors
        /// still unresolved. #2288 (SCR-D6-NEW5-02) — capped by
        /// [`MAX_ACTORS_3D_LOADED_WAIT_SECONDS`] so a permanently-unloadable
        /// alias target (or a quest that was reset out from under the
        /// suspended tail) doesn't retry forever with no eviction path,
        /// unlike `quest_fragment_dispatch_system`'s bounded `MAX_CASCADE`.
        elapsed_seconds: f32,
    },
}

/// Maximum total time (across every re-poll) a suspended
/// `WaitForActors3DLoaded` continuation is allowed to wait before the
/// tail is declined outright. #2288 (SCR-D6-NEW5-02) — real content polls
/// on the order of tenths of a second, so 30s is generous headroom while
/// still guaranteeing every entry eventually leaves the queue.
const MAX_ACTORS_3D_LOADED_WAIT_SECONDS: f32 = 30.0;

/// Runtime queue for latent time waits and bounded-work `Is3DLoaded` polling
/// continuations.
///
/// #2381 (SAVE-D1-16) — registered in
/// `byroredux::save_io::build_save_registry`. A suspended tail's remaining
/// effects and resume condition exist only here; a save taken mid-
/// `Utility.Wait`/`WaitForActors3DLoaded` previously dropped the pending
/// continuation silently on load.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct FragmentExecutionQueue {
    pending: Vec<PendingFragmentExecution>,
}

impl Resource for FragmentExecutionQueue {}

impl FragmentExecutionQueue {
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// `Effect::Activate` targets awaiting delivery as `ActivateEvent`, drained
/// by [`fragment_activation_flush_system`] at the head of the next frame.
///
/// Why a frame of latency instead of a direct insert: `Stage::Update`
/// schedules the `ActivateEvent` consumers (`rumble_on_activate_dispatch`,
/// `quest_advance_system`, `two_state_activator_system`) *before*
/// `quest_fragment_dispatch_system`, because fragment dispatch consumes the
/// `QuestStageAdvanced` markers `quest_advance_system` produces — the order
/// cannot simply be swapped. With `event_cleanup_system` draining the marker
/// at `Stage::Late`, a marker inserted during dispatch was visible to
/// neither this frame's earlier consumers nor the next frame's (#2654).
/// Flushing at the head of the next frame delivers it to all of them,
/// exactly once.
#[derive(Debug, Clone, Default)]
pub struct PendingFragmentActivations(Vec<(EntityId, EntityId)>);

impl Resource for PendingFragmentActivations {}

impl PendingFragmentActivations {
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Deliver queued fragment activations as `ActivateEvent` markers.
///
/// Must be scheduled in `Stage::Update` **before** every `ActivateEvent`
/// consumer; see [`PendingFragmentActivations`].
pub fn fragment_activation_flush_system(world: &World, _dt: f32) {
    let pending: Vec<(EntityId, EntityId)> = {
        let Some(mut queue) = world.try_resource_mut::<PendingFragmentActivations>() else {
            return;
        };
        if queue.0.is_empty() {
            return;
        }
        std::mem::take(&mut queue.0)
    };
    let Some(mut events) = world.query_mut::<crate::ActivateEvent>() else {
        log::debug!("fragment Activate skipped: ActivateEvent component never registered");
        return;
    };
    for (target, activator) in pending {
        events.insert(target, crate::ActivateEvent { activator });
    }
}

/// Resolve an effect's [`QuestRef`] to a concrete quest FormID.
/// `Self`/`GetOwningQuest` resolve to the advancing quest; a
/// `Quest Property` resolves through the quest's VMAD (when supplied).
fn resolve_quest(
    via: &QuestRef,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
) -> Option<QuestFormId> {
    match via {
        QuestRef::SelfRef | QuestRef::OwningQuest => Some(context),
        QuestRef::Property(name) => vmad?
            .scripts
            .iter()
            .find_map(|s| s.object_form_id(name))
            .map(QuestFormId),
    }
}

/// Resolve a `Quest`/`Self`-typed effect's `QuestRef`, logging a `debug`
/// line on failure. Shared by every quest-scoped `apply_effect` arm so
/// the "skip, never guess" contract has one call site.
fn resolve_quest_logged(
    via: &QuestRef,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
) -> Option<QuestFormId> {
    let quest = resolve_quest(via, context, vmad);
    if quest.is_none() {
        log::debug!("fragment effect skipped: unresolved quest ref {via:?}");
    }
    quest
}

/// Resolve an [`ObjectRef::Property`] to its VMAD-bound FormID. Requires
/// an `Object`-typed property with `alias == -1` — an alias-bound entry
/// (`ReferenceAlias Property`) needs the quest-alias-fill subsystem to
/// resolve correctly, so it declines rather than trusting the raw
/// `form_id` sitting next to a live alias index.
///
/// #2186 — the `alias == -1` check itself now lives in
/// `ScriptInstance::object_form_id`, which `resolve_quest` also uses.
/// This function open-coded the strict match while `object_form_id`
/// stayed lax, so the two sibling resolvers disagreed; sharing one
/// accessor is what keeps them from drifting apart again.
fn resolve_property_form_id(vmad: Option<&ScriptInstanceData>, name: &str) -> Option<u32> {
    vmad?.scripts.iter().find_map(|s| s.object_form_id(name))
}

/// The global form ID of a live entity — the inverse of
/// [`crate::condition::resolve_entity_by_global_form_id`], and keyed the same
/// way (`FormIdPool::resolve(..).local.0`) so the two agree.
///
/// #3278 — needed by [`Effect::Disable`], whose sink
/// ([`ReferenceEnableState`]) is deliberately FormID-keyed rather than
/// entity-keyed so a disable survives its reference's cell being unloaded.
/// Alias-bound receivers resolve to an *entity*, so they have to come back
/// to a form ID to be recorded.
fn entity_global_form_id(
    world: &World,
    entity: byroredux_core::ecs::storage::EntityId,
) -> Option<u32> {
    use byroredux_core::ecs::components::FormIdComponent;
    use byroredux_core::form_id::FormIdPool;
    let component = world.get::<FormIdComponent>(entity)?;
    let pool = world.try_resource::<FormIdPool>()?;
    pool.resolve(component.0).map(|pair| pair.local.0)
}

/// Resolve an [`ObjectRef`] all the way to a live entity. Direct VMAD object
/// properties go through FormID → loaded entity; alias-bound properties go
/// through the owning quest's [`crate::scene::SceneActorBindings`] snapshot
/// (`bindings`, captured by [`DeferredFragmentEffects::new`] before the
/// quest-state guards are taken — #2660 / SCR-D6-NEW11-03 — rather than
/// acquired live here, which would nest a `SceneActorBindings` read inside
/// that scope). `debug`-logs and returns `None` at whichever hop fails — an
/// unresolved object is a runtime data fact (not loaded, not spawned yet),
/// not a shape the fragment failed to understand.
fn resolve_object(
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    context: QuestFormId,
    via: &ObjectRef,
    bindings: &crate::scene::SceneActorBindings,
) -> Option<byroredux_core::ecs::storage::EntityId> {
    let name = via.property_name();
    let Some(value) = vmad?
        .scripts
        .iter()
        .find_map(|script| script.property(name))
        .map(|property| &property.value)
    else {
        log::debug!("fragment effect skipped: object ref '{name}' has no VMAD binding");
        return None;
    };
    let entity = match value {
        PropertyValue::Object { form_id, alias: -1 } => {
            crate::condition::resolve_entity_by_global_form_id(world, *form_id)
        }
        PropertyValue::Object { alias, .. } if *alias >= 0 => {
            bindings.resolve(context, i32::from(*alias))
        }
        _ => None,
    };
    if entity.is_none() {
        log::debug!("fragment effect skipped: object ref '{name}' has no live entity");
    }
    entity
}

fn resolve_actor(
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    context: QuestFormId,
    via: &ActorRef,
    bindings: &crate::scene::SceneActorBindings,
) -> Option<byroredux_core::ecs::storage::EntityId> {
    match via {
        ActorRef::Player => world
            .try_resource::<crate::papyrus_demo::PlayerEntity>()
            .map(|player| player.0),
        ActorRef::Object(via) => resolve_object(vmad, world, context, via, bindings),
    }
}

fn actors_3d_loaded(
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    context: QuestFormId,
    actors: &[ActorRef],
    bindings: &crate::scene::SceneActorBindings,
) -> bool {
    actors.iter().all(|actor| {
        resolve_actor(vmad, world, context, actor, bindings)
            .is_some_and(|entity| world.has::<Transform>(entity))
    })
}

fn update_actor_cinematic_state(
    world: &World,
    actor: byroredux_core::ecs::storage::EntityId,
    update: impl FnOnce(&mut crate::ActorCinematicState),
) -> bool {
    let Some(mut states) = world.query_mut::<crate::ActorCinematicState>() else {
        return false;
    };
    if states.get_mut(actor).is_none() {
        states.insert(actor, crate::ActorCinematicState::default());
    }
    let Some(state) = states.get_mut(actor) else {
        return false;
    };
    update(state);
    true
}

fn exit_cart_idle_property(seat: u8) -> Option<&'static str> {
    match seat {
        1 => Some("IdleCartPassengerAExit"),
        2 => Some("IdleCartPassengerBExit"),
        3 => Some("IdleCartPassengerDExit"),
        4 => Some("IdleCartPassengerCExit"),
        5 => Some("IdleCartDriverExit"),
        _ => None,
    }
}

#[derive(Debug)]
enum DeferredCinematicPresentationEffect {
    SetSittingRotation(f32),
    RegisterPlayerAnimationEvent {
        event: crate::CinematicAnimationEvent,
        quest: QuestFormId,
        image_space_modifiers: Vec<crate::ImageSpaceModifierApplication>,
    },
}

/// Resource snapshots and side effects that bracket a fragment's canonical
/// quest-stage/objective guard scope.
///
/// Construct the batch before taking those guards, then apply it after they
/// drop. This prevents nested acquisitions of `QuestDefinitionRegistry` and
/// the presentation/alias-binding resources (#2269, #2539), and — as of
/// #2660 (SCR-D6-NEW11-03) — of `SceneActorBindings` itself: every
/// `resolve_object` alias lookup previously called `world.try_resource::
/// <SceneActorBindings>()` directly, nesting a read acquisition inside the
/// `(QuestStageState, QuestObjectiveState)` scope even after #2539 stopped
/// nesting the *write* side (`mark_scene_actor_bindings_dirty`). Reads now
/// go through the snapshot below instead.
///
/// This does NOT eliminate every nested acquisition inside that scope —
/// `PlayerControlState` (3 writes) and 12 component-storage acquisitions
/// (`Inventory` for `AddItem`, `GlobalTransform`+`Transform` for `MoveTo`,
/// and others across `apply_effect`'s match arms) are direct ECS mutations
/// the effects perform, not alias lookups, and stay nested. See
/// `apply_effect`'s doc for the current complete list and the exclusive-
/// scheduling invariant that makes it safe today.
#[derive(Debug)]
pub struct DeferredFragmentEffects {
    quest_definitions: Option<crate::QuestDefinitionRegistry>,
    /// #2660 (SCR-D6-NEW11-03) — snapshot of `SceneActorBindings`, cloned
    /// before the quest-state guards so `resolve_object` can resolve
    /// alias-bound `ObjectRef`s without a nested resource acquisition.
    /// `SceneActorBindings::resolve` only reads; nothing inside the guard
    /// scope can invalidate this snapshot mid-cascade (the table is only
    /// ever rebuilt by a separate, later-scheduled system after this one's
    /// `scene_actor_bindings_dirty` flag below is applied), so a snapshot
    /// taken once at the top of the exclusive system is behaviorally
    /// identical to a live read throughout.
    scene_actor_bindings: crate::scene::SceneActorBindings,
    cinematic_presentation: Vec<DeferredCinematicPresentationEffect>,
    scene_actor_bindings_dirty: bool,
    /// `(target, activator)` pairs from `Effect::Activate`, handed to
    /// [`PendingFragmentActivations`] so they are delivered at the head of
    /// the *next* frame (#2654) — see that resource's docs.
    activations: Vec<(EntityId, EntityId)>,
    reference_enable_changes: Vec<(u32, bool)>,
}

impl DeferredFragmentEffects {
    /// Snapshot fragment resources before acquiring the quest-state guards.
    ///
    /// Called unconditionally by every production caller of this batch,
    /// including on frames with no quest-stage activity at all — see
    /// [`quest_fragment_dispatch_system`]'s early-bail, which runs AFTER
    /// this snapshot for the #2539 lock-ordering reason documented on the
    /// struct above. `QuestDefinitionRegistry::clone()` is an O(1) `Arc`
    /// refcount bump (#2659 / SCR-D6-NEW11-02), not a deep copy — that's
    /// what keeps this unconditional call cheap on every load-order size.
    /// `SceneActorBindings::clone()` (#2660) is a real `HashMap` clone, but
    /// the table only holds bound aliases for currently-active quests, not
    /// the whole load order — negligible next to the per-frame ECS work
    /// this system already does.
    pub fn new(world: &World) -> Self {
        Self {
            quest_definitions: world
                .try_resource::<crate::QuestDefinitionRegistry>()
                .map(|definitions| definitions.clone()),
            scene_actor_bindings: world
                .try_resource::<crate::scene::SceneActorBindings>()
                .map(|bindings| bindings.clone())
                .unwrap_or_default(),
            cinematic_presentation: Vec::new(),
            scene_actor_bindings_dirty: false,
            activations: Vec::new(),
            reference_enable_changes: Vec::new(),
        }
    }

    /// Apply queued mutations after releasing the `QuestStageState` /
    /// `QuestObjectiveState` guards passed to [`apply_effects`].
    pub fn apply(self, world: &World) {
        if self.scene_actor_bindings_dirty {
            crate::scene::mark_scene_actor_bindings_dirty(world);
        }
        if !self.activations.is_empty() {
            match world.try_resource_mut::<PendingFragmentActivations>() {
                Some(mut pending) => pending.0.extend(self.activations),
                None => log::debug!(
                    "fragment Activate dropped: PendingFragmentActivations is unavailable"
                ),
            }
        }
        if !self.reference_enable_changes.is_empty() {
            if let Some(mut state) = world.try_resource_mut::<ReferenceEnableState>() {
                for (form_id, enabled) in self.reference_enable_changes {
                    state.set_enabled(form_id, enabled);
                }
            }
        }
        if self.cinematic_presentation.is_empty() {
            return;
        }
        let Some(mut state) = world.try_resource_mut::<crate::CinematicPresentationState>() else {
            return;
        };
        for effect in self.cinematic_presentation {
            match effect {
                DeferredCinematicPresentationEffect::SetSittingRotation(degrees) => {
                    state.sitting_rotation_degrees = degrees;
                }
                DeferredCinematicPresentationEffect::RegisterPlayerAnimationEvent {
                    event,
                    quest,
                    image_space_modifiers,
                } => {
                    state.register_player_animation_event(event, quest, image_space_modifiers);
                }
            }
        }
    }
}

/// Read one entity's [`Transform`] as an owned copy, holding no guard on
/// return (#3250).
fn copied_transform(world: &World, entity: EntityId) -> Option<Transform> {
    // #3250 — `World::get` returns an owning read guard. Copy the component
    // before another Transform lookup so a queued writer can never strand a
    // recursive read behind the first guard.
    world.get::<Transform>(entity).map(|transform| *transform)
}

/// Apply one effect to the canonical stage/objective state (or, for the
/// object-targeting variants, to the live ECS world). Returns a
/// [`QuestStageAdvanced`] when the effect was a `SetStage` (so the caller
/// can cascade), or `None` otherwise / when a target can't resolve.
///
/// **Nested-lock safety depends on exclusive scheduling.** The full
/// residual list, current as of #2660 (SCR-D6-NEW11-03):
///   - `PlayerControlState` (3 writes — `SetPlayerControls`-family arms)
///   - `Globals` (1 write — `SetGlobalValue`)
///   - 12 component-storage acquisitions across the match arms below,
///     including `Inventory` (`AddItem`) and `GlobalTransform` +
///     `Transform` (`MoveTo`)
///
/// all taken while the caller ([`quest_fragment_dispatch_system`]) still
/// holds the `QuestStageFragments`/`QuestStageState`/`QuestObjectiveState`
/// resource locks for the whole cascade loop. This is only safe because
/// every system that touches those quest resources is registered
/// `add_exclusive` in `byroredux/src/boot.rs` (parallel systems never run
/// concurrently with an exclusive one), so no other holder can ever form
/// the other half of an ABBA cycle. Adding a new nested component/resource
/// lock here, or moving `quest_fragment_dispatch_system` (or any sibling
/// quest-resource system) onto the parallel lane, needs the same analysis
/// re-derived — see SCR-D6-NEW3-03 / #2126, and record the change under
/// the house-rule doc #2270 asks for.
///
/// `SceneActorBindings` used to be part of this residual list too (every
/// `resolve_object` alias lookup read it live) but is now resolved through
/// the pre-lock snapshot in `deferred` instead (#2660) — see
/// [`DeferredFragmentEffects`]'s doc for why a snapshot is safe here.
/// Quest-definition reads use the same snapshot, and presentation /
/// alias-binding *mutations* are appended to it rather than acquired here.
/// Every production caller constructs the batch before taking quest
/// resource guards and applies it only after those guards have dropped
/// (#2269, #2539, #2660).
fn apply_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &mut QuestStageState,
    objectives: &mut QuestObjectiveState,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::SetGlobalValue { global, value } => {
            let form_id = resolve_property_form_id(vmad, global.property_name())?;
            if let Some(mut globals) = world.try_resource_mut::<crate::Globals>() {
                globals.set(form_id, *value);
            } else {
                log::debug!(
                    "fragment SetValue skipped: Globals resource is unavailable for {form_id:08X}"
                );
            }
            None
        }
        Effect::AddItem {
            container,
            item,
            count,
        } => {
            let container_entity = resolve_object(
                vmad,
                world,
                context,
                container,
                &deferred.scene_actor_bindings,
            )?;
            let item_form_id = resolve_property_form_id(vmad, item.property_name())?;
            let Some(mut inventories) = world.query_mut::<Inventory>() else {
                log::debug!("fragment AddItem skipped: Inventory component never registered");
                return None;
            };
            if inventories.get_mut(container_entity).is_none() {
                // The container doesn't carry an Inventory yet — every
                // object can receive items in Bethesda's runtime, so
                // create one on demand (an interior-mutable insert onto
                // an already-registered storage, not a structural
                // `&mut World` mutation).
                inventories.insert(container_entity, Inventory::new());
            }
            inventories
                .get_mut(container_entity)
                .expect("just present or inserted above")
                .push(ItemStack::new(item_form_id, *count));
            None
        }
        Effect::EquipItem {
            actor,
            item,
            silent: _,
        } => {
            let actor = resolve_actor(vmad, world, context, actor, &deferred.scene_actor_bindings)?;
            let item_form_id = resolve_property_form_id(vmad, item.property_name())?;
            let Some(slot_mask) = world
                .try_resource::<crate::EquipItemCatalog>()
                .and_then(|catalog| catalog.slot_mask(item_form_id))
            else {
                log::debug!(
                    "fragment EquipItem skipped: item {item_form_id:08X} has no armor biped-slot metadata"
                );
                return None;
            };
            let Some((mut inventories, mut equipment)) =
                world.query_2_mut_mut::<Inventory, EquipmentSlots>()
            else {
                log::debug!(
                    "fragment EquipItem skipped: inventory/equipment storage is unavailable"
                );
                return None;
            };
            let Some(inventory) = inventories.get_mut(actor) else {
                log::debug!("fragment EquipItem skipped: actor has no Inventory");
                return None;
            };
            let Some(index) = inventory
                .items
                .iter()
                .position(|stack| stack.base_form_id == item_form_id && stack.count > 0)
                .and_then(|index| u32::try_from(index).ok())
                .map(InventoryIndex)
            else {
                log::debug!(
                    "fragment EquipItem skipped: actor does not carry item {item_form_id:08X}"
                );
                return None;
            };
            if equipment.get_mut(actor).is_none() {
                equipment.insert(actor, EquipmentSlots::new());
            }
            equipment
                .get_mut(actor)
                .expect("just present or inserted above")
                .equip(slot_mask, index);
            None
        }
        Effect::MoveTo { moved, destination } => {
            let moved_entity =
                resolve_object(vmad, world, context, moved, &deferred.scene_actor_bindings)?;
            let destination_entity = resolve_object(
                vmad,
                world,
                context,
                destination,
                &deferred.scene_actor_bindings,
            )?;
            let Some(translation) = world
                .get::<GlobalTransform>(destination_entity)
                .map(|gt| gt.translation)
            else {
                log::debug!("fragment MoveTo skipped: destination entity has no GlobalTransform");
                return None;
            };
            let Some(mut transforms) = world.query_mut::<Transform>() else {
                log::debug!("fragment MoveTo skipped: Transform component never registered");
                return None;
            };
            // Unlike AddItem, a "moved" entity with no Transform isn't a
            // container the effect can adopt — it isn't a placed spatial
            // entity at all, so this declines rather than fabricating one.
            let Some(transform) = transforms.get_mut(moved_entity) else {
                log::debug!("fragment MoveTo skipped: moved entity has no Transform");
                return None;
            };
            transform.translation = translation;
            None
        }
        Effect::Disable {
            object,
            fade_out: _,
        } => {
            // #3278 (SCR-D5-2026-08-24-01) — `prim_disable` classifies its
            // receiver through the same `receiver_object` as
            // `AddItem`/`MoveTo`/`EquipItem`, so it can bind to a
            // quest-alias-filled `ObjectReference Property`. Dispatch used
            // only the strict `resolve_property_form_id`, which sees direct
            // VMAD FormID properties and nothing else — so
            // `<AliasBoundMarker>.Disable()` silently declined in exactly the
            // cases where the same alias-bound receiver resolves fine for
            // every sibling effect.
            //
            // Direct-FormID first, alias-aware second: that keeps the cheap,
            // world-free path unchanged for the case that already worked and
            // makes the alias arm strictly additive. `ReferenceEnableState`
            // is FormID-keyed (so the state outlives the reference's cell),
            // hence the trip back through `entity_global_form_id`.
            let form_id = resolve_property_form_id(vmad, object.property_name()).or_else(|| {
                let entity =
                    resolve_object(vmad, world, context, object, &deferred.scene_actor_bindings)?;
                entity_global_form_id(world, entity)
            })?;
            deferred.reference_enable_changes.push((form_id, false));
            None
        }
        Effect::StartScene { scene } | Effect::StopScene { scene } => {
            let scene_form_id = resolve_property_form_id(vmad, scene.property_name())?;
            let scene_entity = world
                .try_resource::<crate::scene::SceneRegistry>()
                .and_then(|registry| registry.scene_entity(scene_form_id));
            if let Some(scene_entity) = scene_entity {
                if matches!(effect, Effect::StartScene { .. }) {
                    if let Some(mut requests) = world.query_mut::<crate::scene::SceneStartRequest>()
                    {
                        requests.insert(scene_entity, crate::scene::SceneStartRequest);
                    }
                } else if let Some(mut requests) =
                    world.query_mut::<crate::scene::SceneStopRequest>()
                {
                    requests.insert(scene_entity, crate::scene::SceneStopRequest);
                }
                return None;
            }

            // `Start`/`Stop` are shared by Scene and Quest in Papyrus, while
            // the decompiled AST does not retain a property's declared type.
            // Resolve the FormID against installed definitions at dispatch.
            let quest = QuestFormId(scene_form_id);
            let (start_up_stage, shut_down_stage) = {
                let definitions = deferred.quest_definitions.as_ref()?;
                if !definitions.contains(quest) {
                    log::debug!(
                        "fragment Start/Stop skipped: '{}' resolved to unknown form {scene_form_id:08X}",
                        scene.property_name()
                    );
                    return None;
                }
                (
                    definitions.start_up_stage(quest),
                    definitions.shut_down_stage(quest),
                )
            };
            if matches!(effect, Effect::StartScene { .. }) {
                let event = stages.start_quest(quest, start_up_stage);
                if event.is_some() {
                    deferred.scene_actor_bindings_dirty = true;
                }
                event
            } else {
                let event = stages
                    .is_running(quest)
                    .then(|| {
                        shut_down_stage.map(|stage| {
                            let previous_stage = stages.set_stage(quest, stage);
                            QuestStageAdvanced {
                                quest,
                                previous_stage,
                                new_stage: stage,
                            }
                        })
                    })
                    .flatten();
                if stages.stop(quest) {
                    deferred.scene_actor_bindings_dirty = true;
                }
                event
            }
        }
        Effect::Activate { target, activator } => {
            let target_entity =
                resolve_object(vmad, world, context, target, &deferred.scene_actor_bindings)?;
            let activator = match activator {
                Some(activator) => resolve_object(
                    vmad,
                    world,
                    context,
                    activator,
                    &deferred.scene_actor_bindings,
                )?,
                None => world
                    .try_resource::<crate::papyrus_demo::PlayerEntity>()
                    .map(|player| player.0)
                    .unwrap_or(target_entity),
            };
            // Queue rather than insert: `quest_fragment_dispatch_system` is
            // the LAST producer of `ActivateEvent` in Stage::Update, but
            // three of the four consumers (rumble, quest_advance,
            // two_state_activator) are scheduled earlier and
            // `event_cleanup_system` drains the marker at Stage::Late the
            // same frame — so a directly-inserted marker reached none of
            // them, ever (#2654). `fragment_activation_flush_system` turns
            // this into a real `ActivateEvent` at the head of the next
            // frame, ahead of every consumer.
            deferred.activations.push((target_entity, activator));
            None
        }
        Effect::SetOpen { target, open } => {
            let target_entity =
                resolve_object(vmad, world, context, target, &deferred.scene_actor_bindings)?;
            if !crate::vm_state::set_two_state_open(world, target_entity, *open) {
                log::debug!(
                    "fragment SetOpen skipped: '{}' is not a recognized two-state activator",
                    target.property_name()
                );
            }
            None
        }
        Effect::SetPlayerRestrained { restrained } => {
            let Some(player) = world
                .try_resource::<crate::papyrus_demo::PlayerEntity>()
                .map(|player| player.0)
            else {
                log::debug!("fragment SetRestrained skipped: player entity is unavailable");
                return None;
            };
            let Some(mut states) = world.query_mut::<crate::ActorControlState>() else {
                log::debug!("fragment SetRestrained skipped: actor-control storage is unavailable");
                return None;
            };
            if let Some(state) = states.get_mut(player) {
                state.restrained = *restrained;
            } else {
                states.insert(
                    player,
                    crate::ActorControlState {
                        restrained: *restrained,
                    },
                );
            }
            None
        }
        Effect::SetPlayerControls { enabled, selection } => {
            if let Some(mut controls) = world.try_resource_mut::<crate::PlayerControlState>() {
                controls.apply_selection(*enabled, *selection);
            }
            None
        }
        Effect::SetPlayerAiDriven { ai_driven } => {
            if let Some(mut controls) = world.try_resource_mut::<crate::PlayerControlState>() {
                controls.ai_driven = *ai_driven;
            }
            None
        }
        Effect::SetHudCartMode { cart_mode } => {
            if let Some(mut controls) = world.try_resource_mut::<crate::PlayerControlState>() {
                controls.hud_cart_mode = *cart_mode;
            }
            None
        }
        Effect::PlayIdle { actor, idle } => {
            let actor = resolve_actor(vmad, world, context, actor, &deferred.scene_actor_bindings)?;
            let idle_form_id = resolve_property_form_id(vmad, idle.property_name())?;
            if !update_actor_cinematic_state(world, actor, |state| {
                state.request_idle(idle_form_id);
            }) {
                log::debug!("fragment PlayIdle skipped: cinematic actor storage is unavailable");
            }
            None
        }
        Effect::SetVehicle { actor, vehicle } => {
            let actor = resolve_actor(vmad, world, context, actor, &deferred.scene_actor_bindings)?;
            let vehicle = match vehicle {
                Some(vehicle) => Some(resolve_object(
                    vmad,
                    world,
                    context,
                    vehicle,
                    &deferred.scene_actor_bindings,
                )?),
                None => None,
            };
            let relative_pose = vehicle.and_then(|vehicle| {
                let actor_transform = copied_transform(world, actor)?;
                let vehicle_transform = copied_transform(world, vehicle)?;
                if vehicle_transform.scale.abs() <= f32::EPSILON {
                    return None;
                }
                let inverse_rotation = vehicle_transform.rotation.conjugate();
                Some((
                    inverse_rotation
                        * (actor_transform.translation - vehicle_transform.translation)
                        / vehicle_transform.scale,
                    inverse_rotation * actor_transform.rotation,
                ))
            });
            if !update_actor_cinematic_state(world, actor, |state| {
                state.vehicle = vehicle;
                if vehicle.is_some() {
                    state.cart_seat = None;
                }
                state.vehicle_local_translation = relative_pose.map(|pose| pose.0);
                state.vehicle_local_rotation = relative_pose.map(|pose| pose.1);
            }) {
                log::debug!("fragment SetVehicle skipped: cinematic actor storage is unavailable");
            }
            None
        }
        Effect::TetherToHorse { cart, horse } => {
            let cart = resolve_object(vmad, world, context, cart, &deferred.scene_actor_bindings)?;
            let horse =
                resolve_object(vmad, world, context, horse, &deferred.scene_actor_bindings)?;
            let Some(cart_transform) = copied_transform(world, cart) else {
                log::debug!("fragment TetherToHorse skipped: cart has no Transform");
                return None;
            };
            let Some(horse_transform) = copied_transform(world, horse) else {
                log::debug!("fragment TetherToHorse skipped: horse has no Transform");
                return None;
            };
            if horse_transform.scale.abs() <= f32::EPSILON {
                log::debug!("fragment TetherToHorse skipped: horse has zero scale");
                return None;
            }
            let inverse_rotation = horse_transform.rotation.conjugate();
            let state = crate::HorseTetherState {
                horse,
                horse_local_translation: inverse_rotation
                    * (cart_transform.translation - horse_transform.translation)
                    / horse_transform.scale,
                horse_local_rotation: inverse_rotation * cart_transform.rotation,
                route_target_form_id: None,
            };
            if let Some(mut tethers) = world.query_mut::<crate::HorseTetherState>() {
                tethers.insert(cart, state);
                log::info!("TetherToHorse attached cart entity {cart} to horse entity {horse}");
            }
            // The native engine realizes this relation through Havok. Redux's
            // deterministic transform follower needs the cart to stop being
            // pulled back from a dynamic Rapier body after PostUpdate, so make
            // the tethered cart keyframed through the same one-shot bridge as
            // authored SetMotionType calls.
            if let Some(mut requests) = world.query_mut::<crate::MotionTypeChangeRequest>() {
                requests.insert(
                    cart,
                    crate::MotionTypeChangeRequest {
                        motion_type: byroredux_core::ecs::components::MotionType::Keyframed,
                        allow_activate: true,
                    },
                );
            }
            None
        }
        Effect::SetMotionType {
            target,
            motion_type,
            allow_activate,
        } => {
            let target =
                resolve_object(vmad, world, context, target, &deferred.scene_actor_bindings)?;
            if let Some(mut requests) = world.query_mut::<crate::MotionTypeChangeRequest>() {
                requests.insert(
                    target,
                    crate::MotionTypeChangeRequest {
                        motion_type: *motion_type,
                        allow_activate: *allow_activate,
                    },
                );
            }
            None
        }
        Effect::SetSittingRotation { degrees } => {
            deferred.cinematic_presentation.push(
                DeferredCinematicPresentationEffect::SetSittingRotation(*degrees),
            );
            None
        }
        Effect::ExitCart { actor, seat } => {
            let actor =
                resolve_object(vmad, world, context, actor, &deferred.scene_actor_bindings)?;
            let idle_form_id = exit_cart_idle_property(*seat)
                .and_then(|property| resolve_property_form_id(vmad, property));
            let attachment = world
                .get::<crate::ActorCinematicState>(actor)
                .and_then(|state| Some((state.vehicle?, state.vehicle_local_rotation?)));
            let exit_root_motion_rotation = world.query::<Transform>().and_then(|transforms| {
                attachment
                    .and_then(|(vehicle, local_rotation)| {
                        transforms
                            .get(vehicle)
                            .map(|vehicle| vehicle.rotation * local_rotation)
                    })
                    .or_else(|| transforms.get(actor).map(|actor| actor.rotation))
            });
            if !update_actor_cinematic_state(world, actor, |state| {
                state.vehicle = None;
                state.vehicle_local_translation = None;
                state.vehicle_local_rotation = None;
                state.cart_seat = Some(*seat);
                state.awaited_event = Some(crate::CinematicAnimationEvent::ExitCartEnd);
                state.exit_root_motion_rotation = exit_root_motion_rotation;
                if let Some(idle_form_id) = idle_form_id {
                    state.request_idle(idle_form_id);
                }
            }) {
                log::debug!("fragment ExitCart skipped: cinematic actor storage is unavailable");
            }
            None
        }
        Effect::RegisterPlayerAnimationEvent { event } => {
            let image_space_modifiers = match event {
                crate::CinematicAnimationEvent::PlayImod => {
                    ["PlayerAlduinIMOD", "CGDragonAttackBlurLong"]
                        .into_iter()
                        .filter_map(|property| resolve_property_form_id(vmad, property))
                        .map(|form_id| crate::ImageSpaceModifierApplication {
                            form_id,
                            strength: 1.0,
                        })
                        .collect()
                }
                crate::CinematicAnimationEvent::IdleFurnitureExit
                | crate::CinematicAnimationEvent::ExitCartEnd => Vec::new(),
            };
            deferred.cinematic_presentation.push(
                DeferredCinematicPresentationEffect::RegisterPlayerAnimationEvent {
                    event: *event,
                    quest: context,
                    image_space_modifiers,
                },
            );
            None
        }
        Effect::EvaluatePackage { actor } => {
            let actor =
                resolve_object(vmad, world, context, actor, &deferred.scene_actor_bindings)?;
            if let Some(mut requests) = world.query_mut::<crate::EvaluatePackageRequest>() {
                requests.insert(actor, crate::EvaluatePackageRequest);
            }
            None
        }
        Effect::Wait { .. } | Effect::WaitForActors3DLoaded { .. } => None,
        Effect::Conditional { .. } => {
            unreachable!("conditional effects are expanded by apply_effects")
        }
        _ => apply_quest_scoped_effect(effect, context, vmad, stages, objectives, deferred),
    }
}

/// The original quest-scoped effect arms (`SetStage`/objectives), split
/// out so [`apply_effect`]'s object-targeting arms — which need `world`
/// but no `QuestRef` resolution — read cleanly above.
fn apply_quest_scoped_effect(
    effect: &Effect,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    stages: &mut QuestStageState,
    objectives: &mut QuestObjectiveState,
    deferred: &mut DeferredFragmentEffects,
) -> Option<QuestStageAdvanced> {
    match effect {
        Effect::SetStage { quest, stage } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let allow_repeated = deferred
                .quest_definitions
                .as_ref()
                .is_some_and(|definitions| definitions.allows_repeated_stages(quest));
            if !allow_repeated && stages.get_stage_done(quest, *stage) {
                return None;
            }
            let was_started = stages.is_started(quest);
            let previous_stage = stages.set_stage(quest, *stage);
            if !was_started {
                deferred.scene_actor_bindings_dirty = true;
            }
            Some(QuestStageAdvanced {
                quest,
                previous_stage,
                new_stage: *stage,
            })
        }
        Effect::StartQuest { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let start_up_stage = deferred
                .quest_definitions
                .as_ref()
                .and_then(|definitions| definitions.start_up_stage(quest));
            let event = stages.start_quest(quest, start_up_stage);
            if event.is_some() {
                deferred.scene_actor_bindings_dirty = true;
            }
            event
        }
        Effect::StopQuest { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let shut_down_stage = deferred
                .quest_definitions
                .as_ref()
                .and_then(|definitions| definitions.shut_down_stage(quest));
            let event = stages
                .is_running(quest)
                .then(|| {
                    shut_down_stage.map(|stage| {
                        let previous_stage = stages.set_stage(quest, stage);
                        QuestStageAdvanced {
                            quest,
                            previous_stage,
                            new_stage: stage,
                        }
                    })
                })
                .flatten();
            if stages.stop(quest) {
                deferred.scene_actor_bindings_dirty = true;
            }
            event
        }
        Effect::CompleteQuest { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            if stages.complete(quest) {
                deferred.scene_actor_bindings_dirty = true;
            }
            None
        }
        Effect::ResetQuest { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            stages.reset(quest);
            objectives.reset(quest);
            deferred.scene_actor_bindings_dirty = true;
            None
        }
        Effect::SetQuestActive { quest, active } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            stages.set_active(quest, *active);
            None
        }
        Effect::SetObjectiveDisplayed {
            quest,
            objective,
            displayed,
        } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            objectives.set_displayed(quest, *objective, *displayed);
            None
        }
        Effect::SetObjectiveCompleted {
            quest,
            objective,
            completed,
        } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            objectives.set_completed(quest, *objective, *completed);
            None
        }
        Effect::SetObjectiveFailed {
            quest,
            objective,
            failed,
        } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            objectives.set_failed(quest, *objective, *failed);
            None
        }
        Effect::CompleteAllObjectives { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let authored = deferred
                .quest_definitions
                .as_ref()
                .map(|definitions| definitions.objectives(quest).to_vec())
                .unwrap_or_default();
            if authored.is_empty() {
                objectives.complete_all(quest);
            } else {
                objectives.complete_all_authored(quest, authored);
            }
            None
        }
        Effect::FailAllObjectives { quest } => {
            let quest = resolve_quest_logged(quest, context, vmad)?;
            let authored = deferred
                .quest_definitions
                .as_ref()
                .map(|definitions| definitions.objectives(quest).to_vec())
                .unwrap_or_default();
            if authored.is_empty() {
                objectives.fail_all(quest);
            } else {
                objectives.fail_all_authored(quest, authored);
            }
            None
        }
        Effect::Conditional { .. }
        | Effect::SetGlobalValue { .. }
        | Effect::AddItem { .. }
        | Effect::EquipItem { .. }
        | Effect::MoveTo { .. }
        | Effect::Disable { .. }
        | Effect::StartScene { .. }
        | Effect::StopScene { .. }
        | Effect::Activate { .. }
        | Effect::SetOpen { .. }
        | Effect::SetPlayerRestrained { .. }
        | Effect::SetPlayerControls { .. }
        | Effect::SetPlayerAiDriven { .. }
        | Effect::SetHudCartMode { .. }
        | Effect::PlayIdle { .. }
        | Effect::SetVehicle { .. }
        | Effect::TetherToHorse { .. }
        | Effect::SetMotionType { .. }
        | Effect::SetSittingRotation { .. }
        | Effect::ExitCart { .. }
        | Effect::RegisterPlayerAnimationEvent { .. }
        | Effect::EvaluatePackage { .. }
        | Effect::Wait { .. }
        | Effect::WaitForActors3DLoaded { .. } => {
            unreachable!("object-targeting effects are handled by apply_effect directly")
        }
    }
}

/// Apply a whole fragment's effects, returning the chained
/// [`QuestStageAdvanced`]s its `SetStage`s produced. Resource reads use the
/// pre-lock snapshot in `deferred`; queued mutations must be applied only after
/// releasing the quest-state guards passed here.
pub fn apply_effects(
    effects: &[Effect],
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &mut QuestStageState,
    objectives: &mut QuestObjectiveState,
    deferred: &mut DeferredFragmentEffects,
) -> Vec<QuestStageAdvanced> {
    let mut advances = Vec::new();
    for (index, effect) in effects.iter().enumerate() {
        if let Effect::Conditional {
            guards,
            then_effects,
            else_effects,
        } = effect
        {
            let passes = guards.iter().all(|guard| {
                resolve_quest_logged(&guard.quest, context, vmad)
                    .is_some_and(|quest| stages.get_stage_done(quest, guard.stage) == guard.done)
            });
            let branch = if passes { then_effects } else { else_effects };
            advances.extend(apply_effects(
                branch, context, vmad, world, stages, objectives, deferred,
            ));
            continue;
        }
        let suspension = match effect {
            Effect::Wait { seconds } => Some((*seconds, FragmentResumeCondition::DelayElapsed)),
            Effect::WaitForActors3DLoaded {
                actors,
                poll_seconds,
            } if !actors_3d_loaded(
                vmad,
                world,
                context,
                actors,
                &deferred.scene_actor_bindings,
            ) =>
            {
                Some((
                    *poll_seconds,
                    FragmentResumeCondition::Actors3DLoaded {
                        actors: actors.clone(),
                        poll_seconds: *poll_seconds,
                        elapsed_seconds: 0.0,
                    },
                ))
            }
            Effect::WaitForActors3DLoaded { .. } => None,
            _ => None,
        };
        if let Some((remaining_seconds, resume_when)) = suspension {
            let tail = &effects[index + 1..];
            if !tail.is_empty() {
                if let Some(mut queue) = world.try_resource_mut::<FragmentExecutionQueue>() {
                    queue.pending.push(PendingFragmentExecution {
                        context,
                        vmad: vmad.cloned(),
                        effects: tail.to_vec(),
                        remaining_seconds,
                        resume_when,
                    });
                } else {
                    log::debug!(
                        "fragment continuation dropped: FragmentExecutionQueue is unavailable"
                    );
                }
            }
            break;
        }
        if matches!(effect, Effect::WaitForActors3DLoaded { .. }) {
            continue;
        }
        if let Some(advance) =
            apply_effect(effect, context, vmad, world, stages, objectives, deferred)
        {
            advances.push(advance);
        }
    }
    advances
}

/// Tick latent fragment continuations and execute every tail whose
/// `Utility.Wait` has elapsed. A resumed tail can encounter another wait and
/// requeue itself through [`apply_effects`].
pub fn fragment_continuation_system(world: &World, dt: f32) {
    // #2660 (SCR-D6-NEW11-03) — constructed up front (before the resume-
    // condition loop below, which needs the `SceneActorBindings` snapshot
    // for its own `actors_3d_loaded` check) rather than only once `ready`
    // is known non-empty. Matches `DeferredFragmentEffects::new`'s
    // documented "called unconditionally by every production caller" rule.
    let mut deferred = DeferredFragmentEffects::new(world);
    let mut ready = Vec::new();
    let pending = {
        let Some(mut queue) = world.try_resource_mut::<FragmentExecutionQueue>() else {
            return;
        };
        std::mem::take(&mut queue.pending)
    };
    let mut still_pending = Vec::with_capacity(pending.len());
    for mut pending in pending {
        pending.remaining_seconds -= dt.max(0.0);
        if pending.remaining_seconds > 0.0 {
            still_pending.push(pending);
            continue;
        }
        match &mut pending.resume_when {
            FragmentResumeCondition::DelayElapsed => ready.push(pending),
            FragmentResumeCondition::Actors3DLoaded {
                actors,
                poll_seconds,
                elapsed_seconds,
            } => {
                if actors_3d_loaded(
                    pending.vmad.as_ref(),
                    world,
                    pending.context,
                    actors,
                    &deferred.scene_actor_bindings,
                ) {
                    ready.push(pending);
                } else {
                    *elapsed_seconds += *poll_seconds;
                    // #2288 (SCR-D6-NEW5-02) — give up once the total wait
                    // exceeds the cap instead of re-arming forever. Matches
                    // the crate's "skip, never guess" contract: the tail is
                    // declined outright (dropped, not pushed to either
                    // queue), the same choice `MAX_CASCADE` makes for a
                    // runaway `SetStage` cascade in the same file.
                    if *elapsed_seconds > MAX_ACTORS_3D_LOADED_WAIT_SECONDS {
                        log::warn!(
                            "fragment continuation dropped: WaitForActors3DLoaded exceeded \
                             {MAX_ACTORS_3D_LOADED_WAIT_SECONDS}s with actors still unresolved \
                             (context quest {:?})",
                            pending.context
                        );
                        continue;
                    }
                    pending.remaining_seconds = *poll_seconds;
                    still_pending.push(pending);
                }
            }
        }
    }
    if !still_pending.is_empty() {
        if let Some(mut queue) = world.try_resource_mut::<FragmentExecutionQueue>() {
            queue.pending.extend(still_pending);
        }
    }
    if ready.is_empty() {
        return;
    }

    let mut emitted = Vec::new();
    for pending in ready {
        let advances = {
            let (mut stages, mut objectives) =
                world.resource_2_mut::<QuestStageState, QuestObjectiveState>();
            apply_effects(
                &pending.effects,
                pending.context,
                pending.vmad.as_ref(),
                world,
                &mut stages,
                &mut objectives,
                &mut deferred,
            )
        };
        emitted.extend(advances);
    }
    deferred.apply(world);

    if emitted.is_empty() {
        return;
    }
    let Some(player_entity) = world
        .try_resource::<crate::papyrus_demo::PlayerEntity>()
        .map(|player| player.0)
    else {
        return;
    };
    crate::quest_stages::push_quest_stage_advances(world, player_entity, emitted);
}

/// Maximum stage-fragment cascade depth in one dispatch pass — a fragment
/// `SetStage`-ing the next stage runs that stage's fragment too. The cap
/// is a backstop against a cyclic SetStage chain (a fragment that sets a
/// stage whose fragment sets it back); real quest chains are short.
const MAX_CASCADE: usize = 64;

/// Register the fragment-dispatch resources. Both are empty
/// default-constructible runtime stores (unlike `PlayerEntity` /
/// `QuestStageState`, which carry per-app-instance state and stay
/// caller-inserted), so initialising them at world-init is safe and
/// keeps [`quest_fragment_dispatch_system`] panic-free out of the box.
pub fn register(world: &mut World) {
    world.register::<Inventory>();
    world.register::<EquipmentSlots>();
    world.insert_resource(QuestStageFragments::default());
    world.insert_resource(SceneFragments::default());
    world.insert_resource(FragmentExecutionQueue::default());
    world.insert_resource(PendingFragmentActivations::default());
    world.insert_resource(ReferenceEnableState::default());
    world.insert_resource(QuestObjectiveState::default());
}

/// A top-level (or state) function body from a decompiled script, by name
/// (Papyrus identifiers are case-insensitive). Quest `Fragment_N`
/// functions are top-level, but state functions are checked too so the
/// lookup is robust.
/// Lowercased names of `script`'s top-level `Quest Property` declarations.
/// #2538 / SCR-D5-NEW10-01 — feeds `lower_fragment_with_quest_properties`
/// so the effect-primitive chain can distinguish a bare `Quest.Start()`/
/// `Stop()` receiver from an identically-shaped `Scene.Start()`/`Stop()`
/// one, which the AST alone cannot. Properties are always top-level
/// (never nested in a `State` block, unlike functions/events), so no
/// `StateItem` walk is needed here.
///
/// #2657 (SCR-D5-NEW11-01) — the receiver-side key-space mismatch (a
/// decompiled `.pex` reads a property through its `::X_var` backing
/// variable, not the authored name this set is keyed by) is fixed;
/// `receiver_object`/`explicit_quest_receiver` normalize through
/// `quest_property_key` before consulting this set (#2653, `53f7de9d`).
///
/// A SEPARATE, still-open gap from the same issue: the type test below
/// is an exact `Type::Object("quest")` match, so a property typed with a
/// Quest-*derived* script (`mq206script`, `dn019script`, `min03script`,
/// …) is never collected — real corpus content does this. Closing it
/// needs a script-class-hierarchy resolver (something that can answer
/// "does `mq206script` transitively extend `Quest`?"); nothing in this
/// crate builds one today, and this single-`Script` function has no
/// access to any other script's `extends` declaration to improvise one.
/// Not attempted here — flagged as a candidate follow-up issue rather
/// than guessed at with a naming-convention heuristic.
///
/// `pub` (#2658 / SCR-D5-NEW11-03) — so `examples/fragment_coverage.rs`
/// and `examples/mq101_conformance.rs` can build the same per-script set
/// this function's one production caller
/// ([`populate_quest_fragments_from_script`]) does, and measure
/// `lower_fragment_with_quest_properties` (what production actually
/// runs) instead of context-free `lower_fragment`.
pub fn quest_property_names(script: &Script) -> std::collections::HashSet<String> {
    script
        .body
        .iter()
        .filter_map(|item| match &item.node {
            ScriptItem::Property(p) => match &p.ty.node {
                Type::Object(ty_name) if ty_name.0.eq_ignore_ascii_case("quest") => {
                    Some(p.name.node.0.to_ascii_lowercase())
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn function_body<'a>(script: &'a Script, name: &str) -> Option<&'a [Spanned<Stmt>]> {
    for item in &script.body {
        match &item.node {
            ScriptItem::Function(f) if f.name.node.0.eq_ignore_ascii_case(name) => {
                return Some(&f.body);
            }
            ScriptItem::State(st) => {
                for si in &st.body {
                    if let StateItem::Function(f) = &si.node {
                        if f.name.node.0.eq_ignore_ascii_case(name) {
                            return Some(&f.body);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Populate [`QuestStageFragments`] for one quest from its compiled quest
/// script (`.pex`). Each `(stage, fragment_name)` binding comes from the
/// QUST `VMAD` fragment section
/// ([`byroredux_plugin::esm::records::script_instance::parse_quest_fragments`]);
/// all fragments of a quest share a single `QF_` script, so the caller
/// resolves and passes its bytes once. Returns the number of stage
/// fragments inserted (non-empty, fully-lowered ones).
///
/// This is the runtime half of the M47.2 keystone: it takes the
/// stage→`Fragment_N` binding the decoder recovered and turns each
/// fragment body into the canonical [`Effect`]s the dispatcher applies —
/// closing the loop from real game data to quest behavior on screen.
///
/// Mirrors [`crate::translate::translate_pex`]'s hostile-input contract:
/// a `.pex` that fails to parse/decompile — including a decompiler panic
/// (#1816) — inserts nothing (logged at debug), never aborts the load. A
/// fragment carrying a statement no effect primitive claims lowers to
/// `None` and is declined (safe — no behavior attached), never partially
/// applied.
pub fn populate_quest_fragments_from_pex(
    frags: &mut QuestStageFragments,
    quest: QuestFormId,
    pex_bytes: &[u8],
    bindings: &[(u16, &str)],
) -> usize {
    let pex = match byroredux_pex::parse(pex_bytes) {
        Ok(p) => p,
        Err(e) => {
            log::debug!(
                "populate_quest_fragments: .pex parse failed (quest {:08X}): {e}",
                quest.0
            );
            return 0;
        }
    };
    let script = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        byroredux_pex::decompile::decompile_script(&pex)
    })) {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            log::debug!(
                "populate_quest_fragments: decompile failed (quest {:08X}): {e}",
                quest.0
            );
            return 0;
        }
        Err(_) => {
            log::debug!(
                "populate_quest_fragments: decompile panicked (quest {:08X})",
                quest.0
            );
            return 0;
        }
    };
    populate_quest_fragments_from_script(frags, quest, &script, bindings)
}

/// The AST half of [`populate_quest_fragments_from_pex`] — lower each
/// `(stage, fragment_name)` binding against an already-decompiled (or
/// source-parsed) [`Script`] and register the non-empty lowerings.
/// Split out so the lowering path is unit-testable from a `.psc` source
/// without game-data `.pex` bytes.
pub fn populate_quest_fragments_from_script(
    frags: &mut QuestStageFragments,
    quest: QuestFormId,
    script: &Script,
    bindings: &[(u16, &str)],
) -> usize {
    let mut inserted = 0;
    // A QUST stage may carry several log entries, each with its own
    // Fragment_N binding (MQ101 stage 0 has five). They all run when that
    // stage is set, in VMAD order. Build one ordered effect chain per stage
    // and replace the installed chain once, rather than letting the last
    // binding silently overwrite its siblings. Replacing once also keeps
    // repeated cell-load population idempotent.
    let mut stage_order = Vec::new();
    let mut effects_by_stage: HashMap<u16, Vec<Effect>> = HashMap::new();
    // #2538 / SCR-D5-NEW10-01 — computed once per script, not per
    // fragment; every fragment in the same script shares the same
    // property declarations.
    let quest_properties = quest_property_names(script);
    for (stage, fragment_name) in bindings {
        let Some(body) = function_body(script, fragment_name) else {
            log::debug!(
                "populate_quest_fragments: fn '{fragment_name}' absent in quest {:08X} .pex",
                quest.0
            );
            continue;
        };
        // Decline-on-any-unmodeled-term: a fragment the effect table can't
        // fully lower is skipped, not partially applied. An empty
        // fully-lowered fragment carries no effects, so it needn't occupy
        // the map (a lookup miss is equivalent to an empty entry).
        if let Some(effects) = lower_fragment_with_quest_properties(body, &quest_properties) {
            if !effects.is_empty() {
                if !effects_by_stage.contains_key(stage) {
                    stage_order.push(*stage);
                }
                effects_by_stage.entry(*stage).or_default().extend(effects);
                inserted += 1;
            }
        }
    }
    for stage in stage_order {
        if let Some(effects) = effects_by_stage.remove(&stage) {
            frags.insert(quest, stage, effects);
        }
    }
    inserted
}

/// Decompile and lower the lifecycle fragments for one authored scene.
/// Scene fragments use the same conservative effect vocabulary and property
/// resolution as quest-stage fragments; any function containing an unknown
/// operation is declined as a whole.
pub fn populate_scene_fragments_from_pex(
    frags: &mut SceneFragments,
    scene_form_id: u32,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    pex_bytes: &[u8],
    bindings: &[(SceneFragmentEvent, &str)],
) -> usize {
    let pex = match byroredux_pex::parse(pex_bytes) {
        Ok(pex) => pex,
        Err(error) => {
            log::debug!(
                "populate_scene_fragments: .pex parse failed (scene {scene_form_id:08X}): {error}"
            );
            return 0;
        }
    };
    let script = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        byroredux_pex::decompile::decompile_script(&pex)
    })) {
        Ok(Ok(script)) => script,
        Ok(Err(error)) => {
            log::debug!(
                "populate_scene_fragments: decompile failed (scene {scene_form_id:08X}): {error}"
            );
            return 0;
        }
        Err(_) => {
            log::debug!("populate_scene_fragments: decompile panicked (scene {scene_form_id:08X})");
            return 0;
        }
    };
    populate_scene_fragments_from_script(frags, scene_form_id, context, vmad, &script, bindings)
}

/// AST half of [`populate_scene_fragments_from_pex`], exposed for focused
/// conformance tests without requiring compiled game-data fixtures.
pub fn populate_scene_fragments_from_script(
    frags: &mut SceneFragments,
    scene_form_id: u32,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    script: &Script,
    bindings: &[(SceneFragmentEvent, &str)],
) -> usize {
    let quest_properties = quest_property_names(script);
    let mut inserted = 0;
    for (event, fragment_name) in bindings {
        let Some(body) = function_body(script, fragment_name) else {
            log::debug!(
                "populate_scene_fragments: fn '{fragment_name}' absent in scene {scene_form_id:08X} .pex"
            );
            continue;
        };
        if let Some(effects) = lower_fragment_with_quest_properties(body, &quest_properties) {
            if !effects.is_empty() {
                frags.insert(scene_form_id, *event, context, vmad.cloned(), effects);
                inserted += 1;
            }
        }
    }
    inserted
}

/// Execute the lowered Papyrus fragments emitted by scene playback this
/// frame. Quest advances enter the canonical journal immediately, allowing
/// the later quest-fragment dispatcher to cascade the corresponding QUST
/// fragment in the same update.
pub fn scene_fragment_dispatch_system(world: &World, _dt: f32) {
    let invocations: Vec<crate::scene::SceneFragmentInvocation> = world
        .query::<crate::scene::SceneFragmentInvocationBatch>()
        .map(|query| {
            query
                .iter()
                .flat_map(|(_, batch)| batch.0.iter().cloned())
                .collect()
        })
        .unwrap_or_default();
    if invocations.is_empty() {
        return;
    }
    let fragments = world.resource::<SceneFragments>().clone();
    if fragments.is_empty() {
        return;
    }

    let mut advances = Vec::new();
    let mut deferred = DeferredFragmentEffects::new(world);
    {
        let (mut stages, mut objectives) =
            world.resource_2_mut::<QuestStageState, QuestObjectiveState>();
        for invocation in invocations {
            let Some(fragment) = fragments.get(invocation.scene_form_id, invocation.event) else {
                continue;
            };
            advances.extend(apply_effects(
                &fragment.effects,
                fragment.context,
                fragment.vmad.as_ref(),
                world,
                &mut stages,
                &mut objectives,
                &mut deferred,
            ));
        }
    }
    deferred.apply(world);
    if advances.is_empty() {
        return;
    }

    // #3580 — copy the entity out and DROP the `PlayerEntity` guard before
    // `push_quest_stage_advances` acquires the `QuestStageAdvancedBatch`
    // storage. Binding the guard to a `let ... else` local kept it alive
    // across that call, recording `PlayerEntity -> QuestStageAdvancedBatch`
    // and closing a ring with the other two edges the quest systems record.
    // Every other `PlayerEntity` site in the crate already copies the id out
    // of a statement-scoped temporary; this one did not.
    let Some(player) = world
        .try_resource::<crate::papyrus_demo::PlayerEntity>()
        .map(|player| player.0)
    else {
        return;
    };
    crate::quest_stages::push_quest_stage_advances(world, player, advances);
}

/// Consume [`QuestStageAdvanced`] markers and run the matching
/// `(quest, stage)` fragments, cascading any `SetStage`s they perform
/// (bounded by [`MAX_CASCADE`]). Runs after `quest_advance_system` (which
/// emits the initial markers) and before end-of-frame cleanup.
///
/// Effect resolution passes the quest's own registered VMAD (see
/// [`QuestStageFragments::insert_vmad`]), when one was registered, so
/// `Self`/owning-quest-targeted effects always apply and a cross-quest
/// `Property`-targeted effect resolves too, as long as the named property
/// is an `Object`-typed binding on the quest's own VMAD. Object, scene,
/// player-control, package, and cinematic effects apply directly against the
/// live ECS world; latent tails are handed to [`FragmentExecutionQueue`].
/// Unrecognized operations still decline the whole fragment at lowering. The
/// table is empty (and this a no-op) on loads without `--scripts-bsa` or on
/// pre-Papyrus games.
pub fn quest_fragment_dispatch_system(world: &World) {
    // Snapshot compatibility ingress before taking resource locks. The
    // sequenced journal below is authoritative; batches remain accepted for
    // direct tests/tools and are deduplicated against journal entries.
    let mut legacy_events = Vec::new();
    if let Some(markers) = world.query::<QuestStageAdvancedBatch>() {
        for (_entity, batch) in markers.iter() {
            legacy_events.extend(batch.0.iter().copied());
        }
    }

    // Static fragment data is cloned before taking mutable quest resources.
    // This avoids a read→write nested resource-lock order and lets the paired
    // mutable resources use the ECS's TypeId-sorted deadlock-safe API.
    let frags = world.resource::<QuestStageFragments>().clone();
    // Do not claim the destructive quest-event journal when there is no
    // fragment table to consume it. The next install/population pass must
    // still be able to dispatch these transitions (#3012).
    if frags.is_empty() {
        return;
    }

    let mut chained: Vec<QuestStageAdvanced> = Vec::new();
    let mut deferred = DeferredFragmentEffects::new(world);
    let mut cascade_steps = 0usize;
    {
        let (mut stages, mut objectives) =
            world.resource_2_mut::<QuestStageState, QuestObjectiveState>();
        // The boolean distinguishes authored ingress from a SetStage emitted
        // by a fragment. The cascade cap must constrain only the latter: a
        // Skyrim bootstrap can legitimately deliver hundreds of independent
        // Start Game Enabled quest events in one tick.
        let mut queue: VecDeque<(QuestFormId, u16, bool)> = VecDeque::new();
        // Compatibility batches mirror journal events but carry no sequence.
        // Count mirrors as a multiset: all sequenced journal commits remain in
        // order, including two legitimate identical transitions, and only the
        // corresponding unsequenced copies are suppressed.
        let mut journal_mirrors: HashMap<(QuestFormId, u16, u16), usize> = HashMap::new();
        let journal_read = stages.poll_quest_events(FRAGMENT_QUEST_EVENT_SUBSCRIBER);
        if journal_read.missed_events > 0 {
            log::error!(
                target: "scripting::quest_fragments",
                "fragment subscriber missed {} retained quest transition(s); \
                 canonical quest state remains valid but skipped fragment effects cannot be reconstructed",
                journal_read.missed_events
            );
        }
        for sequenced in journal_read.events {
            let event = sequenced.event;
            *journal_mirrors
                .entry((event.quest, event.previous_stage, event.new_stage))
                .or_default() += 1;
            queue.push_back((event.quest, event.new_stage, false));
        }
        for event in legacy_events {
            let key = (event.quest, event.previous_stage, event.new_stage);
            if let Some(mirrors) = journal_mirrors.get_mut(&key) {
                if *mirrors > 0 {
                    *mirrors -= 1;
                    continue;
                }
            }
            queue.push_back((event.quest, event.new_stage, false));
        }

        if queue.is_empty() {
            return;
        }

        while let Some((quest, stage, is_cascade)) = queue.pop_front() {
            if is_cascade {
                cascade_steps += 1;
            }
            if cascade_steps > MAX_CASCADE {
                log::warn!(
                    "quest fragment cascade exceeded {MAX_CASCADE} steps at quest {:?} stage {stage}; \
                     stopping (possible cyclic SetStage)",
                    quest
                );
                break;
            }
            let Some(effects) = frags.get(quest, stage) else {
                continue;
            };
            let advances = apply_effects(
                effects,
                quest,
                frags.vmad(quest),
                world,
                &mut stages,
                &mut objectives,
                &mut deferred,
            );
            for adv in advances {
                // Only cascade genuine transitions (skip a no-op re-set of
                // the same stage to avoid trivial self-loops). #2124 — this
                // must compare `adv`'s own previous/new stage, not the
                // *currently-dispatching* fragment's `(quest, stage)` pair:
                // comparing against `stage` alone let a different quest's
                // genuine transition collide (false negative, silently
                // dropped) or a same-fragment double-`SetStage` re-queue
                // (false positive, duplicate effect application) whenever
                // `adv.new_stage` happened to numerically equal `stage`.
                if adv.previous_stage != adv.new_stage {
                    queue.push_back((adv.quest, adv.new_stage, true));
                }
                chained.push(adv);
            }
        }

        // Chained SetStage calls were dispatched synchronously above while
        // this same state lock excluded other producers. Advance only this
        // consumer's cursor so they are not replayed next time; every other
        // subscriber retains its independent position.
        stages.acknowledge_quest_events(FRAGMENT_QUEST_EVENT_SUBSCRIBER);
    }

    deferred.apply(world);

    // Emit markers for the chained advances so other consumers (journal
    // UI, further-frame dispatch) observe them. Co-opts the same
    // player-entity sink quest_advance_system uses.
    //
    // #1864 / SCR-D7-NEW-01 — insert the whole batch ONCE. A single
    // `apply_effects` call (let alone the whole cascade) can produce >1
    // chained advance; looping `insert()` onto this one shared sink entity
    // would silently collapse every advance but the last.
    if chained.is_empty() {
        return;
    }
    let player_entity = world.resource::<crate::papyrus_demo::PlayerEntity>().0;
    // #3277 — was a bare `insert()`, the one non-defensive writer of the six.
    // Harmless only while this system was the last same-frame producer in the
    // schedule; `quest_alias_readiness_stage_system` and
    // `scene_fragment_dispatch_system` now run immediately before it.
    crate::quest_stages::push_quest_stage_advances(world, player_entity, chained);
}

#[cfg(test)]
mod tests;
