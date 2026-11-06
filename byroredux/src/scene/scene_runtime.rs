//! Per-load scene runtime install (#6716: moved out of `asset_provider::script`).
//!
//! `populate_scene_runtime` is the one funnel both loaders (interior `finish`, exterior
//! `assemble_exterior_streaming`) go through to install the plugin-derived runtime tables;
//! it names the gameplay systems that own those tables, so it belongs with scene setup, not
//! with the archive-extraction layer. The fragment/script lowering helpers it calls stay in
//! `asset_provider::script`, whose job is script lookup.

use crate::asset_provider::{
    populate_info_fragments, populate_package_fragments, populate_perk_fragments,
    populate_quest_fragments, populate_quest_trigger_approaches, populate_scene_fragments,
    populate_terminal_fragments, VmAliasScriptChecks,
};

pub(crate) fn populate_scene_runtime(
    world: &mut byroredux_core::ecs::world::World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) {
    // #3010 / #3161 — M47.2 keystone. Lives here, alongside its siblings,
    // rather than at each loader's call site: #3010 was a HIGH that survived
    // every prior audit precisely because the interior loader called it and
    // the exterior loader did not, and nothing pinned either call. Fixing
    // that by adding a *second* external call site left the drift surface
    // unchanged in kind and one site wider. Both loader entry points already
    // funnel through this function, so this is the one place that cannot
    // drift from the three siblings below.
    populate_quest_fragments(world, index);
    populate_info_fragments(world, index);
    populate_package_fragments(world, index);
    populate_terminal_fragments(world, index);
    populate_perk_fragments(world, index);
    crate::inventory::install_catalog(world, index);
    // M43 — Skyrim `COBJ` recipes and the keywords that make a furniture a crafting bench.
    crate::lockpick::install_lockpick_item(world, index);
    crate::armor::install_armor_book(world, index);
    crate::lockpick::install_terminal_bases(world, index);
    crate::systems::mines::install_mine_book(world, index);
    crate::systems::attraction::install_attraction_book(world, index);
    crate::systems::random_conversation::install_complex_scene_object(world, index);
    crate::systems::iron_sights::install_aim_weapons(world, index);
    let recipes = crate::crafting::install_crafting(world, index);
    if recipes > 0 {
        log::info!("Installed {recipes} crafting recipes");
    }
    // #4415 — canonical spells for runtime `AddSpell` / `RemoveSpell`.
    world.insert_resource(byroredux_scripting::SpellCatalog::from_index(index));
    // #4414 — the ambient-hostility detection range (`fSneakMaxDistance`).
    crate::systems::install_detection_config(world, index);
    // The per-game crime fines (Oblivion's theft bounty), cleared for a game without a model.
    crate::crime::install_crime_model(world, index);
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
    // M47.3 — ObScript quest scripts (Oblivion/FO3/FNV): resolve each
    // quest's `script_ref` to its parsed SCPT once per load order so the
    // 5 s GameMode tick can execute it against live quest state.
    byroredux_scripting::obscript_quests::install_quest_scripts(
        world,
        &index.quests,
        &index.scripts,
    );
    // M47.3 — the result scripts embedded in their stage items, run when the
    // stage is set (the Fallout-era quests' objectives live there).
    byroredux_scripting::obscript_results::install_stage_result_scripts(world, &index.quests);
    // M47.3 — ObScript object scripts (the `SCRI` scripts on placed references).
    // Empty on Skyrim+, whose records carry VMAD instead.
    byroredux_scripting::obscript_objects::install_object_scripts(world, &index.scripts);
    // M47.3 — the scripts of items carried in inventories (OnAdd / OnDrop / OnEquip /
    // OnUnequip), found by the item's base form.
    byroredux_scripting::obscript_objects::install_item_scripts(world, index);
    // M47.3 — the scripts of scripted magic effects (ScriptEffectStart / Update / Finish).
    byroredux_scripting::magic_effects::install_effect_scripts(world, &index.scripts);
    // #5366 — the Story Manager tree (Skyrim+): fold the parsed
    // SMBN/SMEN/SMQN node map into the dispatchable resource. Empty and
    // a no-op on Oblivion/FO3/FNV, which author no SM records — their
    // quest autostart is the script path above.
    let sm_nodes = byroredux_scripting::story_manager::install_story_manager(
        world,
        &index.story_manager_nodes,
    );
    if sm_nodes > 0 {
        log::info!("Installed Story Manager tree: {sm_nodes} nodes");
    }
    // M47.2 — the location runtime (Skyrim+ LCTN): hierarchy, keywords and
    // the cell / editor-location maps the location conditions and location
    // aliases read. Empty on Oblivion/FO3/FNV, which author no LCTN.
    let locations = byroredux_scripting::install_location_table(world, index);
    if locations > 0 {
        log::info!("Installed location table: {locations} locations");
    }
    // M47.2 — the inventory item records a Created Object alias may create
    // "In" another alias's inventory.
    world.insert_resource(byroredux_scripting::InventoryItemForms(
        index
            .record_types
            .iter()
            .filter(|(_, kind)| {
                matches!(
                    *kind,
                    b"ALCH" | b"AMMO" | b"ARMO" | b"BOOK" | b"INGR" | b"KEYM" | b"MISC" | b"NOTE"
                        | b"SLGM" | b"WEAP"
                )
            })
            .map(|(&form_id, _)| form_id)
            .collect(),
    ));
    // M47.2 — quest script variables: the authored numeric property values
    // of every quest VMAD, the fallback `GetVMQuestVariable` reads until a
    // fragment writes the variable (writes survive a reinstall).
    let quest_variable_quests = byroredux_scripting::install_quest_script_variables(
        world,
        index
            .quests
            .values()
            .filter_map(|quest| Some((quest.form_id, quest.script_instance.as_ref()?))),
    );
    log::info!("Installed quest script variable defaults for {quest_variable_quests} quests");
    // M47.2 — perk entry-point composition table (FO3/FNV, Skyrim): the
    // value-math entries of every PERK, composed against an actor's ranked
    // perks whenever a calculation consults a hook.
    // M47.2 — map markers' authored visibility, for IsMapMarkerVisible.
    byroredux_scripting::install_map_markers(world, index);
    // M47.2 — RELA relationship ranks, for GetRelationshipRank.
    byroredux_scripting::install_relationships(world, index);
    // M47.2 — MESG messages, display names and globals, for Message.Show.
    let messages = byroredux_scripting::install_message_catalog(world, index);
    log::info!("Installed message catalog: {messages} messages");
    // M47.2 — base objects' keywords (actors with their race's), for
    // HasKeyword.
    let keyworded = byroredux_scripting::keywords::install_keyword_table(world, index);
    log::info!("Installed keyword table: {keyworded} base objects");
    // M47.2 — activate parents (levers, pull chains, tripwires -> what they open).
    let activate_edges = byroredux_scripting::install_activate_parent_table(world, index);
    log::info!("Installed activate parents: {activate_edges} parent -> child edges");
    // M47.2 — sound editor ids, for the SoundPlay.<name> notifies of animation graphs.
    byroredux_scripting::install_sound_editor_ids(world, index);
    // M47.2 — base objects' destructible data, for DamageObject.
    let destructible = byroredux_scripting::destruction::install_destruction_table(world, index);
    log::info!("Installed destruction table: {destructible} destructible base objects");
    // M47.2 — RACE subgraph data and the archetype keywords, for
    // ChangeAnimArchetype and the actor's graph selection.
    let archetypes = byroredux_scripting::anim_subgraph::install_subgraph_table(world, index);
    if archetypes > 0 {
        log::info!("Installed subgraph table: {archetypes} animation archetype keywords");
    }
    // M47.2 — vendor factions and barter prices, for ShowBarterMenu.
    let vendors = crate::barter::install_barter_rules(world, index);
    if vendors > 0 {
        log::info!("Installed barter rules: {vendors} vendor factions");
    }
    // M47.2 — every quest's VMAD, for quest-script bodies inlined into
    // INFO and scene fragments.
    byroredux_scripting::install_quest_script_instances(world, index);
    // M47.2 — Weather.GetClassification.
    byroredux_scripting::install_weather_classifications(world, index);
    // M47.2 — FormList.GetSize / GetAt / HasForm / Find.
    byroredux_scripting::install_form_lists(world, index);
    // M47.2 — GetFactionOwner / GetActorOwner.
    byroredux_scripting::install_owner_kinds(world, index);
    // Oblivion's guard classes (the `CLAS` Guard flag): `IsGuard`, `IsTurnArrest`.
    byroredux_scripting::guards::install_guard_classes(world, index);
    // The jails (linked `PrisonMarker` door pairs): `PayFine`, `GoToJail`.
    byroredux_scripting::jail::install_jail_table(world, index);
    // M47.2 — Actor.GetCrimeFaction.
    byroredux_scripting::install_crime_factions(world, index);
    // The factions' Crime tabs (Skyrim, FO4): what each tracks, ignores and fines.
    byroredux_scripting::crime_faction::install_crime_faction_data(world, index);
    // M47.2 — ActorBase.GetSex / GetVoiceType.
    byroredux_scripting::install_actor_base_traits(world, index);
    // M47.2 — the scripts quests attach to their aliases, checked when their
    // quest first runs.
    let alias_scripts = byroredux_scripting::install_quest_alias_scripts(world, index);
    if alias_scripts > 0 {
        log::info!("Installed {alias_scripts} quest alias scripts");
    }
    world.init_resource::<VmAliasScriptChecks>();
    // M47.2 — references that start disabled, for IsDisabled / IsEnabled.
    byroredux_scripting::install_authored_disabled_references(world, index);
    // M47.2 — Skyrim's SetAV("Name", v) resolves names through the AVIFs.
    byroredux_scripting::install_actor_value_names(world, index);
    // M47.2 — CalculateEncounterLevel (encounter zones + difficulty GMSTs).
    byroredux_scripting::install_encounter_level_rules(world, index);
    // M47.2 — Actor.GetBribeAmount (Skyrim's bribe settings).
    byroredux_scripting::install_bribe_rules(world, index);
    // M47.2 — Starfield ConditionForm.IsTrue (CNDF condition forms).
    byroredux_scripting::install_condition_forms(world, index);
    // M47.2 — ObjectReference.ApplyHavokImpulse (Skyrim's gameplay impulse settings).
    byroredux_scripting::install_havok_impulse_rules(world, index);
    // M47.2 — New Vegas Damage Threshold (armor DT + the DT actor value,
    // perk-composed); absent on load orders without it.
    if crate::combat::install_damage_threshold_rules(world, index) {
        log::info!("Installed Damage Threshold rules");
    }
    // M47.2 — encumbrance (CarryWeight against the carried weight).
    if crate::encumbrance::install_encumbrance_rules(world, index) {
        log::info!("Installed encumbrance rules");
    }
    // M47.2 — essential actors' bleedout timing (fEssentialDeathTime).
    byroredux_scripting::install_bleedout_rules(world, index);
    // M47.3 — `PushActorAway`'s force cap (`fKnockbackForceMax`).
    crate::systems::knockdown::install_force_max(world, index);
    // #6620 — the `MenuMode` codes that differ by game.
    crate::systems::script_queries::install_menu_codes(world, index);
    // M47.2 — Game.RewardPlayerXP's Intelligence modifier (FO4 GMSTs).
    crate::combat::install_xp_modifier_rules(world, index);
    // M47.2 — Fallout 3 / New Vegas kill experience (GMST level bands).
    if crate::combat::install_experience_rules(world, index) {
        log::info!("Installed kill experience rules");
    }
    let perk_entry_perks =
        byroredux_scripting::perk_entry_points::install_perk_entry_points(world, index);
    if perk_entry_perks > 0 {
        log::info!("Installed perk entry points: {perk_entry_perks} perks");
    }
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
    // #5271 — the INFO-level QSTI semantics need every quest's authored
    // priority (a multi-quest topic's INFOs compete by it), not just the
    // scene-referenced subset; installed unconditionally because the
    // activation dialogue route reads it without any SCEN in play.
    world.insert_resource(byroredux_scripting::DialogueQuestPriorities::from_quests(
        index.quests.values().map(|quest| (quest.form_id, quest.priority)),
    ));
    // #5734 — the quest-level dialogue gates beside the priorities that
    // order against them: the CK's Quest *Conditions* tab gates every
    // INFO the quest owns before its own per-INFO list, and without this
    // the priority ordering pre-empted every speaker with the
    // highest-priority quest's unconditioned greeting.
    let quest_condition_count = byroredux_scripting::install_dialogue_quest_conditions(
        world,
        index
            .quests
            .values()
            .filter(|quest| !quest.dialogue_conditions.is_empty())
            .map(|quest| (quest.form_id, quest.dialogue_conditions.clone())),
    );
    if quest_condition_count > 0 {
        log::info!("Installed dialogue quest gates: {quest_condition_count} quests");
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
                    crate::zup_yup::rotation_zup_to_yup_quat(placed.rotation)
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
    load_order: &crate::cell_loader::load_order::LoadOrder,
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
    let mut scripted = 0;
    for placed in missing {
        if !existing.insert(placed.form_id) {
            continue;
        }
        // #4331 (SCR-D7-2026-09-14-03) — this was the second identity-only
        // stub path, and like the persistent-worldspace one (#4112) it never
        // called an `attach_*` function, so these actors' base SCRI/VMAD and
        // the ACHR's own VMAD were dropped while they were stub-only. Both now
        // route through the one helper that cannot spawn without attaching.
        let (entity, attached) =
            crate::cell_loader::references::spawn_logical_quest_reference_with_scripts(
                world,
                &placed,
                load_order,
                index,
                crate::zup_yup::position_zup_to_yup(placed.position),
                crate::zup_yup::rotation_zup_to_yup_quat(placed.rotation),
                placed.scale,
            );
        log::debug!(
            "Materialized remote scene actor entity={entity} ref=0x{:08X} base=0x{:08X} scripts={attached} links={:?}",
            placed.form_id,
            placed.base_form_id,
            placed.linked_refs
        );
        world.insert(entity, byroredux_scripting::RemoteSceneActorStub);
        spawned += 1;
        if attached {
            scripted += 1;
        }
    }
    if spawned > 0 {
        log::info!(
            "Materialized {spawned} remote actor alias identities ({scripted} with scripts) for scene 0x{scene_form_id:08X}"
        );
    }
    spawned
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #4331 (SCR-D7-2026-09-14-03) — `materialize_scene_actor_alias_stubs`
    /// was the second identity-only stub path, spawning a scene actor and
    /// never attaching its scripts. Driving it needs a real `EsmIndex` with a
    /// Skyrim QUST/SCEN pair and its home cells, out of `cargo test` scope, so
    /// the wiring is pinned the way `exterior.rs` pins its twin; the helper's
    /// own behavior is covered by
    /// `logical_quest_reference_spawns_identity_and_attaches_scripts`.
    #[test]
    fn scene_actor_alias_stubs_spawn_through_the_script_attaching_helper() {
        // #5650 — production text only (this file carries one trailing test
        // module, so the first-cut split is exact): a whole-file scan is
        // half-vacuous, the positive `contains` satisfiable by this
        // assertion's own literal even with the production call deleted.
        let src = byroredux_core::source_scan::production_text(include_str!("scene_runtime.rs"));

        assert!(
            src.contains("references::spawn_logical_quest_reference_with_scripts("),
            "the scene-actor alias stub must call the shared spawner that also \
             attaches scripts (#4331)"
        );
        // The identity-only spawner's call syntax must be absent from
        // production text entirely — a call site here drops the base
        // record's SCRI/VMAD and the ACHR's own VMAD (#4331).
        assert_eq!(
            src.matches("spawn_logical_quest_reference(").count(),
            0,
            "the identity-only spawner must not appear on this path — it drops \
             the base record's SCRI/VMAD and the ACHR's own VMAD (#4331)"
        );
    }

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

    /// #3161 — pins that the M47.2 quest-fragment walk is actually reached
    /// from `populate_scene_runtime`.
    ///
    /// #3010 (a HIGH) was that the interior loader called the walk and the
    /// exterior loader did not; it survived every prior audit because
    /// nothing pinned the call at all. The first fix added a second external
    /// call site, which left that condition intact and one site wider — a
    /// single refactor of the streaming job from silently reverting. Folding
    /// the walk into `populate_scene_runtime` is only half the fix; this is
    /// the half that fails if it is lifted back out.
    ///
    /// The observable is the populated latch rather than a lowered fragment,
    /// because lowering a real `QF_` body needs a script archive and game
    /// data. The latch is set by `populate_quest_fragments` and by nothing
    /// else, so it is a faithful witness that the call happened.
    #[test]
    fn populate_scene_runtime_runs_the_quest_fragment_walk() {
        let mut world = byroredux_core::ecs::World::new();
        byroredux_scripting::register(&mut world);
        let index = byroredux_plugin::esm::records::EsmIndex::default();

        assert!(
            !world
                .resource::<byroredux_scripting::QuestStageFragments>()
                .is_populated(),
            "precondition: the walk has not run yet"
        );

        populate_scene_runtime(&mut world, &index);

        assert!(
            world
                .resource::<byroredux_scripting::QuestStageFragments>()
                .is_populated(),
            "populate_scene_runtime must reach populate_quest_fragments — without it \
             exterior launches silently lose every quest-stage handler (#3010)"
        );
    }
}
