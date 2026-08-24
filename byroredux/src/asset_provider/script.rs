use super::*;

/// Searches game archives for compiled Papyrus scripts (`.pex`) by
/// script name. The M47.2 attach path resolves an attached script
/// name — read from a base record's `VMAD` — to its bytecode here, then
/// hands the bytes to `byroredux_scripting::translate_pex`, which
/// decompiles and recognizes them. Held as a World resource so the cell
/// loader's REFR-attach path can reach it without threading a parameter
/// through every spawn call.
///
/// Vanilla scripts live in a dedicated archive — Skyrim's
/// `Skyrim - Misc.bsa`, FO4's `Fallout4 - Misc.ba2`, etc. — passed via
/// the repeatable `--scripts-bsa <path>` flag. An empty provider (no
/// flag) makes every lookup a clean miss, so the attach path simply
/// finds no compiled behavior and falls through, exactly like an
/// unregistered SCPT.
pub(crate) struct ScriptProvider {
    archives: Vec<Archive>,
}

impl ScriptProvider {
    pub(crate) fn new() -> Self {
        Self {
            archives: Vec::new(),
        }
    }

    /// True when no script archive was supplied — the attach path can
    /// skip the VMAD branch entirely (avoids per-REFR path-building on
    /// loads that never passed `--scripts-bsa`).
    pub(crate) fn is_empty(&self) -> bool {
        self.archives.is_empty()
    }

    /// Resolve a Papyrus script name (e.g. `DA10MainDoorScript`, as
    /// authored in a `VMAD`) to its compiled `.pex` bytes. Normalises to
    /// the archive key `scripts\<lowercase-name>.pex`; a name that
    /// already carries the folder and/or extension is accepted too.
    ///
    /// **Precedence: first-listed `--scripts-bsa` archive wins** on a name
    /// collision (searched in flag order, first hit returned) — list
    /// override/mod archives *before* the vanilla one. This is the
    /// inverse of typical mod-manager load order (there, later = higher
    /// priority) — see #1743 / SCR-D7-03. Returns `None` when no archive
    /// carries the script.
    pub(crate) fn extract_pex(&self, script_name: &str) -> Option<Vec<u8>> {
        let name = pex_archive_path(script_name);
        for archive in &self.archives {
            if let Ok(data) = archive.extract(&name) {
                return Some(data);
            }
        }
        None
    }
}

impl byroredux_core::ecs::resource::Resource for ScriptProvider {}

/// Normalise a Papyrus script name to its archive key: lowercase,
/// backslash-separated, under the `scripts\` folder with a `.pex`
/// extension. A name authored with the folder and/or extension already
/// present (or with forward slashes) is accepted unchanged in meaning.
pub(crate) fn pex_archive_path(script_name: &str) -> String {
    let mut name = script_name.replace('/', "\\").to_ascii_lowercase();
    if !name.ends_with(".pex") {
        name.push_str(".pex");
    }
    if !name.starts_with("scripts\\") {
        name = format!("scripts\\{name}");
    }
    name
}

