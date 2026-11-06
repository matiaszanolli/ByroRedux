use super::*;
use byroredux_sdk::identity::PrincipalId;

struct ScriptArchive {
    archive: Archive,
    principal: PrincipalId,
}

pub(crate) struct ResolvedPex {
    pub(crate) bytes: Vec<u8>,
    pub(crate) principal: PrincipalId,
}

/// A script's `.pex` after the reference-independent work: extracted, parsed,
/// preflighted and decompiled once (#5999), with the archive's principal.
pub(crate) struct CachedPex {
    pub(crate) decompiled: byroredux_scripting::DecompiledPex,
    pub(crate) principal: PrincipalId,
}

/// Per script name (lowercased), the [`CachedPex`] — or `None` for a script no
/// archive holds. Cell load used to extract, parse, preflight and decompile a
/// `.pex` for **every scripted reference** carrying it; only the VM verdict was
/// cached. Provider lowering and recognition depend on per-reference
/// properties and still run per reference, against this shared AST.
#[derive(Default)]
pub(crate) struct DecompiledPexCache(
    std::collections::HashMap<String, Option<std::sync::Arc<CachedPex>>>,
);

impl byroredux_core::ecs::resource::Resource for DecompiledPexCache {}

/// The cached decompile of `script_name`, doing the extraction + decompile on
/// first sight. The first decompile also records the compatibility preflight in
/// the load order's registry (once per script — the registry dedups by
/// fingerprint anyway).
pub(crate) fn decompiled_pex(
    world: &mut byroredux_core::ecs::world::World,
    script_name: &str,
) -> Option<std::sync::Arc<CachedPex>> {
    let key = script_name.to_ascii_lowercase();
    if let Some(hit) = world
        .try_resource::<DecompiledPexCache>()
        .and_then(|cache| cache.0.get(&key).cloned())
    {
        return hit;
    }
    // Scope the provider borrow: extract the owned `.pex` bytes, then drop
    // the resource read before the `&mut World` work below.
    let resolved = {
        let provider = world.try_resource::<ScriptProvider>()?;
        provider.resolve_pex(script_name)
    };
    let cached = resolved.map(|resolved| {
        let mut decompiled = byroredux_scripting::decompile_pex(&resolved.bytes);
        byroredux_scripting::record_compatibility_report(
            world,
            decompiled.fingerprint,
            decompiled.compatibility.take(),
        );
        std::sync::Arc::new(CachedPex {
            decompiled,
            principal: resolved.principal,
        })
    });
    world.init_resource::<DecompiledPexCache>();
    world
        .resource_mut::<DecompiledPexCache>()
        .0
        .insert(key, cached.clone());
    cached
}

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
    archives: Vec<ScriptArchive>,
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
        self.resolve_pex(script_name).map(|resolved| resolved.bytes)
    }

    pub(crate) fn resolve_pex(&self, script_name: &str) -> Option<ResolvedPex> {
        let name = pex_archive_path(script_name);
        // #4658 — a present-but-corrupt `.pex` is named, not silently missed.
        self.archives.iter().find_map(|source| {
            source
                .archive
                .extract_or_warn(&name)
                .map(|bytes| ResolvedPex {
                    bytes,
                    principal: source.principal.clone(),
                })
        })
    }

    pub(crate) fn principals(&self) -> impl Iterator<Item = &PrincipalId> {
        self.archives.iter().map(|source| &source.principal)
    }
}

impl byroredux_core::ecs::resource::Resource for ScriptProvider {}

/// M47.2 — the [`ScriptProvider`]'s scripts, decompiled on demand and cached,
/// for inlining a quest fragment's calls into its quest's own script. Script
/// type names map to archive keys the way the compiler lays them out
/// (`DN101:QuestScript` → `scripts\dn101\questscript.pex`).
pub(crate) struct ProviderScriptLibrary<'a> {
    provider: &'a ScriptProvider,
    cache: std::cell::RefCell<
        std::collections::HashMap<String, Option<std::sync::Arc<byroredux_scripting::translate::effects::PapyrusScript>>>,
    >,
}

impl<'a> ProviderScriptLibrary<'a> {
    pub(crate) fn new(provider: &'a ScriptProvider) -> Self {
        Self {
            provider,
            cache: Default::default(),
        }
    }
}

impl byroredux_scripting::translate::effects::ScriptLibrary for ProviderScriptLibrary<'_> {
    fn script(&self, name: &str) -> Option<std::sync::Arc<byroredux_scripting::translate::effects::PapyrusScript>> {
        let key = name.to_ascii_lowercase().replace(':', "\\");
        if let Some(hit) = self.cache.borrow().get(&key) {
            return hit.clone();
        }
        let decoded = self.provider.extract_pex(&key).and_then(|bytes| {
            byroredux_scripting::translate::effects::decompile_pex_for_library(&bytes)
        });
        self.cache.borrow_mut().insert(key, decoded.clone());
        decoded
    }
}

