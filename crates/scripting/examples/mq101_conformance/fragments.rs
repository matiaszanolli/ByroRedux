//! QF fragment decompile, lowering coverage and stage handoff checks.

use super::*;

/// Extract, parse, and decompile the QF script, recording the first
/// failing stage under the shared "QF decompile" check name.
pub(super) fn decompile_qf_script(checks: &mut Checks, scripts: &BsaArchive) -> Option<Script> {
    let qf_path = CRITICAL_SCRIPTS[0];
    let bytes = match scripts.extract(qf_path) {
        Err(error) => {
            checks.record("QF decompile", false, format!("extract failed: {error}"));
            return None;
        }
        Ok(bytes) => bytes,
    };
    let pex = match parse(&bytes) {
        Err(error) => {
            checks.record("QF decompile", false, format!("PEX parse failed: {error:?}"));
            return None;
        }
        Ok(pex) => pex,
    };
    let decompiled =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decompile_script(&pex)));
    match decompiled {
        Err(_) => {
            checks.record("QF decompile", false, "decompiler panicked");
            None
        }
        Ok(Err(error)) => {
            checks.record(
                "QF decompile",
                false,
                format!("AST lowering failed: {error:?}"),
            );
            None
        }
        Ok(Ok(script)) => Some(script),
    }
}

/// Effect-lowering coverage tallies for the informational report.
#[derive(Default)]
pub(super) struct FragmentCoverage {
    pub(super) no_op: usize,
    pub(super) behavioral: usize,
    pub(super) lowered: usize,
    pub(super) lowered_with_placeholder: usize,
    pub(super) effects: BTreeMap<&'static str, usize>,
    pub(super) declined_shapes: BTreeMap<String, usize>,
    pub(super) declined_samples: Vec<String>,
}

/// Lower every QUST-bound fragment, tallying no-op/behavioral/lowered
/// counts and the declined top-level statement shapes.
pub(super) fn analyze_fragment_bindings(
    checks: &mut Checks,
    quest: &QustRecord,
    functions: &HashMap<String, &Function>,
    quest_properties: &HashSet<String>,
) -> FragmentCoverage {
    let mut missing_bindings = Vec::new();
    // #4328 — see `Effect::is_placeholder`.
    let mut coverage = FragmentCoverage::default();

    for binding in &quest.fragments {
        let key = binding.fragment_name.to_ascii_lowercase();
        let Some(function) = functions.get(&key) else {
            missing_bindings.push(binding.fragment_name.clone());
            continue;
        };
        if function.body.is_empty() {
            coverage.no_op += 1;
            continue;
        }
        coverage.behavioral += 1;
        if let Some(fragment_effects) =
            lower_fragment_with_quest_properties(&function.body, quest_properties)
        {
            coverage.lowered += 1;
            if fragment_effects.iter().any(Effect::contains_placeholder) {
                coverage.lowered_with_placeholder += 1;
            }
            for effect in &fragment_effects {
                effect.visit_nested(&mut |n| *coverage.effects.entry(n.kind()).or_default() += 1);
            }
        } else {
            let shape = function
                .body
                .iter()
                .map(|statement| statement_shape(&statement.node))
                .collect::<Vec<_>>()
                .join(";");
            *coverage.declined_shapes.entry(shape.clone()).or_default() += 1;
            if coverage.declined_samples.len() < 16 {
                coverage
                    .declined_samples
                    .push(format!("{} {}", binding.fragment_name, shape));
            }
        }
    }

    checks.record(
        "QF decompile",
        true,
        format!("{} callable functions recovered", functions.len()),
    );
    checks.record(
        "fragment linkage",
        missing_bindings.is_empty(),
        if missing_bindings.is_empty() {
            format!(
                "all {} QUST bindings resolve in the QF PEX",
                quest.fragments.len()
            )
        } else {
            format!(
                "{} missing: {}",
                missing_bindings.len(),
                preview(&missing_bindings, 8)
            )
        },
    );
    coverage
}

/// The authored stage-0 handoff (Fragment_2): set GameHour, then start the
/// quest at stage 10.
pub(super) fn check_stage_zero_handoff(
    checks: &mut Checks,
    functions: &HashMap<String, &Function>,
    quest_properties: &HashSet<String>,
) {
    let startup_effects = functions.get("fragment_2").and_then(|function| {
        lower_fragment_with_quest_properties(&function.body, quest_properties)
    });
    let startup_ok = matches!(
        startup_effects.as_deref(),
        Some([
            Effect::SetGlobalValue { global, value },
            Effect::SetStage { quest: QuestRef::SelfRef, stage: 10 },
        ]) if global.property_name().eq_ignore_ascii_case("GameHour")
            && matches!(value, ValueExpr::Literal(v) if (*v - 7.0).abs() <= f32::EPSILON)
    );
    checks.record(
        "stage 0 handoff",
        startup_ok,
        startup_effects.map_or_else(
            || "Fragment_2 did not lower".to_owned(),
            |effects| format!("Fragment_2 lowered to {effects:?}"),
        ),
    );
}

