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