/// M47.2 — per script name, whether the Papyrus VM runs it as a reference
/// script: the events it handles and the scripts its runs need, or `None`.
/// Checked once per session — a script decompiles and checks the same for
/// every reference that carries it.
#[derive(Default)]
pub(crate) struct VmReferenceScriptChecks(std::collections::HashMap<String, VmReferenceScriptCheck>);

/// A script's handled events and the scripts its runs need.
type VmReferenceScriptCheck =
    Option<(Vec<String>, Vec<std::sync::Arc<byroredux_scripting::translate::effects::PapyrusScript>>)>;

impl byroredux_core::ecs::resource::Resource for VmReferenceScriptChecks {}

/// Attach `instance` to `entity` as a reference script the Papyrus VM runs,
/// when the VM runs it ([`byroredux_scripting::check_reference_script`]).
pub(crate) fn attach_vm_reference_script(
    world: &mut byroredux_core::ecs::world::World,
    entity: byroredux_core::ecs::EntityId,
    instance: &byroredux_plugin::esm::records::script_instance::ScriptInstance,
) -> bool {
    // #5998 — the VM reaches a reference script by the reference's FormID: the
    // index (`ReferenceScriptIndex::refresh`) and delivery
    // (`collect_deliveries`) both need `entity_global_form_id`. A non-primary
    // SCOL / PKIN child carries no identity (`stamp_quest_reference` runs for
    // the primary child only, #3016), so an instance attached to it could never
    // run — yet it counted as attached and inflated `scripts_recognized`. Decline
    // it here, before the decompile + check, so the telemetry matches what runs.
    // Every caller stamps identity before it attaches.
    if byroredux_scripting::condition::entity_global_form_id(world, entity).is_none() {
        log::trace!(
            "M47.2: the Papyrus VM skips '{}' on entity {entity:?}: the reference has no FormID to deliver to",
            instance.name,
        );
        return false;
    }
    let key = instance.name.to_ascii_lowercase();
    let cached = world
        .try_resource::<VmReferenceScriptChecks>()
        .and_then(|checks| checks.0.get(&key).cloned());
    let checked = match cached {
        Some(checked) => checked,
        None => {
            let checked = {
                let Some(provider) = world.try_resource::<ScriptProvider>() else {
                    return false;
                };
                let library = ProviderScriptLibrary::new(&provider);
                use byroredux_scripting::translate::effects::ScriptLibrary as _;
                library.script(&instance.name).and_then(|script| {
                    byroredux_scripting::check_reference_script(
                        script,
                        &byroredux_scripting::DELIVERED_EVENTS,
                        Some(&library),
                    )
                    .map_err(|why| log::trace!("M47.2: the Papyrus VM declines '{}': {why}", instance.name))
                    .ok()
                })
            };
            world.init_resource::<VmReferenceScriptChecks>();
            world
                .resource_mut::<VmReferenceScriptChecks>()
                .0
                .insert(key, checked.clone());
            checked
        }
    };
    let Some((events, scripts)) = checked else {
        return false;
    };
    byroredux_scripting::attach_reference_script(world, entity, instance, events, scripts);
    true
}

/// M47.2 — per script name, whether the Papyrus VM runs it as an alias script:
/// the events it handles and the scripts its runs need, or `None`. Checked
/// once per session, like [`VmReferenceScriptChecks`].
#[derive(Default)]
pub(crate) struct VmAliasScriptChecks(std::collections::HashMap<String, VmReferenceScriptCheck>);

impl byroredux_core::ecs::resource::Resource for VmAliasScriptChecks {}

/// The most quests whose alias scripts one frame decompiles and checks.
const ALIAS_CHECKS_PER_FRAME: usize = 4;

/// #6001 — the most *distinct, uncached* alias scripts one frame decompiles and
/// checks. [`ALIAS_CHECKS_PER_FRAME`] bounds quests, but a quest's VMAD alias
/// section can list dozens of distinct scripts, each an archive extraction plus
/// a decompile; the verdict cache only helps repeated names. A quest the budget
/// stops is left unchecked (its finished scripts are cached) and resumes next
/// frame, so it always makes progress.
const ALIAS_SCRIPT_CHECKS_PER_FRAME: usize = 8;

/// A per-frame allowance of expensive checks.
struct ScriptCheckBudget(usize);

impl ScriptCheckBudget {
    /// Spend one check; `false` when the frame's allowance is gone.
    fn try_spend(&mut self) -> bool {
        match self.0.checked_sub(1) {
            Some(left) => {
                self.0 = left;
                true
            }
            None => false,
        }
    }
}

