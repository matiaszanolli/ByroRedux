//! Population of the fragment tables from compiled `.pex` and parsed `.psc`.
//!
//! Split out of `fragment.rs` (#3854). This is the most edit-prone surface in
//! the module — each new provider/ownership flavour adds another four entry
//! points — and it no longer forces a recompile of the interpreter it does
//! not touch.

use super::*;

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
    populate_quest_fragments_from_pex_detailed(frags, quest, pex_bytes, bindings).inserted
}

/// Detailed quest-fragment result, retaining compatibility evidence from the
/// same PEX parse used for lowering.
pub fn populate_quest_fragments_from_pex_detailed(
    frags: &mut QuestStageFragments,
    quest: QuestFormId,
    pex_bytes: &[u8],
    bindings: &[(u16, &str)],
) -> FragmentPexTranslation {
    populate_quest_fragments_from_pex_detailed_internal(frags, quest, pex_bytes, bindings, None)
}

/// Provider catalog paired with the legacy script package that supplied a
/// quest or scene fragment.
#[derive(Clone, Copy)]
pub struct OwnedFragmentProviders<'a> {
    pub catalog: &'a crate::PapyrusProviderCatalog,
    pub principal: &'a byroredux_sdk::identity::PrincipalId,
}

impl<'a> OwnedFragmentProviders<'a> {
    pub fn new(
        catalog: &'a crate::PapyrusProviderCatalog,
        principal: &'a byroredux_sdk::identity::PrincipalId,
    ) -> Self {
        Self { catalog, principal }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FragmentProviderScope<'a> {
    catalog: &'a crate::PapyrusProviderCatalog,
    principal: Option<&'a byroredux_sdk::identity::PrincipalId>,
}

/// Provider-aware quest-fragment lowering from the same decompiled PEX AST.
pub fn populate_quest_fragments_from_pex_detailed_with_providers(
    frags: &mut QuestStageFragments,
    quest: QuestFormId,
    pex_bytes: &[u8],
    bindings: &[(u16, &str)],
    providers: &crate::PapyrusProviderCatalog,
) -> FragmentPexTranslation {
    populate_quest_fragments_from_pex_detailed_internal(
        frags,
        quest,
        pex_bytes,
        bindings,
        Some(FragmentProviderScope {
            catalog: providers,
            principal: None,
        }),
    )
}

/// Provider-aware quest-fragment lowering attributed to the archive package
/// that supplied the compiled PEX.
pub fn populate_owned_quest_fragments_from_pex_detailed_with_providers(
    frags: &mut QuestStageFragments,
    quest: QuestFormId,
    pex_bytes: &[u8],
    bindings: &[(u16, &str)],
    providers: OwnedFragmentProviders<'_>,
) -> FragmentPexTranslation {
    populate_quest_fragments_from_pex_detailed_internal(
        frags,
        quest,
        pex_bytes,
        bindings,
        Some(FragmentProviderScope {
            catalog: providers.catalog,
            principal: Some(providers.principal),
        }),
    )
}

pub(crate) fn populate_quest_fragments_from_pex_detailed_internal(
    frags: &mut QuestStageFragments,
    quest: QuestFormId,
    pex_bytes: &[u8],
    bindings: &[(u16, &str)],
    providers: Option<FragmentProviderScope<'_>>,
) -> FragmentPexTranslation {
    let fingerprint = crate::translate::pex_fingerprint(pex_bytes);
    // #3948 — the net spans the whole sequence, not just the decompile. See
    // `translate::catching_panics`; the same reasoning applies here, and this
    // is the variant the cell loader actually calls for quest fragments.
    crate::translate::catching_panics("populate_quest_fragments", || {
        let pex = match byroredux_pex::parse(pex_bytes) {
            Ok(p) => p,
            Err(e) => {
                log::debug!(
                    "populate_quest_fragments: .pex parse failed (quest {:08X}): {e}",
                    quest.0
                );
                return FragmentPexTranslation::failed(fingerprint);
            }
        };
        let compatibility = crate::compatibility::analyze_pex_compatibility(&pex);
        crate::compatibility::log_compatibility_report(&compatibility);
        let script = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            byroredux_pex::decompile::decompile_script(&pex)
        })) {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                log::debug!(
                    "populate_quest_fragments: decompile failed (quest {:08X}): {e}",
                    quest.0
                );
                return FragmentPexTranslation::declined(fingerprint, compatibility);
            }
            Err(_) => {
                log::debug!(
                    "populate_quest_fragments: decompile panicked (quest {:08X})",
                    quest.0
                );
                return FragmentPexTranslation::declined(fingerprint, compatibility);
            }
        };
        FragmentPexTranslation {
            inserted: populate_quest_fragments_from_script_internal(
                frags, quest, &script, bindings, providers,
            ),
            compatibility: Some(compatibility),
            fingerprint,
        }
    })
    .unwrap_or_else(|| FragmentPexTranslation::failed(fingerprint))
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
    populate_quest_fragments_from_script_internal(frags, quest, script, bindings, None)
}

