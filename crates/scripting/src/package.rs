//! Skyrim+ custom PACK execution for scene package actions.
//!
//! `SCEN` owns action lifetime; this module resolves each package stack to
//! the first eligible concrete PACK, follows its PKCU template, and executes
//! the completion-bearing procedure leaf. Travel-family leaves get a small
//! ECS-native move-to runtime using authored REFR positions. Interaction
//! leaves expose presentation events and use a conservative fallback timer
//! until animation/combat/activation adapters acknowledge them explicitly.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use byroredux_core::ecs::components::Transform;
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_core::math::Vec3;
use byroredux_plugin::esm::records::{
    PackDataInput, PackDataTargetKind, PackDataValue, PackLocationTarget, PackProcedure,
    PackRecord, SceneActionType,
};

use crate::condition::{evaluate, resolve_entity_by_global_form_id, ConditionContext};
use crate::papyrus_demo::PlayerEntity;
use crate::quest_stages::QuestFormId;
use crate::scene::{
    SceneActionCompletionBatch, SceneActorBindings, SceneEvent, SceneEventBatch, ScenePlayer,
};

const MOVE_SPEED_UNITS_PER_SECOND: f32 = 140.0;
const DEFAULT_ARRIVAL_RADIUS: f32 = 40.0;
const INTERACTION_FALLBACK_SECONDS: f32 = 0.75;
/// #2287 (SCR-D6-NEW5-01) — maximum time a `MoveTo` action tolerates being
/// unable to resolve the acting actor's live `Transform` before giving up
/// and reporting itself complete. Without this bound, an actor despawned
/// mid-travel (e.g. exterior cell-streaming unloading its cell while the
/// player is elsewhere) leaves the action permanently stuck: `tick_command`
/// returns `false` forever, so a scene phase whose only exit gate is
/// `ending_actions_complete` never advances. Mirrors the sibling
/// `TimedInteraction` leaf's `INTERACTION_FALLBACK_SECONDS` bound.
const MOVE_STALL_TIMEOUT_SECONDS: f32 = 5.0;

/// Parsed PACK definitions keyed in global load-order FormID space.
#[derive(Debug, Clone, Default)]
pub struct PackageRegistry {
    packages: HashMap<u32, Arc<PackRecord>>,
}

impl Resource for PackageRegistry {}

impl PackageRegistry {
    pub fn from_records(records: impl IntoIterator<Item = PackRecord>) -> Self {
        let mut registry = Self::default();
        for record in records {
            registry.packages.insert(record.form_id, Arc::new(record));
        }
        registry
    }

    pub fn package(&self, form_id: u32) -> Option<&PackRecord> {
        self.packages.get(&form_id).map(Arc::as_ref)
    }

    pub fn len(&self) -> usize {
        self.packages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }
}

/// Authored placed-reference positions used even when invisible XMarkers are
/// not spawned into the render world.
#[derive(Debug, Clone, Default)]
pub struct PackageTargetRegistry {
    positions: HashMap<u32, Vec3>,
}

impl Resource for PackageTargetRegistry {}

impl PackageTargetRegistry {
    pub fn position(&self, reference_form_id: u32) -> Option<Vec3> {
        self.positions.get(&reference_form_id).copied()
    }

    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}

/// Resolved execution strategy for one scene package action.
#[derive(Debug, Clone, PartialEq)]
pub enum ScenePackageCommand {
    MoveTo {
        procedure_type: String,
        destination: Vec3,
        arrival_radius: f32,
        /// Seconds this action has spent unable to resolve the actor's
        /// live `Transform`. See [`MOVE_STALL_TIMEOUT_SECONDS`].
        stall_seconds: f32,
    },
    TimedInteraction {
        procedure_type: String,
        remaining_seconds: f32,
        target: Option<EntityId>,
        dispatched: bool,
    },
    AwaitExternal {
        procedure_type: String,
    },
}

impl ScenePackageCommand {
    pub fn procedure_type(&self) -> &str {
        match self {
            Self::MoveTo { procedure_type, .. }
            | Self::TimedInteraction { procedure_type, .. }
            | Self::AwaitExternal { procedure_type } => procedure_type,
        }
    }
}

/// Mutable execution state for one active scene package action.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveScenePackageAction {
    pub scene_form_id: u32,
    pub action_index: u32,
    pub actor: EntityId,
    /// Original SCEN package stack, retained so EvaluatePackage can rerun
    /// authored condition selection instead of blindly restarting the last
    /// winner.
    pub package_candidates: Vec<u32>,
    pub package_form_id: u32,
    pub template_form_id: u32,
    pub command: ScenePackageCommand,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScenePackagePlayback {
    pub active_actions: Vec<ActiveScenePackageAction>,
}