/// Check the alias scripts of the quests that run and have not been checked
/// ([`byroredux_scripting::QuestAliasScripts`]), a few per frame: the scripts
/// of the whole load order would cost a second of load for the quests nothing
/// ever starts.
pub(crate) fn check_running_quest_alias_scripts(world: &byroredux_core::ecs::world::World) {
    // (quest, the quest's VMAD script table keyed by index)
    type PendingScript = (
        byroredux_scripting::QuestFormId,
        std::sync::Arc<Vec<(i16, byroredux_plugin::esm::records::script_instance::ScriptInstance)>>,
    );
    let pending: Vec<PendingScript> = {
        let Some(alias_scripts) = world.try_resource::<byroredux_scripting::QuestAliasScripts>() else {
            return;
        };
        if alias_scripts.is_empty() {
            return;
        }
        let stages = world.resource::<byroredux_scripting::quest_stages::QuestStageState>();
        alias_scripts
            .unchecked()
            .filter(|(quest, _)| stages.is_running(*quest))
            .take(ALIAS_CHECKS_PER_FRAME)
            .map(|(quest, scripts)| (quest, std::sync::Arc::clone(scripts)))
            .collect()
    };
    if pending.is_empty() {
        return;
    }
    let Some(provider) = world.try_resource::<ScriptProvider>() else {
        return;
    };
    if provider.is_empty() {
        // No script archive: nothing can ever be checked.
        drop(provider);
        let mut alias_scripts = world.resource_mut::<byroredux_scripting::QuestAliasScripts>();
        for (quest, _) in pending {
            alias_scripts.set_checked(quest, Vec::new());
        }
        return;
    }
    let library = ProviderScriptLibrary::new(&provider);
    use byroredux_scripting::translate::effects::ScriptLibrary as _;
    let mut results = Vec::new();
    let mut budget = ScriptCheckBudget(ALIAS_SCRIPT_CHECKS_PER_FRAME);
    // Without the verdict cache a deferred quest would redo its finished
    // scripts forever, so the budget only applies when the cache exists.
    let budgeted = world.try_resource::<VmAliasScriptChecks>().is_some();
    'quests: for (quest, raw) in pending {
        let mut checked = Vec::new();
        let mut installed = Vec::new();
        for (alias, instance) in raw.iter() {
            let key = instance.name.to_ascii_lowercase();
            let cached = world
                .try_resource::<VmAliasScriptChecks>()
                .and_then(|checks| checks.0.get(&key).cloned());
            let outcome = match cached {
                Some(outcome) => outcome,
                None => {
                    if budgeted && !budget.try_spend() {
                        // #6001 — out of allowance: leave this quest (and the
                        // rest) unchecked; what was checked is cached.
                        break 'quests;
                    }
                    let outcome = library.script(&instance.name).and_then(|script| {
                        byroredux_scripting::check_alias_script(
                            script,
                            &byroredux_scripting::DELIVERED_ALIAS_EVENTS,
                            Some(&library),
                        )
                        .map_err(|why| log::trace!("M47.2: the Papyrus VM declines alias script '{}': {why}", instance.name))
                        .ok()
                    });
                    if let Some(mut checks) = world.try_resource_mut::<VmAliasScriptChecks>() {
                        checks.0.insert(key, outcome.clone());
                    }
                    outcome
                }
            };
            let Some((events, scripts)) = outcome else {
                continue;
            };
            installed.extend(scripts);
            checked.push(byroredux_scripting::AliasScript {
                alias: *alias,
                script: instance.name.clone(),
                vmad: std::sync::Arc::new(byroredux_plugin::esm::records::script_instance::ScriptInstanceData {
                    version: 5,
                    object_format: 2,
                    scripts: vec![instance.clone()],
                }),
                events,
            });
        }
        results.push((quest, checked, installed));
    }
    drop(library);
    drop(provider);
    for (quest, checked, installed) in results {
        if !checked.is_empty() {
            log::debug!("M47.2: {} alias script(s) of quest {:08X} run on the Papyrus VM", checked.len(), quest.0);
        }
        let mut fragments = world.resource_mut::<byroredux_scripting::QuestStageFragments>();
        for script in installed {
            fragments.insert_vm_script(script);
        }
        drop(fragments);
        world.resource_mut::<byroredux_scripting::QuestAliasScripts>().set_checked(quest, checked);
    }
}

/// Normalise a Papyrus script name to its archive key: lowercase,
/// backslash-separated, under the `scripts\` folder with a `.pex`
/// extension. A name authored with the folder and/or extension already
/// present (or with forward slashes) is accepted unchanged in meaning.
pub(crate) fn pex_archive_path(script_name: &str) -> String {
    // A Fallout 4 / Starfield script name carries its namespace with colons
    // (`Fragments:Quests:QF_FC05_001F0267`): each is a folder in the archive.
    let mut name = script_name.replace(['/', ':'], "\\").to_ascii_lowercase();
    if !name.ends_with(".pex") {
        name.push_str(".pex");
    }
    if !name.starts_with("scripts\\") {
        name = format!("scripts\\{name}");
    }
    name
}