/// Populate the [`byroredux_scripting::QuestStageFragments`] table from a
/// merged index's QUST `VMAD` fragment bindings — the M47.2 runtime
/// keystone. For every quest carrying stage→`Fragment_N` bindings, resolve
/// its compiled `QF_` `.pex` once (via the [`ScriptProvider`]), decompile,
/// lower each bound fragment body to canonical effects, and register them
/// keyed by `(quest, stage)` for `quest_fragment_dispatch_system`.
///
/// No-op when no `--scripts-bsa` archive is present (nothing to
/// decompile) or on pre-Papyrus games (empty `fragments`). Runs once per
/// cell load; re-registering a `(quest, stage)` on a later load simply
/// overwrites with the identical lowering.
pub(crate) fn populate_quest_fragments(
    world: &mut byroredux_core::ecs::world::World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) {
    // Fast-out before any per-quest work when no script archive was
    // supplied (the common mesh-only / FO3-FNV case).
    let have_archive = world
        .try_resource::<ScriptProvider>()
        .is_some_and(|p| !p.is_empty());
    if !have_archive {
        return;
    }

    let mut total = 0usize;
    let mut quests_with_fragments = 0usize;
    for (&form_id, quest) in index.quests.iter() {
        if quest.fragments.is_empty() {
            continue;
        }
        quests_with_fragments += 1;
        // Register the quest's own VMAD scripts-section (its declared
        // `Quest Property` bindings) so a fragment's cross-quest
        // `Property`-targeted effect can resolve at dispatch time,
        // independent of whether any fragment below lowers successfully.
        if let Some(vmad) = &quest.script_instance {
            let mut frags = world.resource_mut::<byroredux_scripting::QuestStageFragments>();
            frags.insert_vmad(byroredux_scripting::QuestFormId(form_id), vmad.clone());
        }
        // All of a quest's fragments share one QF_ script, but group by
        // script name defensively (and resolve each `.pex` once).
        let mut by_script: std::collections::HashMap<&str, Vec<(u16, &str)>> =
            std::collections::HashMap::new();
        for f in &quest.fragments {
            by_script
                .entry(f.script_name.as_str())
                .or_default()
                .push((f.stage, f.fragment_name.as_str()));
        }
        for (script_name, bindings) in by_script {
            // Scope the provider borrow: extract owned `.pex` bytes, then
            // drop the resource read before the `&mut` resource access.
            let bytes = {
                let provider = world.resource::<ScriptProvider>();
                provider.extract_pex(script_name)
            };
            let Some(bytes) = bytes else {
                log::trace!(
                    "M47.2 quest-fragment: .pex '{script_name}' not in archive (quest {form_id:08X})"
                );
                continue;
            };
            let mut frags = world.resource_mut::<byroredux_scripting::QuestStageFragments>();
            total += byroredux_scripting::populate_quest_fragments_from_pex(
                &mut frags,
                byroredux_scripting::QuestFormId(form_id),
                &bytes,
                &bindings,
            );
        }
    }
    if total > 0 {
        log::info!(
            "M47.2: populated {total} quest-stage fragments from {quests_with_fragments} scripted quests"
        );
    }
}

/// Populate lowered `SCEN` lifecycle fragments from the compiled `SF_` scripts
/// referenced by each scene's VMAD fragment table.
fn populate_scene_fragments(
    world: &mut byroredux_core::ecs::world::World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) -> usize {
    let have_archive = world
        .try_resource::<ScriptProvider>()
        .is_some_and(|provider| !provider.is_empty());
    if !have_archive {
        return 0;
    }

    let mut total = 0;
    for (&scene_form_id, scene) in &index.scenes {
        let Some(quest_form_id) = scene.quest_form_id else {
            continue;
        };
        if scene.fragments.is_empty() {
            continue;
        }
        let mut by_script: std::collections::HashMap<
            &str,
            Vec<(
                byroredux_plugin::esm::records::script_instance::SceneFragmentEvent,
                &str,
            )>,
        > = std::collections::HashMap::new();
        for fragment in &scene.fragments {
            by_script
                .entry(fragment.script_name.as_str())
                .or_default()
                .push((fragment.event, fragment.fragment_name.as_str()));
        }
        for (script_name, bindings) in by_script {
            let bytes = {
                let provider = world.resource::<ScriptProvider>();
                provider.extract_pex(script_name)
            };
            let Some(bytes) = bytes else {
                log::trace!(
                    "scene-fragment: .pex '{script_name}' not in archive (scene {scene_form_id:08X})"
                );
                continue;
            };
            let mut fragments = world.resource_mut::<byroredux_scripting::SceneFragments>();
            total += byroredux_scripting::populate_scene_fragments_from_pex(
                &mut fragments,
                scene_form_id,
                byroredux_scripting::QuestFormId(quest_form_id),
                scene.script_instance.as_ref(),
                &bytes,
                &bindings,
            );
        }
    }
    total
}

