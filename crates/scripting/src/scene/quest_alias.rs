//! Quest-alias resolution, injection, and diagnostics.
//!
//! Split out of `scene.rs` (#2408 / TD1-005), which had accumulated four
//! separately-arrived responsibilities. This half owns the alias types, the
//! SCEN-declared alias registry, candidate matching / fill resolution, and
//! the per-frame injection of alias-derived overlays (factions, inventory
//! grants) onto resolved actors. Contents moved verbatim.

use std::borrow::Cow;
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
use crate::condition::{
    evaluate, subject_requirement, ConditionContext, ConditionFunction, IdentityTest,
    SubjectRequirement,
};
use crate::papyrus_demo::PapyrusPlayerEntity;
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

/// Bootstrap-only identity for a forced scene actor whose owning cell is not
/// resident. Alias resolution prefers an ordinary loaded candidate with the
/// same authored identity once streaming materializes it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RemoteSceneActorStub;

impl Component for RemoteSceneActorStub {
    type Storage = SparseSetStorage<Self>;
}

impl Component for SceneAliasCandidate {
    type Storage = SparseSetStorage<Self>;
}

#[derive(Debug, Clone, Default)]
/// Installed parsed QUST alias definitions keyed by quest. Read-mostly at
/// runtime — `install_scene_quest_aliases` fills it, and the alias refresh
/// rebuilds the live binding table beside it. Public so scheduler access
/// rows can name it; mutate only through the installer.
pub struct SceneQuestAliasRegistry {
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

/// Candidate positions (indices into one refresh's ordered candidate list)
/// keyed by every field an alias fill matches on. Each alias then visits only
/// the candidates its fill can accept: the full scan was aliases × candidates
/// — 1 455 × ~8 500 ≈ 11 M checks and ~840 ms per refresh on FO4
/// Commonwealth — although every fill but a condition-only one matches a
/// single field exactly. Buckets hold ascending positions, so visiting one
/// preserves the candidate order that first-eligible and closest selection
/// depend on.
#[derive(Default)]
struct CandidateIndex {
    by_entity: HashMap<EntityId, usize>,
    by_reference: HashMap<u32, Vec<usize>>,
    by_base: HashMap<u32, Vec<usize>>,
    by_location_ref_type: HashMap<u32, Vec<usize>>,
    /// Candidates with a linked ref targeting this form id.
    by_linked_target: HashMap<u32, Vec<usize>>,
}

impl CandidateIndex {
    fn new(candidates: &[(EntityId, SceneAliasCandidate)]) -> Self {
        // Positions arrive in ascending order, so a candidate listing one key
        // twice only ever repeats the bucket's last entry.
        fn push(bucket: &mut Vec<usize>, position: usize) {
            if bucket.last() != Some(&position) {
                bucket.push(position);
            }
        }
        let mut index = Self::default();
        for (position, (entity, candidate)) in candidates.iter().enumerate() {
            index.by_entity.insert(*entity, position);
            push(
                index
                    .by_reference
                    .entry(candidate.reference_form_id)
                    .or_default(),
                position,
            );
            push(
                index.by_base.entry(candidate.base_form_id).or_default(),
                position,
            );
            for &ref_type in &candidate.location_ref_types {
                push(
                    index.by_location_ref_type.entry(ref_type).or_default(),
                    position,
                );
            }
            for &(_, target) in &candidate.linked_refs {
                push(index.by_linked_target.entry(target).or_default(), position);
            }
        }
        index
    }

    fn bucket(map: &HashMap<u32, Vec<usize>>, key: u32) -> Cow<'_, [usize]> {
        Cow::Borrowed(map.get(&key).map_or(&[][..], Vec::as_slice))
    }

