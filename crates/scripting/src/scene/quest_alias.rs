//! Quest-alias resolution, injection, and diagnostics.
//!
//! Split out of `scene.rs` (#2408 / TD1-005), which had accumulated four
//! separately-arrived responsibilities. This half owns the alias types, the
//! SCEN-declared alias registry, candidate matching / fill resolution, and
//! the per-frame injection of alias-derived overlays (factions, inventory
//! grants) onto resolved actors. Contents moved verbatim.

use std::collections::{HashMap, HashSet};

use byroredux_core::ecs::components::{Dead, FactionRanks, GlobalTransform, Inventory, ItemStack};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::{
    AliasFillType, QuestAlias, QustRecord, ALIAS_FLAG_ALLOW_DEAD, ALIAS_FLAG_ALLOW_RESERVED,
    ALIAS_FLAG_ALLOW_REUSE, ALIAS_FLAG_CLOSEST, ALIAS_FLAG_RESERVES,
};

use super::{register, SceneActorBindings};
use crate::condition::{evaluate, ConditionContext};
use crate::papyrus_demo::PlayerEntity;
use crate::quest_stages::{QuestFormId, QuestStageState};

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
pub(super) struct SceneQuestAliasRegistry {
    aliases: HashMap<QuestFormId, Vec<QuestAlias>>,
}

impl Resource for SceneQuestAliasRegistry {}

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
///
/// FO4 `ALCS` reference-collection aliases (`alias.is_collection`) are
/// excluded from this fill loop entirely (#2661 / SCR-D6-NEW11-04) — they
/// are a documented Phase 4+ deferral
/// (`docs/engine/m47-3-quest-alias-design.md`, "Reference collections")
/// with no collection-fill runtime yet. Pre-fix, a collection alias
/// carrying match conditions fell through to the ordinary single-entity
/// `eligible` path below: it bound exactly ONE candidate, which then
/// received the whole collection's injected factions/inventory via
/// `apply_alias_injections`, while `quest_alias_diagnostics` reported
/// `Bound` — a documented "not built yet" path silently half-working
/// instead of declining. `quest_alias_diagnostics` already classifies an
/// unbound collection alias as `ReferenceCollectionRuntimeUnavailable`;
/// simply never binding it here is what makes that diagnostic accurate.
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
        for alias in aliases
            .iter()
            .filter(|alias| !alias.is_location && !alias.is_collection)
        {
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
