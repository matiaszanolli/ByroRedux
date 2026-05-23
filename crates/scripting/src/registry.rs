//! M47.0 Phase 2 — `ScriptRegistry` resource.
//!
//! Maps Papyrus / SCPT `editor_id` strings to script-spawn functions
//! that install ECS state components on a target entity. Driven by
//! the cell loader at REFR spawn time: when a REFR's base record
//! carries a `SCRI` cross-ref → SCPT → editor_id, the loader looks
//! the editor_id up here and runs the spawner to attach script state.
//!
//! ## Why editor_id (not FormID)
//!
//! Editor IDs are stable across plugin loads — they're authored
//! strings in the ESM source, not FormIDs that shift with load order.
//! Two SCPT records across plugins that share an editor_id
//! ("defaultRumbleOnActivate") are intentionally the same script;
//! they should resolve to the same spawner. Keying on FormID would
//! force per-plugin registration even for vanilla content.
//!
//! ## Spawner shape
//!
//! ```rust,ignore
//! fn spawn_rumble_on_activate(world: &mut World, entity: EntityId) {
//!     let mut q = world.query_mut::<RumbleOnActivate>().unwrap();
//!     q.insert(entity, RumbleOnActivate::default());
//! }
//! ```
//!
//! Spawners take only `(world, entity)` — they don't see the
//! `ScriptRecord` because the spawner IS the script implementation
//! and already knows what state to attach. Property overrides from
//! Skyrim+ `VMAD` are a follow-up; today the spawner ships .psc
//! defaults.

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use std::collections::HashMap;

/// Function pointer that installs a script's ECS state component(s)
/// on `entity`. Called by the cell loader after the REFR has spawned.
/// Spawners are expected to be cheap: a single `query_mut + insert`
/// per call is the typical shape.
pub type ScriptSpawnFn = fn(world: &mut World, entity: EntityId);

/// Resource: editor_id → spawn function map.
///
/// Populated at engine init by [`crate::papyrus_demo::register_spawners`]
/// (and downstream registries when additional scripts land). Consumed
/// by the cell loader's per-REFR walk at spawn time.
#[derive(Default)]
pub struct ScriptRegistry {
    spawners: HashMap<String, ScriptSpawnFn>,
}

impl ScriptRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `spawn` to fire for any SCPT record whose `editor_id`
    /// equals `editor_id`. Case-sensitive — Papyrus / SCPT editor_ids
    /// are case-sensitive in Bethesda tooling, so the registry stays
    /// case-sensitive too.
    ///
    /// Re-registering the same `editor_id` replaces the prior spawner.
    /// This is the intended way to override a vanilla script with a
    /// mod-shipped variant of the same name.
    pub fn register(&mut self, editor_id: &str, spawn: ScriptSpawnFn) {
        self.spawners.insert(editor_id.to_string(), spawn);
    }

    /// Look up the spawn function for an editor_id. Returns `None`
    /// when no spawner is registered — the cell loader treats this
    /// as a "no consumer for this script (yet)" signal and skips
    /// silently. Unregistered scripts are common in real plugins
    /// (every SCPT in vanilla Fallout 3 = ~1257 scripts, of which
    /// M47.0 ships hand-translated equivalents for ~5).
    pub fn lookup(&self, editor_id: &str) -> Option<ScriptSpawnFn> {
        self.spawners.get(editor_id).copied()
    }

    /// Number of registered spawners. Surfaced for the
    /// engine-startup log + diagnostic console commands.
    pub fn len(&self) -> usize {
        self.spawners.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spawners.is_empty()
    }

    /// Iterate registered editor_ids. Order is hash-table-arbitrary;
    /// for stable display the consumer should sort. Used by the
    /// `script.registry` debug command + the startup log line.
    pub fn editor_ids(&self) -> impl Iterator<Item = &str> {
        self.spawners.keys().map(|s| s.as_str())
    }
}

impl Resource for ScriptRegistry {}

impl std::fmt::Debug for ScriptRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut names: Vec<&str> = self.editor_ids().collect();
        names.sort();
        f.debug_struct("ScriptRegistry")
            .field("count", &self.spawners.len())
            .field("editor_ids", &names)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::storage::EntityId;
    use byroredux_core::ecs::world::World;

    fn dummy_spawn(_world: &mut World, _entity: EntityId) {}
    fn other_spawn(_world: &mut World, _entity: EntityId) {}

    #[test]
    fn empty_registry_lookup_returns_none() {
        let r = ScriptRegistry::new();
        assert!(r.lookup("any").is_none());
        assert_eq!(r.len(), 0);
        assert!(r.is_empty());
    }

    #[test]
    fn register_then_lookup_returns_same_fn() {
        let mut r = ScriptRegistry::new();
        r.register("foo", dummy_spawn);
        let f = r.lookup("foo").expect("registered spawner missing");
        // Function-pointer equality is observable via fn-pointer-to-usize cast.
        assert_eq!(f as usize, dummy_spawn as usize);
    }

    #[test]
    fn register_is_case_sensitive() {
        let mut r = ScriptRegistry::new();
        r.register("DefaultRumbleOnActivate", dummy_spawn);
        // Exact match hits.
        assert!(r.lookup("DefaultRumbleOnActivate").is_some());
        // Different case misses — Papyrus / SCPT editor_ids are
        // case-sensitive by convention.
        assert!(r.lookup("defaultrumbleonactivate").is_none());
        assert!(r.lookup("DEFAULTRUMBLEONACTIVATE").is_none());
    }

    #[test]
    fn re_register_replaces_prior_spawner() {
        let mut r = ScriptRegistry::new();
        r.register("foo", dummy_spawn);
        r.register("foo", other_spawn);
        let f = r.lookup("foo").expect("re-registered spawner missing");
        assert_eq!(f as usize, other_spawn as usize);
        // Count stays at one — re-registration overwrites, doesn't append.
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn editor_ids_iterator_yields_every_registered_key() {
        let mut r = ScriptRegistry::new();
        r.register("foo", dummy_spawn);
        r.register("bar", dummy_spawn);
        r.register("baz", dummy_spawn);
        let mut ids: Vec<&str> = r.editor_ids().collect();
        ids.sort();
        assert_eq!(ids, vec!["bar", "baz", "foo"]);
    }
}
