//! ECS → SDK snapshot capture.
//!
//! Everything a guest sees about the world is built here, before entry,
//! from read guards this module owns and releases. The `Raw*` types are
//! the pre-projection identity snapshots; the `capture_*` functions turn
//! live component state into the fixed-size SDK records.

use super::*;

/// Raw ECS identity snapshot captured before entering untrusted code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RawActivation {
    pub subject: EntityId,
    pub subject_form: Option<FormRef>,
    pub activator: Option<EntityId>,
    pub activator_form: Option<FormRef>,
}

/// Raw cell-load identity captured before entering untrusted code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RawCellLoad {
    pub subject: EntityId,
    pub subject_form: Option<FormRef>,
}

/// Raw equipment change captured before entering untrusted code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RawEquipmentChange {
    pub wearer: EntityId,
    pub wearer_form: Option<FormRef>,
    pub item: FormRef,
    pub equipped: bool,
}

/// Raw combat event captured before entering untrusted code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RawHit {
    pub subject: EntityId,
    pub subject_form: Option<FormRef>,
    pub aggressor: Option<EntityId>,
    pub aggressor_form: Option<FormRef>,
    pub source: Option<EntityId>,
    pub source_form: Option<FormRef>,
    pub projectile: Option<EntityId>,
    pub projectile_form: Option<FormRef>,
    pub damage: f32,
    pub power_attack: bool,
    pub sneak_attack: bool,
    pub bash_attack: bool,
    pub blocked: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct RawEntityProjection {
    pub(crate) name: Option<String>,
    pub(crate) world_transform: Option<WorldTransform>,
    pub(crate) actor_values: Option<Vec<(FormRef, ActorValueState)>>,
    pub(crate) inventory: Option<InventorySnapshot>,
    pub(crate) factions: Option<FactionSnapshot>,
    pub(crate) perks: Option<PerkSnapshot>,
    pub(crate) packages: Option<PackageSnapshot>,
    pub(crate) animation: Option<AnimationSnapshot>,
    pub(crate) reputation: Option<ReputationSnapshot>,
}

pub(super) fn entity_projection(
    entity: EntityRef,
    form: Option<FormRef>,
    raw: Option<&RawEntityProjection>,
) -> EntityProjection {
    let projection = EntityProjection::new(
        entity,
        form,
        raw.and_then(|projection| projection.name.clone()),
        raw.and_then(|projection| projection.world_transform),
    )
    .expect("live projection capture enforces SDK bounds");
    let projection = match raw.and_then(|projection| projection.actor_values.as_ref()) {
        Some(actor_values) => projection
            .with_actor_values(actor_values.iter().copied())
            .expect("live actor-value capture enforces SDK bounds"),
        None => projection,
    };
    let projection = match raw.and_then(|projection| projection.inventory.clone()) {
        Some(inventory) => projection.with_inventory(inventory),
        None => projection,
    };
    let projection = match raw.and_then(|projection| projection.factions.clone()) {
        Some(factions) => projection.with_factions(factions),
        None => projection,
    };
    let projection = match raw.and_then(|projection| projection.perks.clone()) {
        Some(perks) => projection.with_perks(perks),
        None => projection,
    };
    let projection = match raw.and_then(|projection| projection.packages.clone()) {
        Some(packages) => projection.with_packages(packages),
        None => projection,
    };
    let projection = match raw.and_then(|projection| projection.animation) {
        Some(animation) => projection.with_animation(animation),
        None => projection,
    };
    match raw.and_then(|projection| projection.reputation.clone()) {
        Some(reputation) => projection.with_reputation(reputation),
        None => projection,
    }
}

pub(super) fn forms_by_entity(world: &World) -> BTreeMap<EntityId, FormRef> {
    match (
        world.query::<FormIdComponent>(),
        world.try_resource::<FormIdPool>(),
    ) {
        (Some(forms), Some(pool)) => forms
            .iter()
            .filter_map(|(entity, component)| {
                pool.resolve(component.0)
                    .copied()
                    .map(|pair| (entity, form_ref(pair)))
            })
            .collect(),
        _ => BTreeMap::new(),
    }
}