impl Component for ScenePackagePlayback {
    type Storage = SparseSetStorage<Self>;
}

/// Presentation/diagnostic boundary for animation, combat, activation, and
/// future navmesh-backed AI adapters.
#[derive(Debug, Clone, PartialEq)]
pub enum ScenePackageEvent {
    Started(ActiveScenePackageAction),
    Reevaluated(ActiveScenePackageAction),
    Finished {
        action_index: u32,
        package_form_id: u32,
    },
    Stopped {
        action_index: u32,
        package_form_id: u32,
    },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScenePackageEventBatch(pub Vec<ScenePackageEvent>);

impl Component for ScenePackageEventBatch {
    type Storage = SparseSetStorage<Self>;
}

/// Completion ingress for a richer gameplay presenter/executor. Action
/// indices are converted to the scene runtime's canonical completion batch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScenePackageCompletionBatch(pub Vec<u32>);

impl Component for ScenePackageCompletionBatch {
    type Storage = SparseSetStorage<Self>;
}

/// One-shot Actor.EvaluatePackage ingress. The package system drains this on
/// its next tick and replans every active scene package owned by the actor.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EvaluatePackageRequest;

impl Component for EvaluatePackageRequest {
    type Storage = SparseSetStorage<Self>;
}

pub fn register(world: &mut World) {
    world.register::<ScenePackagePlayback>();
    world.register::<ScenePackageEventBatch>();
    world.register::<ScenePackageCompletionBatch>();
    world.register::<EvaluatePackageRequest>();
    if world.try_resource::<PackageRegistry>().is_none() {
        world.insert_resource(PackageRegistry::default());
    }
    if world.try_resource::<PackageTargetRegistry>().is_none() {
        world.insert_resource(PackageTargetRegistry::default());
    }
}

pub fn install_package_records(
    world: &mut World,
    records: impl IntoIterator<Item = PackRecord>,
) -> usize {
    if world.try_resource::<PackageRegistry>().is_none() {
        register(world);
    }
    let records: Vec<PackRecord> = records.into_iter().collect();
    let count = records.len();
    let mut registry = world.resource_mut::<PackageRegistry>();
    for record in records {
        registry.packages.insert(record.form_id, Arc::new(record));
    }
    count
}

/// Install already-converted Y-up reference positions.
pub fn install_package_target_positions(
    world: &mut World,
    positions: impl IntoIterator<Item = (u32, Vec3)>,
) -> usize {
    if world.try_resource::<PackageTargetRegistry>().is_none() {
        register(world);
    }
    let positions: Vec<(u32, Vec3)> = positions.into_iter().collect();
    let count = positions.len();
    let mut registry = world.resource_mut::<PackageTargetRegistry>();
    registry.positions.extend(positions);
    count
}

fn snapshot<T: Component + Clone>(world: &World) -> HashMap<EntityId, T> {
    world
        .query::<T>()
        .map(|query| {
            query
                .iter()
                .map(|(entity, component)| (entity, component.clone()))
                .collect()
        })
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

fn append_scene_completions(world: &World, pending: HashMap<EntityId, Vec<u32>>) {
    let Some(mut batches) = world.query_mut::<SceneActionCompletionBatch>() else {
        return;
    };
    for (entity, action_indices) in pending {
        if action_indices.is_empty() {
            continue;
        }
        if let Some(batch) = batches.get_mut(entity) {
            for action_index in action_indices {
                if !batch.0.contains(&action_index) {
                    batch.0.push(action_index);
                }
            }
        } else {
            batches.insert(entity, SceneActionCompletionBatch(action_indices));
        }
    }
}

fn select_package(
    package_ids: &[u32],
    packages: &HashMap<u32, Arc<PackRecord>>,
    world: &World,
    actor: EntityId,
    player: Option<EntityId>,
) -> Option<Arc<PackRecord>> {
    package_ids.iter().find_map(|form_id| {
        let package = packages.get(form_id)?;
        let mut context = ConditionContext::for_subject(actor);
        context.target = player;
        if let Some(quest_form_id) = package.owner_quest_form_id {
            context = context.with_quest(QuestFormId(quest_form_id));
        }
        evaluate(&package.conditions, world, &context).then(|| package.clone())
    })
}

fn procedure_inputs<'a>(
    procedure: &PackProcedure,
    package: &'a PackRecord,
) -> Vec<&'a PackDataInput> {
    procedure
        .data_input_indexes
        .iter()
        .filter_map(|index| {
            package
                .data_inputs
                .iter()
                .find(|input| input.index == *index)
        })
        .collect()
}