/// Provider-aware AST lowering for quest-stage fragments.
pub fn populate_quest_fragments_from_script_with_providers(
    frags: &mut QuestStageFragments,
    quest: QuestFormId,
    script: &Script,
    bindings: &[(u16, &str)],
    providers: &crate::PapyrusProviderCatalog,
) -> usize {
    populate_quest_fragments_from_script_internal(
        frags,
        quest,
        script,
        bindings,
        Some(FragmentProviderScope {
            catalog: providers,
            principal: None,
        }),
    )
}

/// Provider-aware AST lowering attributed to one legacy script package.
pub fn populate_owned_quest_fragments_from_script_with_providers(
    frags: &mut QuestStageFragments,
    quest: QuestFormId,
    script: &Script,
    bindings: &[(u16, &str)],
    providers: OwnedFragmentProviders<'_>,
) -> usize {
    populate_quest_fragments_from_script_internal(
        frags,
        quest,
        script,
        bindings,
        Some(FragmentProviderScope {
            catalog: providers.catalog,
            principal: Some(providers.principal),
        }),
    )
}

fn populate_quest_fragments_from_script_internal(
    frags: &mut QuestStageFragments,
    quest: QuestFormId,
    script: &Script,
    bindings: &[(u16, &str)],
    providers: Option<FragmentProviderScope<'_>>,
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
        if let Some(mut effects) =
            crate::translate::effects::lower_fragment_with_quest_properties_and_providers(
                body,
                &quest_properties,
                providers.map(|scope| scope.catalog),
            )
        {
            if let Some(principal) = providers.and_then(|scope| scope.principal) {
                crate::translate::effects::attribute_provider_calls(&mut effects, principal);
            }
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
    populate_scene_fragments_from_pex_detailed(
        frags,
        scene_form_id,
        context,
        vmad,
        pex_bytes,
        bindings,
    )
    .inserted
}

/// Detailed scene-fragment result, retaining compatibility evidence from the
/// same PEX parse used for lowering.
pub fn populate_scene_fragments_from_pex_detailed(
    frags: &mut SceneFragments,
    scene_form_id: u32,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    pex_bytes: &[u8],
    bindings: &[(SceneFragmentEvent, &str)],
) -> FragmentPexTranslation {
    populate_scene_fragments_from_pex_detailed_internal(
        frags,
        scene_form_id,
        context,
        vmad,
        pex_bytes,
        bindings,
        None,
    )
}

/// Provider-aware scene-fragment lowering from the same decompiled PEX AST.
pub fn populate_scene_fragments_from_pex_detailed_with_providers(
    frags: &mut SceneFragments,
    scene_form_id: u32,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    pex_bytes: &[u8],
    bindings: &[(SceneFragmentEvent, &str)],
    providers: &crate::PapyrusProviderCatalog,
) -> FragmentPexTranslation {
    populate_scene_fragments_from_pex_detailed_internal(
        frags,
        scene_form_id,
        context,
        vmad,
        pex_bytes,
        bindings,
        Some(FragmentProviderScope {
            catalog: providers,
            principal: None,
        }),
    )
}

/// Provider-aware scene-fragment lowering attributed to the archive package
/// that supplied the compiled PEX.
pub fn populate_owned_scene_fragments_from_pex_detailed_with_providers(
    frags: &mut SceneFragments,
    scene_form_id: u32,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    pex_bytes: &[u8],
    bindings: &[(SceneFragmentEvent, &str)],
    providers: OwnedFragmentProviders<'_>,
) -> FragmentPexTranslation {
    populate_scene_fragments_from_pex_detailed_internal(
        frags,
        scene_form_id,
        context,
        vmad,
        pex_bytes,
        bindings,
        Some(FragmentProviderScope {
            catalog: providers.catalog,
            principal: Some(providers.principal),
        }),
    )
}

pub(crate) fn populate_scene_fragments_from_pex_detailed_internal(
    frags: &mut SceneFragments,
    scene_form_id: u32,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    pex_bytes: &[u8],
    bindings: &[(SceneFragmentEvent, &str)],
    providers: Option<FragmentProviderScope<'_>>,
) -> FragmentPexTranslation {
    let fingerprint = crate::translate::pex_fingerprint(pex_bytes);
    // #3948 — same whole-sequence net as the quest sibling above.
    crate::translate::catching_panics("populate_scene_fragments", || {
        let pex = match byroredux_pex::parse(pex_bytes) {
            Ok(pex) => pex,
            Err(error) => {
                log::debug!(
                    "populate_scene_fragments: .pex parse failed (scene {scene_form_id:08X}): {error}"
                );
                return FragmentPexTranslation::failed(fingerprint);
            }
        };
        let compatibility = crate::compatibility::analyze_pex_compatibility(&pex);
        crate::compatibility::log_compatibility_report(&compatibility);
        let script = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            byroredux_pex::decompile::decompile_script(&pex)
        })) {
            Ok(Ok(script)) => script,
            Ok(Err(error)) => {
                log::debug!(
                    "populate_scene_fragments: decompile failed (scene {scene_form_id:08X}): {error}"
                );
                return FragmentPexTranslation::declined(fingerprint, compatibility);
            }
            Err(_) => {
                log::debug!(
                    "populate_scene_fragments: decompile panicked (scene {scene_form_id:08X})"
                );
                return FragmentPexTranslation::declined(fingerprint, compatibility);
            }
        };
        FragmentPexTranslation {
            inserted: populate_scene_fragments_from_script_internal(
                frags,
                scene_form_id,
                context,
                vmad,
                &script,
                bindings,
                providers,
            ),
            compatibility: Some(compatibility),
            fingerprint,
        }
    })
    .unwrap_or_else(|| FragmentPexTranslation::failed(fingerprint))
}

