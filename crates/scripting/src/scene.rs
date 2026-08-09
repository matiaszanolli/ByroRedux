//! ECS runtime for Skyrim+ `SCEN` records.
//!
//! Parsed scene records are immutable definitions held by [`SceneRegistry`].
//! Each definition owns one persistent [`ScenePlayer`] entity containing only
//! playback state.  Actions communicate with their eventual dialogue/package
//! executors through transient [`SceneEventBatch`] components, while compiled
//! Papyrus bindings are surfaced as [`SceneFragmentInvocationBatch`] events.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use byroredux_core::ecs::components::{Dead, FactionRanks, GlobalTransform, Inventory, ItemStack};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::script_instance::{SceneFragmentEvent, SceneScriptFragment};
use byroredux_plugin::esm::records::{
    AliasFillType, QuestAlias, QustRecord, ScenRecord, SceneAction, SceneActionType,
    ALIAS_FLAG_ALLOW_DEAD, ALIAS_FLAG_ALLOW_RESERVED, ALIAS_FLAG_ALLOW_REUSE, ALIAS_FLAG_CLOSEST,
    ALIAS_FLAG_RESERVES, SCENE_BEGIN_ON_QUEST_START,
};

use crate::condition::{evaluate, ConditionContext};
use crate::papyrus_demo::PlayerEntity;
use crate::quest_stages::{
    QuestFormId, QuestStageAdvancedBatch, QuestStageState, SCENE_QUEST_EVENT_SUBSCRIBER,
};

/// Immutable scene definitions and the ECS entity assigned to each one.
#[derive(Debug, Clone, Default)]
pub struct SceneRegistry {
    definitions: HashMap<u32, Arc<ScenRecord>>,
    entities: HashMap<u32, EntityId>,
}

impl Resource for SceneRegistry {}

impl SceneRegistry {
    /// Build a definition-only registry, useful for tools and conformance
    /// probes that do not own an ECS [`World`].
    pub fn from_records(records: impl IntoIterator<Item = ScenRecord>) -> Self {
        let mut registry = Self::default();
        for record in records {
            registry
                .definitions
                .insert(record.form_id, Arc::new(record));
        }
        registry
    }

    pub fn definition(&self, form_id: u32) -> Option<&ScenRecord> {
        self.definitions.get(&form_id).map(Arc::as_ref)
    }

    pub fn scene_entity(&self, form_id: u32) -> Option<EntityId> {
        self.entities.get(&form_id).copied()
    }

    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    fn definition_arc(&self, form_id: u32) -> Option<Arc<ScenRecord>> {
        self.definitions.get(&form_id).cloned()
    }
}

/// Runtime resolution of `(owning quest, alias id)` to a live reference.
///
/// The historical name is retained because SCEN actor slots were the first
/// consumer. The table is now the canonical quest-alias surface used by
/// conditions, fragment object resolution, packages, dialogue, and scenes.
#[derive(Debug, Clone, Default)]
pub struct SceneActorBindings {
    actors: HashMap<(QuestFormId, i32), EntityId>,
    dirty: bool,
}

impl Resource for SceneActorBindings {}

impl SceneActorBindings {
    pub fn bind(&mut self, quest: QuestFormId, alias_id: i32, entity: EntityId) {
        self.actors.insert((quest, alias_id), entity);
    }

    pub fn unbind(&mut self, quest: QuestFormId, alias_id: i32) -> Option<EntityId> {
        self.actors.remove(&(quest, alias_id))
    }

    pub fn resolve(&self, quest: QuestFormId, alias_id: i32) -> Option<EntityId> {
        self.actors.get(&(quest, alias_id)).copied()
    }

    /// Whether cell/lifecycle changes have invalidated the cached table and
    /// the scheduled alias refresh has not run yet.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

/// Operator-facing explanation of one quest alias's current fill state.
///
/// The categories deliberately stop at the runtime's observable boundary:
/// `NoEligibleLoadedCandidate` can include fill mismatch, CTDA rejection,
/// reservation/reuse flags, or a dead actor. It never claims which predicate
/// rejected a candidate without retaining a second copy of resolver state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestAliasResolutionState {
    Bound(EntityId),
    QuestNotRunning,
    LocationRuntimeUnavailable,
    ReferenceCollectionRuntimeUnavailable,
    CreatedObjectRuntimeUnavailable,
    StoryManagerEventUnavailable,
    ExternalSourceUnbound { quest: QuestFormId, alias_id: i32 },
    DependencyAliasUnbound(i32),
    ForceIntoSourcesUnbound(Vec<i32>),
    NoEligibleLoadedCandidate,
    NoFillMechanism,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuestAliasDiagnostic {
    pub alias: QuestAlias,
    pub state: QuestAliasResolutionState,
}

/// Raw, source-attributed alias overlays attached to a filled reference.
///
/// Consumers that understand package/spell/keyword/display-name semantics can
/// read this component without reparsing QUST. Factions and inventory are also
/// materialised into their canonical ECS components by the alias refresher.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuestAliasInjectedOverlays(
    pub HashMap<(QuestFormId, i32), byroredux_plugin::esm::records::AliasInjectedData>,
);

impl Component for QuestAliasInjectedOverlays {
    type Storage = SparseSetStorage<Self>;
}

/// Full authored alias overlays for consumers of flags/fill metadata that do
/// not yet have dedicated ECS components (essential/protected/quest-object,
/// package overrides, voice/name data, and FO4 extensions).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestAliasRuntimeOverlays(pub HashMap<(QuestFormId, i32), QuestAlias>);

impl Component for QuestAliasRuntimeOverlays {
    type Storage = SparseSetStorage<Self>;
}

/// Bookkeeping for QUST alias injections.
///
/// Faction overlays are reconstructed from the immutable alias definitions on
/// load. Permanent CNTO grants are retained by stable reference FormID so the
/// first post-load alias refresh cannot grant the same authored inventory
/// entry a second time even when the live entity has a new [`EntityId`].
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct QuestAliasInjectionState {
    /// Memberships owned by alias injection. `original_rank` preserves a
    /// base membership when the last alias source releases its overlay.
    #[cfg_attr(feature = "save", serde(skip, default))]
    factions: HashMap<(EntityId, u32), InjectedFactionMembership>,
    /// `(quest, alias, reference, item, count)` entries are deliberately free
    /// of session-local entity IDs. Refilling the alias onto another authored
    /// reference still grants there because its reference FormID differs.
    inventory_grants: HashSet<(QuestFormId, i32, u32, u32, u32)>,
}

impl Resource for QuestAliasInjectionState {}

#[derive(Debug, Clone, Default)]
struct InjectedFactionMembership {
    original_rank: Option<i8>,
}

/// Identity carried by a spawned reference that may fill a quest alias.
/// `reference_form_id` is the ACHR/REFR identity, `base_form_id` is its NPC_
/// (or other base record), `linked_refs` are its XLKR keyword/target pairs,
/// and `location_ref_types` are the placement's XLRT LCRT tags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SceneAliasCandidate {
    pub reference_form_id: u32,
    pub base_form_id: u32,
    pub linked_refs: Vec<(u32, u32)>,
    pub location_ref_types: Vec<u32>,
}

impl Component for SceneAliasCandidate {
    type Storage = SparseSetStorage<Self>;
}

#[derive(Debug, Clone, Default)]
struct SceneQuestAliasRegistry {
    aliases: HashMap<QuestFormId, Vec<QuestAlias>>,
}

impl Resource for SceneQuestAliasRegistry {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenePlaybackState {
    Dormant,
    WaitingForStart,
    WaitingForPhaseStart,
    Playing,
    Finished,
    Stopped,
}

/// Runtime state for one active action. The authored action remains in the
/// registry; this component stores only mutable lifecycle state.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveSceneAction {
    pub action_index: u32,
    pub action_type: SceneActionType,
    pub end_phase: u32,
    pub remaining_seconds: Option<f32>,
    pub completed: bool,
}

/// Persistent playback state for one scene definition.
///
/// # Save registry — deliberately NOT registered (#2294 / SAVE-D1-11)
///
/// Structurally similar to `AnimationPlayer`/`AnimationStack`, which were
/// deliberately excluded from `byroredux::save_io::build_save_registry`
/// (#1696) because the reloaded cell's systems reconstruct equivalent state
/// from scratch. The same call is believed to apply here: `current_phase`/
/// `active_actions`/`completed_actions` are re-derived from the persisted
/// `QuestStageState` (which IS saved) via the normal `SceneStartRequest`
/// flow when the driving quest stage re-fires on reload, rather than proven
/// byte-for-byte here — flagged by #2294 as a documented, not merely
/// omitted, decision. If a live path is ever found where a reload does NOT
/// re-derive this state, register `ScenePlayer` instead of leaving this
/// comment stale.
///
/// **Caveat (#2380 / SAVE-D1-15)**: the #2295 registry-completeness sweep
/// found sibling fragment-driven state (`ActorCinematicState` et al.) whose
/// superficially-similar "reload re-derives it" assumption did NOT hold —
/// `quest_fragment_dispatch_system` is edge-triggered off an acknowledged
/// event journal, so a `SetStage` transition never replays on reload. This
/// type's `SceneStartRequest`-driven mechanism is structurally different
/// (scene entities are (re)created from live `SceneEvent`s, not a one-shot
/// edge-triggered dispatch), so the distinction is believed to still hold,
/// but has not been independently re-verified since that finding.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenePlayer {
    pub scene_form_id: u32,
    pub state: ScenePlaybackState,
    pub current_phase: u32,
    pub active_actions: Vec<ActiveSceneAction>,
    /// Action indices completed during the current run. Skyrim's
    /// `IsSceneActionComplete` CTDA can query an action after it has left the
    /// active set, so completion cannot be represented only on
    /// [`ActiveSceneAction`]. Cleared whenever the scene starts again.
    pub completed_actions: HashSet<u32>,
}