fn input_arrival_radius(input: &PackDataInput) -> Option<f32> {
    match input.value {
        PackDataValue::Location(location) if location.radius > 0 => Some(location.radius as f32),
        PackDataValue::Target(target) if target.count_or_distance > 0 => {
            Some(target.count_or_distance as f32)
        }
        _ => None,
    }
}

fn entity_position(world: &World, entity: EntityId) -> Option<Vec3> {
    world
        .get::<Transform>(entity)
        .map(|transform| transform.translation)
}

fn reference_position(
    world: &World,
    targets: &PackageTargetRegistry,
    reference_form_id: u32,
    player: Option<EntityId>,
) -> Option<Vec3> {
    targets.position(reference_form_id).or_else(|| {
        let entity = if reference_form_id == 0x14 {
            player
        } else {
            resolve_entity_by_global_form_id(world, reference_form_id)
        }?;
        entity_position(world, entity)
    })
}

fn alias_position(world: &World, package: &PackRecord, alias_id: i32) -> Option<Vec3> {
    let quest = QuestFormId(package.owner_quest_form_id?);
    let entity = world
        .try_resource::<SceneActorBindings>()?
        .resolve(quest, alias_id)?;
    entity_position(world, entity)
}

fn input_destination(
    input: &PackDataInput,
    package: &PackRecord,
    targets: &PackageTargetRegistry,
    world: &World,
    actor: EntityId,
    player: Option<EntityId>,
) -> Option<Vec3> {
    match input.value {
        PackDataValue::Location(location) => match location.target {
            PackLocationTarget::NearReference(form_id) => {
                reference_position(world, targets, form_id, player)
            }
            // Type 2 is Near Package Start Location. Type 3 is Near Editor
            // Location; without a separate editor-location payload the live
            // actor position is the safest faithful center for both.
            PackLocationTarget::Other(_) if matches!(location.location_type, 2 | 3) => {
                entity_position(world, actor)
            }
            // Skyrim types 8/9 address an alias/reference in the owning
            // quest's package namespace.
            PackLocationTarget::Other(raw) if matches!(location.location_type, 8 | 9) => {
                alias_position(world, package, raw as i32)
            }
            _ => None,
        },
        PackDataValue::Target(target) => match target.target {
            PackDataTargetKind::SpecificReference(form_id)
            | PackDataTargetKind::LinkedReference(form_id) => {
                reference_position(world, targets, form_id, player)
            }
            PackDataTargetKind::ReferenceAlias(alias_id) => {
                alias_position(world, package, alias_id)
            }
            PackDataTargetKind::SelfTarget => entity_position(world, actor),
            _ => None,
        },
        _ => None,
    }
}

fn input_target_entity(
    input: &PackDataInput,
    package: &PackRecord,
    world: &World,
    actor: EntityId,
    player: Option<EntityId>,
) -> Option<EntityId> {
    let PackDataValue::Target(target) = &input.value else {
        return None;
    };
    match target.target {
        PackDataTargetKind::SpecificReference(form_id)
        | PackDataTargetKind::LinkedReference(form_id) => {
            if form_id == 0x14 {
                player
            } else {
                resolve_entity_by_global_form_id(world, form_id)
            }
        }
        PackDataTargetKind::ReferenceAlias(alias_id) => {
            let quest = QuestFormId(package.owner_quest_form_id?);
            world
                .try_resource::<SceneActorBindings>()?
                .resolve(quest, alias_id)
        }
        PackDataTargetKind::SelfTarget => Some(actor),
        _ => None,
    }
}

