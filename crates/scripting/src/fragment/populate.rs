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
/// This single-`Script` form matches only an exact `Type::Object("quest")`.
/// A property typed with a Quest-*derived* script (`mq206script`,
/// `tg04pqsscript`, …) needs the other script's `extends` chain:
/// [`quest_property_names_with_library`] (M47.2) walks it through a
/// [`ScriptLibrary`](crate::translate::effects::ScriptLibrary).
///
/// `pub` (#2658 / SCR-D5-NEW11-03) — so `examples/fragment_coverage.rs`
/// and `examples/mq101_conformance.rs` can build the same per-script set
/// this function's one production caller
/// ([`populate_quest_fragments_from_script`]) does, and measure
/// `lower_fragment_with_quest_properties` (what production actually
/// runs) instead of context-free `lower_fragment`.
pub fn quest_property_names(script: &Script) -> std::collections::HashSet<String> {
    quest_property_names_with_library(script, None)
}

/// How many `extends` links [`extends_quest`] follows.
const MAX_EXTENDS_DEPTH: usize = 8;

/// Whether the script type `name` is `Quest` or transitively extends it,
/// following each script's `extends` through `library`.
pub(crate) fn extends_quest(
    name: &str,
    library: &dyn crate::translate::effects::ScriptLibrary,
) -> bool {
    let mut name = name.to_owned();
    for _ in 0..MAX_EXTENDS_DEPTH {
        if name.eq_ignore_ascii_case("quest") {
            return true;
        }
        let Some(parent) = library
            .script(&name)
            .and_then(|script| script.parent.as_ref().map(|parent| parent.node.0.clone()))
        else {
            return false;
        };
        name = parent;
    }
    false
}