/// Portable package identity for legacy scripts supplied by one archive.
/// The filename is stable across installations; its hash keeps the principal
/// compact and disambiguates archive names that normalize to the same slug.
pub(crate) fn legacy_script_principal(path: &str) -> PrincipalId {
    let filename = path.replace('\\', "/");
    let filename = filename.rsplit('/').next().unwrap_or(&filename);
    let lowercase = filename.to_ascii_lowercase();
    let stem = lowercase
        .strip_suffix(".bsa")
        .or_else(|| lowercase.strip_suffix(".ba2"))
        .unwrap_or(&lowercase);
    let mut slug = String::with_capacity(stem.len().min(64));
    let mut separator = false;
    for byte in stem.bytes() {
        if byte.is_ascii_alphanumeric() {
            slug.push(char::from(byte));
            separator = false;
        } else if !separator && !slug.is_empty() {
            slug.push('-');
            separator = true;
        }
        if slug.len() == 64 {
            break;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("archive");
    }
    let hash = lowercase
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    PrincipalId::new(format!("legacy.scripts.{slug}.{hash:016x}"))
        .expect("generated legacy script principal must satisfy the SDK identity grammar")
}

/// Populate the [`byroredux_scripting::QuestStageFragments`] table from a
/// merged index's QUST `VMAD` fragment bindings — the M47.2 runtime
/// keystone. For every quest carrying stage→`Fragment_N` bindings, resolve
/// its compiled `QF_` `.pex` once (via the [`ScriptProvider`]), decompile,
/// lower each bound fragment body to canonical effects, and register them
/// keyed by `(quest, stage)` for `quest_fragment_dispatch_system`.
///
/// No-op when no `--scripts-bsa` archive is present (nothing to
/// decompile) or on pre-Papyrus games (empty `fragments`). #3161 — the
/// `is_populated()` self-guard below makes this run once per **session**
/// (process lifetime), not once per cell load: the walk is idempotent but
/// not cheap (845 quests on Skyrim, a per-quest `HashMap` build and an
/// archive `extract_pex` per script name), so a later cell load's call is
/// a guarded no-op rather than a re-walk.
pub(crate) fn populate_quest_fragments(
    world: &mut byroredux_core::ecs::world::World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) {
    // #3161 — self-guard rather than trusting each call site to guard.
    // The walk is idempotent but not cheap (845 quests on Skyrim, a
    // per-quest `HashMap` build and an archive `extract_pex` per script
    // name), and it now has more than one caller.
    if world
        .resource::<byroredux_scripting::QuestStageFragments>()
        .is_populated()
    {
        return;
    }
    world
        .resource_mut::<byroredux_scripting::QuestStageFragments>()
        .mark_populated();

    // Fast-out before any per-quest work when no script archive was
    // supplied (the common mesh-only / FO3-FNV case).
    let have_archive = world
        .try_resource::<ScriptProvider>()
        .is_some_and(|p| !p.is_empty());
    if !have_archive {
        return;
    }
    // #3939 — `servable_catalog`, not `catalog`: lowering against aliases the
    // dispatcher has no callback to serve turns a provider call into a
    // barrier that is later dropped along with its tail, mid-fragment, after
    // the prefix already mutated quest state. Declining at the boundary is
    // the consistent outcome.
    let providers = world
        .resource::<byroredux_scripting::PapyrusProviderRuntime>()
        .servable_catalog();

    // M47.2 — own the provider for the walk (re-inserted below) so the
    // callee library can read it while the fragment table is written,
    // without a provider guard held across that write.
    let Some(script_provider) = world.remove_resource::<ScriptProvider>() else {
        return;
    };
    let library = ProviderScriptLibrary::new(&script_provider);
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
            let Some(resolved) = script_provider.resolve_pex(script_name) else {
                log::trace!(
                    "M47.2 quest-fragment: .pex '{script_name}' not in archive (quest {form_id:08X})"
                );
                continue;
            };
            let translation = {
                let mut frags = world.resource_mut::<byroredux_scripting::QuestStageFragments>();
                byroredux_scripting::populate_owned_quest_fragments_from_pex_detailed_with_providers(
                    &mut frags,
                    byroredux_scripting::QuestFormId(form_id),
                    &resolved.bytes,
                    &bindings,
                    byroredux_scripting::OwnedFragmentProviders::new(
                        &providers,
                        &resolved.principal,
                    )
                    .with_library(&library),
                )
            };
            total += translation.inserted;
            byroredux_scripting::record_compatibility_report(
                world,
                translation.fingerprint,
                translation.compatibility,
            );
        }
    }
    drop(library);
    world.insert_resource(script_provider);
    if total > 0 {
        log::info!(
            "M47.2: populated {total} quest-stage fragments from {quests_with_fragments} scripted quests"
        );
    } else {
        log::debug!("M47.2: no quest-stage fragment lowered ({quests_with_fragments} scripted quests with bindings)");
    }
}

