//! BSA script ingress and QUST-bound fragment / IMAD / two-state-gate checks.

use super::*;

/// The opened script archive plus corpus counts the inventory reports.
pub(super) struct ScriptCorpus {
    pub(super) scripts: BsaArchive,
    pub(super) mq101_pex_count: usize,
    pub(super) scene_count: usize,
    pub(super) package_count: usize,
}

/// Stage 5 — BSA script ingress: the stage-22 specific-actor trigger, the
/// MQ101 PEX corpus, and SCEN-bound script-name linkage.
pub(super) fn check_scripts(
    checks: &mut Checks,
    index: &EsmIndex,
    scripts_path: &Path,
    inventory: &SceneInventory,
) -> Result<ScriptCorpus, Box<dyn Error>> {
    let scripts = BsaArchive::open(scripts_path)?;
    let stage22_trigger = index
        .cells
        .cells
        .values()
        .flat_map(|cell| &cell.references)
        .chain(
            index
                .cells
                .exterior_cells
                .values()
                .flat_map(|grids| grids.values())
                .flat_map(|cell| &cell.references),
        )
        .chain(
            index
                .cells
                .worldspace_persistent_cells
                .values()
                .flat_map(|cell| &cell.references),
        )
        .find(|placed| placed.form_id == 0x000C_1F80);
    let stage22_recognized = stage22_trigger
        .and_then(|placed| placed.script_instance.as_ref())
        .and_then(|vmad| {
            scripts
                .extract("scripts\\defaultsetstagetrigspecificactor.pex")
                .ok()
                .and_then(|pex| translate_pex(&pex, index.game, Some(vmad), None))
        });
    let mut trigger_world = World::new();
    byroredux_scripting::register(&mut trigger_world);
    let trigger_entity = trigger_world.spawn();
    if let Some(recognized) = &stage22_recognized {
        (recognized.spawn)(&mut trigger_world, trigger_entity);
    }
    let trigger_component = trigger_world.get::<QuestAdvanceOnActivate>(trigger_entity);
    checks.record(
        "specific actor trigger",
        trigger_component.is_some_and(|component| {
            component.owning_quest == QuestFormId(MQ101_FORM_ID)
                && component.target_stage == 22
                && component.conditions.len() == 1
                && component.conditions[0].param_2 == 15
                && matches!(
                    component.activator_gate,
                    ActivatorGate::BaseForm(0x0006_54E5)
                )
                && component.disable_after_advance
        }),
        "0x000C1F80: MQ101Horse base gate + prereq 15 -> stage 22 + disable",
    );
    let mut mq101_pex: Vec<String> = scripts
        .list_files()
        .into_iter()
        .filter(|path| is_mq101_pex(path))
        .map(str::to_owned)
        .collect();
    mq101_pex.sort();
    let scene_count = mq101_pex
        .iter()
        .filter(|path| is_scene_fragment_pex(path))
        .count();
    let package_count = mq101_pex
        .iter()
        .filter(|path| is_package_fragment_pex(path))
        .count();
    checks.record(
        "MQ101 PEX corpus",
        !mq101_pex.is_empty(),
        format!(
            "{} scripts ({scene_count} scene fragments, {package_count} package fragments)",
            mq101_pex.len()
        ),
    );
    checks.record(
        "scene fragment assets",
        scene_count > 0,
        format!("{scene_count} SF_MQ101*.pex files"),
    );
    let mut missing_scene_fragment_scripts: Vec<String> = inventory
        .scene_fragment_scripts
        .iter()
        .filter_map(|script_name| {
            let path = format!("scripts\\{script_name}.pex");
            (!scripts.contains(&path)).then_some(path)
        })
        .collect();
    missing_scene_fragment_scripts.sort();
    checks.record(
        "scene fragment linkage",
        missing_scene_fragment_scripts.is_empty(),
        if missing_scene_fragment_scripts.is_empty() {
            format!(
                "all {} SCEN-bound script names resolve to PEX assets",
                inventory.scene_fragment_scripts.len()
            )
        } else {
            format!(
                "{} missing: {}",
                missing_scene_fragment_scripts.len(),
                preview(&missing_scene_fragment_scripts, 8)
            )
        },
    );
    Ok(ScriptCorpus {
        mq101_pex_count: mq101_pex.len(),
        scene_count,
        package_count,
        scripts,
    })
}