/// Install every parsed Skyrim+ `SCEN` definition and its referenced
/// `DIAL`/`INFO` topics into the ECS runtime.
///
/// The scripting crate deduplicates by FormID and preserves existing player
/// state, so this is safe on cell transitions and repeated load-order parses.
/// It intentionally does not require a script archive: SCEN phase/action data
/// and VMAD function bindings are carried by the plugin record itself.
fn populate_quest_trigger_approaches(
    world: &mut byroredux_core::ecs::world::World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) -> usize {
    let mut candidates = Vec::new();
    let mut collect = |cell: &byroredux_plugin::esm::cell::CellData| {
        candidates.extend(
            cell.references
                .iter()
                .filter(|placed| placed.primitive.is_some())
                .filter_map(|placed| {
                    placed
                        .script_instance
                        .as_ref()
                        .map(|instance| (placed.form_id, placed.position, instance.clone()))
                }),
        );
    };
    for cell in index.cells.cells.values() {
        collect(cell);
    }
    for grids in index.cells.exterior_cells.values() {
        for cell in grids.values() {
            collect(cell);
        }
    }
    for cell in index.cells.worldspace_persistent_cells.values() {
        collect(cell);
    }

    let mut installed = 0;
    for (reference_form_id, position, instance) in candidates {
        for script in instance
            .scripts
            .iter()
            .filter(|script| script.name.to_ascii_lowercase().contains("trig"))
        {
            let bytes = {
                let Some(provider) = world.try_resource::<ScriptProvider>() else {
                    return installed;
                };
                provider.extract_pex(&script.name)
            };
            let Some(bytes) = bytes else {
                continue;
            };
            let Some(recognized) =
                byroredux_scripting::translate_pex(&bytes, index.game, Some(&instance), None)
            else {
                continue;
            };
            let probe = world.spawn();
            (recognized.spawn)(world, probe);
            let advance = world
                .get::<byroredux_scripting::papyrus_demo::quest_advance::QuestAdvanceOnActivate>(
                    probe,
                )
                .map(|advance| advance.clone());
            world.despawn(probe);
            let Some(advance) = advance else {
                continue;
            };
            if !matches!(
                advance.activator_gate,
                byroredux_scripting::papyrus_demo::quest_advance::ActivatorGate::BaseForm(_)
            ) {
                continue;
            }
            byroredux_scripting::papyrus_demo::quest_advance::install_quest_trigger_approach(
                world,
                reference_form_id,
                byroredux_core::math::Vec3::from_array(
                    byroredux_core::math::coord::zup_to_yup_pos(position),
                ),
                advance,
            );
            installed += 1;
            break;
        }
    }
    installed
}