    /// Every position whose candidate can pass `alias`'s fill test (the
    /// `fill_matches` half of the refresh's `eligible`), in candidate order;
    /// `None` when that is every candidate — a condition-only fill. A
    /// superset is harmless (`eligible` re-checks the fill); omitting a
    /// match would change the binding, so each arm mirrors
    /// [`candidate_matches_fill`] or the `NearAlias` link test exactly.
    fn fill_positions(
        &self,
        alias: &QuestAlias,
        quest: QuestFormId,
        resolved: &HashMap<(QuestFormId, i32), EntityId>,
        candidates: &[(EntityId, SceneAliasCandidate)],
    ) -> Option<Cow<'_, [usize]>> {
        let none = Cow::Borrowed(&[][..]);
        Some(match alias.fill_type.as_ref() {
            Some(AliasFillType::ForcedReference(reference)) => {
                Self::bucket(&self.by_reference, *reference)
            }
            Some(AliasFillType::UniqueActor(base)) => Self::bucket(&self.by_base, *base),
            Some(AliasFillType::LocationAliasReference {
                ref_type: Some(ref_type),
                ..
            }) => Self::bucket(&self.by_location_ref_type, *ref_type),
            Some(AliasFillType::NearAlias {
                alias_id,
                relation: 0 | 1,
            }) => {
                let Some(source) = resolved
                    .get(&(quest, *alias_id))
                    .and_then(|entity| self.by_entity.get(entity))
                    .map(|&position| &candidates[position].1)
                else {
                    return Some(none);
                };
                // Candidates the source links to, plus candidates linking to
                // the source — the two halves of the `NearAlias` test.
                let mut positions: Vec<usize> = source
                    .linked_refs
                    .iter()
                    .flat_map(|(_, target)| Self::bucket(&self.by_reference, *target).into_owned())
                    .chain(
                        Self::bucket(&self.by_linked_target, source.reference_form_id).into_owned(),
                    )
                    .collect();
                positions.sort_unstable();
                positions.dedup();
                Cow::Owned(positions)
            }
            // Every other fill is rejected by `candidate_matches_fill`.
            Some(_) => none,
            None if alias.match_conditions.is_empty() => none,
            None => return None,
        })
    }
}

/// Candidate positions keyed by the id each identity-test function reads off
/// the candidate — [`ConditionFunction::run_on_identity`], the value the
/// evaluator itself compares with `param_1` — built per function on first use
/// in a refresh.
///
/// It serves condition-only fills, which [`CandidateIndex`] cannot narrow:
/// on FO4 Commonwealth such a fill evaluated its conditions against all
/// ~8 500 candidates each refresh, ~1–2 ms apiece, even when an identity test
/// such as `GetIsRace` admits only a handful of them.
#[derive(Default)]
struct IdentityIndex {
    by_function: HashMap<ConditionFunction, HashMap<u32, Vec<usize>>>,
}

