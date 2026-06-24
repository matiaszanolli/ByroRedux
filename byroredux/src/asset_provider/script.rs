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
    /// Returns the first archive hit, or `None` when no archive carries
    /// the script.
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
