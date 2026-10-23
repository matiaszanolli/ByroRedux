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
fn populate_info_fragments(
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
fn populate_package_fragments(
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
fn populate_perk_fragments(
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
fn populate_terminal_fragments(
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
/// An interior load spawns its references *before* [`populate_scene_runtime`] runs (that
/// happens in the job's `finish`), and a reference's script attaches as it spawns — so on the
/// first interior load of a session the table the attach asks for did not exist, and no object
/// script ran in that cell (found live: `obscript.objects` listed none in a cell full of them,
/// the trace saying "table: None"). The interior job calls this once its records are parsed;
/// [`populate_scene_runtime`] still refreshes the tables on every load.
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
    // #4415 — canonical spells for runtime `AddSpell` / `RemoveSpell`.
    world.insert_resource(byroredux_scripting::SpellCatalog::from_index(index));
    // #4414 — the ambient-hostility detection range (`fSneakMaxDistance`).
    crate::systems::install_detection_config(world, index);
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
    // M47.2 — Actor.GetCrimeFaction.
    byroredux_scripting::install_crime_factions(world, index);
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
                crate::cell_loader::position_zup_to_yup(placed.position),
                crate::cell_loader::rotation_zup_to_yup_quat(placed.rotation),
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
        let src = byroredux_core::source_scan::production_text(include_str!("script.rs"));

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