impl IdentityIndex {
    /// Positions that can pass every one of `blocks`, in candidate order —
    /// those passing an identity test of whichever block admits the fewest —
    /// or `None` when there is no block to narrow by.
    fn narrowest(
        &mut self,
        blocks: &[Vec<IdentityTest>],
        world: &World,
        candidates: &[(EntityId, SceneAliasCandidate)],
    ) -> Option<Cow<'_, [usize]>> {
        for test in blocks.iter().flatten() {
            self.by_function.entry(test.function).or_insert_with(|| {
                let mut buckets: HashMap<u32, Vec<usize>> = HashMap::new();
                for (position, (entity, _)) in candidates.iter().enumerate() {
                    if let Some(id) = test.function.run_on_identity(*entity, world) {
                        buckets.entry(id).or_default().push(position);
                    }
                }
                buckets
            });
        }
        let bucket = |test: &IdentityTest| {
            self.by_function[&test.function]
                .get(&test.id)
                .map_or(&[][..], Vec::as_slice)
        };
        let block = blocks
            .iter()
            .min_by_key(|block| block.iter().map(|test| bucket(test).len()).sum::<usize>())?;
        Some(match block.as_slice() {
            [test] => Cow::Borrowed(bucket(test)),
            tests => {
                let mut positions: Vec<usize> = tests
                    .iter()
                    .flat_map(|test| bucket(test).iter().copied())
                    .collect();
                positions.sort_unstable();
                positions.dedup();
                Cow::Owned(positions)
            }
        })
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
                    // #2670 — the SAVE-D6-01 rekey made `reference_form_id`
                    // (a stable authored ESM FormID that survives an
                    // in-session cell reload, unlike a raw `EntityId`) the
                    // grant key. Unreachable today: every alias-bindable
                    // entity is stamped with a `SceneAliasCandidate` at REFR
                    // spawn, so the lookup always hits.
                    //
                    // It becomes reachable with the Phase 4+ **Created
                    // Object** alias fill, which by definition produces
                    // entities with no authored REFR and therefore no
                    // `reference_form_id`. Dropping the grant silently there
                    // would be indistinguishable from "already granted", so
                    // say so — and note that the fix at that point is a
                    // synthetic stable key for created objects, not an
                    // authored FormID this entity will never have.
                    match reference_form_ids.get(&entity) {
                        Some(&reference_form_id) => desired_inventory.push((
                            source.0,
                            source.1,
                            entity,
                            reference_form_id,
                            item,
                            count,
                        )),
                        None => log::warn!(
                            "quest alias inventory grant skipped: quest {:08X} alias {} \
                             resolved to entity {entity} which carries no \
                             SceneAliasCandidate, so it has no stable reference_form_id \
                             to key the grant by (item {item:08X} x{count}). Created-Object \
                             alias fill will need a synthetic stable key here (#2670).",
                            source.0 .0,
                            source.1,
                        ),
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
        let mut package_changes = Vec::new();
        let old_entities: Vec<EntityId> = overlays.iter().map(|(entity, _)| entity).collect();
        for entity in old_entities {
            if !desired_overlays.contains_key(&entity) {
                if overlays
                    .get(entity)
                    .is_some_and(|old| old.0.values().any(|data| !data.packages.is_empty()))
                {
                    package_changes.push(entity);
                }
                overlays.remove(entity);
            }
        }
        for (entity, injected) in desired_overlays {
            let old_packages: HashSet<u32> = overlays
                .get(entity)
                .into_iter()
                .flat_map(|old| old.0.values())
                .flat_map(|data| data.packages.iter().copied())
                .collect();
            let new_packages: HashSet<u32> = injected
                .values()
                .flat_map(|data| data.packages.iter().copied())
                .collect();
            if old_packages != new_packages {
                package_changes.push(entity);
            }
            overlays.insert(entity, QuestAliasInjectedOverlays(injected));
        }
        drop(overlays);
        if let Some(mut requests) = world.query_mut::<crate::EvaluatePackageRequest>() {
            for entity in package_changes {
                requests.insert(entity, crate::EvaluatePackageRequest);
            }
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
    // Loaded candidates ahead of remote stubs, each group in entity order.
    // Stubs are few, so collect them once instead of probing their storage
    // from the comparator — a lock round trip per comparison, ~1 ms of every
    // FO4 Commonwealth refresh.
    let stubs: HashSet<EntityId> = world
        .query::<RemoteSceneActorStub>()
        .map(|query| query.iter().map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    candidates.sort_unstable_by_key(|(entity, _)| (stubs.contains(entity), *entity));
    let index = CandidateIndex::new(&candidates);
    let mut identities = IdentityIndex::default();

    let registered_quests: HashSet<QuestFormId> = world
        .resource::<SceneQuestAliasRegistry>()
        .aliases
        .keys()
        .copied()
        .collect();
    // Alias values exist only for running quest instances. Unit-test/tool
    // worlds that intentionally omit the lifecycle store retain the old
    // data-only behavior; the live engine always installs QuestStageState.
    let running: Vec<QuestFormId> = match world.try_resource::<QuestStageState>() {
        Some(stages) => registered_quests
            .iter()
            .copied()
            .filter(|quest| stages.is_running(*quest))
            .collect(),
        None => registered_quests.iter().copied().collect(),
    };
    // Clone only what this refresh fills: 274 of FO4's 1 336 alias-bearing
    // quests run at startup.
    let mut quests: Vec<(QuestFormId, Vec<QuestAlias>)> = {
        let registry = world.resource::<SceneQuestAliasRegistry>();
        running
            .into_iter()
            .filter_map(|quest| Some((quest, registry.aliases.get(&quest)?.clone())))
            .collect()
    };
    quests.sort_by_key(|(quest, _)| quest.0);
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
                        .and_then(|source| index.by_entity.get(source))
                        .map(|&position| &candidates[position].1)
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
                    // #2671 — hand the evaluator the bindings resolved so
                    // far in THIS pass. `resolved` is committed to
                    // `SceneActorBindings` only after the loop, so a
                    // match-CTDA naming a sibling alias via
                    // `RunOn::QuestAlias` otherwise read last refresh's
                    // table (or nothing on the first refresh) and the fill
                    // lagged a refresh behind its own dependency.
                    || !evaluate(
                        &alias.match_conditions,
                        world,
                        &ConditionContext::for_subject(*entity)
                            .with_quest(*quest)
                            .with_pending_alias_bindings(&resolved),
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
                        .then(|| {
                            world
                                .try_resource::<PapyrusPlayerEntity>()
                                .map(|player| player.0)
                        })
                        .flatten()
                })
                .and_then(|entity| {
                    world
                        .get::<GlobalTransform>(entity)
                        .map(|gt| gt.translation)
                });
            let positions = match index.fill_positions(alias, *quest, &resolved, &candidates) {
                Some(positions) => positions,
                // A condition-only fill would evaluate its conditions against
                // every candidate. Judge once what they demand of any
                // candidate: a block none can pass empties the scan, and one
                // gated on identity tests visits only the candidates carrying
                // those identities.
                None => match subject_requirement(
                    &alias.match_conditions,
                    world,
                    &ConditionContext::for_subject(EntityId::MAX)
                        .with_quest(*quest)
                        .with_pending_alias_bindings(&resolved),
                ) {
                    SubjectRequirement::Unsatisfiable => Cow::Borrowed(&[][..]),
                    SubjectRequirement::IdentityBlocks(blocks) => identities
                        .narrowest(&blocks, world, &candidates)
                        .unwrap_or_else(|| Cow::Owned((0..candidates.len()).collect())),
                },
            };
            let mut in_fill = positions.iter().map(|&position| &candidates[position]);
            let chosen = if let Some(anchor) = anchor {
                in_fill
                    .filter_map(|candidate| {
                        let entity = eligible(candidate)?;
                        let position = world.get::<GlobalTransform>(entity)?.translation;
                        Some((entity, position.distance_squared(anchor)))
                    })
                    .min_by(|left, right| left.1.total_cmp(&right.1))
                    .map(|(entity, _)| entity)
            } else {
                in_fill.find_map(eligible)
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

/// Quests whose live alias bindings name `entity` and that are currently
/// running, ascending by quest FormID. P4's activation-driven topic
/// selection walks this list to find the DIALs an activated NPC owns
/// (`DialRecord::quest_refs` is the authored NPC→topic edge the engine has
/// been missing); there is no authored direct NPC→DIAL map.
///
/// A quest with no installed alias definition or no `QuestStageState`
/// resource cannot bind anything, so it never appears.
pub fn running_quests_binding_entity(world: &World, entity: EntityId) -> Vec<QuestFormId> {
    let Some(registry) = world.try_resource::<SceneQuestAliasRegistry>() else {
        return Vec::new();
    };
    let Some(bindings) = world.try_resource::<SceneActorBindings>() else {
        return Vec::new();
    };
    let running = world.try_resource::<QuestStageState>();
    let mut quests: Vec<(u32, QuestFormId)> = registry
        .aliases
        .keys()
        .copied()
        .filter(|quest| running.as_ref().is_none_or(|stages| stages.is_running(*quest)))
        .filter(|quest| {
            registry
                .aliases
                .get(quest)
                .into_iter()
                .flatten()
                .any(|alias| bindings.resolve(*quest, alias.alias_id) == Some(entity))
        })
        .map(|quest| (quest.0, quest))
        .collect();
    quests.sort_unstable_by_key(|(raw, _)| *raw);
    quests.into_iter().map(|(_, quest)| quest).collect()
}
