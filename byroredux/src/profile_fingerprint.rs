//! The profile fingerprint a save stores (PRD legacy-mod-compatibility LMC-ID-007, LMC-SAV-001; #6670).
//!
//! A [`ProfileRecord`] is what a session's profile is made of, as far as a save can tell: the plugins in load order,
//! each with its identity and the xxHash64 of its file; the mod roots in order (LMC-ID-012), a disabled one marked; each
//! extension manifest with the hash of its file; the extension grants; and the record policy (LMC-REC-003). The
//! fingerprint is the xxHash64 of all of it, so two sessions with one fingerprint ran the same profile.
//!
//! The session's record is the [`SessionProfile`] resource, built wherever a load path installs the load order (the
//! plugins' hashes come from the fold, which reads every plugin's bytes anyway). A save writes it; loading a save
//! compares the record the save carries with the session's and reports every difference ([`differences`], in `load` and
//! `save.info`), while the session keeps its own: a load never changes the profile. What a difference means for the
//! loaded world (LMC-SAV-002..004) is not decided here.
//!
//! Not in the record yet: the extension packages' component hashes and selected providers, which wait for the
//! package manifest of the sandbox design (PKG-010), and the content of a root's loose files (a root enters by
//! identity and order).

use byroredux_core::ecs::Resource;
use byroredux_core::form_id::plugin_key;
use serde::{Deserialize, Serialize};

/// A named file of the profile and the xxHash64 of its content (`0` when it could not be read).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashedEntry {
    pub name: String,
    pub content_hash: u64,
}

/// What a session's profile is made of, and its fingerprint (see the module docs).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileRecord {
    /// xxHash64 of every other field.
    pub fingerprint: u64,
    /// The plugins in load order, by identity (LMC-ID-001).
    pub plugins: Vec<HashedEntry>,
    /// The mod roots in order, by identity; a root activation disabled carries ` (disabled)`.
    pub mod_roots: Vec<String>,
    /// The extension manifests in the order given, by file name.
    pub extensions: Vec<HashedEntry>,
    /// The extension grants, sorted.
    pub grants: Vec<String>,
    /// The record policy: `merge` or `legacy`.
    pub merge_policy: String,
}

impl ProfileRecord {
    /// The record with its fingerprint computed over every other field.
    pub fn sealed(mut self) -> Self {
        self.fingerprint = 0;
        let bytes = serde_json::to_vec(&self).expect("a profile record always serialises");
        self.fingerprint = twox_hash::XxHash64::oneshot(0, &bytes);
        self
    }
}

/// The session's profile record; a load keeps it (see the module docs).
#[derive(Clone, Debug, Default)]
pub struct SessionProfile(pub ProfileRecord);

impl Resource for SessionProfile {}

/// The save column's encoder: the session's record.
pub(crate) fn save_profile(
    _: &byroredux_core::ecs::World,
    live: &SessionProfile,
) -> Result<ProfileRecord, byroredux_save::SaveError> {
    Ok(live.0.clone())
}

/// The save column's decoder: the session keeps its own profile, whatever the save carried ([`differences`] reports
/// what changed).
pub(crate) fn load_profile(
    world: &byroredux_core::ecs::World,
    _saved: ProfileRecord,
) -> Result<SessionProfile, byroredux_save::SaveError> {
    Ok(world.try_resource::<SessionProfile>().map(|live| live.clone()).unwrap_or_default())
}

/// The xxHash64 of the file at `path`, `0` when it cannot be read.
fn file_hash(path: &str) -> u64 {
    std::fs::read(path).map(|bytes| twox_hash::XxHash64::oneshot(0, &bytes)).unwrap_or(0)
}

/// The record of the session whose plugins are `plugins` (identity and content hash, in load order), whose roots are
/// `roots` and whose effective args are `args`.
pub(crate) fn session_record(
    plugins: impl IntoIterator<Item = (String, u64)>,
    roots: &[crate::mod_roots::ModRoot],
    args: &[String],
) -> ProfileRecord {
    let mut grants = crate::cli_args::parse_all_string_args(args, "--extension-grant");
    grants.sort();
    ProfileRecord {
        fingerprint: 0,
        plugins: plugins.into_iter().map(|(name, content_hash)| HashedEntry { name, content_hash }).collect(),
        mod_roots: roots
            .iter()
            .filter(|root| !root.unmanaged)
            .map(|root| if root.disabled { format!("{} (disabled)", root.identity) } else { root.identity.clone() })
            .collect(),
        extensions: crate::cli_args::parse_all_string_args(args, "--extension")
            .iter()
            .map(|path| HashedEntry { name: plugin_key(path), content_hash: file_hash(path) })
            .collect(),
        grants,
        merge_policy: crate::plugin_activation::merge_policy().name().to_owned(),
    }
    .sealed()
}