fn resolve_command(
    package: &PackRecord,
    template: &PackRecord,
    targets: &PackageTargetRegistry,
    world: &World,
    actor: EntityId,
    player: Option<EntityId>,
) -> ScenePackageCommand {
    let procedure = template
        .procedures
        .iter()
        .find(|procedure| procedure.success_completes_package())
        .or_else(|| template.procedures.first());
    let Some(procedure) = procedure else {
        return ScenePackageCommand::AwaitExternal {
            procedure_type: "Unknown".to_owned(),
        };
    };
    let inputs = procedure_inputs(procedure, package);
    let procedure_type = procedure.procedure_type.clone();
    if matches!(
        procedure_type.as_str(),
        "Travel" | "Patrol" | "Escort" | "FollowTo"
    ) {
        // Escort/FollowTo inputs often mention the escorted/followed actor
        // before the actual destination. Prefer an authored Location for
        // those procedures, then fall back to any resolvable selector.
        let preferred_location = matches!(procedure_type.as_str(), "Escort" | "FollowTo")
            .then(|| {
                inputs.iter().find_map(|input| {
                    matches!(input.value, PackDataValue::Location(_))
                        .then(|| {
                            input_destination(input, package, targets, world, actor, player).map(
                                |destination| {
                                    (
                                        destination,
                                        input_arrival_radius(input)
                                            .unwrap_or(DEFAULT_ARRIVAL_RADIUS),
                                    )
                                },
                            )
                        })
                        .flatten()
                })
            })
            .flatten();
        let destination = preferred_location.or_else(|| {
            inputs.iter().find_map(|input| {
                input_destination(input, package, targets, world, actor, player).map(
                    |destination| {
                        (
                            destination,
                            input_arrival_radius(input).unwrap_or(DEFAULT_ARRIVAL_RADIUS),
                        )
                    },
                )
            })
        });
        if let Some((destination, radius)) = destination {
            return ScenePackageCommand::MoveTo {
                procedure_type,
                destination,
                arrival_radius: radius.max(DEFAULT_ARRIVAL_RADIUS),
                stall_seconds: 0.0,
            };
        }
        return ScenePackageCommand::AwaitExternal { procedure_type };
    }
    if matches!(
        procedure_type.as_str(),
        "Acquire" | "Activate" | "Shout" | "Sit" | "UseIdleMarker" | "UseWeapon"
    ) {
        let target = inputs
            .iter()
            .find_map(|input| input_target_entity(input, package, world, actor, player));
        return ScenePackageCommand::TimedInteraction {
            procedure_type,
            remaining_seconds: INTERACTION_FALLBACK_SECONDS,
            target,
            dispatched: false,
        };
    }
    ScenePackageCommand::AwaitExternal { procedure_type }
}

/// #2287 (SCR-D6-NEW5-01) — bump a `MoveTo` action's stall counter and
/// report whether it has exceeded [`MOVE_STALL_TIMEOUT_SECONDS`]. Shared by
/// both of `tick_command`'s `MoveTo` early-return paths (no `Transform`
/// storage registered at all vs. the specific actor entity missing one —
/// both mean "can't resolve the actor's position right now", most often
/// because the entity was despawned mid-travel). Returning `true` reports
/// the action complete so the caller removes it and the scene phase can
/// still advance, instead of retrying forever.
fn move_stalled_past_timeout(
    stall_seconds: &mut f32,
    dt: f32,
    action_index: u32,
    actor: EntityId,
) -> bool {
    *stall_seconds += dt.max(0.0);
    if *stall_seconds > MOVE_STALL_TIMEOUT_SECONDS {
        log::warn!(
            "scene package MoveTo action {action_index} for actor {actor:?} gave up after \
             {MOVE_STALL_TIMEOUT_SECONDS}s unable to resolve a live Transform \
             (likely despawned mid-travel)"
        );
        true
    } else {
        false
    }
}

fn tick_command(world: &World, action: &mut ActiveScenePackageAction, dt: f32) -> bool {
    match &mut action.command {
        ScenePackageCommand::TimedInteraction {
            procedure_type,
            remaining_seconds,
            target,
            dispatched,
        } => {
            if !*dispatched {
                if procedure_type == "Activate" {
                    if let (Some(target), Some(mut events)) =
                        (*target, world.query_mut::<crate::events::ActivateEvent>())
                    {
                        events.insert(
                            target,
                            crate::events::ActivateEvent {
                                activator: action.actor,
                            },
                        );
                    }
                }
                *dispatched = true;
            }
            *remaining_seconds -= dt.max(0.0);
            *remaining_seconds <= 0.0
        }
        ScenePackageCommand::MoveTo {
            destination,
            arrival_radius,
            stall_seconds,
            ..
        } => {
            let Some(mut transforms) = world.query_mut::<Transform>() else {
                return move_stalled_past_timeout(
                    stall_seconds,
                    dt,
                    action.action_index,
                    action.actor,
                );
            };
            let Some(transform) = transforms.get_mut(action.actor) else {
                return move_stalled_past_timeout(
                    stall_seconds,
                    dt,
                    action.action_index,
                    action.actor,
                );
            };
            *stall_seconds = 0.0;
            let target = Vec3::new(destination.x, transform.translation.y, destination.z);
            let delta = target - transform.translation;
            let distance = delta.length();
            if distance <= *arrival_radius {
                return true;
            }
            let step = (MOVE_SPEED_UNITS_PER_SECOND * dt.max(0.0)).min(distance);
            if step > 0.0 {
                transform.translation += delta / distance * step;
            }
            distance - step <= *arrival_radius
        }
        ScenePackageCommand::AwaitExternal { .. } => false,
    }
}

