//! BSA/BA2-backed texture and mesh extraction.

mod actor_graph;
mod animation;
mod archetype_idle;
mod archive;
pub(crate) mod audio;
pub(crate) mod material;
mod object_graph;
mod script;
pub(crate) mod terrain_height;
mod texture;
mod texture_prefetch;

pub(crate) use actor_graph::*;
pub(crate) use animation::*;
pub(crate) use archetype_idle::*;
pub(crate) use archive::*;
pub(crate) use audio::*;
pub(crate) use material::*;
pub(crate) use object_graph::*;
pub(crate) use script::*;
pub(crate) use texture::*;
pub(crate) use texture_prefetch::{prefetch_textures, PrefetchStats};

// The mesh-path prefix rule lives in `byroredux_core::paths` (#7145: the renderer's mesh
// cache keys through it too); re-exported here, `pub` because it is used outside the crate.
pub use byroredux_core::paths::{archive_mesh_path, canonical_mesh_key, normalize_mesh_path};

/// Whether this is the first report of `key` under `scope` in this process:
/// the one home of the "warn once per asset path" sets the providers and
/// their consumers keep, so a defect seen on every NPC, cell or frame logs
/// one line (#7130). Scopes keep different warnings about one path apart.
pub(crate) fn report_once(scope: &'static str, key: &str) -> bool {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static REPORTED: OnceLock<Mutex<HashSet<(&'static str, String)>>> = OnceLock::new();
    REPORTED
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert((scope, key.to_owned()))
}

#[cfg(test)]
mod report_once_tests {
    use super::report_once;

    #[test]
    fn a_key_is_reported_once_per_scope() {
        assert!(report_once("report_once_tests/a", "meshes\\x.egm"));
        assert!(!report_once("report_once_tests/a", "meshes\\x.egm"), "the second sighting is quiet");
        assert!(report_once("report_once_tests/b", "meshes\\x.egm"), "another scope reports it again");
        assert!(report_once("report_once_tests/a", "meshes\\y.egm"));
    }
}

#[cfg(test)]
mod tests;