/// The session's record for `order`, from the boot activation's roots and the effective args.
pub(crate) fn session_profile(order: &crate::cell_loader::load_order::LoadOrder) -> SessionProfile {
    let plugins = order.iter().enumerate().map(|(position, name)| (plugin_key(name), order.content_hash(position).unwrap_or(0)));
    let roots = crate::plugin_activation::activation_report().map_or(&[][..], |report| report.roots.as_slice());
    SessionProfile(session_record(plugins, roots, &crate::cli_args::effective_args()))
}

/// What changed between the profile a save was made with and the session's, one line each; empty when the
/// fingerprints match.
pub fn differences(saved: &ProfileRecord, live: &ProfileRecord) -> Vec<String> {
    if saved.fingerprint == live.fingerprint {
        return Vec::new();
    }
    let mut lines = Vec::new();
    files_changed(&mut lines, "plugin", &saved.plugins, &live.plugins);
    let order = |of: &[HashedEntry], other: &[HashedEntry]| -> Vec<String> {
        of.iter().filter(|entry| other.iter().any(|o| o.name == entry.name)).map(|entry| entry.name.clone()).collect()
    };
    let (was, now) = (order(&saved.plugins, &live.plugins), order(&live.plugins, &saved.plugins));
    if was != now {
        lines.push(format!("load order changed: {} → {}", was.join(", "), now.join(", ")));
    }
    if saved.mod_roots != live.mod_roots {
        lines.push(format!("mod roots changed: [{}] → [{}]", saved.mod_roots.join(", "), live.mod_roots.join(", ")));
    }
    files_changed(&mut lines, "extension", &saved.extensions, &live.extensions);
    if saved.grants != live.grants {
        lines.push(format!("extension grants changed: [{}] → [{}]", saved.grants.join(", "), live.grants.join(", ")));
    }
    if saved.merge_policy != live.merge_policy {
        lines.push(format!("record policy changed: {} → {}", saved.merge_policy, live.merge_policy));
    }
    if lines.is_empty() {
        lines.push("the profile record differs in a way no field shows (a record from another engine version)".to_owned());
    }
    lines
}

/// The profile record `snapshot` carries; `None` for a snapshot without one.
pub fn saved_record(snapshot: &byroredux_save::Snapshot) -> Option<ProfileRecord> {
    snapshot.resources.get("SessionProfile").and_then(|value| serde_json::from_value(value.clone()).ok())
}

/// How the profile of the save `snapshot` differs from the session's in `world`: a summary line, then one line per
/// difference; empty when they match, or when either side has no record.
pub fn save_profile_lines(world: &byroredux_core::ecs::World, snapshot: &byroredux_save::Snapshot) -> Vec<String> {
    let Some(saved) = saved_record(snapshot) else { return Vec::new() };
    let Some(live) = world.try_resource::<SessionProfile>().map(|live| live.0.clone()) else { return Vec::new() };
    let changes = differences(&saved, &live);
    if changes.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![format!(
        "the save was made with another profile (fingerprint {:016x}, this session {:016x}), {} change{}:",
        saved.fingerprint,
        live.fingerprint,
        changes.len(),
        if changes.len() == 1 { "" } else { "s" }
    )];
    lines.extend(changes.into_iter().map(|line| format!("  {line}")));
    lines
}