pub(super) fn capture_spatial_snapshot(world: &World) -> SpatialSnapshot {
    let forms = forms_by_entity(world);
    let mut references = BTreeMap::<FormRef, SpatialReference>::new();
    let mut truncated = false;
    // #3819 — `Transform` before `GlobalTransform`, not the reverse. Both
    // queries are held live through the whole loop below (`.as_ref()` on
    // each), so the acquisition order here is real, not incidental — and
    // this was the one site in the whole codebase acquiring the pair in
    // `GlobalTransform → Transform` order while every other site
    // (`make_transform_propagation_system`, `ragdoll_writeback_system`,
    // `escort`/`follow`/`guard`/`travel` systems) acquires
    // `Transform → GlobalTransform`, closing a cross-thread ABBA cycle
    // the `BYRO_LOCK_ORDER_CHECK=1` graph correctly flagged. See
    // `docs/engine/ecs.md`'s documented cluster order.
    let local_transforms = world.query::<Transform>();
    let global_transforms = world.query::<GlobalTransform>();
    for (&entity, &form) in &forms {
        let position = global_transforms
            .as_ref()
            .and_then(|transforms| transforms.get(entity))
            .map(|transform| transform.translation.to_array())
            .or_else(|| {
                local_transforms
                    .as_ref()
                    .and_then(|transforms| transforms.get(entity))
                    .map(|transform| transform.translation.to_array())
            });
        let Some(position) = position else {
            continue;
        };
        let Ok(reference) = SpatialReference::new(form, position) else {
            truncated = true;
            continue;
        };
        if let std::collections::btree_map::Entry::Vacant(entry) = references.entry(form) {
            entry.insert(reference);
        } else {
            truncated = true;
        }
    }
    let mut references = references.into_values().collect::<Vec<_>>();
    if references.len() > MAX_SPATIAL_REFERENCES {
        references.truncate(MAX_SPATIAL_REFERENCES);
        truncated = true;
    }
    SpatialSnapshot::new(references, truncated)
        .expect("live spatial capture enforces SDK bounds and portable ordering")
}