pub(crate) fn populate_scene_runtime(
    world: &mut byroredux_core::ecs::world::World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) {
    crate::inventory::install_catalog(world, index);
    let imad_count = byroredux_scripting::install_image_space_modifiers(
        world,
        index.imagespace_modifiers.values().cloned(),
    );
    if imad_count > 0 {
        log::info!(
            "Installed {imad_count} IMAD definitions into the cinematic presentation runtime"
        );
    }
    let equip_item_count = byroredux_scripting::install_equip_item_catalog(
        world,
        index.items.iter().filter_map(|(&form_id, item)| {
            let byroredux_plugin::esm::records::ItemKind::Armor { biped_flags, .. } = &item.kind
            else {
                return None;
            };
            Some((form_id, *biped_flags))
        }),
    );
    log::info!("Installed {equip_item_count} armor biped-slot definitions for scripted EquipItem");
    let start_game_quests =
        byroredux_scripting::install_start_game_quests(world, index.quests.values().cloned());
    let mut engine_start_quests = 0usize;
    if index.game == byroredux_plugin::esm::reader::GameKind::Skyrim {
        // Skyrim.exe treats MQ101 (Unbound) as the canonical new-game root
        // even though its QUST does not carry Start Game Enabled. Its INDX
        // metadata still supplies startup stage 0.
        const MQ101: u32 = 0x0003_372B;
        if let Some(quest) = index.quests.get(&MQ101) {
            engine_start_quests += usize::from(byroredux_scripting::install_engine_start_quest(
                world,
                byroredux_scripting::QuestFormId(quest.form_id),
                quest.start_up_stage,
            ));
            // MQ101StartingCellLoadRegisterScript increments a quest-owned
            // load counter and sets stage 12 once every opening actor/ref is
            // resident. Alias-attached method dispatch is represented by the
            // equivalent canonical readiness gate here.
            byroredux_scripting::install_quest_alias_readiness_gate(
                world,
                byroredux_scripting::QuestAliasReadinessGate {
                    quest: byroredux_scripting::QuestFormId(MQ101),
                    required_aliases: vec![
                        119, 1, 12, 4, 9, 13, 14, 38, 42, 43, 44, 29, 17, 34, 93, 94, 116, 117, 22,
                        23, 24,
                    ],
                    target_stage: 12,
                    only_below_stage: 15,
                },
            );
        }
    }
    log::info!(
        "Installed {start_game_quests} Start Game Enabled and {engine_start_quests} engine-root quest definitions"
    );
    let quest_aliases =
        byroredux_scripting::install_scene_quest_aliases(world, index.quests.values().cloned());
    if quest_aliases > 0 {
        log::info!("Installed alias definitions for {quest_aliases} quests");
    }
    // M42.9 / #2652 — ambient NPC package stacks must remain resolvable after
    // spawn so schedule boundaries and Papyrus EvaluatePackage can select a
    // new winner. Previously this registry received only SCEN-referenced PACK
    // records, leaving ordinary NPC_.PKID candidates available solely through
    // the short-lived EsmIndex borrow at spawn time.
    let package_count =
        byroredux_scripting::install_package_records(world, index.packages.values().cloned());
    if package_count > 0 {
        log::info!("Installed {package_count} PACK definitions into the live package runtime");
    }
    if !index.scenes.is_empty() {
        let trigger_approach_count = populate_quest_trigger_approaches(world, index);
        log::info!(
            "Prepared {trigger_approach_count} actor-specific quest trigger approaches across the parsed load order"
        );
        // The current runtime consumes dialogue only through SCEN actions.
        // Keep the registry proportional to that live surface instead of
        // duplicating every ambient/conversation DIAL retained by EsmIndex.
        let dialogue_topics: std::collections::HashSet<u32> = index
            .scenes
            .values()
            .flat_map(|scene| {
                scene
                    .actions
                    .iter()
                    .filter_map(|action| action.topic_form_id)
            })
            .collect();
        let dialogue_count = byroredux_scripting::install_dialogue_records(
            world,
            dialogue_topics
                .iter()
                .filter_map(|topic| index.dialogues.get(topic).cloned()),
        );
        if dialogue_count > 0 {
            log::info!(
                "Installed {dialogue_count} scene-referenced DIAL definitions into the ECS dialogue runtime"
            );
        }
        let mut package_ids: std::collections::HashSet<u32> = index
            .scenes
            .values()
            .flat_map(|scene| &scene.actions)
            .flat_map(|action| action.packages.iter().copied())
            .collect();
        let template_ids: Vec<u32> = package_ids
            .iter()
            .filter_map(|form_id| index.packages.get(form_id))
            .filter_map(|package| package.package_template_form_id)
            .collect();
        package_ids.extend(template_ids);
        // Travel/Patrol/Escort packages and native cart tethers overwhelmingly
        // target invisible XMarkers. Those references are intentionally not
        // render-spawned, so retain authored coordinates and XLKR edges in a
        // lightweight registry rather than requiring live ECS entities.
        let package_target_ids: std::collections::HashSet<u32> = package_ids
            .iter()
            .filter_map(|form_id| index.packages.get(form_id))
            .flat_map(|package| &package.data_inputs)
            .filter_map(|input| match input.value {
                byroredux_plugin::esm::records::PackDataValue::Location(location) => {
                    match location.target {
                        byroredux_plugin::esm::records::PackLocationTarget::NearReference(
                            form_id,
                        ) => Some(form_id),
                        _ => None,
                    }
                }
                byroredux_plugin::esm::records::PackDataValue::Target(target) => {
                    match target.target {
                        byroredux_plugin::esm::records::PackDataTargetKind::SpecificReference(
                            form_id,
                        )
                        | byroredux_plugin::esm::records::PackDataTargetKind::LinkedReference(
                            form_id,
                        ) => Some(form_id),
                        _ => None,
                    }
                }
                _ => None,
            })
            .collect();
        let mut linked_references: Vec<(u32, Vec<(u32, u32)>)> = Vec::new();
        let mut collect_cell_links = |cell: &byroredux_plugin::esm::cell::CellData| {
            linked_references.extend(
                cell.references
                    .iter()
                    .filter(|placed| !placed.linked_refs.is_empty())
                    .map(|placed| {
                        (
                            placed.form_id,
                            placed
                                .linked_refs
                                .iter()
                                .map(|link| (link.keyword, link.target))
                                .collect(),
                        )
                    }),
            );
        };
        for cell in index.cells.cells.values() {
            collect_cell_links(cell);
        }
        for grids in index.cells.exterior_cells.values() {
            for cell in grids.values() {
                collect_cell_links(cell);
            }
        }
        for cell in index.cells.worldspace_persistent_cells.values() {
            collect_cell_links(cell);
        }

        let mut retained_reference_ids = package_target_ids.clone();
        for (source, links) in &linked_references {
            retained_reference_ids.insert(*source);
            retained_reference_ids.extend(links.iter().map(|(_, target)| *target));
        }
        let mut reference_positions = Vec::new();
        let mut reference_directions = Vec::new();
        let mut collect_cell_targets = |cell: &byroredux_plugin::esm::cell::CellData| {
            for placed in cell
                .references
                .iter()
                .filter(|placed| retained_reference_ids.contains(&placed.form_id))
            {
                reference_positions.push((
                    placed.form_id,
                    byroredux_core::math::Vec3::from_array(
                        byroredux_core::math::coord::zup_to_yup_pos(placed.position),
                    ),
                ));
                reference_directions.push((
                    placed.form_id,
                    crate::cell_loader::rotation_zup_to_yup_quat(placed.rotation)
                        * -byroredux_core::math::Vec3::Z,
                ));
            }
        };
        for cell in index.cells.cells.values() {
            collect_cell_targets(cell);
        }
        for grids in index.cells.exterior_cells.values() {
            for cell in grids.values() {
                collect_cell_targets(cell);
            }
        }
        for cell in index.cells.worldspace_persistent_cells.values() {
            collect_cell_targets(cell);
        }
        let package_target_count = reference_positions
            .iter()
            .filter(|(form_id, _)| package_target_ids.contains(form_id))
            .count();
        let reference_count =
            byroredux_scripting::install_package_target_positions(world, reference_positions);
        byroredux_scripting::install_package_target_directions(world, reference_directions);
        let route_edge_count =
            byroredux_scripting::install_package_linked_references(world, linked_references);
        if !package_ids.is_empty() {
            log::info!(
                "Prepared {} scene PACK references, {package_target_count}/{} authored movement targets, and {route_edge_count} linked routes across {reference_count} placed refs",
                package_ids.len(),
                package_target_ids.len()
            );
        }
        let count =
            byroredux_scripting::install_scene_records(world, index.scenes.values().cloned());
        log::info!("Installed {count} SCEN definitions into the ECS scene runtime");
        let fragment_count = populate_scene_fragments(world, index);
        if fragment_count > 0 {
            log::info!(
                "Populated {fragment_count} lowered SCEN lifecycle fragments from compiled scripts"
            );
        }
    }
}