/// Result of one fragment PEX parse/decompile/lower pass.
pub struct FragmentPexTranslation {
    pub inserted: usize,
    pub compatibility: Option<crate::compatibility::CompatibilityReport>,
    pub fingerprint: u64,
}

impl FragmentPexTranslation {
    fn failed(fingerprint: u64) -> Self {
        Self {
            inserted: 0,
            compatibility: None,
            fingerprint,
        }
    }

    fn declined(
        fingerprint: u64,
        compatibility: crate::compatibility::CompatibilityReport,
    ) -> Self {
        Self {
            inserted: 0,
            compatibility: Some(compatibility),
            fingerprint,
        }
    }
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
    populate_scene_fragments_from_script_internal(
        frags,
        scene_form_id,
        context,
        vmad,
        script,
        bindings,
        None,
    )
}

/// Provider-aware AST lowering for authored scene lifecycle fragments.
pub fn populate_scene_fragments_from_script_with_providers(
    frags: &mut SceneFragments,
    scene_form_id: u32,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    script: &Script,
    bindings: &[(SceneFragmentEvent, &str)],
    providers: &crate::PapyrusProviderCatalog,
) -> usize {
    populate_scene_fragments_from_script_internal(
        frags,
        scene_form_id,
        context,
        vmad,
        script,
        bindings,
        Some(FragmentProviderScope {
            catalog: providers,
            principal: None,
        }),
    )
}

/// Provider-aware scene AST lowering attributed to one legacy script package.
pub fn populate_owned_scene_fragments_from_script_with_providers(
    frags: &mut SceneFragments,
    scene_form_id: u32,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    script: &Script,
    bindings: &[(SceneFragmentEvent, &str)],
    providers: OwnedFragmentProviders<'_>,
) -> usize {
    populate_scene_fragments_from_script_internal(
        frags,
        scene_form_id,
        context,
        vmad,
        script,
        bindings,
        Some(FragmentProviderScope {
            catalog: providers.catalog,
            principal: Some(providers.principal),
        }),
    )
}

fn populate_scene_fragments_from_script_internal(
    frags: &mut SceneFragments,
    scene_form_id: u32,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    script: &Script,
    bindings: &[(SceneFragmentEvent, &str)],
    providers: Option<FragmentProviderScope<'_>>,
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
        if let Some(mut effects) =
            crate::translate::effects::lower_fragment_with_quest_properties_and_providers(
                body,
                &quest_properties,
                providers.map(|scope| scope.catalog),
            )
        {
            if let Some(principal) = providers.and_then(|scope| scope.principal) {
                crate::translate::effects::attribute_provider_calls(&mut effects, principal);
            }
            if !effects.is_empty() {
                frags.insert(scene_form_id, *event, context, vmad.cloned(), effects);
                inserted += 1;
            }
        }
    }
    inserted
}