pub(super) fn capture_entity_projections(
    world: &World,
    entities: &BTreeSet<EntityId>,
) -> BTreeMap<EntityId, RawEntityProjection> {
    let mut projections = entities
        .iter()
        .copied()
        .map(|entity| (entity, RawEntityProjection::default()))
        .collect::<BTreeMap<_, _>>();

    if let (Some(names), Some(pool)) = (world.query::<Name>(), world.try_resource::<StringPool>()) {
        for (entity, projection) in &mut projections {
            projection.name = names
                .get(*entity)
                .and_then(|name| pool.resolve(name.0))
                .filter(|name| name.len() <= MAX_ENTITY_NAME_BYTES)
                .map(str::to_owned);
        }
    }

    if let Some(transforms) = world.query::<GlobalTransform>() {
        for (entity, projection) in &mut projections {
            projection.world_transform = transforms.get(*entity).and_then(|transform| {
                WorldTransform::new(
                    transform.translation.to_array(),
                    transform.rotation.to_array(),
                    transform.scale,
                )
                .ok()
            });
        }
    }
    if let Some(transforms) = world.query::<Transform>() {
        for (entity, projection) in &mut projections {
            if projection.world_transform.is_some() {
                continue;
            }
            projection.world_transform = transforms.get(*entity).and_then(|transform| {
                WorldTransform::new(
                    transform.translation.to_array(),
                    transform.rotation.to_array(),
                    transform.scale,
                )
                .ok()
            });
        }
    }
    if let (Some(actor_values), Some(resolver)) = (
        world.query::<ActorValues>(),
        world.try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>(),
    ) {
        for (entity, projection) in &mut projections {
            let Some(values) = actor_values.get(*entity) else {
                continue;
            };
            let mut values = values
                .iter()
                .filter_map(|(form_id, value)| {
                    let actor_value = resolver.resolve(form_id).map(form_ref)?;
                    let value = ActorValueState::new(
                        value.base,
                        value.permanent_mod,
                        value.temporary_mod,
                        value.damage,
                    )
                    .ok()?;
                    Some((actor_value, value))
                })
                .collect::<Vec<_>>();
            values.sort_by_key(|(form, _)| *form);
            values.truncate(MAX_ACTOR_VALUES_PER_ENTITY);
            projection.actor_values = Some(values);
        }
    }
    let equipment_by_entity = world
        .query::<EquipmentSlots>()
        .map(|equipment| {
            projections
                .keys()
                .filter_map(|&entity| equipment.get(entity).cloned().map(|slots| (entity, slots)))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    if let (Some(inventories), Some(resolver)) = (
        world.query::<Inventory>(),
        world.try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>(),
    ) {
        let item_catalog = world.try_resource::<crate::inventory::InventoryCatalog>();
        for (entity, projection) in &mut projections {
            let Some(inventory) = inventories.get(*entity) else {
                continue;
            };
            let equipment = equipment_by_entity.get(entity);
            let mut truncated = false;
            let mut summaries = BTreeMap::new();
            for (raw_index, stack) in inventory.items.iter().enumerate() {
                let Some(item) = resolver.resolve(stack.base_form_id).map(form_ref) else {
                    truncated = true;
                    continue;
                };
                if item.local() == 0 {
                    truncated = true;
                    continue;
                }
                let Ok(raw_index) = u32::try_from(raw_index) else {
                    truncated = true;
                    break;
                };
                let index = InventoryIndex(raw_index);
                let biped_slots = equipment.map_or(0, |slots| {
                    slots
                        .occupants
                        .iter()
                        .enumerate()
                        .fold(0_u32, |mask, (bit, occupant)| {
                            if *occupant == Some(index) {
                                mask | (1_u32 << bit)
                            } else {
                                mask
                            }
                        })
                });
                let weapon_equipped = equipment.is_some_and(|slots| slots.weapon == Some(index));
                let metadata = item_catalog
                    .as_ref()
                    .and_then(|catalog| catalog.sdk_metadata(stack.base_form_id));
                let summary = summaries
                    .entry(item)
                    .or_insert_with(|| (0_u64, 0_u32, false, metadata.clone()));
                summary.0 = summary
                    .0
                    .checked_add(u64::from(stack.count))
                    .unwrap_or_else(|| {
                        truncated = true;
                        u64::MAX
                    });
                summary.1 |= biped_slots;
                summary.2 |= weapon_equipped;
                if summary.3.is_none() {
                    summary.3 = metadata;
                }
            }
            let mut entries = summaries
                .into_iter()
                .map(|(item, (count, biped_slots, weapon_equipped, metadata))| {
                    InventoryEntry::new(item, count, biped_slots, weapon_equipped, metadata)
                        .expect("resolved inventory forms are non-null")
                })
                .collect::<Vec<_>>();
            if entries.len() > MAX_INVENTORY_ENTRIES_PER_ENTITY {
                entries.truncate(MAX_INVENTORY_ENTRIES_PER_ENTITY);
                truncated = true;
            }
            projection.inventory = Some(
                InventorySnapshot::new(entries, truncated)
                    .expect("live inventory capture enforces SDK bounds and ordering"),
            );
        }
    }
    if let (Some(faction_ranks), Some(resolver)) = (
        world.query::<FactionRanks>(),
        world.try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>(),
    ) {
        for (entity, projection) in &mut projections {
            let Some(ranks) = faction_ranks.get(*entity) else {
                continue;
            };
            let mut truncated = false;
            let mut memberships = BTreeMap::<FormRef, i8>::new();
            for &(global_faction, rank) in &ranks.0 {
                let Some(faction) = resolver.resolve(global_faction).map(form_ref) else {
                    truncated = true;
                    continue;
                };
                if faction.local() == 0 {
                    truncated = true;
                    continue;
                }
                memberships.entry(faction).or_insert(rank);
            }
            let mut memberships = memberships
                .into_iter()
                .map(|(faction, rank)| {
                    FactionMembership::new(faction, rank)
                        .expect("resolved faction forms are non-null")
                })
                .collect::<Vec<_>>();
            if memberships.len() > MAX_FACTIONS_PER_ENTITY {
                memberships.truncate(MAX_FACTIONS_PER_ENTITY);
                truncated = true;
            }
            projection.factions = Some(
                FactionSnapshot::new(memberships, truncated)
                    .expect("live faction capture enforces SDK bounds and ordering"),
            );
        }
    }
    if let (Some(perks), Some(resolver)) = (
        world.query::<Perks>(),
        world.try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>(),
    ) {
        for (entity, projection) in &mut projections {
            let Some(perks) = perks.get(*entity) else {
                continue;
            };
            let mut truncated = false;
            let mut entries = BTreeMap::<FormRef, u8>::new();
            for perk in &perks.entries {
                let Some(identity) = resolver.resolve(perk.perk_form_id).map(form_ref) else {
                    truncated = true;
                    continue;
                };
                if identity.local() == 0 || perk.rank == 0 {
                    truncated = true;
                    continue;
                }
                if let std::collections::btree_map::Entry::Vacant(entry) = entries.entry(identity) {
                    entry.insert(perk.rank);
                } else {
                    truncated = true;
                }
            }
            let mut entries = entries
                .into_iter()
                .map(|(perk, rank)| {
                    PerkEntry::new(perk, rank).expect("resolved perk entries are valid")
                })
                .collect::<Vec<_>>();
            if entries.len() > MAX_PERKS_PER_ENTITY {
                entries.truncate(MAX_PERKS_PER_ENTITY);
                truncated = true;
            }
            projection.perks = Some(
                PerkSnapshot::new(entries, truncated)
                    .expect("live perk capture enforces SDK bounds and ordering"),
            );
        }
    }
    if let (Some(reputations), Some(resolver)) = (
        world.query::<FactionReputation>(),
        world.try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>(),
    ) {
        for (entity, projection) in &mut projections {
            let Some(reputation) = reputations.get(*entity) else {
                continue;
            };
            let mut truncated = false;
            let mut entries = BTreeMap::<FormRef, (u16, u16)>::new();
            for standing in &reputation.entries {
                let Some(identity) = resolver.resolve(standing.repu_form_id).map(form_ref) else {
                    truncated = true;
                    continue;
                };
                if identity.local() == 0 {
                    truncated = true;
                    continue;
                }
                if let std::collections::btree_map::Entry::Vacant(entry) = entries.entry(identity) {
                    entry.insert((standing.fame, standing.infamy));
                } else {
                    truncated = true;
                }
            }
            let mut entries = entries
                .into_iter()
                .filter_map(|(reputation, (fame, infamy))| {
                    ReputationEntry::new(reputation, fame, infamy)
                        .map_err(|_| truncated = true)
                        .ok()
                })
                .collect::<Vec<_>>();
            if entries.len() > MAX_REPUTATIONS_PER_ENTITY {
                entries.truncate(MAX_REPUTATIONS_PER_ENTITY);
                truncated = true;
            }
            projection.reputation = Some(
                ReputationSnapshot::new(entries, truncated)
                    .expect("live reputation capture enforces SDK bounds and ordering"),
            );
        }
    }
    if let Some(resolver) =
        world.try_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
    {
        if let Some(states) = world.query::<byroredux_scripting::ActorCinematicState>() {
            for (entity, projection) in &mut projections {
                let Some(state) = states.get(*entity) else {
                    continue;
                };
                projection.animation = Some(AnimationSnapshot::new(
                    state
                        .requested_idle_form_id
                        .and_then(|form| resolver.resolve(form))
                        .map(form_ref)
                        .filter(|form| form.local() != 0),
                    state.idle_request_serial,
                    state.awaited_event.map(animation_event),
                    state.last_animation_event.map(animation_event),
                    state.animation_event_serial,
                ));
            }
        }
        let mut package_captures = projections
            .keys()
            .copied()
            .map(|entity| (entity, PackageCapture::default()))
            .collect::<BTreeMap<_, _>>();
        if let Some(ambient) = world.query::<AmbientPackageRuntime>() {
            for (entity, capture) in &mut package_captures {
                let Some(runtime) = ambient.get(*entity) else {
                    continue;
                };
                let active = runtime
                    .active_package_form_id
                    .and_then(|form| capture_package_form(form, &resolver, capture));
                let candidates =
                    capture_package_candidates(&runtime.package_candidates, &resolver, capture);
                push_package_selection(
                    capture,
                    PackageSelection::ambient(candidates, active)
                        .expect("captured ambient package selection is bounded"),
                );
            }
        }
        let mut scene_actions = world
            .query::<byroredux_scripting::ScenePackagePlayback>()
            .map(|playbacks| {
                playbacks
                    .iter()
                    .flat_map(|(_, playback)| playback.active_actions.iter().cloned())
                    .filter(|action| package_captures.contains_key(&action.actor))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        scene_actions.sort_by_key(|action| {
            (
                action.actor,
                action.scene_form_id,
                action.action_index,
                action.package_form_id,
                action.template_form_id,
            )
        });
        for action in scene_actions {
            let capture = package_captures
                .get_mut(&action.actor)
                .expect("scene action was filtered to a disclosed actor");
            if capture.selections.len() >= MAX_PACKAGE_SELECTIONS_PER_ENTITY {
                capture.truncated = true;
                continue;
            }
            let scene = capture_package_form(action.scene_form_id, &resolver, capture);
            let active = capture_package_form(action.package_form_id, &resolver, capture);
            let template = capture_package_form(action.template_form_id, &resolver, capture);
            let candidates =
                capture_package_candidates(&action.package_candidates, &resolver, capture);
            push_package_selection(
                capture,
                PackageSelection::scene_action(
                    scene,
                    action.action_index,
                    candidates,
                    active,
                    template,
                )
                .expect("captured scene package selection is bounded"),
            );
        }
        for (entity, capture) in package_captures {
            if capture.selections.is_empty() {
                continue;
            }
            projections
                .get_mut(&entity)
                .expect("package capture is scoped to projected entities")
                .packages = Some(
                PackageSnapshot::new(capture.selections, capture.truncated)
                    .expect("live package capture enforces SDK bounds"),
            );
        }
    }
    projections
}

#[derive(Default)]
struct PackageCapture {
    selections: Vec<PackageSelection>,
    references: usize,
    truncated: bool,
}

fn capture_package_form(
    global: u32,
    resolver: &crate::cell_loader::load_order::GlobalFormIdResolver,
    capture: &mut PackageCapture,
) -> Option<FormRef> {
    let Some(form) = resolver.resolve(global).map(form_ref) else {
        capture.truncated = true;
        return None;
    };
    if form.local() == 0 || capture.references >= MAX_PACKAGE_REFERENCES_PER_ENTITY {
        capture.truncated = true;
        return None;
    }
    capture.references += 1;
    Some(form)
}

fn capture_package_candidates(
    candidates: &[u32],
    resolver: &crate::cell_loader::load_order::GlobalFormIdResolver,
    capture: &mut PackageCapture,
) -> Vec<FormRef> {
    if candidates.len() > MAX_PACKAGE_CANDIDATES {
        capture.truncated = true;
    }
    candidates
        .iter()
        .take(MAX_PACKAGE_CANDIDATES)
        .filter_map(|&candidate| capture_package_form(candidate, resolver, capture))
        .collect()
}

fn push_package_selection(capture: &mut PackageCapture, selection: PackageSelection) {
    if capture.selections.len() >= MAX_PACKAGE_SELECTIONS_PER_ENTITY {
        capture.truncated = true;
    } else {
        capture.selections.push(selection);
    }
}

pub(super) fn entities_by_form(world: &World) -> BTreeMap<FormRef, EntityId> {
    forms_by_entity(world)
        .into_iter()
        .map(|(entity, form)| (form, entity))
        .collect()
}