/// Stage 6 — fragment and callback contracts: the Scene1 end handoff
/// lowering, critical-script presence, and the decompiled
/// MQ101PlayerScript animation-event contract.
pub(super) fn check_fragments(checks: &mut Checks, scenes: &SceneSet, corpus: &ScriptCorpus) {
    let scripts = &corpus.scripts;
    let scene_1_end_effects = scenes
        .scenes
        .iter()
        .find(|scene| scene.form_id == 0x000B_ECD4)
        .and_then(|scene| {
            scene
                .fragments
                .iter()
                .find(|binding| binding.event == SceneFragmentEvent::End)
        })
        .and_then(|binding| {
            let path = format!("scripts\\{}.pex", binding.script_name);
            let script = scripts
                .extract(&path)
                .ok()
                .and_then(|bytes| parse(&bytes).ok())
                .and_then(|pex| decompile_script(&pex).ok())?;
            let properties = quest_property_names(&script);
            let function = script.body.iter().find_map(|item| match &item.node {
                ScriptItem::Function(function)
                    if function
                        .name
                        .node
                        .0
                        .eq_ignore_ascii_case(&binding.fragment_name) =>
                {
                    Some(function)
                }
                _ => None,
            })?;
            lower_fragment_with_quest_properties(&function.body, &properties)
        });
    let scene_1_end_ok = matches!(
        scene_1_end_effects.as_deref(),
        Some([Effect::Conditional { guards, then_effects, else_effects }])
            if guards == &[
                FragmentGuard::StageDone { quest: QuestRef::OwningQuest, stage: 2, done: false },
                FragmentGuard::StageDone { quest: QuestRef::OwningQuest, stage: 6, done: false },
                FragmentGuard::StageDone { quest: QuestRef::OwningQuest, stage: 7, done: false },
            ]
            && matches!(then_effects.as_slice(), [Effect::SetStage { quest: QuestRef::OwningQuest, stage: 35 }])
            && else_effects.is_empty()
    );
    checks.record(
        "Scene1 end handoff",
        scene_1_end_ok,
        scene_1_end_effects.map_or_else(
            || "Scene1 Fragment_6 did not lower".to_owned(),
            |effects| format!("Scene1 Fragment_6 lowered to {effects:?}"),
        ),
    );
    checks.record(
        "package fragment assets",
        corpus.package_count > 0,
        format!("{} PF_MQ101*.pex files", corpus.package_count),
    );

    let missing_scripts: Vec<String> = CRITICAL_SCRIPTS
        .iter()
        .filter(|path| !scripts.contains(path))
        .map(|path| (*path).to_owned())
        .collect();
    checks.record(
        "critical scripts",
        missing_scripts.is_empty(),
        if missing_scripts.is_empty() {
            format!("all {} present", CRITICAL_SCRIPTS.len())
        } else {
            format!("missing: {}", preview(&missing_scripts, 5))
        },
    );

    let callback_script = scripts
        .extract(CRITICAL_SCRIPTS[1])
        .ok()
        .and_then(|bytes| parse(&bytes).ok())
        .and_then(|pex| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decompile_script(&pex)))
                .ok()?
                .ok()
        });
    checks.record(
        "player callback contract",
        callback_script
            .as_ref()
            .is_some_and(mq101_player_callback_contract),
        "PlayImod => IMAD x2 + stage 145; IdleFurnitureExit => stage 160; both one-shot",
    );
}