/// Resolve package ActionStarted events, execute supported leaves, and queue
/// canonical SCEN completions. Schedule after `scene_playback_system`.
pub fn scene_package_system(world: &World, dt: f32) {
    drain::<ScenePackageEventBatch>(world);
    let external = snapshot::<ScenePackageCompletionBatch>(world);
    drain::<ScenePackageCompletionBatch>(world);
    let reevaluate: HashSet<EntityId> = snapshot::<EvaluatePackageRequest>(world)
        .into_keys()
        .collect();
    drain::<EvaluatePackageRequest>(world);
    let scene_events = snapshot::<SceneEventBatch>(world);
    let mut playbacks = snapshot::<ScenePackagePlayback>(world);
    let scene_form_ids: HashMap<EntityId, u32> = world
        .query::<ScenePlayer>()
        .map(|query| {
            query
                .iter()
                .map(|(entity, player)| (entity, player.scene_form_id))
                .collect()
        })
        .unwrap_or_default();
    let packages = world
        .try_resource::<PackageRegistry>()
        .map(|registry| registry.packages.clone())
        .unwrap_or_default();
    let targets = world
        .try_resource::<PackageTargetRegistry>()
        .map(|registry| registry.clone())
        .unwrap_or_default();
    let player_entity = world.try_resource::<PlayerEntity>().map(|player| player.0);

    let mut entities: HashSet<EntityId> = playbacks.keys().copied().collect();
    entities.extend(scene_events.keys().copied());
    entities.extend(external.keys().copied());
    let mut outputs: HashMap<EntityId, Vec<ScenePackageEvent>> = HashMap::new();
    let mut scene_completions: HashMap<EntityId, Vec<u32>> = HashMap::new();

    for entity in entities {
        let mut playback = playbacks.remove(&entity).unwrap_or_default();
        let output = outputs.entry(entity).or_default();
        if let Some(batch) = scene_events.get(&entity) {
            for event in &batch.0 {
                match event {
                    SceneEvent::ActionStarted {
                        action_index,
                        action_type: SceneActionType::Package,
                        actor_entity,
                        packages: package_ids,
                        ..
                    } => {
                        if playback
                            .active_actions
                            .iter()
                            .any(|action| action.action_index == *action_index)
                        {
                            continue;
                        }
                        let Some(actor) = *actor_entity else {
                            log::warn!(
                                target: "scripting::package",
                                "Scene package action {action_index} has no bound actor; completing it"
                            );
                            scene_completions
                                .entry(entity)
                                .or_default()
                                .push(*action_index);
                            continue;
                        };
                        let Some(package) =
                            select_package(package_ids, &packages, world, actor, player_entity)
                        else {
                            log::warn!(
                                target: "scripting::package",
                                "Scene package action {action_index} has no eligible PACK; completing it"
                            );
                            scene_completions
                                .entry(entity)
                                .or_default()
                                .push(*action_index);
                            continue;
                        };
                        let template = package
                            .package_template_form_id
                            .and_then(|form_id| packages.get(&form_id).cloned())
                            .unwrap_or_else(|| package.clone());
                        let active = ActiveScenePackageAction {
                            scene_form_id: scene_form_ids.get(&entity).copied().unwrap_or_default(),
                            action_index: *action_index,
                            actor,
                            package_candidates: package_ids.clone(),
                            package_form_id: package.form_id,
                            template_form_id: template.form_id,
                            command: resolve_command(
                                &package,
                                &template,
                                &targets,
                                world,
                                actor,
                                player_entity,
                            ),
                        };
                        output.push(ScenePackageEvent::Started(active.clone()));
                        playback.active_actions.push(active);
                    }
                    SceneEvent::ActionStopped { action_index, .. } => {
                        if let Some(position) = playback
                            .active_actions
                            .iter()
                            .position(|action| action.action_index == *action_index)
                        {
                            let action = playback.active_actions.remove(position);
                            output.push(ScenePackageEvent::Stopped {
                                action_index: *action_index,
                                package_form_id: action.package_form_id,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        for action in &mut playback.active_actions {
            if !reevaluate.contains(&action.actor) {
                continue;
            }
            let Some(package) = select_package(
                &action.package_candidates,
                &packages,
                world,
                action.actor,
                player_entity,
            ) else {
                log::debug!(
                    target: "scripting::package",
                    "EvaluatePackage found no eligible SCEN package for actor {:?}",
                    action.actor
                );
                continue;
            };
            let template = package
                .package_template_form_id
                .and_then(|form_id| packages.get(&form_id).cloned())
                .unwrap_or_else(|| package.clone());
            action.package_form_id = package.form_id;
            action.template_form_id = template.form_id;
            action.command = resolve_command(
                &package,
                &template,
                &targets,
                world,
                action.actor,
                player_entity,
            );
            output.push(ScenePackageEvent::Reevaluated(action.clone()));
        }

        let external: HashSet<u32> = external
            .get(&entity)
            .map(|batch| batch.0.iter().copied().collect())
            .unwrap_or_default();
        let mut completed = Vec::new();
        for action in &mut playback.active_actions {
            if external.contains(&action.action_index) || tick_command(world, action, dt) {
                completed.push(action.action_index);
            }
        }
        for action_index in completed {
            if let Some(position) = playback
                .active_actions
                .iter()
                .position(|action| action.action_index == action_index)
            {
                let action = playback.active_actions.remove(position);
                output.push(ScenePackageEvent::Finished {
                    action_index,
                    package_form_id: action.package_form_id,
                });
                scene_completions
                    .entry(entity)
                    .or_default()
                    .push(action_index);
            }
        }
        playbacks.insert(entity, playback);
    }

    if let Some(mut query) = world.query_mut::<ScenePackagePlayback>() {
        for (entity, playback) in playbacks {
            query.insert(entity, playback);
        }
    }
    if let Some(mut query) = world.query_mut::<ScenePackageEventBatch>() {
        for (entity, events) in outputs {
            if !events.is_empty() {
                query.insert(entity, ScenePackageEventBatch(events));
            }
        }
    }
    append_scene_completions(world, scene_completions);
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::math::Vec3;
    use byroredux_plugin::esm::records::{
        PackDataTarget, PackLocation, ScenRecord, SceneAction, SceneActionType, ScenePhase,
    };

    fn package_and_template() -> (PackRecord, PackRecord) {
        let package = PackRecord {
            form_id: 0x400,
            editor_id: "TravelToMarker".to_owned(),
            package_template_form_id: Some(0x500),
            data_inputs: vec![PackDataInput {
                index: 0,
                value_type: "Location".to_owned(),
                value: PackDataValue::Location(PackLocation {
                    location_type: 0,
                    target: PackLocationTarget::NearReference(0x600),
                    radius: 40,
                }),
            }],
            ..Default::default()
        };
        let template = PackRecord {
            form_id: 0x500,
            editor_id: "Travel".to_owned(),
            procedures: vec![PackProcedure {
                procedure_type: "Travel".to_owned(),
                flags: 1,
                data_input_indexes: vec![0],
                ..Default::default()
            }],
            ..Default::default()
        };
        (package, template)
    }

    #[test]
    fn resolves_template_and_moves_actor_to_authored_marker() {
        let mut world = World::new();
        crate::register(&mut world);
        world.register::<Transform>();
        let scene = world.spawn();
        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(scene, ScenePlayer::new(0x100));
        let (package, template) = package_and_template();
        install_package_records(&mut world, [package, template]);
        install_package_target_positions(&mut world, [(0x600, Vec3::new(100.0, 0.0, 0.0))]);
        world.insert(
            scene,
            SceneEventBatch(vec![SceneEvent::ActionStarted {
                action_index: 7,
                action_type: SceneActionType::Package,
                actor_alias: 1,
                actor_entity: Some(actor),
                topic_form_id: None,
                packages: vec![0x400],
            }]),
        );

        scene_package_system(&world, 0.0);
        {
            let playback = world.get::<ScenePackagePlayback>(scene).unwrap();
            assert_eq!(playback.active_actions.len(), 1);
            assert_eq!(playback.active_actions[0].template_form_id, 0x500);
        }
        scene_package_system(&world, 1.0);

        assert!(world
            .get::<ScenePackagePlayback>(scene)
            .unwrap()
            .active_actions
            .is_empty());
        assert!(world
            .get::<SceneActionCompletionBatch>(scene)
            .is_some_and(|batch| batch.0.contains(&7)));
        assert_eq!(world.get::<Transform>(actor).unwrap().translation.x, 100.0);
    }

    /// Regression: #2287 (SCR-D6-NEW5-01) — an actor despawned mid-travel
    /// (e.g. exterior cell-streaming unloading its cell) must not stall the
    /// `MoveTo` action forever. `MOVE_STALL_TIMEOUT_SECONDS` worth of ticks
    /// with the actor's `Transform` unresolvable must still resolve the
    /// action as complete so the scene phase can advance.
    #[test]
    fn move_to_gives_up_after_actor_despawns_mid_travel() {
        let mut world = World::new();
        crate::register(&mut world);
        world.register::<Transform>();
        let scene = world.spawn();
        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(scene, ScenePlayer::new(0x100));
        let (package, template) = package_and_template();
        install_package_records(&mut world, [package, template]);
        install_package_target_positions(&mut world, [(0x600, Vec3::new(100.0, 0.0, 0.0))]);
        world.insert(
            scene,
            SceneEventBatch(vec![SceneEvent::ActionStarted {
                action_index: 7,
                action_type: SceneActionType::Package,
                actor_alias: 1,
                actor_entity: Some(actor),
                topic_form_id: None,
                packages: vec![0x400],
            }]),
        );

        scene_package_system(&world, 0.0);
        {
            let playback = world.get::<ScenePackagePlayback>(scene).unwrap();
            assert_eq!(playback.active_actions.len(), 1);
        }

        // Despawn the actor mid-travel — the action's Transform lookup
        // misses every tick from here on, exactly like a cell-streaming
        // unload. MOVE_STALL_TIMEOUT_SECONDS worth of dt must still give
        // up rather than retrying forever.
        world.despawn(actor);
        for _ in 0..6 {
            scene_package_system(&world, 1.0);
        }

        assert!(
            world
                .get::<ScenePackagePlayback>(scene)
                .unwrap()
                .active_actions
                .is_empty(),
            "MoveTo action must give up once the actor is unresolvable past the stall timeout"
        );
        assert!(world
            .get::<SceneActionCompletionBatch>(scene)
            .is_some_and(|batch| batch.0.contains(&7)));
    }

    #[test]
    fn evaluate_package_reselects_and_restarts_active_scene_command() {
        let mut world = World::new();
        crate::register(&mut world);
        world.register::<Transform>();
        let scene = world.spawn();
        let actor = world.spawn();
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        world.insert(scene, ScenePlayer::new(0x100));
        let (package, template) = package_and_template();
        install_package_records(&mut world, [package, template]);
        install_package_target_positions(&mut world, [(0x600, Vec3::new(100.0, 0.0, 0.0))]);
        world.insert(
            scene,
            SceneEventBatch(vec![SceneEvent::ActionStarted {
                action_index: 7,
                action_type: SceneActionType::Package,
                actor_alias: 1,
                actor_entity: Some(actor),
                topic_form_id: None,
                packages: vec![0x400],
            }]),
        );
        scene_package_system(&world, 0.0);
        world.insert(actor, EvaluatePackageRequest);

        scene_package_system(&world, 0.0);

        let playback = world.get::<ScenePackagePlayback>(scene).unwrap();
        assert_eq!(playback.active_actions[0].package_candidates, vec![0x400]);
        assert!(matches!(
            playback.active_actions[0].command,
            ScenePackageCommand::MoveTo { .. }
        ));
        assert!(world
            .get::<ScenePackageEventBatch>(scene)
            .is_some_and(|batch| batch.0.iter().any(|event| matches!(
                event,
                ScenePackageEvent::Reevaluated(action) if action.actor == actor
            ))));
        assert!(!world.has::<EvaluatePackageRequest>(actor));
    }

    #[test]
    fn reference_alias_target_stays_external_instead_of_guessing() {
        let package = PackRecord {
            data_inputs: vec![PackDataInput {
                index: 1,
                value_type: "TargetSelector".to_owned(),
                value: PackDataValue::Target(PackDataTarget {
                    target_type: 4,
                    target: PackDataTargetKind::ReferenceAlias(3),
                    count_or_distance: 0,
                }),
            }],
            ..Default::default()
        };
        let template = PackRecord {
            procedures: vec![PackProcedure {
                procedure_type: "Travel".to_owned(),
                flags: 1,
                data_input_indexes: vec![1],
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(matches!(
            resolve_command(
                &package,
                &template,
                &PackageTargetRegistry::default(),
                &{
                    let mut world = World::new();
                    crate::register(&mut world);
                    world
                },
                EntityId::default(),
                None,
            ),
            ScenePackageCommand::AwaitExternal { .. }
        ));
    }

    #[test]
    fn scene_and_package_systems_complete_travel_end_to_end() {
        let mut world = World::new();
        crate::register(&mut world);
        world.register::<Transform>();
        let player = world.spawn();
        let actor = world.spawn();
        world.insert_resource(PlayerEntity(player));
        world.insert(actor, Transform::from_translation(Vec3::ZERO));
        let (package, template) = package_and_template();
        install_package_records(&mut world, [package, template]);
        install_package_target_positions(&mut world, [(0x600, Vec3::new(100.0, 0.0, 0.0))]);
        crate::scene::install_scene_records(
            &mut world,
            [ScenRecord {
                form_id: 0x100,
                quest_form_id: Some(0x200),
                phases: vec![ScenePhase::default()],
                actions: vec![SceneAction {
                    action_type: SceneActionType::Package,
                    actor_id: 1,
                    index: 7,
                    packages: vec![0x400],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        );
        world
            .resource_mut::<SceneActorBindings>()
            .bind(QuestFormId(0x200), 1, actor);
        let scene = world
            .resource::<crate::scene::SceneRegistry>()
            .scene_entity(0x100)
            .unwrap();
        world.insert(scene, crate::scene::SceneStartRequest);

        crate::scene::scene_playback_system(&world, 0.0);
        scene_package_system(&world, 0.0);
        scene_package_system(&world, 1.0);
        crate::scene::scene_playback_system(&world, 0.0);

        assert_eq!(
            world.get::<ScenePlayer>(scene).unwrap().state,
            crate::scene::ScenePlaybackState::Finished
        );
        assert_eq!(world.get::<Transform>(actor).unwrap().translation.x, 100.0);
    }

    #[test]
    fn activate_package_drives_two_state_vm_variable_end_to_end() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

        const GATE: u32 = 0x0009_0A05;
        let mut world = World::new();
        crate::register(&mut world);
        let mut pool = FormIdPool::new();
        let gate_id = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(GATE),
        });
        world.insert_resource(pool);
        let scene = world.spawn();
        let actor = world.spawn();
        let gate = world.spawn();
        world.insert(scene, ScenePlayer::new(0x100));
        world.insert(gate, FormIdComponent(gate_id));
        world.insert(
            gate,
            crate::TwoStateActivator {
                do_once: true,
                ..Default::default()
            },
        );
        let mut variables = crate::ScriptVariables::default();
        variables.set_by_name("::isOpen_var", 0.0);
        world.insert(gate, variables);
        install_package_records(
            &mut world,
            [
                PackRecord {
                    form_id: 0x400,
                    package_template_form_id: Some(0x500),
                    data_inputs: vec![PackDataInput {
                        index: 0,
                        value_type: "TargetSelector".to_owned(),
                        value: PackDataValue::Target(PackDataTarget {
                            target_type: 0,
                            target: PackDataTargetKind::SpecificReference(GATE),
                            count_or_distance: 0,
                        }),
                    }],
                    ..Default::default()
                },
                PackRecord {
                    form_id: 0x500,
                    procedures: vec![PackProcedure {
                        procedure_type: "Activate".to_owned(),
                        flags: 1,
                        data_input_indexes: vec![0],
                        ..Default::default()
                    }],
                    ..Default::default()
                },
            ],
        );
        world.insert(
            scene,
            SceneEventBatch(vec![SceneEvent::ActionStarted {
                action_index: 17,
                action_type: SceneActionType::Package,
                actor_alias: 1,
                actor_entity: Some(actor),
                topic_form_id: None,
                packages: vec![0x400],
            }]),
        );

        scene_package_system(&world, 0.0);
        crate::two_state_activator_system(&world, 0.0);

        assert!(world.get::<crate::TwoStateActivator>(gate).unwrap().is_open);
        assert_eq!(
            world
                .get::<crate::ScriptVariables>(gate)
                .unwrap()
                .get_by_name("::isOpen_var"),
            Some(1.0)
        );
    }
}