/// The same stage-0 handoff aggregated through the production
/// `populate_quest_fragments_from_script` runtime path.
pub(super) fn check_stage_zero_runtime_chain(
    checks: &mut Checks,
    quest: &QustRecord,
    script: &Script,
) {
    let bindings = quest
        .fragments
        .iter()
        .map(|binding| (binding.stage, binding.fragment_name.as_str()))
        .collect::<Vec<_>>();
    let mut runtime_fragments = QuestStageFragments::default();
    populate_quest_fragments_from_script(
        &mut runtime_fragments,
        QuestFormId(MQ101_FORM_ID),
        script,
        &bindings,
    );
    let runtime_stage_zero = runtime_fragments
        .get(QuestFormId(MQ101_FORM_ID), 0)
        .unwrap_or_default();
    let runtime_handoff_ok = runtime_stage_zero.windows(2).any(|pair| {
        matches!(
            pair,
            [
                Effect::SetGlobalValue { global, value },
                Effect::SetStage {
                    quest: QuestRef::SelfRef,
                    stage: 10,
                },
            ] if global.property_name().eq_ignore_ascii_case("GameHour")
                && matches!(value, ValueExpr::Literal(v) if (*v - 7.0).abs() <= f32::EPSILON)
        )
    });
    checks.record(
        "stage 0 runtime chain",
        runtime_handoff_ok,
        format!(
            "{} aggregate effects from duplicate stage bindings",
            runtime_stage_zero.len()
        ),
    );
}

/// The cart-init fragment (Fragment_175): actor 3D-load gate, horse
/// tethers, vehicle assignment, and idle playback.
pub(super) fn check_cart_init_fragment(
    checks: &mut Checks,
    functions: &HashMap<String, &Function>,
    quest_properties: &HashSet<String>,
) {
    let cart_init_effects = functions.get("fragment_175").and_then(|function| {
        lower_fragment_with_quest_properties(&function.body, quest_properties)
    });
    let cart_init_kinds = cart_init_effects.as_ref().map(|effects| {
        let mut kinds = BTreeMap::<&'static str, usize>::new();
        for effect in effects {
            *kinds.entry((effect).kind()).or_default() += 1;
        }
        kinds
    });
    let cart_init_actor_gate_count =
        cart_init_effects.as_ref().and_then(|effects| {
            effects.iter().find_map(|effect| match effect {
                Effect::WaitForActors3DLoaded { actors, .. } => {
                    Some(actors.len())
                }
                _ => None,
            })
        });
    let cart_init_ok = cart_init_kinds.as_ref().is_some_and(|kinds| {
        cart_init_actor_gate_count == Some(9)
            && kinds.get("WaitForActors3DLoaded") == Some(&1)
            && kinds.get("TetherToHorse") == Some(&2)
            && kinds.get("Wait") == Some(&1)
            && kinds.get("SetVehicle") == Some(&10)
            && kinds.get("PlayIdle") == Some(&10)
            && kinds.get("EquipItem") == Some(&1)
    });
    checks.record(
        "cart init fragment",
        cart_init_ok,
        cart_init_kinds.map_or_else(
            || "Fragment_175 did not lower".to_owned(),
            |kinds| {
                format!(
                    "{} effects, {:?} actors gated: {kinds:?}",
                    cart_init_effects.unwrap().len(),
                    cart_init_actor_gate_count
                )
            },
        ),
    );
}

/// The stage-30 handoff (Fragment_111): disable the civilians marker,
/// advance to stage 27, and re-evaluate both cart horses' packages.
pub(super) fn check_stage_thirty_handoff(
    checks: &mut Checks,
    functions: &HashMap<String, &Function>,
    quest_properties: &HashSet<String>,
) {
    let stage_30_effects = functions.get("fragment_111").and_then(|function| {
        lower_fragment_with_quest_properties(&function.body, quest_properties)
    });
    let stage_30_ok = matches!(
        stage_30_effects.as_deref(),
        Some([
            Effect::Disable { object, fade_out: false },
            Effect::SetStage { quest: QuestRef::SelfRef, stage: 27 },
            Effect::EvaluatePackage { actor: ActorRef::Object(horse_1) },
            Effect::EvaluatePackage { actor: ActorRef::Object(horse_2) },
        ]) if object.property_name().eq_ignore_ascii_case("CiviliansOutsideHelgenMarker")
            && horse_1.property_name().eq_ignore_ascii_case("Alias_CartHorse1")
            && horse_2.property_name().eq_ignore_ascii_case("Alias_CartHorse2")
    );
    checks.record(
        "stage 30 handoff",
        stage_30_ok,
        stage_30_effects.map_or_else(
            || "Fragment_111 did not lower".to_owned(),
            |effects| format!("Fragment_111 lowered to {effects:?}"),
        ),
    );
}

/// Informational (non-gating) per-fragment lowering coverage report.
pub(super) fn print_fragment_coverage(coverage: &FragmentCoverage) {
    let pct = if coverage.behavioral == 0 {
        0.0
    } else {
        100.0 * coverage.lowered as f64 / coverage.behavioral as f64
    };
    println!();
    println!("-- current MQ101 quest-fragment coverage (informational) --");
    println!("bound no-op fragments:       {}", coverage.no_op);
    println!("bound behavioral fragments: {}", coverage.behavioral);
    println!(
        "fully lowered today:         {} ({pct:.1}%)",
        coverage.lowered
    );
    println!(
        "  via a placeholder effect:   {} \
         (counter-only stub inside)",
        coverage.lowered_with_placeholder
    );
    println!(
        "declined/backlog:            {}",
        coverage.behavioral - coverage.lowered
    );
    if !coverage.effects.is_empty() {
        println!("effects emitted:");
        for (kind, count) in &coverage.effects {
            println!("  {kind:<24} {count}");
        }
        println!("declined top-level shapes:");
        for (shape, count) in coverage.declined_shapes.iter().take(20) {
            println!("  {count:>3} {shape}");
        }
        println!(
            "declined samples: {}",
            coverage.declined_samples.join(", ")
        );
    }
}