/// [`quest_property_names`], also collecting properties typed with a script
/// that extends `Quest` (M47.2: `TG04PQSScript Property pTG04PQS`), resolved
/// through `library`'s `extends` chains.
pub fn quest_property_names_with_library(
    script: &Script,
    library: Option<&dyn crate::translate::effects::ScriptLibrary>,
) -> std::collections::HashSet<String> {
    script
        .body
        .iter()
        .filter_map(|item| match &item.node {
            ScriptItem::Property(p) => match &p.ty.node {
                Type::Object(ty_name)
                    if ty_name.0.eq_ignore_ascii_case("quest")
                        || library.is_some_and(|library| extends_quest(&ty_name.0, library)) =>
                {
                    Some(p.name.node.0.to_ascii_lowercase())
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// #5972 — lowercased names of `script`'s properties declared with a
/// per-instance actor type: `Actor`, `ReferenceAlias`, `RefCollectionAlias`.
/// Fallout 4 / Starfield put `SetEssential` / `SetProtected` on these with
/// per-instance semantics; an `ActorBase` property (Skyrim) is not listed.
pub fn per_instance_actor_property_names(script: &Script) -> std::collections::HashSet<String> {
    script
        .body
        .iter()
        .filter_map(|item| match &item.node {
            ScriptItem::Property(p) => match &p.ty.node {
                Type::Object(ty_name)
                    if ["actor", "referencealias", "refcollectionalias"]
                        .iter()
                        .any(|kind| ty_name.0.eq_ignore_ascii_case(kind)) =>
                {
                    Some(p.name.node.0.to_ascii_lowercase())
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// #5407 — lowercased names of `script`'s own ARGLESS functions
/// (top-level and state, matching [`function_body`]'s lookup). Feeds
/// `lower_fragment_with_script_metadata` so script-defined-helper
/// primitives (`AddRaceSpells` / `RemoveRaceSpells`) fire only in a
/// script that actually defines the helper with zero parameters — a
/// same-named function on another script, or with other arity and
/// semantics, never lowers.
pub fn argless_function_names(script: &Script) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let mut visit = |f: &byroredux_papyrus::ast::Function| {
        if f.params.is_empty() {
            out.insert(f.name.node.0.to_ascii_lowercase());
        }
    };
    for item in &script.body {
        match &item.node {
            ScriptItem::Function(f) => visit(f),
            ScriptItem::State(st) => {
                for si in &st.body {
                    if let StateItem::Function(f) = &si.node {
                        visit(f);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn function_body<'a>(script: &'a Script, name: &str) -> Option<&'a [Spanned<Stmt>]> {
    function_item(script, name).map(|f| f.body.as_slice())
}

/// The function named `name` (case-insensitive), top-level or in a state.
fn function_item<'a>(
    script: &'a Script,
    name: &str,
) -> Option<&'a byroredux_papyrus::ast::Function> {
    for item in &script.body {
        match &item.node {
            ScriptItem::Function(f) if f.name.node.0.eq_ignore_ascii_case(name) => {
                return Some(f);
            }
            ScriptItem::State(st) => {
                for si in &st.body {
                    if let StateItem::Function(f) = &si.node {
                        if f.name.node.0.eq_ignore_ascii_case(name) {
                            return Some(f);
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
    /// M47.2 — decompiled scripts for inlining a quest fragment's calls into
    /// its quest's own script, and for the Papyrus VM's check.
    pub library: Option<&'a dyn crate::translate::effects::ScriptLibrary>,
    /// M47.2 — where a scene or INFO population puts the scripts the Papyrus
    /// VM needs for the fragments it installs as `Effect::Interpret`; the
    /// caller installs them in [`QuestStageFragments`]. Without it those
    /// fragments only lower.
    pub vm_scripts: Option<&'a std::cell::RefCell<Vec<std::sync::Arc<Script>>>>,
}

impl<'a> OwnedFragmentProviders<'a> {
    pub fn new(
        catalog: &'a crate::PapyrusProviderCatalog,
        principal: &'a byroredux_sdk::identity::PrincipalId,
    ) -> Self {
        Self {
            catalog,
            principal,
            library: None,
            vm_scripts: None,
        }
    }

    /// Attach a [`crate::translate::effects::ScriptLibrary`] (M47.2).
    pub fn with_library(mut self, library: &'a dyn crate::translate::effects::ScriptLibrary) -> Self {
        self.library = Some(library);
        self
    }

    /// Collect the Papyrus VM's scripts in `sink` (M47.2): see
    /// [`Self::vm_scripts`].
    pub fn with_vm_scripts(mut self, sink: &'a std::cell::RefCell<Vec<std::sync::Arc<Script>>>) -> Self {
        self.vm_scripts = Some(sink);
        self
    }
}

#[derive(Clone, Copy)]
pub(crate) struct FragmentProviderScope<'a> {
    catalog: &'a crate::PapyrusProviderCatalog,
    principal: Option<&'a byroredux_sdk::identity::PrincipalId>,
    library: Option<&'a dyn crate::translate::effects::ScriptLibrary>,
    vm_scripts: Option<&'a std::cell::RefCell<Vec<std::sync::Arc<Script>>>>,
}

/// #5811 — the per-binding tail all three fragment families share: the
/// M47.2 interpret fallback for a fragment the family's lowering
/// declined, then provider-call attribution. Each loop keeps its body
/// lookup (log labels differ), its `lower_*` call, its VM-script sink
/// and its insert.
fn finish_lowered_fragment(
    lowered: Option<Vec<Effect>>,
    interpret_fallback_result: impl FnOnce() -> Option<Vec<Effect>>,
    principal: Option<&byroredux_sdk::identity::PrincipalId>,
) -> Option<Vec<Effect>> {
    let mut effects = lowered.or_else(interpret_fallback_result)?;
    if let Some(principal) = principal {
        crate::translate::effects::attribute_provider_calls(&mut effects, principal);
    }
    Some(effects)
}

/// #5811 — the OwnedFragmentProviders → FragmentProviderScope widening
/// the owned-* entry points repeat (principal becomes Some, the rest
/// move). One conversion, not one per family.
impl<'a> From<OwnedFragmentProviders<'a>> for FragmentProviderScope<'a> {
    fn from(providers: OwnedFragmentProviders<'a>) -> Self {
        Self {
            catalog: providers.catalog,
            principal: Some(providers.principal),
            library: providers.library,
            vm_scripts: providers.vm_scripts,
        }
    }
}

/// M47.2 — a fragment the lowering declines, installed for the Papyrus VM
/// when its ahead-of-time check passes; `install` takes each script the run
/// needs.
fn interpret_fallback(
    script: &Script,
    function: &str,
    library: Option<&dyn crate::translate::effects::ScriptLibrary>,
    install: &mut dyn FnMut(std::sync::Arc<Script>),
) -> Option<Vec<Effect>> {
    let script = std::sync::Arc::new(script.clone());
    let scripts = super::vm::check_function(std::sync::Arc::clone(&script), function, Some(library?))?;
    for needed in scripts {
        install(needed);
    }
    Some(vec![Effect::interpret(script.name.node.0.clone(), function.to_owned(), Vec::new())])
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
        Some(FragmentProviderScope::from(providers)),
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
        Some(FragmentProviderScope::from(providers)),
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
    let quest_properties =
        quest_property_names_with_library(script, providers.and_then(|scope| scope.library));
    // #5407 — the script's own argless helpers, for the race-spells
    // primitives' script-defined check.
    let argless_functions = argless_function_names(script);
    let actor_properties = per_instance_actor_property_names(script);
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
        // M47.2 — quest fragments inline calls into the quest's own script
        // when a script library is supplied.
        let lowered = crate::translate::effects::lower_quest_fragment(
            body,
            &quest_properties,
            &argless_functions,
            &actor_properties,
            providers.map(|scope| scope.catalog),
            providers.and_then(|scope| scope.library),
        );
        // #5811 — the shared decline→interpret→attribute tail.
        let lowered = finish_lowered_fragment(lowered,
            || {
                // M47.2 — a fragment the lowering declines runs on the Papyrus VM
                // when its ahead-of-time check passes.
                interpret_fallback(
                    script,
                    fragment_name,
                    providers.and_then(|scope| scope.library),
                    &mut |needed| frags.insert_vm_script(needed),
                )
            },
            providers.and_then(|scope| scope.principal),
        );
        if let Some(effects) = lowered {
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
        Some(FragmentProviderScope::from(providers)),
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
        Some(FragmentProviderScope::from(providers)),
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
    let quest_properties =
        quest_property_names_with_library(script, providers.and_then(|scope| scope.library));
    // #5407 — the script's own argless helpers, for the race-spells
    // primitives' script-defined check.
    let argless_functions = argless_function_names(script);
    let actor_properties = per_instance_actor_property_names(script);
    let mut inserted = 0;
    for (event, fragment_name) in bindings {
        let Some(body) = function_body(script, fragment_name) else {
            log::debug!(
                "populate_scene_fragments: fn '{fragment_name}' absent in scene {scene_form_id:08X} .pex"
            );
            continue;
        };
        let lowered = crate::translate::effects::lower_scene_fragment(
            body,
            &quest_properties,
            &argless_functions,
            &actor_properties,
            providers.map(|scope| scope.catalog),
            providers.and_then(|scope| scope.library),
        );
        // #5811 — the shared decline→interpret→attribute tail.
        let lowered = finish_lowered_fragment(lowered,
            || {
                let sink = providers.and_then(|scope| scope.vm_scripts)?;
                interpret_fallback(script, fragment_name, providers.and_then(|scope| scope.library), &mut |needed| {
                    sink.borrow_mut().push(needed)
                })
            },
            providers.and_then(|scope| scope.principal),
        );
        if let Some(effects) = lowered {
            if !effects.is_empty() {
                frags.insert(scene_form_id, *event, context, vmad.cloned(), effects);
                inserted += 1;
            }
        }
    }
    inserted
}

/// Provider-aware INFO lowering attributed to one legacy script package.
pub fn populate_owned_info_fragments_from_pex_with_providers(
    frags: &mut DialogueInfoFragments,
    info_form_id: u32,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    pex_bytes: &[u8],
    bindings: &[(bool, &str)],
    providers: OwnedFragmentProviders<'_>,
) -> usize {
    // Same whole-sequence panic net as the quest variant: the dialogue
    // selection dispatches straight from this table, so a half-installed
    // entry must never exist.
    crate::translate::catching_panics("populate_info_fragments", || {
        let pex = match byroredux_pex::parse(pex_bytes) {
            Ok(p) => p,
            Err(e) => {
                log::debug!(
                    "populate_info_fragments: .pex parse failed (info {info_form_id:08X}): {e}"
                );
                return 0;
            }
        };
        let script = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            byroredux_pex::decompile::decompile_script(&pex)
        })) {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                log::debug!(
                    "populate_info_fragments: decompile failed (info {info_form_id:08X}): {e}"
                );
                return 0;
            }
            Err(_) => {
                log::debug!("populate_info_fragments: decompile panicked (info {info_form_id:08X})");
                return 0;
            }
        };
        populate_info_fragments_from_script_internal(
            frags,
            info_form_id,
            context,
            vmad,
            &script,
            bindings,
            Some(FragmentProviderScope {
                catalog: providers.catalog,
                principal: Some(providers.principal),
                library: providers.library,
                vm_scripts: providers.vm_scripts,
            }),
        )
    })
    .unwrap_or(0)
}

fn populate_info_fragments_from_script_internal(
    frags: &mut DialogueInfoFragments,
    info_form_id: u32,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    script: &Script,
    bindings: &[(bool, &str)],
    providers: Option<FragmentProviderScope<'_>>,
) -> usize {
    let quest_properties =
        quest_property_names_with_library(script, providers.and_then(|scope| scope.library));
    // #5407 — the script's own argless helpers, for the race-spells
    // primitives' script-defined check.
    let argless_functions = argless_function_names(script);
    let actor_properties = per_instance_actor_property_names(script);
    let mut begin: Option<Vec<Effect>> = None;
    let mut end: Option<Vec<Effect>> = None;
    for (on_begin, fragment_name) in bindings {
        let Some(function) = function_item(script, fragment_name) else {
            log::debug!(
                "populate_info_fragments: fn '{fragment_name}' absent in info {info_form_id:08X} .pex"
            );
            continue;
        };
        // M47.2 — INFO fragments lower with the line's speaker bound.
        let lowered = crate::translate::effects::lower_info_fragment(
            &function.params,
            &function.body,
            &quest_properties,
            &argless_functions,
            &actor_properties,
            providers.map(|scope| scope.catalog),
            providers.and_then(|scope| scope.library),
        );
        // #5811 — the shared decline→interpret→attribute tail. The run
        // binds the speaker parameter to the line's speaker.
        let Some(effects) = finish_lowered_fragment(lowered,
            || {
                let sink = providers.and_then(|scope| scope.vm_scripts)?;
                interpret_fallback(script, fragment_name, providers.and_then(|scope| scope.library), &mut |needed| {
                    sink.borrow_mut().push(needed)
                })
            },
            providers.and_then(|scope| scope.principal),
        ) else {
            continue;
        };
        if effects.is_empty() {
            continue;
        }
        if *on_begin {
            begin = Some(effects);
        } else {
            end = Some(effects);
        }
    }
    if begin.is_none() && end.is_none() {
        return 0;
    }
    frags.insert(info_form_id, context, vmad.cloned(), begin, end);
    1
}