/// Populate lowered `SCEN` lifecycle fragments from the compiled `SF_` scripts
/// referenced by each scene's VMAD fragment table.
pub(crate) fn populate_scene_fragments(
    world: &mut byroredux_core::ecs::world::World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) -> usize {
    let have_archive = world
        .try_resource::<ScriptProvider>()
        .is_some_and(|provider| !provider.is_empty());
    if !have_archive {
        return 0;
    }
    // #3939 — `servable_catalog`, not `catalog`: lowering against aliases the
    // dispatcher has no callback to serve turns a provider call into a
    // barrier that is later dropped along with its tail, mid-fragment, after
    // the prefix already mutated quest state. Declining at the boundary is
    // the consistent outcome.
    let providers = world
        .resource::<byroredux_scripting::PapyrusProviderRuntime>()
        .servable_catalog();
    // M47.2 — own the provider for the walk (re-inserted below) so the
    // script library can classify quest-script-typed properties.
    let Some(script_provider) = world.remove_resource::<ScriptProvider>() else {
        return 0;
    };
    let library = ProviderScriptLibrary::new(&script_provider);
    // M47.2 — the scripts the Papyrus VM needs for the scene fragments it
    // runs, installed once the walk is done.
    let vm_scripts = std::cell::RefCell::new(Vec::new());

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
            let resolved = script_provider.resolve_pex(script_name);
            let Some(resolved) = resolved else {
                log::trace!(
                    "scene-fragment: .pex '{script_name}' not in archive (scene {scene_form_id:08X})"
                );
                continue;
            };
            let translation = {
                let mut fragments = world.resource_mut::<byroredux_scripting::SceneFragments>();
                byroredux_scripting::populate_owned_scene_fragments_from_pex_detailed_with_providers(
                    &mut fragments,
                    scene_form_id,
                    byroredux_scripting::QuestFormId(quest_form_id),
                    scene.script_instance.as_ref(),
                    &resolved.bytes,
                    &bindings,
                    byroredux_scripting::OwnedFragmentProviders::new(
                        &providers,
                        &resolved.principal,
                    )
                    .with_library(&library)
                    .with_vm_scripts(&vm_scripts),
                )
            };
            total += translation.inserted;
            byroredux_scripting::record_compatibility_report(
                world,
                translation.fingerprint,
                translation.compatibility,
            );
        }
    }
    drop(library);
    world.insert_resource(script_provider);
    install_vm_scripts(world, vm_scripts.into_inner());
    total
}

/// M47.2 — install the scripts a scene or INFO walk's Papyrus VM fragments
/// need where the VM reads them.
fn install_vm_scripts(
    world: &mut byroredux_core::ecs::world::World,
    scripts: Vec<std::sync::Arc<byroredux_scripting::translate::effects::PapyrusScript>>,
) {
    if scripts.is_empty() {
        return;
    }
    let mut fragments = world.resource_mut::<byroredux_scripting::QuestStageFragments>();
    for script in scripts {
        fragments.insert_vm_script(script);
    }
}

/// Populate lowered `INFO` response fragments (the `TIF_` topic-info
/// scripts) from each dialogue's `VMAD` fragment bindings — the #5152
/// sibling of [`populate_quest_fragments`] and `populate_scene_fragments`,
/// dispatched when the dialogue selection speaks a line. The OnBegin
/// binding drives stage-advance-on-speak; the OnEnd binding rides along
/// for the selection-change and conversation-close paths.
///
/// Session-gated like the quest walk: vanilla Skyrim carries 5 257
/// INFO-bound fragments (one `TIF_` script per INFO, so one archive
/// extract each), and a legitimately-empty table (no `--scripts-bsa`,
/// pre-Papyrus game) must not re-walk every cell load.
pub(crate) fn populate_info_fragments(
    world: &mut byroredux_core::ecs::world::World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) {
    if world
        .resource::<byroredux_scripting::DialogueInfoFragments>()
        .is_populated()
    {
        return;
    }
    world
        .resource_mut::<byroredux_scripting::DialogueInfoFragments>()
        .mark_populated();

    let have_archive = world
        .try_resource::<ScriptProvider>()
        .is_some_and(|provider| !provider.is_empty());
    if !have_archive {
        return;
    }
    // #3939 — the servable catalog, for the same boundary-decline reason
    // the quest and scene walks document.
    let providers = world
        .resource::<byroredux_scripting::PapyrusProviderRuntime>()
        .servable_catalog();
    // M47.2 — own the provider for the walk (re-inserted below) so the
    // script library can classify quest-script-typed properties.
    let Some(script_provider) = world.remove_resource::<ScriptProvider>() else {
        return;
    };
    let library = ProviderScriptLibrary::new(&script_provider);
    // M47.2 — as the scene walk: the Papyrus VM's scripts.
    let vm_scripts = std::cell::RefCell::new(Vec::new());

    let mut infos_with_bindings = 0usize;
    let mut total = 0usize;
    for dialogue in index.dialogues.values() {
        // The owning-quest precedence the dialogue selection itself uses.
        let Some(&context) = dialogue.quest_refs.first() else {
            continue;
        };
        for info in &dialogue.infos {
            if info.script_fragments.is_empty() {
                continue;
            }
            infos_with_bindings += 1;
            let vmad = info.script_instance.clone();
            // Every INFO fragment binding names the same TIF_ script, but
            // group defensively like the quest walk.
            let mut by_script: std::collections::HashMap<&str, Vec<(bool, &str)>> =
                std::collections::HashMap::new();
            for fragment in &info.script_fragments {
                by_script
                    .entry(fragment.script_name.as_str())
                    .or_default()
                    .push((fragment.on_begin, fragment.fragment_name.as_str()));
            }
            for (script_name, bindings) in by_script {
                let resolved = script_provider.resolve_pex(script_name);
                let Some(resolved) = resolved else {
                    log::trace!(
                        "info-fragment: .pex '{script_name}' not in archive (info {:#08X})",
                        info.form_id
                    );
                    continue;
                };
                let inserted = {
                    let mut fragments =
                        world.resource_mut::<byroredux_scripting::DialogueInfoFragments>();
                    byroredux_scripting::populate_owned_info_fragments_from_pex_with_providers(
                        &mut fragments,
                        info.form_id,
                        byroredux_scripting::QuestFormId(context),
                        vmad.as_ref(),
                        &resolved.bytes,
                        &bindings,
                        byroredux_scripting::OwnedFragmentProviders::new(
                            &providers,
                            &resolved.principal,
                        )
                        .with_library(&library)
                        .with_vm_scripts(&vm_scripts),
                    )
                };
                total += inserted;
            }
        }
    }
    drop(library);
    world.insert_resource(script_provider);
    install_vm_scripts(world, vm_scripts.into_inner());
    if total > 0 {
        log::info!(
            "M47.2: populated {total} INFO response fragments from {infos_with_bindings} bound infos"
        );
    }
}