/// Materialize transform-bearing identities for actors required by one scene
/// when their canonical persistent references live outside the streamed cell
/// footprint.
///
/// Bethesda keeps forced quest aliases alive independently of ordinary cell
/// residency.  ByroRedux streams geometry by cell, so a new-game scene such
/// as MQ101 otherwise cannot bind Tullius/Ulfric/Elenwen (their canonical
/// ACHRs are owned by remote interiors) and its stage-0 `Is3DLoaded` poll
/// never resumes. These lightweight identities use the same canonical
/// candidate stamper as model-less REFRs; subsequent fragment/package
/// effects can move them without pretending their remote home cell is loaded.
pub(crate) fn materialize_scene_actor_alias_stubs(
    world: &mut byroredux_core::ecs::world::World,
    index: &byroredux_plugin::esm::records::EsmIndex,
    load_order: &[String],
    quest_form_id: u32,
    scene_form_id: u32,
) -> usize {
    use byroredux_plugin::esm::records::AliasFillType;

    let (Some(quest), Some(scene)) = (
        index.quests.get(&quest_form_id),
        index.scenes.get(&scene_form_id),
    ) else {
        return 0;
    };
    let actor_aliases: std::collections::HashSet<i32> = scene
        .actors
        .iter()
        .map(|actor| actor.actor_id as i32)
        .collect();
    let fills: Vec<AliasFillType> = quest
        .aliases
        .iter()
        .filter(|alias| actor_aliases.contains(&alias.alias_id))
        .filter_map(|alias| alias.fill_type.clone())
        .collect();
    let mut existing: std::collections::HashSet<u32> = world
        .query::<byroredux_scripting::SceneAliasCandidate>()
        .map(|query| {
            query
                .iter()
                .map(|(_, candidate)| candidate.reference_form_id)
                .collect()
        })
        .unwrap_or_default();
    let matches_fill = |placed: &byroredux_plugin::esm::cell::PlacedRef| {
        fills.iter().any(|fill| match fill {
            AliasFillType::ForcedReference(reference) => placed.form_id == *reference,
            AliasFillType::UniqueActor(base) => placed.base_form_id == *base,
            AliasFillType::LocationAliasReference {
                ref_type: Some(ref_type),
                ..
            } => placed.location_ref_types.contains(ref_type),
            _ => false,
        })
    };
    let mut missing = Vec::new();
    let mut collect = |cell: &byroredux_plugin::esm::cell::CellData| {
        missing.extend(
            cell.references
                .iter()
                .filter(|placed| matches_fill(placed))
                .filter(|placed| !existing.contains(&placed.form_id))
                .cloned(),
        );
    };
    for cell in index.cells.cells.values() {
        collect(cell);
    }
    for grids in index.cells.exterior_cells.values() {
        for cell in grids.values() {
            collect(cell);
        }
    }
    for cell in index.cells.worldspace_persistent_cells.values() {
        collect(cell);
    }
    missing.sort_by_key(|placed| placed.form_id);
    missing.dedup_by_key(|placed| placed.form_id);

    let mut spawned = 0;
    for placed in missing {
        if !existing.insert(placed.form_id) {
            continue;
        }
        let entity = crate::cell_loader::references::spawn_logical_quest_reference(
            world,
            &placed,
            load_order,
            crate::cell_loader::position_zup_to_yup(placed.position),
            crate::cell_loader::rotation_zup_to_yup_quat(placed.rotation),
            placed.scale,
        );
        log::debug!(
            "Materialized remote scene actor entity={entity} ref=0x{:08X} base=0x{:08X} links={:?}",
            placed.form_id,
            placed.base_form_id,
            placed.linked_refs
        );
        world.insert(entity, byroredux_scripting::RemoteSceneActorStub);
        spawned += 1;
    }
    if spawned > 0 {
        log::info!(
            "Materialized {spawned} remote actor alias identities for scene 0x{scene_form_id:08X}"
        );
    }
    spawned
}