/// One line per `what` file only in `saved`, only in `live`, or in both with different content.
fn files_changed(lines: &mut Vec<String>, what: &str, saved: &[HashedEntry], live: &[HashedEntry]) {
    for entry in saved {
        match live.iter().find(|now| now.name == entry.name) {
            None => lines.push(format!("{what} {}: in the save's profile, absent now", entry.name)),
            Some(now) if now.content_hash != entry.content_hash => {
                lines.push(format!("{what} {}: its content changed since the save", entry.name));
            }
            Some(_) => {}
        }
    }
    for entry in live.iter().filter(|now| !saved.iter().any(|was| was.name == now.name)) {
        lines.push(format!("{what} {}: added since the save", entry.name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(plugins: &[(&str, u64)], roots: &[&str], policy: &str) -> ProfileRecord {
        ProfileRecord {
            fingerprint: 0,
            plugins: plugins.iter().map(|&(name, content_hash)| HashedEntry { name: name.into(), content_hash }).collect(),
            mod_roots: roots.iter().map(|root| (*root).to_owned()).collect(),
            extensions: Vec::new(),
            grants: Vec::new(),
            merge_policy: policy.into(),
        }
        .sealed()
    }

    #[test]
    fn the_fingerprint_covers_every_field() {
        let base = record(&[("skyrim.esm", 1), ("mod.esp", 2)], &["mod"], "merge");
        assert_eq!(base, record(&[("skyrim.esm", 1), ("mod.esp", 2)], &["mod"], "merge"), "deterministic");
        for other in [
            record(&[("skyrim.esm", 1), ("mod.esp", 3)], &["mod"], "merge"),
            record(&[("mod.esp", 2), ("skyrim.esm", 1)], &["mod"], "merge"),
            record(&[("skyrim.esm", 1), ("mod.esp", 2)], &["mod (disabled)"], "merge"),
            record(&[("skyrim.esm", 1), ("mod.esp", 2)], &["mod"], "legacy"),
        ] {
            assert_ne!(base.fingerprint, other.fingerprint, "{other:?}");
        }
    }

    #[test]
    fn the_differences_name_each_change() {
        let saved = record(&[("skyrim.esm", 1), ("a.esp", 2), ("gone.esp", 3), ("b.esp", 4)], &["a"], "merge");
        let live = record(&[("skyrim.esm", 1), ("b.esp", 9), ("a.esp", 2), ("new.esp", 5)], &["a", "b"], "legacy");
        let lines = differences(&saved, &live);
        assert_eq!(
            lines,
            [
                "plugin gone.esp: in the save's profile, absent now",
                "plugin b.esp: its content changed since the save",
                "plugin new.esp: added since the save",
                "load order changed: skyrim.esm, a.esp, b.esp → skyrim.esm, b.esp, a.esp",
                "mod roots changed: [a] → [a, b]",
                "record policy changed: merge → legacy",
            ]
        );
        assert!(differences(&saved, &saved).is_empty());
    }

    /// LMC-SAV-001 — a save carries the session's record; loading it keeps the live session's and reports the
    /// difference.
    #[test]
    fn a_save_carries_its_profile_and_a_load_keeps_the_sessions() {
        let registry = crate::save_io::build_save_registry();
        let mut world = byroredux_core::ecs::World::new();
        let made_with = record(&[("skyrim.esm", 1), ("mod.esp", 2)], &[], "merge");
        world.insert_resource(SessionProfile(made_with.clone()));
        let snapshot = byroredux_save::save_world(&world, &registry).expect("save");
        assert_eq!(saved_record(&snapshot), Some(made_with.clone()));
        assert!(save_profile_lines(&world, &snapshot).is_empty(), "the same profile reports nothing");

        let now = record(&[("skyrim.esm", 1)], &[], "merge");
        world.insert_resource(SessionProfile(now.clone()));
        byroredux_save::restore_resources(&mut world, &registry, &snapshot).expect("restore");
        assert_eq!(world.resource::<SessionProfile>().0, now, "the session keeps its own profile");
        let lines = save_profile_lines(&world, &snapshot);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].starts_with("the save was made with another profile"), "{lines:?}");
        assert_eq!(lines[1], "  plugin mod.esp: in the save's profile, absent now");
    }

    #[test]
    fn a_session_record_reads_roots_extensions_and_grants() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("Mod.toml");
        std::fs::write(&manifest, b"id = 'org.example.mod'").unwrap();
        let manifest = manifest.to_string_lossy().into_owned();
        let args: Vec<String> =
            ["byroredux", "--extension", &manifest, "--extension-grant", "z=*", "--extension-grant", "a=b"]
                .iter()
                .map(|arg| (*arg).to_owned())
                .collect();
        let data = crate::mod_roots::discover(Some(tmp.path()), &[]).0;
        let record = session_record([("skyrim.esm".to_owned(), 7)], &data, &args);
        assert_eq!(record.plugins, [HashedEntry { name: "skyrim.esm".into(), content_hash: 7 }]);
        assert!(record.mod_roots.is_empty(), "the unmanaged root is not a mod root");
        assert_eq!(record.extensions.len(), 1);
        assert_eq!(record.extensions[0].name, "mod.toml");
        assert_ne!(record.extensions[0].content_hash, 0);
        assert_eq!(record.grants, ["a=b", "z=*"]);
        assert_eq!(record, record.clone().sealed(), "sealed once");
    }
}