/// One record's fragment bindings as a populate walk reads them: its FormID,
/// its fragment script's properties, and each binding's (event, script name,
/// function name).
type RecordBindings<'a, E> = (u32, Option<&'a byroredux_plugin::esm::records::script_instance::ScriptInstanceData>, Vec<(E, &'a str, &'a str)>);

/// M47.2 — install the Papyrus VM's fragments for records that bind a fragment
/// script (packages, terminals), grouped by script and checked by the VM.
/// Session-gated like the INFO walk: it runs once per `table`. Returns the
/// number of bindings installed.
fn populate_record_fragments<E: Copy + Send + Sync + 'static>(
    world: &mut byroredux_core::ecs::world::World,
    records: Vec<RecordBindings<'_, E>>,
) -> usize {
    use byroredux_scripting::translate::effects::ScriptLibrary as _;
    if world.resource::<byroredux_scripting::FragmentTable<E>>().is_populated() {
        return 0;
    }
    world
        .resource_mut::<byroredux_scripting::FragmentTable<E>>()
        .mark_populated();
    if world
        .try_resource::<ScriptProvider>()
        .is_none_or(|provider| provider.is_empty())
    {
        return 0;
    }
    let Some(script_provider) = world.remove_resource::<ScriptProvider>() else {
        return 0;
    };
    let library = ProviderScriptLibrary::new(&script_provider);
    let mut vm_scripts = Vec::new();
    let mut fragments = byroredux_scripting::FragmentTable::<E>::default();
    let mut total = 0usize;
    for (form_id, vmad, bindings) in records {
        let mut by_script: std::collections::HashMap<&str, Vec<_>> = std::collections::HashMap::new();
        for (event, script_name, function) in bindings {
            by_script.entry(script_name).or_default().push((event, function));
        }
        for (script_name, bindings) in by_script {
            let Some(script) = library.script(script_name) else {
                continue;
            };
            total += byroredux_scripting::populate_fragments(
                &mut fragments,
                form_id,
                vmad,
                script,
                &bindings,
                &library,
                &mut |needed| vm_scripts.push(needed),
            );
        }
    }
    drop(library);
    world.insert_resource(script_provider);
    install_vm_scripts(world, vm_scripts);
    fragments.mark_populated();
    world.insert_resource(fragments);
    total
}

/// M47.2 — the package (`PF_`) fragments each `PACK`'s `VMAD` binds, for the
/// package-transition dispatcher.
pub(crate) fn populate_package_fragments(
    world: &mut byroredux_core::ecs::world::World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) {
    let records = index
        .packages
        .iter()
        .filter(|(_, package)| !package.script_fragments.is_empty())
        .map(|(&form_id, package)| {
            let bindings = package
                .script_fragments
                .iter()
                .map(|f| (f.event, f.script_name.as_str(), f.fragment_name.as_str()))
                .collect();
            (form_id, package.script_instance.as_ref(), bindings)
        })
        .collect();
    let total = populate_record_fragments(world, records);
    if total > 0 {
        log::info!("M47.2: installed {total} package fragments");
    }
}