/// Stage 7 — image-space modifier linkage, authored curves, and the
/// PlayImod dispatch through the cinematic presentation state.
pub(super) fn check_imad_runtime(checks: &mut Checks, index: &EsmIndex, quest: &QustRecord) {
    let callback_vmad = quest
        .script_instance
        .as_ref()
        .and_then(|vmad| vmad.script("MQ101QuestScript"));
    let modifier_forms = ["PlayerAlduinIMOD", "CGDragonAttackBlurLong"].map(|property| {
        (
            property,
            callback_vmad.and_then(|script| script.object_form_id(property)),
        )
    });
    let modifier_links_resolve = modifier_forms.iter().all(|(_, form_id)| {
        form_id.is_some_and(|form_id| index.imagespace_modifiers.contains_key(&form_id))
    });
    checks.record(
        "callback IMAD linkage",
        modifier_links_resolve,
        modifier_forms
            .iter()
            .map(|(property, form_id)| {
                form_id.map_or_else(
                    || format!("{property}=missing"),
                    |form_id| format!("{property}={form_id:08X}"),
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
    );
    let authored_imads: Vec<_> = modifier_forms
        .iter()
        .filter_map(|(_, form_id)| form_id.and_then(|id| index.imagespace_modifiers.get(&id)))
        .cloned()
        .collect();
    let curve_contract = authored_imads.len() == 2
        && authored_imads
            .iter()
            .all(|record| record.duration_seconds > 0.0 && !record.blur_radius.is_empty())
        && authored_imads
            .iter()
            .any(|record| !record.radial_blur_strength.is_empty());
    checks.record(
        "callback IMAD curves",
        curve_contract,
        authored_imads
            .iter()
            .map(|record| {
                format!(
                    "{}={:.1}s/{} blur keys/{} radial keys",
                    record.editor_id,
                    record.duration_seconds,
                    record.blur_radius.len(),
                    record.radial_blur_strength.len()
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
    );

    let mut imad_world = World::new();
    byroredux_scripting::register(&mut imad_world);
    imad_world.insert_resource(QuestStageState::default());
    install_image_space_modifiers(&mut imad_world, authored_imads);
    let applications: Vec<_> = modifier_forms
        .iter()
        .filter_map(|(_, form_id)| {
            form_id.map(|form_id| ImageSpaceModifierApplication {
                form_id,
                strength: 1.0,
            })
        })
        .collect();
    imad_world
        .resource_mut::<CinematicPresentationState>()
        .register_player_animation_event(
            CinematicAnimationEvent::PlayImod,
            QuestFormId(MQ101_FORM_ID),
            applications,
        );
    let callback_dispatched =
        dispatch_player_cinematic_animation_event(&imad_world, CinematicAnimationEvent::PlayImod)
            .is_some();
    image_space_modifier_system(&imad_world, 1.0);
    let sampled_imad = imad_world
        .resource::<CinematicPresentationState>()
        .image_space_modifier_frame;
    checks.record(
        "callback IMAD runtime",
        callback_dispatched
            && sampled_imad.blur_radius_pixels > 0.0
            && sampled_imad.radial_blur_strength > 0.0,
        format!(
            "t=1.0s blur={:.3}px radial={:.3} saturation={:.3}",
            sampled_imad.blur_radius_pixels,
            sampled_imad.radial_blur_strength,
            sampled_imad.saturation
        ),
    );
}

/// Stage 8 — the opening two-state gate recognized from the vanilla
/// default2StateActivator PEX against its placed VMAD instance.
pub(super) fn check_two_state_gate(checks: &mut Checks, index: &EsmIndex, scripts: &BsaArchive) {
    let gate_script_instance = index
        .cells
        .cells
        .values()
        .flat_map(|cell| &cell.references)
        .find(|reference| reference.form_id == MQ101_GATE_1)
        .and_then(|reference| reference.script_instance.as_ref());
    let two_state_recognition = scripts
        .extract(TWO_STATE_ACTIVATOR_SCRIPT)
        .ok()
        .and_then(|bytes| translate_pex(&bytes, index.game, gate_script_instance, None));
    checks.record(
        "two-state gate runtime",
        two_state_recognition
            .as_ref()
            .is_some_and(|recognized| recognized.archetype.starts_with("two_state_activator@")),
        two_state_recognition.map_or_else(
            || "default2StateActivator PEX did not recognize".to_owned(),
            |recognized| format!("{} recognized from vanilla PEX", recognized.archetype),
        ),
    );
}

/// Stage 9 — decompile the QF_MQ101 quest-fragment script and audit the
/// authored per-stage fragments against the production lowering path.
pub(super) fn check_quest_fragments(checks: &mut Checks, quest: &QustRecord, scripts: &BsaArchive) {
    let Some(script) = decompile_qf_script(checks, scripts) else {
        return;
    };
    let functions: HashMap<String, _> = script
        .body
        .iter()
        .filter_map(|item| match &item.node {
            ScriptItem::Function(function) => {
                Some((function.name.node.0.to_ascii_lowercase(), function))
            }
            _ => None,
        })
        .collect();
    // #2658 (SCR-D5-NEW11-03) — computed once per script,
    // matching the production caller
    // (`populate_quest_fragments_from_script`), and fed to
    // `lower_fragment_with_quest_properties` at both call
    // sites below so this conformance gate measures the
    // SAME lowering path production runs.
    let quest_properties = quest_property_names(&script);

    let coverage = analyze_fragment_bindings(checks, quest, &functions, &quest_properties);
    check_stage_zero_handoff(checks, &functions, &quest_properties);
    check_stage_zero_runtime_chain(checks, quest, &script);
    check_cart_init_fragment(checks, &functions, &quest_properties);
    check_stage_thirty_handoff(checks, &functions, &quest_properties);
    print_fragment_coverage(&coverage);
}