/// Build a [`ScriptProvider`] from CLI arguments. Accepts repeated
/// `--scripts-bsa <path>` flags so modded script archives can layer over
/// the vanilla one (first hit wins, so list overrides before the base).
/// Silently returns an empty provider when no flag is present.
pub(crate) fn build_script_provider(args: &[String]) -> ScriptProvider {
    let mut provider = ScriptProvider::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--scripts-bsa" {
            if let Some(path) = args.get(i + 1) {
                match Archive::open(path) {
                    Ok(a) => {
                        log::info!("Opened script archive: '{path}'");
                        provider.archives.push(a);
                    }
                    Err(e) => log::warn!("Failed to open script archive '{path}': {e}"),
                }
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    provider
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn populate_scene_runtime_installs_ambient_packages_without_scenes() {
        let mut world = byroredux_core::ecs::World::new();
        byroredux_scripting::register(&mut world);
        let mut index = byroredux_plugin::esm::records::EsmIndex::default();
        index.packages.insert(
            0x1234,
            byroredux_plugin::esm::records::PackRecord {
                form_id: 0x1234,
                ..Default::default()
            },
        );

        populate_scene_runtime(&mut world, &index);

        assert!(
            world
                .resource::<byroredux_scripting::PackageRegistry>()
                .package(0x1234)
                .is_some(),
            "NPC_.PKID packages must remain available even when no SCEN references them"
        );
    }
}