/// M47.2 — the perk (`PRKF_`) fragments each Fallout 4 / Starfield perk binds to
/// its entries, for the perk-entry dispatcher.
pub(crate) fn populate_perk_fragments(
    world: &mut byroredux_core::ecs::world::World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) {
    let records = index
        .perks
        .iter()
        .filter(|(_, perk)| !perk.script_fragments.is_empty())
        .map(|(&form_id, perk)| {
            let bindings = perk
                .script_fragments
                .iter()
                .map(|f| (byroredux_scripting::PerkEntryId(f.index), f.script_name.as_str(), f.fragment_name.as_str()))
                .collect();
            (form_id, None, bindings)
        })
        .collect();
    let total = populate_record_fragments(world, records);
    if total > 0 {
        log::info!("M47.2: installed {total} perk fragments");
    }
}

/// M47.2 — the terminal (`TERM_`) fragments each Fallout 4 / Starfield
/// terminal binds to its menu items, for the terminal-selection dispatcher.
pub(crate) fn populate_terminal_fragments(
    world: &mut byroredux_core::ecs::world::World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) {
    let records = index
        .terminals
        .iter()
        .filter(|(_, terminal)| !terminal.script_fragments.is_empty())
        .map(|(&form_id, terminal)| {
            let bindings = terminal
                .script_fragments
                .iter()
                .map(|f| (byroredux_scripting::TerminalItem(f.index), f.script_name.as_str(), f.fragment_name.as_str()))
                .collect();
            (form_id, terminal.script_instance.as_ref(), bindings)
        })
        .collect();
    let total = populate_record_fragments(world, records);
    if total > 0 {
        log::info!("M47.2: installed {total} terminal fragments");
    }
}