impl ScenePlayer {
    pub fn new(scene_form_id: u32) -> Self {
        Self {
            scene_form_id,
            state: ScenePlaybackState::Dormant,
            current_phase: 0,
            active_actions: Vec::new(),
            completed_actions: HashSet::new(),
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(
            self.state,
            ScenePlaybackState::WaitingForStart
                | ScenePlaybackState::WaitingForPhaseStart
                | ScenePlaybackState::Playing
        )
    }
}

impl Component for ScenePlayer {
    type Storage = SparseSetStorage<Self>;
}

/// Explicitly request that the scene attached to this entity start.
#[derive(Debug, Clone, Copy, Default)]
pub struct SceneStartRequest;

impl Component for SceneStartRequest {
    type Storage = SparseSetStorage<Self>;
}

/// Explicitly stop the scene attached to this entity.
#[derive(Debug, Clone, Copy, Default)]
pub struct SceneStopRequest;

impl Component for SceneStopRequest {
    type Storage = SparseSetStorage<Self>;
}

/// External completion signals for dialogue, package, or unknown actions.
/// Timer actions normally complete internally but accept the same signal.
#[derive(Debug, Clone, Default)]
pub struct SceneActionCompletionBatch(pub Vec<u32>);

impl Component for SceneActionCompletionBatch {
    type Storage = SparseSetStorage<Self>;
}

/// Scene/action lifecycle output consumed by dialogue, package, animation,
/// audio, and future quest orchestration systems.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneEvent {
    SceneStarted,
    PhaseStarted {
        phase_index: u32,
    },
    ActionStarted {
        action_index: u32,
        action_type: SceneActionType,
        actor_alias: i32,
        actor_entity: Option<EntityId>,
        topic_form_id: Option<u32>,
        packages: Vec<u32>,
    },
    ActionCompleted {
        action_index: u32,
    },
    ActionStopped {
        action_index: u32,
        completed: bool,
    },
    PhaseCompleted {
        phase_index: u32,
    },
    SceneFinished,
    SceneStopped,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SceneEventBatch(pub Vec<SceneEvent>);

impl Component for SceneEventBatch {
    type Storage = SparseSetStorage<Self>;
}

/// One SCEN VMAD binding selected by the player state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneFragmentInvocation {
    pub scene_form_id: u32,
    pub event: SceneFragmentEvent,
    pub script_name: String,
    pub fragment_name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SceneFragmentInvocationBatch(pub Vec<SceneFragmentInvocation>);

impl Component for SceneFragmentInvocationBatch {
    type Storage = SparseSetStorage<Self>;
}

/// Register scene components/resources without replacing an already-populated
/// registry. Called by the scripting subsystem's top-level `register` hook.
pub fn register(world: &mut World) {
    world.register::<FactionRanks>();
    world.register::<Inventory>();
    world.register::<ScenePlayer>();
    world.register::<SceneAliasCandidate>();
    world.register::<QuestAliasInjectedOverlays>();
    world.register::<QuestAliasRuntimeOverlays>();
    world.register::<SceneStartRequest>();
    world.register::<SceneStopRequest>();
    world.register::<SceneActionCompletionBatch>();
    world.register::<SceneEventBatch>();
    world.register::<SceneFragmentInvocationBatch>();
    if world.try_resource::<SceneRegistry>().is_none() {
        world.insert_resource(SceneRegistry::default());
    }
    if world.try_resource::<SceneActorBindings>().is_none() {
        world.insert_resource(SceneActorBindings::default());
    }
    if world.try_resource::<SceneQuestAliasRegistry>().is_none() {
        world.insert_resource(SceneQuestAliasRegistry::default());
    }
    if world.try_resource::<QuestAliasInjectionState>().is_none() {
        world.insert_resource(QuestAliasInjectionState::default());
    }
}

/// Install parsed definitions and create one persistent scene-player entity
/// per new FormID. Re-installing a load order refreshes definitions in place
/// without resetting live playback state or creating duplicate entities.
pub fn install_scene_records(
    world: &mut World,
    records: impl IntoIterator<Item = ScenRecord>,
) -> usize {
    if world.try_resource::<SceneRegistry>().is_none() {
        register(world);
    }

    let records: Vec<ScenRecord> = records.into_iter().collect();
    let existing = {
        let registry = world.resource::<SceneRegistry>();
        registry.entities.clone()
    };

    let mut installed = Vec::with_capacity(records.len());
    for record in records {
        let form_id = record.form_id;
        let entity = existing.get(&form_id).copied().unwrap_or_else(|| {
            let entity = world.spawn();
            world.insert(entity, ScenePlayer::new(form_id));
            entity
        });
        installed.push((form_id, entity, record));
    }

    let count = installed.len();
    let mut registry = world.resource_mut::<SceneRegistry>();
    for (form_id, entity, record) in installed {
        registry.definitions.insert(form_id, Arc::new(record));
        registry.entities.insert(form_id, entity);
    }
    count
}

/// Install the parsed QUST alias definitions used by scene actor slots.
/// Definitions replace prior copies by quest FormID and mark the live binding
/// table for a refresh after the cell loader attaches alias candidates.
pub fn install_scene_quest_aliases(
    world: &mut World,
    records: impl IntoIterator<Item = QustRecord>,
) -> usize {
    if world.try_resource::<SceneQuestAliasRegistry>().is_none() {
        register(world);
    }
    let records: Vec<QustRecord> = records.into_iter().collect();
    let count = records.len();
    {
        let mut registry = world.resource_mut::<SceneQuestAliasRegistry>();
        for record in records {
            registry
                .aliases
                .insert(QuestFormId(record.form_id), record.aliases);
        }
    }
    world.resource_mut::<SceneActorBindings>().dirty = true;
    count
}

/// Notify the alias resolver that the live candidate set changed.
pub fn mark_scene_actor_bindings_dirty(world: &World) {
    if let Some(mut bindings) = world.try_resource_mut::<SceneActorBindings>() {
        bindings.dirty = true;
    }
}

/// Snapshot authored aliases and explain their current live binding state.
/// Returns `None` when the quest has no installed QUST alias definition;
/// `Some([])` is a known quest with zero aliases.
pub fn quest_alias_diagnostics(
    world: &World,
    quest: QuestFormId,
) -> Option<Vec<QuestAliasDiagnostic>> {
    let aliases = world
        .try_resource::<SceneQuestAliasRegistry>()?
        .aliases
        .get(&quest)?
        .clone();
    let running = world
        .try_resource::<QuestStageState>()
        .is_none_or(|stages| stages.is_running(quest));
    let bindings = world.try_resource::<SceneActorBindings>();

    let mut force_sources: HashMap<i32, Vec<i32>> = HashMap::new();
    for alias in &aliases {
        if let Some(target) = alias.force_into_alias {
            force_sources
                .entry(target)
                .or_default()
                .push(alias.alias_id);
        }
    }

    Some(
        aliases
            .iter()
            .cloned()
            .map(|alias| {
                let bound = bindings
                    .as_ref()
                    .and_then(|bindings| bindings.resolve(quest, alias.alias_id));
                let state = if !running {
                    QuestAliasResolutionState::QuestNotRunning
                } else if let Some(entity) = bound {
                    QuestAliasResolutionState::Bound(entity)
                } else if alias.is_collection {
                    QuestAliasResolutionState::ReferenceCollectionRuntimeUnavailable
                } else if alias.is_location
                    || matches!(alias.fill_type, Some(AliasFillType::ForcedLocation(_)))
                {
                    QuestAliasResolutionState::LocationRuntimeUnavailable
                } else if matches!(alias.fill_type, Some(AliasFillType::CreatedObject { .. })) {
                    QuestAliasResolutionState::CreatedObjectRuntimeUnavailable
                } else if matches!(alias.fill_type, Some(AliasFillType::FromEvent { .. })) {
                    QuestAliasResolutionState::StoryManagerEventUnavailable
                } else if let Some(AliasFillType::ExternalAlias {
                    quest: source_quest,
                    alias_id,
                }) = alias.fill_type
                {
                    QuestAliasResolutionState::ExternalSourceUnbound {
                        quest: QuestFormId(source_quest),
                        alias_id,
                    }
                } else if let Some(anchor) = alias.closest_to_alias {
                    if bindings
                        .as_ref()
                        .and_then(|bindings| bindings.resolve(quest, anchor))
                        .is_none()
                    {
                        QuestAliasResolutionState::DependencyAliasUnbound(anchor)
                    } else {
                        QuestAliasResolutionState::NoEligibleLoadedCandidate
                    }
                } else if let Some(AliasFillType::NearAlias { alias_id, .. }) = alias.fill_type {
                    if bindings
                        .as_ref()
                        .and_then(|bindings| bindings.resolve(quest, alias_id))
                        .is_none()
                    {
                        QuestAliasResolutionState::DependencyAliasUnbound(alias_id)
                    } else {
                        QuestAliasResolutionState::NoEligibleLoadedCandidate
                    }
                } else if alias.fill_type.is_some() || !alias.match_conditions.is_empty() {
                    QuestAliasResolutionState::NoEligibleLoadedCandidate
                } else if let Some(sources) = force_sources.get(&alias.alias_id) {
                    QuestAliasResolutionState::ForceIntoSourcesUnbound(sources.clone())
                } else {
                    QuestAliasResolutionState::NoFillMechanism
                };
                QuestAliasDiagnostic { alias, state }
            })
            .collect(),
    )
}

fn candidate_matches_fill(fill: &AliasFillType, candidate: &SceneAliasCandidate) -> bool {
    match fill {
        AliasFillType::ForcedReference(reference) => candidate.reference_form_id == *reference,
        AliasFillType::UniqueActor(base) => candidate.base_form_id == *base,
        AliasFillType::LocationAliasReference {
            ref_type: Some(ref_type),
            ..
        } => candidate.location_ref_types.contains(ref_type),
        AliasFillType::LocationAliasReference { ref_type: None, .. }
        | AliasFillType::NearAlias { .. } => false,
        AliasFillType::ForcedLocation(_)
        | AliasFillType::CreatedObject { .. }
        | AliasFillType::ExternalAlias { .. }
        | AliasFillType::FromEvent { .. } => false,
    }
}

fn apply_alias_injections(
    world: &World,
    quests: &[(QuestFormId, Vec<QuestAlias>)],
    resolved: &HashMap<(QuestFormId, i32), EntityId>,
    candidates: &[(EntityId, SceneAliasCandidate)],
) {
    let mut desired_factions: HashMap<(EntityId, u32), HashSet<(QuestFormId, i32)>> =
        HashMap::new();
    let mut desired_overlays: HashMap<
        EntityId,
        HashMap<(QuestFormId, i32), byroredux_plugin::esm::records::AliasInjectedData>,
    > = HashMap::new();
    let mut desired_runtime_overlays: HashMap<EntityId, HashMap<(QuestFormId, i32), QuestAlias>> =
        HashMap::new();
    let mut desired_inventory = Vec::new();
    let reference_form_ids: HashMap<EntityId, u32> = candidates
        .iter()
        .map(|(entity, candidate)| (*entity, candidate.reference_form_id))
        .collect();

    for (quest, aliases) in quests {
        for alias in aliases {
            let source = (*quest, alias.alias_id);
            let Some(&entity) = resolved.get(&source) else {
                continue;
            };
            desired_runtime_overlays
                .entry(entity)
                .or_default()
                .insert(source, alias.clone());
            if alias.injected != Default::default() {
                desired_overlays
                    .entry(entity)
                    .or_default()
                    .insert(source, alias.injected.clone());
            }
            for &faction in &alias.injected.factions {
                desired_factions
                    .entry((entity, faction))
                    .or_default()
                    .insert(source);
            }
            for &(item, count) in &alias.injected.inventory {
                if count > 0 {
                    if let Some(&reference_form_id) = reference_form_ids.get(&entity) {
                        desired_inventory.push((
                            source.0,
                            source.1,
                            entity,
                            reference_form_id,
                            item,
                            count,
                        ));
                    }
                }
            }
        }
    }

    let previous_factions = world
        .resource::<QuestAliasInjectionState>()
        .factions
        .clone();
    let mut next_factions = HashMap::new();
    {
        let mut ranks = world
            .query_mut::<FactionRanks>()
            .expect("FactionRanks storage registered");

        for (&key @ (entity, faction), prior) in &previous_factions {
            if desired_factions.contains_key(&key) {
                continue;
            }
            if prior.original_rank.is_none() {
                if let Some(current) = ranks.get_mut(entity) {
                    current.0.retain(|(form_id, _)| *form_id != faction);
                }
            }
        }

        for (key @ (entity, faction), _sources) in desired_factions {
            let original_rank = if let Some(prior) = previous_factions.get(&key) {
                prior.original_rank
            } else if let Some(current) = ranks.get_mut(entity) {
                let original = current.rank(faction);
                if original.is_none() {
                    current.0.push((faction, 0));
                }
                original
            } else {
                ranks.insert(entity, FactionRanks::from_pairs([(faction, 0)]));
                None
            };
            next_factions.insert(key, InjectedFactionMembership { original_rank });
        }
    }

    {
        let mut overlays = world
            .query_mut::<QuestAliasRuntimeOverlays>()
            .expect("QuestAliasRuntimeOverlays storage registered");
        let old_entities: Vec<EntityId> = overlays.iter().map(|(entity, _)| entity).collect();
        for entity in old_entities {
            if !desired_runtime_overlays.contains_key(&entity) {
                overlays.remove(entity);
            }
        }
        for (entity, aliases) in desired_runtime_overlays {
            overlays.insert(entity, QuestAliasRuntimeOverlays(aliases));
        }
    }

    let previous_grants = world
        .resource::<QuestAliasInjectionState>()
        .inventory_grants
        .clone();
    let mut next_grants = previous_grants;
    {
        let mut inventories = world
            .query_mut::<Inventory>()
            .expect("Inventory storage registered");
        for (quest, alias, entity, reference_form_id, item, count) in desired_inventory {
            if !next_grants.insert((quest, alias, reference_form_id, item, count)) {
                continue;
            }
            if let Some(inventory) = inventories.get_mut(entity) {
                inventory.push(ItemStack::new(item, count));
            } else {
                let mut inventory = Inventory::new();
                inventory.push(ItemStack::new(item, count));
                inventories.insert(entity, inventory);
            }
        }
    }

    {
        let mut overlays = world
            .query_mut::<QuestAliasInjectedOverlays>()
            .expect("QuestAliasInjectedOverlays storage registered");
        let old_entities: Vec<EntityId> = overlays.iter().map(|(entity, _)| entity).collect();
        for entity in old_entities {
            if !desired_overlays.contains_key(&entity) {
                overlays.remove(entity);
            }
        }
        for (entity, injected) in desired_overlays {
            overlays.insert(entity, QuestAliasInjectedOverlays(injected));
        }
    }

    let mut state = world.resource_mut::<QuestAliasInjectionState>();
    state.factions = next_factions;
    state.inventory_grants = next_grants;
}

/// Rebuild quest-alias → entity bindings from currently loaded candidates.
///
/// Direct references, unique actors, and XLRT location-ref roles are resolved
/// now. Creation/event/location aliases still require their owning systems.
/// Unless an alias opts into `Allow Reuse`, one entity fills only the first
/// matching role in authored alias order; this is what lets two MQ101 soldier
/// aliases sharing one LCRT select two distinct actors deterministically.
pub fn refresh_scene_actor_bindings(world: &World) -> usize {
    let should_refresh = world
        .try_resource::<SceneActorBindings>()
        .is_some_and(|bindings| bindings.dirty);
    if !should_refresh {
        return 0;
    }

    let mut candidates: Vec<(EntityId, SceneAliasCandidate)> = world
        .query::<SceneAliasCandidate>()
        .map(|query| {
            query
                .iter()
                .map(|(entity, candidate)| (entity, candidate.clone()))
                .collect()
        })
        .unwrap_or_default();
    candidates.sort_by_key(|(entity, _)| *entity);

    let mut quests: Vec<(QuestFormId, Vec<QuestAlias>)> = world
        .resource::<SceneQuestAliasRegistry>()
        .aliases
        .iter()
        .map(|(&quest, aliases)| (quest, aliases.clone()))
        .collect();
    quests.sort_by_key(|(quest, _)| quest.0);
    let registered_quests: HashSet<QuestFormId> = quests.iter().map(|(quest, _)| *quest).collect();
    // Alias values exist only for running quest instances. Unit-test/tool
    // worlds that intentionally omit the lifecycle store retain the old
    // data-only behavior; the live engine always installs QuestStageState.
    if let Some(stages) = world.try_resource::<QuestStageState>() {
        quests.retain(|(quest, _)| stages.is_running(*quest));
    }
    let mut resolved = world.resource::<SceneActorBindings>().actors.clone();
    resolved.retain(|(quest, _), _| !registered_quests.contains(quest));
    let mut external_aliases = Vec::new();
    let mut reserved = HashSet::new();

    for (quest, aliases) in &quests {
        let mut used = HashSet::new();
        for alias in aliases.iter().filter(|alias| !alias.is_location) {
            if let Some(AliasFillType::ExternalAlias {
                quest: source_quest,
                alias_id: source_alias,
            }) = alias.fill_type
            {
                external_aliases.push((*quest, alias.clone(), source_quest, source_alias));
                continue;
            }

            let allow_reuse = alias.flags.has(ALIAS_FLAG_ALLOW_REUSE);
            let allow_reserved = alias.flags.has(ALIAS_FLAG_ALLOW_RESERVED);
            let eligible = |(entity, candidate): &(EntityId, SceneAliasCandidate)| {
                if !alias.flags.has(ALIAS_FLAG_ALLOW_DEAD) && world.has::<Dead>(*entity) {
                    return None;
                }
                if !allow_reuse && used.contains(entity) {
                    return None;
                }
                if !allow_reserved && reserved.contains(entity) {
                    return None;
                }
                let fill_matches = match alias.fill_type.as_ref() {
                    Some(AliasFillType::NearAlias {
                        alias_id,
                        relation: 0 | 1,
                    }) => resolved
                        .get(&(*quest, *alias_id))
                        .and_then(|source| {
                            candidates
                                .iter()
                                .find(|(entity, _)| entity == source)
                                .map(|(_, source)| source)
                        })
                        .is_some_and(|source| {
                            source
                                .linked_refs
                                .iter()
                                .any(|(_, target)| *target == candidate.reference_form_id)
                                || candidate
                                    .linked_refs
                                    .iter()
                                    .any(|(_, target)| *target == source.reference_form_id)
                        }),
                    Some(fill) => candidate_matches_fill(fill, candidate),
                    None => !alias.match_conditions.is_empty(),
                };
                if !fill_matches
                    || !evaluate(
                        &alias.match_conditions,
                        world,
                        &ConditionContext::for_subject(*entity).with_quest(*quest),
                    )
                {
                    return None;
                }
                Some(*entity)
            };
            let anchor = alias
                .closest_to_alias
                .and_then(|anchor_alias| resolved.get(&(*quest, anchor_alias)).copied())
                .or_else(|| {
                    alias
                        .flags
                        .has(ALIAS_FLAG_CLOSEST)
                        .then(|| world.try_resource::<PlayerEntity>().map(|player| player.0))
                        .flatten()
                })
                .and_then(|entity| {
                    world
                        .get::<GlobalTransform>(entity)
                        .map(|gt| gt.translation)
                });
            let chosen = if let Some(anchor) = anchor {
                candidates
                    .iter()
                    .filter_map(|candidate| {
                        let entity = eligible(candidate)?;
                        let position = world.get::<GlobalTransform>(entity)?.translation;
                        Some((entity, position.distance_squared(anchor)))
                    })
                    .min_by(|left, right| left.1.total_cmp(&right.1))
                    .map(|(entity, _)| entity)
            } else {
                candidates.iter().find_map(eligible)
            };
            if let Some(entity) = chosen {
                resolved.insert((*quest, alias.alias_id), entity);
                if !allow_reuse {
                    used.insert(entity);
                }
                if alias.flags.has(ALIAS_FLAG_RESERVES) {
                    reserved.insert(entity);
                }
                if let Some(target_alias) = alias.force_into_alias {
                    resolved.insert((*quest, target_alias), entity);
                }
            }
        }
    }

    // External aliases can chain. Iterate to a fixed point bounded by the
    // number of external definitions; a cycle makes no progress and exits.
    for _ in 0..external_aliases.len() {
        let mut changed = false;
        for (quest, alias, source_quest, source_alias) in &external_aliases {
            let Some(entity) = resolved
                .get(&(QuestFormId(*source_quest), *source_alias))
                .copied()
            else {
                continue;
            };
            changed |= resolved.insert((*quest, alias.alias_id), entity) != Some(entity);
            if let Some(target_alias) = alias.force_into_alias {
                changed |= resolved.insert((*quest, target_alias), entity) != Some(entity);
            }
        }
        if !changed {
            break;
        }
    }

    apply_alias_injections(world, &quests, &resolved, &candidates);

    let count = resolved
        .keys()
        .filter(|(quest, _)| registered_quests.contains(quest))
        .count();
    let mut bindings = world.resource_mut::<SceneActorBindings>();
    bindings.actors = resolved;
    bindings.dirty = false;
    count
}

/// Scheduler-shaped quest alias refresh independent of SCEN playback.
pub fn quest_alias_refresh_system(world: &World, _dt: f32) {
    refresh_scene_actor_bindings(world);
}

fn snapshot_entities<T: Component>(world: &World) -> HashSet<EntityId> {
    world
        .query::<T>()
        .map(|query| query.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default()
}

fn drain<T: Component>(world: &World) {
    let Some(mut query) = world.query_mut::<T>() else {
        return;
    };
    let entities: Vec<EntityId> = query.iter().map(|(entity, _)| entity).collect();
    for entity in entities {
        query.remove(entity);
    }
}

fn fragments_for(
    scene: &ScenRecord,
    event: SceneFragmentEvent,
    out: &mut Vec<SceneFragmentInvocation>,
) {
    out.extend(
        scene
            .fragments
            .iter()
            .filter(|binding| binding.event == event)
            .map(|binding: &SceneScriptFragment| SceneFragmentInvocation {
                scene_form_id: scene.form_id,
                event: binding.event,
                script_name: binding.script_name.clone(),
                fragment_name: binding.fragment_name.clone(),
            }),
    );
}

fn actor_entity(
    scene: &ScenRecord,
    action: &SceneAction,
    bindings: &SceneActorBindings,
) -> Option<EntityId> {
    let quest = QuestFormId(scene.quest_form_id?);
    (action.actor_id >= 0)
        .then(|| bindings.resolve(quest, action.actor_id))
        .flatten()
}

fn start_phase_actions(
    player: &mut ScenePlayer,
    scene: &ScenRecord,
    bindings: &SceneActorBindings,
    events: &mut Vec<SceneEvent>,
) {
    for action in scene
        .actions
        .iter()
        .filter(|action| action.start_phase == player.current_phase)
    {
        if player
            .active_actions
            .iter()
            .any(|active| active.action_index == action.index)
        {
            continue;
        }
        player.active_actions.push(ActiveSceneAction {
            action_index: action.index,
            action_type: action.action_type,
            end_phase: action.end_phase,
            remaining_seconds: (action.action_type == SceneActionType::Timer)
                .then_some(action.timer_seconds.unwrap_or_default().max(0.0)),
            completed: false,
        });
        events.push(SceneEvent::ActionStarted {
            action_index: action.index,
            action_type: action.action_type,
            actor_alias: action.actor_id,
            actor_entity: actor_entity(scene, action, bindings),
            topic_form_id: action.topic_form_id,
            packages: action.packages.clone(),
        });
    }
}

fn stop_actions(player: &mut ScenePlayer, phase: Option<u32>, events: &mut Vec<SceneEvent>) {
    let mut retained = Vec::with_capacity(player.active_actions.len());
    for action in player.active_actions.drain(..) {
        if phase.is_none_or(|phase| action.end_phase <= phase) {
            events.push(SceneEvent::ActionStopped {
                action_index: action.action_index,
                completed: action.completed,
            });
        } else {
            retained.push(action);
        }
    }
    player.active_actions = retained;
}

fn finish_scene(
    player: &mut ScenePlayer,
    scene: &ScenRecord,
    events: &mut Vec<SceneEvent>,
    fragments: &mut Vec<SceneFragmentInvocation>,
) {
    stop_actions(player, None, events);
    player.state = ScenePlaybackState::Finished;
    events.push(SceneEvent::SceneFinished);
    fragments_for(scene, SceneFragmentEvent::End, fragments);
}

struct TickContext<'a> {
    world: &'a World,
    subject: EntityId,
    bindings: &'a SceneActorBindings,
    explicit_start: bool,
    auto_start: bool,
    stop_requested: bool,
    completions: &'a HashSet<u32>,
    dt: f32,
}

#[derive(Default)]
struct TickOutput {
    events: Vec<SceneEvent>,
    fragments: Vec<SceneFragmentInvocation>,
}

fn tick_player(
    player: &mut ScenePlayer,
    scene: &ScenRecord,
    context: &TickContext<'_>,
) -> TickOutput {
    let mut output = TickOutput::default();
    if context.stop_requested && player.is_running() {
        stop_actions(player, None, &mut output.events);
        player.state = ScenePlaybackState::Stopped;
        output.events.push(SceneEvent::SceneStopped);
        fragments_for(scene, SceneFragmentEvent::End, &mut output.fragments);
        return output;
    }

    if (context.explicit_start || context.auto_start) && !player.is_running() {
        player.current_phase = 0;
        player.active_actions.clear();
        player.completed_actions.clear();
        player.state = ScenePlaybackState::WaitingForStart;
    }

    let condition_context = scene.quest_form_id.map(QuestFormId).map_or_else(
        || ConditionContext::for_subject(context.subject),
        |quest| ConditionContext::for_subject(context.subject).with_quest(quest),
    );
    if player.state == ScenePlaybackState::WaitingForStart {
        if !evaluate(&scene.conditions, context.world, &condition_context) {
            return output;
        }
        player.state = ScenePlaybackState::WaitingForPhaseStart;
        output.events.push(SceneEvent::SceneStarted);
        fragments_for(scene, SceneFragmentEvent::Begin, &mut output.fragments);
        if scene.phases.is_empty() {
            finish_scene(player, scene, &mut output.events, &mut output.fragments);
            return output;
        }
    }

    if player.state == ScenePlaybackState::WaitingForPhaseStart {
        let Some(phase) = scene.phases.get(player.current_phase as usize) else {
            finish_scene(player, scene, &mut output.events, &mut output.fragments);
            return output;
        };
        if !evaluate(&phase.start_conditions, context.world, &condition_context) {
            return output;
        }
        output.events.push(SceneEvent::PhaseStarted {
            phase_index: player.current_phase,
        });
        if let Ok(phase_index) = u8::try_from(player.current_phase) {
            fragments_for(
                scene,
                SceneFragmentEvent::PhaseStart { phase_index },
                &mut output.fragments,
            );
        }
        start_phase_actions(player, scene, context.bindings, &mut output.events);
        player.state = ScenePlaybackState::Playing;
        // A newly-entered phase lives for at least one scheduler tick. This
        // gives fragment/action consumers a frame to react before empty
        // completion conditions can advance it.
        return output;
    }

    if player.state != ScenePlaybackState::Playing {
        return output;
    }

    for action in &mut player.active_actions {
        if action.completed {
            continue;
        }
        if context.completions.contains(&action.action_index) {
            action.completed = true;
        } else if let Some(remaining) = &mut action.remaining_seconds {
            *remaining -= context.dt.max(0.0);
            if *remaining <= 0.0 {
                action.completed = true;
            }
        }
        if action.completed {
            player.completed_actions.insert(action.action_index);
            output.events.push(SceneEvent::ActionCompleted {
                action_index: action.action_index,
            });
        }
    }

    let Some(phase) = scene.phases.get(player.current_phase as usize) else {
        finish_scene(player, scene, &mut output.events, &mut output.fragments);
        return output;
    };
    let ending_actions: Vec<&ActiveSceneAction> = player
        .active_actions
        .iter()
        .filter(|action| action.end_phase == player.current_phase)
        .collect();
    // Creation Kit phase semantics are alternatives: a phase ends when all
    // actions ending here reach Done OR its authored completion conditions
    // pass. This distinction matters for Sandbox/Follow-style packages that
    // never become Done and intentionally rely on CTDAs to release the phase.
    // An entirely empty, unconditional phase still advances; an empty phase
    // with CTDAs waits for those CTDAs instead of using vacuous `all(true)`.
    let ending_actions_complete = if ending_actions.is_empty() {
        phase.completion_conditions.is_empty()
    } else {
        ending_actions.iter().all(|action| action.completed)
    };
    let completion_conditions_pass = !phase.completion_conditions.is_empty()
        && evaluate(
            &phase.completion_conditions,
            context.world,
            &condition_context,
        );
    if !ending_actions_complete && !completion_conditions_pass {
        return output;
    }

    output.events.push(SceneEvent::PhaseCompleted {
        phase_index: player.current_phase,
    });
    if let Ok(phase_index) = u8::try_from(player.current_phase) {
        fragments_for(
            scene,
            SceneFragmentEvent::PhaseCompletion { phase_index },
            &mut output.fragments,
        );
    }
    stop_actions(player, Some(player.current_phase), &mut output.events);

    player.current_phase += 1;
    if player.current_phase as usize >= scene.phases.len() {
        finish_scene(player, scene, &mut output.events, &mut output.fragments);
    } else {
        player.state = ScenePlaybackState::WaitingForPhaseStart;
    }
    output
}

/// Advance all installed scenes by one frame.
///
/// Start/stop/completion inputs are consumed exactly once. Outputs replace the
/// previous frame's batches on each scene entity and are also drained by the
/// global end-of-frame cleanup system.
pub fn scene_playback_system(world: &World, dt: f32) {
    refresh_scene_actor_bindings(world);

    // Ensure direct unit-test/manual invocations retain one-frame semantics
    // even when the scheduler's cleanup system is not run between calls.
    drain::<SceneEventBatch>(world);
    drain::<SceneFragmentInvocationBatch>(world);

    let explicit_starts = snapshot_entities::<SceneStartRequest>(world);
    let stop_requests = snapshot_entities::<SceneStopRequest>(world);
    let completion_batches: HashMap<EntityId, HashSet<u32>> = world
        .query::<SceneActionCompletionBatch>()
        .map(|query| {
            query
                .iter()
                .map(|(entity, batch)| (entity, batch.0.iter().copied().collect()))
                .collect()
        })
        .unwrap_or_default();
    drain::<SceneStartRequest>(world);
    drain::<SceneStopRequest>(world);
    drain::<SceneActionCompletionBatch>(world);

    // The journal is authoritative and independently cursor-driven: scene
    // playback can run at any cadence without racing another consumer for a
    // one-frame marker. The component batch remains accepted as compatibility
    // ingress for tests/tools and is deduplicated by the HashSet.
    let mut quest_starts: HashSet<QuestFormId> = world
        .try_resource_mut::<QuestStageState>()
        .map(|mut state| {
            let read = state.poll_quest_events(SCENE_QUEST_EVENT_SUBSCRIBER);
            if read.missed_events > 0 {
                log::error!(
                    target: "scripting::scene",
                    "scene quest-event subscriber missed {} retained transition(s); \
                     Begin On Quest Start edges may require state resync",
                    read.missed_events
                );
            }
            read.events
                .into_iter()
                .filter(|advance| advance.event.previous_stage == 0)
                .map(|advance| advance.event.quest)
                .collect()
        })
        .unwrap_or_default();
    quest_starts.extend(
        world
            .query::<QuestStageAdvancedBatch>()
            .map(|query| {
                query
                    .iter()
                    .flat_map(|(_, batch)| batch.0.iter())
                    .filter(|advance| advance.previous_stage == 0)
                    .map(|advance| advance.quest)
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default(),
    );

    let players: Vec<(EntityId, ScenePlayer)> = world
        .query::<ScenePlayer>()
        .map(|query| {
            query
                .iter()
                .filter(|(entity, player)| {
                    !quest_starts.is_empty()
                        || player.is_running()
                        || explicit_starts.contains(entity)
                        || stop_requests.contains(entity)
                        || completion_batches.contains_key(entity)
                })
                .map(|(entity, player)| (entity, player.clone()))
                .collect()
        })
        .unwrap_or_default();
    if players.is_empty() {
        return;
    }

    let definitions: Vec<(EntityId, ScenePlayer, Arc<ScenRecord>)> = {
        let registry = world.resource::<SceneRegistry>();
        players
            .into_iter()
            .filter_map(|(entity, player)| {
                registry
                    .definition_arc(player.scene_form_id)
                    .map(|definition| (entity, player, definition))
            })
            .collect()
    };
    let bindings = world.resource::<SceneActorBindings>().clone();
    let subject = world
        .try_resource::<PlayerEntity>()
        .map(|player| player.0)
        .unwrap_or_default();

    let empty_completions = HashSet::new();
    let mut updates = Vec::with_capacity(definitions.len());
    for (entity, mut player, scene) in definitions {
        let previous_state = player.state;
        let previous_phase = player.current_phase;
        let auto_start = scene.flags & SCENE_BEGIN_ON_QUEST_START != 0
            && scene
                .quest_form_id
                .is_some_and(|quest| quest_starts.contains(&QuestFormId(quest)));
        let output = tick_player(
            &mut player,
            &scene,
            &TickContext {
                world,
                subject,
                bindings: &bindings,
                explicit_start: explicit_starts.contains(&entity),
                auto_start,
                stop_requested: stop_requests.contains(&entity),
                completions: completion_batches
                    .get(&entity)
                    .unwrap_or(&empty_completions),
                dt,
            },
        );
        if player.state != previous_state || player.current_phase != previous_phase {
            log::info!(
                target: "scripting::scene",
                "SCEN {:08X} '{}' {:?}/phase {} -> {:?}/phase {}",
                scene.form_id,
                scene.editor_id,
                previous_state,
                previous_phase,
                player.state,
                player.current_phase,
            );
        }
        updates.push((entity, player, output.events, output.fragments));
    }

    {
        let mut query = world
            .query_mut::<ScenePlayer>()
            .expect("ScenePlayer storage registered");
        for (entity, player, _, _) in &updates {
            query.insert(*entity, player.clone());
        }
    }
    {
        let mut query = world
            .query_mut::<SceneEventBatch>()
            .expect("SceneEventBatch storage registered");
        for (entity, _, events, _) in &updates {
            if !events.is_empty() {
                query.insert(*entity, SceneEventBatch(events.clone()));
            }
        }
    }
    {
        let mut query = world
            .query_mut::<SceneFragmentInvocationBatch>()
            .expect("SceneFragmentInvocationBatch storage registered");
        for (entity, _, _, fragments) in updates {
            if !fragments.is_empty() {
                query.insert(entity, SceneFragmentInvocationBatch(fragments));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::condition::{
        ComparisonOp, Condition, ConditionValue, RunOn,
    };
    use byroredux_plugin::esm::records::{ScenePhase, SCENE_BEGIN_ON_QUEST_START};

    use crate::quest_stages::{
        install_engine_start_quest, quest_startup_system, QuestStageAdvanced, QuestStageState,
    };

    const SCENE: u32 = 0x100;
    const QUEST: u32 = 0x200;

    #[test]
    fn quest_alias_diagnostics_classify_runtime_boundaries_and_dependencies() {
        let mut world = World::new();
        crate::register(&mut world);
        world.insert_resource(QuestStageState::default());
        world
            .resource_mut::<QuestStageState>()
            .start_quest(QuestFormId(QUEST), None);
        install_scene_quest_aliases(
            &mut world,
            [QustRecord {
                form_id: QUEST,
                aliases: vec![
                    QuestAlias {
                        alias_id: 1,
                        fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                        force_into_alias: Some(6),
                        ..Default::default()
                    },
                    QuestAlias {
                        alias_id: 2,
                        fill_type: Some(AliasFillType::CreatedObject {
                            base: 0xB1,
                            target_alias: 1,
                            create_mode: 0,
                            level: 1,
                        }),
                        ..Default::default()
                    },
                    QuestAlias {
                        alias_id: 3,
                        fill_type: Some(AliasFillType::FromEvent {
                            event_type: *b"ACTV",
                            data: 0,
                        }),
                        ..Default::default()
                    },
                    QuestAlias {
                        alias_id: 4,
                        fill_type: Some(AliasFillType::ExternalAlias {
                            quest: 0x300,
                            alias_id: 9,
                        }),
                        ..Default::default()
                    },
                    QuestAlias {
                        alias_id: 5,
                        fill_type: Some(AliasFillType::NearAlias {
                            alias_id: 1,
                            relation: 0,
                        }),
                        ..Default::default()
                    },
                    QuestAlias {
                        alias_id: 6,
                        ..Default::default()
                    },
                    QuestAlias {
                        alias_id: 7,
                        is_location: true,
                        ..Default::default()
                    },
                    QuestAlias {
                        alias_id: 8,
                        is_collection: true,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
        );
        refresh_scene_actor_bindings(&world);

        let diagnostics = quest_alias_diagnostics(&world, QuestFormId(QUEST)).unwrap();
        let state = |alias_id| {
            &diagnostics
                .iter()
                .find(|diagnostic| diagnostic.alias.alias_id == alias_id)
                .unwrap()
                .state
        };
        assert_eq!(
            state(1),
            &QuestAliasResolutionState::NoEligibleLoadedCandidate
        );
        assert_eq!(
            state(2),
            &QuestAliasResolutionState::CreatedObjectRuntimeUnavailable
        );
        assert_eq!(
            state(3),
            &QuestAliasResolutionState::StoryManagerEventUnavailable
        );
        assert_eq!(
            state(4),
            &QuestAliasResolutionState::ExternalSourceUnbound {
                quest: QuestFormId(0x300),
                alias_id: 9,
            }
        );
        assert_eq!(
            state(5),
            &QuestAliasResolutionState::DependencyAliasUnbound(1)
        );
        assert_eq!(
            state(6),
            &QuestAliasResolutionState::ForceIntoSourcesUnbound(vec![1])
        );
        assert_eq!(
            state(7),
            &QuestAliasResolutionState::LocationRuntimeUnavailable
        );
        assert_eq!(
            state(8),
            &QuestAliasResolutionState::ReferenceCollectionRuntimeUnavailable
        );

        world
            .resource_mut::<QuestStageState>()
            .stop(QuestFormId(QUEST));
        assert!(quest_alias_diagnostics(&world, QuestFormId(QUEST))
            .unwrap()
            .iter()
            .all(|diagnostic| diagnostic.state == QuestAliasResolutionState::QuestNotRunning));
    }

    #[test]
    fn quest_alias_refresh_resolves_direct_unique_and_distinct_xlrt_roles() {
        let mut world = World::new();
        crate::register(&mut world);
        let forced = world.spawn();
        let unique = world.spawn();
        let soldier_a = world.spawn();
        let soldier_b = world.spawn();
        for (entity, candidate) in [
            (
                forced,
                SceneAliasCandidate {
                    reference_form_id: 0xA1,
                    base_form_id: 0xB1,
                    linked_refs: Vec::new(),
                    location_ref_types: Vec::new(),
                },
            ),
            (
                unique,
                SceneAliasCandidate {
                    reference_form_id: 0xA2,
                    base_form_id: 0xB2,
                    linked_refs: Vec::new(),
                    location_ref_types: Vec::new(),
                },
            ),
            (
                soldier_a,
                SceneAliasCandidate {
                    reference_form_id: 0xA3,
                    base_form_id: 0xB3,
                    linked_refs: Vec::new(),
                    location_ref_types: vec![0xC1],
                },
            ),
            (
                soldier_b,
                SceneAliasCandidate {
                    reference_form_id: 0xA4,
                    base_form_id: 0xB3,
                    linked_refs: Vec::new(),
                    location_ref_types: vec![0xC1],
                },
            ),
        ] {
            world.insert(entity, candidate);
        }
        install_scene_quest_aliases(
            &mut world,
            [QustRecord {
                form_id: QUEST,
                aliases: vec![
                    QuestAlias {
                        alias_id: 1,
                        name: "Forced".to_owned(),
                        fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                        force_into_alias: Some(10),
                        ..Default::default()
                    },
                    QuestAlias {
                        alias_id: 2,
                        name: "Unique".to_owned(),
                        fill_type: Some(AliasFillType::UniqueActor(0xB2)),
                        ..Default::default()
                    },
                    QuestAlias {
                        alias_id: 3,
                        name: "SoldierA".to_owned(),
                        fill_type: Some(AliasFillType::LocationAliasReference {
                            alias_id: 0,
                            keyword: None,
                            ref_type: Some(0xC1),
                        }),
                        ..Default::default()
                    },
                    QuestAlias {
                        alias_id: 4,
                        name: "SoldierB".to_owned(),
                        fill_type: Some(AliasFillType::LocationAliasReference {
                            alias_id: 0,
                            keyword: None,
                            ref_type: Some(0xC1),
                        }),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
        );

        assert_eq!(refresh_scene_actor_bindings(&world), 5);
        let bindings = world.resource::<SceneActorBindings>();
        assert_eq!(bindings.resolve(QuestFormId(QUEST), 1), Some(forced));
        assert_eq!(bindings.resolve(QuestFormId(QUEST), 10), Some(forced));
        assert_eq!(bindings.resolve(QuestFormId(QUEST), 2), Some(unique));
        assert_eq!(bindings.resolve(QuestFormId(QUEST), 3), Some(soldier_a));
        assert_eq!(bindings.resolve(QuestFormId(QUEST), 4), Some(soldier_b));
        assert_eq!(refresh_scene_actor_bindings(&world), 0, "clean fast path");
    }

    #[test]
    fn quest_alias_reservations_block_later_quests_unless_allowed() {
        let mut world = World::new();
        crate::register(&mut world);
        let actor = world.spawn();
        world.insert(
            actor,
            SceneAliasCandidate {
                reference_form_id: 0xA1,
                base_form_id: 0xB1,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        install_scene_quest_aliases(
            &mut world,
            [
                QustRecord {
                    form_id: 0x100,
                    aliases: vec![QuestAlias {
                        alias_id: 1,
                        fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                        flags: byroredux_plugin::esm::records::AliasFlags(ALIAS_FLAG_RESERVES),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                QustRecord {
                    form_id: 0x200,
                    aliases: vec![QuestAlias {
                        alias_id: 1,
                        fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                QustRecord {
                    form_id: 0x300,
                    aliases: vec![QuestAlias {
                        alias_id: 1,
                        fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                        flags: byroredux_plugin::esm::records::AliasFlags(
                            ALIAS_FLAG_ALLOW_RESERVED,
                        ),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
        );

        refresh_scene_actor_bindings(&world);
        let bindings = world.resource::<SceneActorBindings>();
        assert_eq!(bindings.resolve(QuestFormId(0x100), 1), Some(actor));
        assert_eq!(bindings.resolve(QuestFormId(0x200), 1), None);
        assert_eq!(bindings.resolve(QuestFormId(0x300), 1), Some(actor));
    }

    #[test]
    fn quest_alias_closest_to_alias_selects_by_world_distance() {
        use byroredux_core::math::{Quat, Vec3};

        let mut world = World::new();
        crate::register(&mut world);
        world.register::<GlobalTransform>();
        let anchor = world.spawn();
        let far = world.spawn();
        let near = world.spawn();
        for (entity, reference, base, x) in [
            (anchor, 0xA0, 0xB0, 0.0),
            (far, 0xA1, 0xB1, 20.0),
            (near, 0xA2, 0xB1, 2.0),
        ] {
            world.insert(
                entity,
                SceneAliasCandidate {
                    reference_form_id: reference,
                    base_form_id: base,
                    linked_refs: Vec::new(),
                    location_ref_types: Vec::new(),
                },
            );
            world.insert(
                entity,
                GlobalTransform::new(Vec3::new(x, 0.0, 0.0), Quat::IDENTITY, 1.0),
            );
        }
        install_scene_quest_aliases(
            &mut world,
            [QustRecord {
                form_id: QUEST,
                aliases: vec![
                    QuestAlias {
                        alias_id: 0,
                        fill_type: Some(AliasFillType::ForcedReference(0xA0)),
                        ..Default::default()
                    },
                    QuestAlias {
                        alias_id: 1,
                        fill_type: Some(AliasFillType::UniqueActor(0xB1)),
                        closest_to_alias: Some(0),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
        );

        refresh_scene_actor_bindings(&world);
        assert_eq!(
            world
                .resource::<SceneActorBindings>()
                .resolve(QuestFormId(QUEST), 1),
            Some(near)
        );
    }

    #[test]
    fn quest_alias_near_alias_resolves_linked_ref_child() {
        let mut world = World::new();
        crate::register(&mut world);
        let source = world.spawn();
        let child = world.spawn();
        let unrelated = world.spawn();
        for (entity, candidate) in [
            (
                source,
                SceneAliasCandidate {
                    reference_form_id: 0xA0,
                    base_form_id: 0xB0,
                    linked_refs: vec![(0, 0xA1)],
                    location_ref_types: Vec::new(),
                },
            ),
            (
                child,
                SceneAliasCandidate {
                    reference_form_id: 0xA1,
                    base_form_id: 0xB1,
                    linked_refs: Vec::new(),
                    location_ref_types: Vec::new(),
                },
            ),
            (
                unrelated,
                SceneAliasCandidate {
                    reference_form_id: 0xA2,
                    base_form_id: 0xB1,
                    linked_refs: Vec::new(),
                    location_ref_types: Vec::new(),
                },
            ),
        ] {
            world.insert(entity, candidate);
        }
        install_scene_quest_aliases(
            &mut world,
            [QustRecord {
                form_id: QUEST,
                aliases: vec![
                    QuestAlias {
                        alias_id: 0,
                        fill_type: Some(AliasFillType::ForcedReference(0xA0)),
                        ..Default::default()
                    },
                    QuestAlias {
                        alias_id: 1,
                        fill_type: Some(AliasFillType::NearAlias {
                            alias_id: 0,
                            relation: 0,
                        }),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
        );

        refresh_scene_actor_bindings(&world);
        assert_eq!(
            world
                .resource::<SceneActorBindings>()
                .resolve(QuestFormId(QUEST), 1),
            Some(child)
        );
    }

    #[test]
    fn quest_aliases_exclude_dead_actors_unless_allowed() {
        let mut world = World::new();
        crate::register(&mut world);
        world.register::<Dead>();
        let actor = world.spawn();
        world.insert(
            actor,
            SceneAliasCandidate {
                reference_form_id: 0xA1,
                base_form_id: 0xB1,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        world.insert(actor, Dead);
        install_scene_quest_aliases(
            &mut world,
            [
                QustRecord {
                    form_id: 0x100,
                    aliases: vec![QuestAlias {
                        alias_id: 1,
                        fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                QustRecord {
                    form_id: 0x200,
                    aliases: vec![QuestAlias {
                        alias_id: 1,
                        fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                        flags: byroredux_plugin::esm::records::AliasFlags(ALIAS_FLAG_ALLOW_DEAD),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
        );

        refresh_scene_actor_bindings(&world);
        let bindings = world.resource::<SceneActorBindings>();
        assert_eq!(bindings.resolve(QuestFormId(0x100), 1), None);
        assert_eq!(bindings.resolve(QuestFormId(0x200), 1), Some(actor));
    }

    #[test]
    fn quest_alias_injections_are_idempotent_and_clear_transient_factions() {
        let mut world = World::new();
        crate::register(&mut world);
        let actor = world.spawn();
        world.insert(
            actor,
            SceneAliasCandidate {
                reference_form_id: 0xA1,
                base_form_id: 0xB1,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        world.insert(actor, FactionRanks::from_pairs([(0xF1, 3)]));
        let injected_alias = QuestAlias {
            alias_id: 1,
            fill_type: Some(AliasFillType::ForcedReference(0xA1)),
            injected: byroredux_plugin::esm::records::AliasInjectedData {
                factions: vec![0xF1, 0xF2],
                inventory: vec![(0xC1, 2), (0xC2, 1)],
                ..Default::default()
            },
            ..Default::default()
        };
        install_scene_quest_aliases(
            &mut world,
            [QustRecord {
                form_id: QUEST,
                aliases: vec![injected_alias],
                ..Default::default()
            }],
        );

        refresh_scene_actor_bindings(&world);
        assert_eq!(
            world.get::<FactionRanks>(actor).unwrap().rank(0xF1),
            Some(3)
        );
        assert_eq!(
            world.get::<FactionRanks>(actor).unwrap().rank(0xF2),
            Some(0)
        );
        assert_eq!(world.get::<Inventory>(actor).unwrap().items.len(), 2);
        assert_eq!(
            world
                .get::<QuestAliasInjectedOverlays>(actor)
                .unwrap()
                .0
                .len(),
            1
        );
        assert_eq!(
            world
                .get::<QuestAliasRuntimeOverlays>(actor)
                .unwrap()
                .0
                .len(),
            1
        );

        mark_scene_actor_bindings_dirty(&world);
        refresh_scene_actor_bindings(&world);
        assert_eq!(
            world.get::<Inventory>(actor).unwrap().items.len(),
            2,
            "permanent CNTO grants must not duplicate on a dirty refresh"
        );

        install_scene_quest_aliases(
            &mut world,
            [QustRecord {
                form_id: QUEST,
                aliases: vec![QuestAlias {
                    alias_id: 1,
                    fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                    ..Default::default()
                }],
                ..Default::default()
            }],
        );
        refresh_scene_actor_bindings(&world);
        assert_eq!(
            world.get::<FactionRanks>(actor).unwrap().rank(0xF1),
            Some(3)
        );
        assert_eq!(world.get::<FactionRanks>(actor).unwrap().rank(0xF2), None);
        assert!(world.get::<QuestAliasInjectedOverlays>(actor).is_none());
        assert!(world.get::<QuestAliasRuntimeOverlays>(actor).is_some());
        assert_eq!(
            world.get::<Inventory>(actor).unwrap().items.len(),
            2,
            "CNTO grants are permanent when an alias clears"
        );
    }

    #[test]
    fn quest_aliases_fill_only_while_the_quest_is_running() {
        let mut world = World::new();
        crate::register(&mut world);
        world.insert_resource(QuestStageState::default());
        let actor = world.spawn();
        world.insert(
            actor,
            SceneAliasCandidate {
                reference_form_id: 0xA1,
                base_form_id: 0xB1,
                linked_refs: Vec::new(),
                location_ref_types: Vec::new(),
            },
        );
        install_scene_quest_aliases(
            &mut world,
            [QustRecord {
                form_id: QUEST,
                aliases: vec![QuestAlias {
                    alias_id: 1,
                    fill_type: Some(AliasFillType::ForcedReference(0xA1)),
                    ..Default::default()
                }],
                ..Default::default()
            }],
        );

        assert_eq!(refresh_scene_actor_bindings(&world), 0);
        assert_eq!(
            world
                .resource::<SceneActorBindings>()
                .resolve(QuestFormId(QUEST), 1),
            None
        );

        world
            .resource_mut::<QuestStageState>()
            .start_quest(QuestFormId(QUEST), None);
        mark_scene_actor_bindings_dirty(&world);
        assert_eq!(refresh_scene_actor_bindings(&world), 1);
        assert_eq!(
            world
                .resource::<SceneActorBindings>()
                .resolve(QuestFormId(QUEST), 1),
            Some(actor)
        );

        world
            .resource_mut::<QuestStageState>()
            .stop(QuestFormId(QUEST));
        mark_scene_actor_bindings_dirty(&world);
        assert_eq!(refresh_scene_actor_bindings(&world), 0);
        assert_eq!(
            world
                .resource::<SceneActorBindings>()
                .resolve(QuestFormId(QUEST), 1),
            None
        );
        assert!(world.get::<QuestAliasRuntimeOverlays>(actor).is_none());
    }

    fn fragment(event: SceneFragmentEvent, name: &str) -> SceneScriptFragment {
        SceneScriptFragment {
            event,
            script_name: "SF_TestScene_00000100".to_owned(),
            fragment_name: name.to_owned(),
        }
    }

    fn base_scene() -> ScenRecord {
        ScenRecord {
            form_id: SCENE,
            editor_id: "TestScene".to_owned(),
            quest_form_id: Some(QUEST),
            phases: vec![ScenePhase {
                name: "Opening".to_owned(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn setup(scene: ScenRecord) -> (World, EntityId, EntityId) {
        let mut world = World::new();
        crate::register(&mut world);
        let player = world.spawn();
        world.insert_resource(PlayerEntity(player));
        world.insert_resource(QuestStageState::default());
        assert_eq!(install_scene_records(&mut world, [scene]), 1);
        let entity = world
            .resource::<SceneRegistry>()
            .scene_entity(SCENE)
            .expect("scene entity");
        (world, entity, player)
    }

    fn state(world: &World, entity: EntityId) -> ScenePlayer {
        world
            .get::<ScenePlayer>(entity)
            .expect("scene player")
            .clone()
    }

    fn events(world: &World, entity: EntityId) -> Vec<SceneEvent> {
        world
            .get::<SceneEventBatch>(entity)
            .map(|batch| batch.0.clone())
            .unwrap_or_default()
    }

    #[test]
    fn install_is_idempotent_and_refreshes_the_definition() {
        let scene = base_scene();
        let (mut world, entity, _) = setup(scene.clone());
        let mut replacement = scene;
        replacement.editor_id = "Refreshed".to_owned();

        assert_eq!(install_scene_records(&mut world, [replacement]), 1);
        let registry = world.resource::<SceneRegistry>();
        assert_eq!(registry.len(), 1);
        assert_eq!(registry.scene_entity(SCENE), Some(entity));
        assert_eq!(registry.definition(SCENE).unwrap().editor_id, "Refreshed");
    }

    #[test]
    fn explicit_start_enters_phase_starts_action_and_dispatches_fragments() {
        let mut scene = base_scene();
        scene.actions.push(SceneAction {
            action_type: SceneActionType::Dialogue,
            actor_id: 7,
            index: 12,
            topic_form_id: Some(0x300),
            ..Default::default()
        });
        scene.fragments = vec![
            fragment(SceneFragmentEvent::Begin, "Fragment_0"),
            fragment(
                SceneFragmentEvent::PhaseStart { phase_index: 0 },
                "Fragment_1",
            ),
        ];
        let (mut world, entity, actor) = setup(scene);
        world
            .resource_mut::<SceneActorBindings>()
            .bind(QuestFormId(QUEST), 7, actor);
        world.insert(entity, SceneStartRequest);

        scene_playback_system(&world, 0.016);

        let player = state(&world, entity);
        assert_eq!(player.state, ScenePlaybackState::Playing);
        assert_eq!(player.current_phase, 0);
        assert_eq!(player.active_actions.len(), 1);
        assert!(events(&world, entity).contains(&SceneEvent::ActionStarted {
            action_index: 12,
            action_type: SceneActionType::Dialogue,
            actor_alias: 7,
            actor_entity: Some(actor),
            topic_form_id: Some(0x300),
            packages: Vec::new(),
        }));
        let invocations = world
            .get::<SceneFragmentInvocationBatch>(entity)
            .expect("fragment invocations");
        assert_eq!(invocations.0.len(), 2);
        assert_eq!(invocations.0[0].event, SceneFragmentEvent::Begin);
        assert_eq!(
            invocations.0[1].event,
            SceneFragmentEvent::PhaseStart { phase_index: 0 }
        );
    }

    #[test]
    fn timer_and_external_completion_preserve_overlapping_action_lifetimes() {
        let mut scene = base_scene();
        scene.phases.push(ScenePhase {
            name: "Second".to_owned(),
            ..Default::default()
        });
        scene.actions = vec![
            SceneAction {
                action_type: SceneActionType::Package,
                actor_id: 4,
                index: 20,
                start_phase: 0,
                end_phase: 1,
                packages: vec![0x400],
                ..Default::default()
            },
            SceneAction {
                action_type: SceneActionType::Timer,
                actor_id: -1,
                index: 21,
                start_phase: 0,
                end_phase: 0,
                timer_seconds: Some(1.0),
                ..Default::default()
            },
        ];
        let (mut world, entity, _) = setup(scene);
        world.insert(entity, SceneStartRequest);
        scene_playback_system(&world, 0.0);
        assert_eq!(state(&world, entity).active_actions.len(), 2);

        scene_playback_system(&world, 0.5);
        assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);
        scene_playback_system(&world, 0.5);
        let after_timer = state(&world, entity);
        assert_eq!(after_timer.state, ScenePlaybackState::WaitingForPhaseStart);
        assert_eq!(after_timer.current_phase, 1);
        assert_eq!(after_timer.active_actions.len(), 1);
        assert_eq!(after_timer.active_actions[0].action_index, 20);

        scene_playback_system(&world, 0.0);
        assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);
        world.insert(entity, SceneActionCompletionBatch(vec![20]));
        scene_playback_system(&world, 0.0);

        let finished = state(&world, entity);
        assert_eq!(finished.state, ScenePlaybackState::Finished);
        assert!(finished.active_actions.is_empty());
        let final_events = events(&world, entity);
        assert!(final_events.contains(&SceneEvent::ActionCompleted { action_index: 20 }));
        assert!(final_events.contains(&SceneEvent::ActionStopped {
            action_index: 20,
            completed: true,
        }));
        assert!(final_events.contains(&SceneEvent::SceneFinished));
    }

    #[test]
    fn completion_conditions_can_release_an_unfinished_package_action() {
        let mut scene = base_scene();
        scene.phases[0].completion_conditions.push(Condition {
            function_index: 58, // GetStage
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(10.0),
            param_1: QUEST,
            run_on: RunOn::Subject,
            ..Default::default()
        });
        scene.actions.push(SceneAction {
            action_type: SceneActionType::Package,
            actor_id: 4,
            index: 20,
            start_phase: 0,
            end_phase: 0,
            packages: vec![0x400],
            ..Default::default()
        });
        let (mut world, entity, _) = setup(scene);
        world.insert(entity, SceneStartRequest);
        scene_playback_system(&world, 0.0);
        scene_playback_system(&world, 0.0);
        assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);

        world
            .resource_mut::<QuestStageState>()
            .set_stage(QuestFormId(QUEST), 10);
        scene_playback_system(&world, 0.0);

        assert_eq!(state(&world, entity).state, ScenePlaybackState::Finished);
        assert!(events(&world, entity).contains(&SceneEvent::ActionStopped {
            action_index: 20,
            completed: false,
        }));
    }

    #[test]
    fn empty_phase_with_completion_conditions_waits_for_them() {
        let mut scene = base_scene();
        scene.phases[0].completion_conditions.push(Condition {
            function_index: 58, // GetStage
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(10.0),
            param_1: QUEST,
            run_on: RunOn::Subject,
            ..Default::default()
        });
        let (mut world, entity, _) = setup(scene);
        world.insert(entity, SceneStartRequest);
        scene_playback_system(&world, 0.0);
        scene_playback_system(&world, 0.0);
        assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);

        world
            .resource_mut::<QuestStageState>()
            .set_stage(QuestFormId(QUEST), 10);
        scene_playback_system(&world, 0.0);
        assert_eq!(state(&world, entity).state, ScenePlaybackState::Finished);
    }

    #[test]
    fn completed_action_history_drives_scene_action_completion_condition() {
        let mut scene = base_scene();
        scene.phases.push(ScenePhase {
            name: "WaitForOpeningAction".to_owned(),
            completion_conditions: vec![Condition {
                function_index: 550,
                comparator: ComparisonOp::Eq,
                comparand: ConditionValue::Literal(1.0),
                param_1: SCENE,
                param_2: 21,
                run_on: RunOn::Subject,
                ..Default::default()
            }],
            ..Default::default()
        });
        scene.actions.push(SceneAction {
            action_type: SceneActionType::Timer,
            actor_id: -1,
            index: 21,
            start_phase: 0,
            end_phase: 0,
            timer_seconds: Some(0.0),
            ..Default::default()
        });
        let (mut world, entity, _) = setup(scene);
        world.insert(entity, SceneStartRequest);

        scene_playback_system(&world, 0.0); // enter phase 0
        scene_playback_system(&world, 0.0); // complete action 21
        scene_playback_system(&world, 0.0); // enter phase 1
        scene_playback_system(&world, 0.0); // CTDA observes completion history

        let finished = state(&world, entity);
        assert_eq!(finished.state, ScenePlaybackState::Finished);
        assert!(finished.completed_actions.contains(&21));

        world.insert(entity, SceneStartRequest);
        scene_playback_system(&world, 0.0);
        assert!(
            state(&world, entity).completed_actions.is_empty(),
            "a new run must not inherit completed actions from the old run"
        );
    }

    #[test]
    fn phase_start_conditions_are_retried_until_true() {
        let mut scene = base_scene();
        scene.phases[0].start_conditions.push(Condition {
            function_index: 58,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(10.0),
            param_1: QUEST,
            run_on: RunOn::Subject,
            ..Default::default()
        });
        let (mut world, entity, _) = setup(scene);
        world.insert(entity, SceneStartRequest);

        scene_playback_system(&world, 0.0);
        assert_eq!(
            state(&world, entity).state,
            ScenePlaybackState::WaitingForPhaseStart
        );

        world
            .resource_mut::<QuestStageState>()
            .set_stage(QuestFormId(QUEST), 10);
        scene_playback_system(&world, 0.0);
        assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);
    }

    #[test]
    fn quest_start_event_auto_starts_flagged_scene() {
        let mut scene = base_scene();
        scene.flags = SCENE_BEGIN_ON_QUEST_START;
        let (mut world, entity, _) = setup(scene);
        let sink = world.spawn();
        world.insert(
            sink,
            QuestStageAdvancedBatch(vec![QuestStageAdvanced {
                quest: QuestFormId(QUEST),
                previous_stage: 0,
                new_stage: 10,
            }]),
        );

        scene_playback_system(&world, 0.0);

        assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);
        assert!(events(&world, entity).contains(&SceneEvent::SceneStarted));
    }

    #[test]
    fn engine_root_quest_bootstrap_auto_starts_flagged_scene() {
        let mut scene = base_scene();
        scene.flags = SCENE_BEGIN_ON_QUEST_START;
        let (mut world, entity, _) = setup(scene);
        assert!(install_engine_start_quest(
            &mut world,
            QuestFormId(QUEST),
            Some(10),
        ));

        quest_startup_system(&world, 0.0);
        scene_playback_system(&world, 0.0);

        assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);
        assert!(events(&world, entity).contains(&SceneEvent::SceneStarted));
        assert_eq!(
            world
                .resource::<QuestStageState>()
                .get_stage(QuestFormId(QUEST)),
            10
        );
    }

    #[test]
    fn phase_condition_resolves_run_on_quest_alias() {
        use byroredux_core::character::CharacterLevel;

        let mut scene = base_scene();
        scene.phases[0].start_conditions.push(Condition {
            function_index: 80,
            comparator: ComparisonOp::Eq,
            comparand: ConditionValue::Literal(12.0),
            run_on: RunOn::QuestAlias,
            extra_data_id: 7,
            ..Default::default()
        });
        let (mut world, entity, actor) = setup(scene);
        world.register::<CharacterLevel>();
        world.insert(actor, CharacterLevel { level: 12, xp: 0 });
        world
            .resource_mut::<SceneActorBindings>()
            .bind(QuestFormId(QUEST), 7, actor);
        world.insert(entity, SceneStartRequest);

        scene_playback_system(&world, 0.0);

        assert_eq!(state(&world, entity).state, ScenePlaybackState::Playing);
    }

    #[test]
    fn stop_request_stops_active_actions_and_dispatches_end_fragment() {
        let mut scene = base_scene();
        scene.actions.push(SceneAction {
            action_type: SceneActionType::Package,
            index: 1,
            end_phase: 0,
            ..Default::default()
        });
        scene.fragments = vec![fragment(SceneFragmentEvent::End, "Fragment_End")];
        let (mut world, entity, _) = setup(scene);
        world.insert(entity, SceneStartRequest);
        scene_playback_system(&world, 0.0);
        world.insert(entity, SceneStopRequest);

        scene_playback_system(&world, 0.0);

        assert_eq!(state(&world, entity).state, ScenePlaybackState::Stopped);
        assert!(events(&world, entity).contains(&SceneEvent::ActionStopped {
            action_index: 1,
            completed: false,
        }));
        assert_eq!(
            world
                .get::<SceneFragmentInvocationBatch>(entity)
                .expect("end fragment")
                .0[0]
                .event,
            SceneFragmentEvent::End
        );
    }
}