/// Install every parsed Skyrim+ `SCEN` definition and its referenced
/// `DIAL`/`INFO` topics into the ECS runtime.
///
/// The scripting crate deduplicates by FormID and preserves existing player
/// state, so this is safe on cell transitions and repeated load-order parses.
/// It intentionally does not require a script archive: SCEN phase/action data
/// and VMAD function bindings are carried by the plugin record itself.
pub(crate) fn populate_quest_trigger_approaches(
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
            let translation = byroredux_scripting::translate_pex_detailed(
                &bytes,
                index.game,
                Some(&instance),
                None,
            );
            byroredux_scripting::record_compatibility_report(
                world,
                translation.fingerprint,
                translation.compatibility,
            );
            let Some(recognized) = translation.recognized else {
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

/// M47.3 — install the ObScript object and item script tables if this world has none yet.
///
/// An interior load spawns its references *before* [`crate::scene::populate_scene_runtime`] runs (that
/// happens in the job's `finish`), and a reference's script attaches as it spawns — so on the
/// first interior load of a session the table the attach asks for did not exist, and no object
/// script ran in that cell (found live: `obscript.objects` listed none in a cell full of them,
/// the trace saying "table: None"). The interior job calls this once its records are parsed;
/// [`crate::scene::populate_scene_runtime`] still refreshes the tables on every load.
pub(crate) fn ensure_obscript_tables(
    world: &mut byroredux_core::ecs::world::World,
    index: &byroredux_plugin::esm::records::EsmIndex,
) {
    if world
        .try_resource::<byroredux_scripting::obscript_objects::ObScriptObjectTable>()
        .is_some()
    {
        return;
    }
    byroredux_scripting::obscript_objects::install_object_scripts(world, &index.scripts);
    byroredux_scripting::obscript_objects::install_item_scripts(world, index);
    byroredux_scripting::magic_effects::install_effect_scripts(world, &index.scripts);
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
                        provider.archives.push(ScriptArchive {
                            archive: a,
                            principal: legacy_script_principal(path),
                        });
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
    /// #5998 — a reference without a FormID can never be delivered to, so the VM
    /// attach declines before it spends a decompile + check on it. Driving the
    /// positive path needs a script archive; pin the order at the source: the
    /// identity gate precedes the verdict cache and the provider lookup.
    #[test]
    fn the_vm_reference_attach_declines_an_identityless_reference_first() {
        let production = byroredux_core::source_scan::production_text(include_str!("script.rs"));
        let attach = &production[production
            .find("pub(crate) fn attach_vm_reference_script")
            .expect("attach present")..];
        let gate = attach
            .find("condition::entity_global_form_id(world, entity).is_none()")
            .expect("the identity gate (#5998)");
        let cache = attach.find("VmReferenceScriptChecks").expect("verdict cache lookup");
        let provider = attach.find("ScriptProvider>()").expect("provider lookup");
        assert!(gate < cache && gate < provider, "the gate must come before any decompile work");
    }

    /// #6001 — the allowance is spent per uncached script, runs out after
    /// `ALIAS_SCRIPT_CHECKS_PER_FRAME`, and the check loop spends it before
    /// every decompile + check (the quest loop stops, the quest resumes next frame).
    #[test]
    fn alias_script_checks_are_budgeted_per_script_not_per_quest() {
        let mut budget = super::ScriptCheckBudget(super::ALIAS_SCRIPT_CHECKS_PER_FRAME);
        for spent in 0..super::ALIAS_SCRIPT_CHECKS_PER_FRAME {
            assert!(budget.try_spend(), "check {spent} is inside the allowance");
        }
        assert!(!budget.try_spend(), "the allowance is exhausted");
        assert!(!budget.try_spend(), "and stays exhausted");

        let production = byroredux_core::source_scan::production_text(include_str!("script.rs"));
        let system = &production[production
            .find("pub(crate) fn check_running_quest_alias_scripts")
            .expect("alias checker present")..];
        let spend = system.find("budget.try_spend()").expect("the loop spends the budget");
        let check = system.find("check_alias_script(").expect("the loop checks the script");
        assert!(spend < check, "the budget must be spent before the decompile + check");
    }

    use super::*;

    /// #3161 — the walk latches on a dedicated flag, not on `is_empty()`.
    ///
    /// `is_empty()` reads only the `map` half of a resource the walk fills in
    /// two independent halves, so a session whose VMAD side populated but
    /// whose `.pex` side resolved nothing — a wrong or missing
    /// `--scripts-bsa` — re-ran the whole 845-quest walk on every exterior
    /// cell `begin`. Both maps are empty here, the case where `is_empty()`
    /// stays true forever, so a regression to that guard makes the second
    /// call re-walk and this assertion fail.
    #[test]
    fn the_quest_fragment_walk_does_not_repeat_when_it_found_nothing() {
        let mut world = byroredux_core::ecs::World::new();
        byroredux_scripting::register(&mut world);
        let index = byroredux_plugin::esm::records::EsmIndex::default();

        populate_quest_fragments(&mut world, &index);
        let frags = world.resource::<byroredux_scripting::QuestStageFragments>();
        assert!(frags.is_empty(), "nothing to lower without an archive");
        assert!(frags.is_populated(), "but the walk is latched as done");
        drop(frags);

        // A second call must early-out on the latch, not on `is_empty()` —
        // which is still true and would wave it straight back through.
        populate_quest_fragments(&mut world, &index);
        assert!(world
            .resource::<byroredux_scripting::QuestStageFragments>()
            .is_populated());
    }

    /// Regression for #3939. Every production seam that lowers against the
    /// provider catalog must read `servable_catalog`, never `catalog`: the
    /// latter can return a non-empty `engine_compatibility()` catalog with
    /// no live callback behind it — `PapyrusProviderRuntime::default()` does
    /// exactly that, and so does every early return in
    /// `load_requested_extensions` that exits before the callback is synced.
    /// A fragment lowered that way turns a provider call into a barrier that
    /// dispatch later drops together with its tail, after the prefix has
    /// already mutated quest state.
    ///
    /// Source scan because the alternative is booting the extension host
    /// with a deliberately broken manifest set; the distinction is one
    /// method name at three call sites, which is exactly what a scan is for.
    #[test]
    fn every_lowering_seam_reads_the_servable_catalog() {
        const SEAMS: [(&str, &str); 2] = [
            ("asset_provider/script.rs", include_str!("script.rs")),
            (
                "cell_loader/references/attach.rs",
                include_str!("../cell_loader/references/attach.rs"),
            ),
        ];
        // Composed at runtime so this test's own body cannot satisfy — or
        // trip — the scan it performs.
        let unservable = format!("{}{}", ".catalog", "()");
        let servable = format!("{}{}", ".servable_catalog", "()");

        // Anchored on each mention of the runtime type rather than on a
        // leading `split("#[cfg(test)]")`: `attach.rs` interleaves test
        // modules from line 335, so a prefix scan would stop before its
        // seam at 702 and pass vacuously — the exact failure mode #3442
        // found in `draw.rs`.
        let anchor = format!("{}{}", "PapyrusProvider", "Runtime");
        let mut seams = 0usize;
        for (name, source) in SEAMS {
            for (index, _) in source.match_indices(anchor.as_str()) {
                let window = &source[index..(index + 240).min(source.len())];
                if window.contains(servable.as_str()) {
                    seams += 1;
                    continue;
                }
                assert!(
                    !window.contains(unservable.as_str()),
                    "{name} lowers against the raw provider catalog. With no \
                     live callback that catalog is unservable, and the \
                     fragment it lowers strands its own tail at dispatch \
                     instead of declining (#3939)"
                );
            }
        }
        assert!(
            seams >= 3,
            "expected the three production lowering seams, found {seams} — \
             re-derive this pin (#3939)"
        );
    }

    /// A namespaced Fallout 4 / Starfield script name is a folder path in the archive:
    /// every `Fragments:Quests:QF_*` quest fragment script used to be "not in archive".
    #[test]
    fn a_namespaced_script_name_maps_to_its_archive_folders() {
        assert_eq!(
            pex_archive_path("Fragments:Quests:QF_FC05_001F0267"),
            r"scripts\fragments\quests\qf_fc05_001f0267.pex"
        );
        assert_eq!(pex_archive_path("DA10MainDoorScript"), r"scripts\da10maindoorscript.pex");
        assert_eq!(pex_archive_path(r"scripts\a\b.pex"), r"scripts\a\b.pex");
    }
}
