//! DataStore: central registry of all loaded records from all plugins.
//!
//! After all plugins are loaded via [`add_plugin`](DataStore::add_plugin),
//! call [`resolve_conflicts`](DataStore::resolve_conflicts) to determine
//! winners for any records touched by multiple plugins. The resolution
//! is deterministic and requires no external sorting.

use byroredux_core::ecs::resource::Resource;
use byroredux_core::form_id::{FormIdPair, PluginId};
use std::collections::HashMap;

use crate::manifest::PluginManifest;
use crate::record::Record;
use crate::resolver::{ConflictResolution, DependencyResolver};

/// A record after conflict resolution — tracks which plugin's version
/// won and the full override chain.
pub struct ResolvedRecord {
    pub record: Record,
    /// Which plugin's version of this record was chosen.
    pub source: PluginId,
    /// Plugins whose versions were overridden (in resolution order).
    pub overridden_by: Vec<PluginId>,
}

/// A detected conflict: multiple plugins provide the same record.
pub struct Conflict {
    pub form_id: FormIdPair,
    pub plugins: Vec<PluginId>,
    pub resolution: ConflictResolution,
}

/// Central registry of all loaded records from all plugins.
///
/// Usage:
/// 1. Call [`add_plugin`](Self::add_plugin) for each plugin
/// 2. Call [`resolve_conflicts`](Self::resolve_conflicts) once all plugins are loaded
/// 3. Query records via [`get`](Self::get)
///
/// Register as a global [`Resource`] on the [`World`](byroredux_core::ecs::world::World).
pub struct DataStore {
    /// All candidate records, grouped by FormIdPair.
    /// Before resolution: may contain multiple entries per form ID.
    /// After resolution: exactly one entry per form ID.
    candidates: HashMap<FormIdPair, Vec<(PluginId, Record)>>,
    records: HashMap<FormIdPair, ResolvedRecord>,
    plugins: Vec<PluginManifest>,
    pub conflicts: Vec<Conflict>,
}

impl DataStore {
    pub fn new() -> Self {
        Self {
            candidates: HashMap::new(),
            records: HashMap::new(),
            plugins: Vec::new(),
            conflicts: Vec::new(),
        }
    }

    /// Register a plugin and its records.
    ///
    /// Records are staged as candidates — call [`resolve_conflicts`](Self::resolve_conflicts)
    /// after all plugins are loaded to determine winners.
    pub fn add_plugin(&mut self, manifest: PluginManifest, records: Vec<Record>) {
        let plugin_id = manifest.id;
        self.plugins.push(manifest);

        for record in records {
            self.candidates
                .entry(record.form_id)
                .or_default()
                .push((plugin_id, record));
        }
    }

    /// Run conflict resolution on all staged candidates.
    ///
    /// For each [`FormIdPair`] touched by multiple plugins, uses the
    /// dependency DAG to determine the winner:
    /// - If one plugin transitively depends on another → deeper wins
    /// - No dependency relationship → UUID tiebreak, flagged as conflict
    ///
    /// After this call, [`get`](Self::get) returns resolved records.
    pub fn resolve_conflicts(&mut self) {
        let resolver = DependencyResolver::new(&self.plugins);

        let candidates = std::mem::take(&mut self.candidates);

        for (form_id, mut entries) in candidates {
            if entries.len() == 1 {
                // No conflict — single provider.
                let (source, record) = entries.pop().unwrap();
                self.records.insert(
                    form_id,
                    ResolvedRecord {
                        record,
                        source,
                        overridden_by: Vec::new(),
                    },
                );
            } else {
                // Multiple providers — resolve.
                let plugin_ids: Vec<PluginId> =
                    entries.iter().map(|(pid, _)| *pid).collect();

                let (winner, resolution) = resolver.resolve_winner(&plugin_ids);

                // Extract the winning record, collect losers.
                let winner_idx = entries
                    .iter()
                    .position(|(pid, _)| *pid == winner)
                    .expect("winner must be in entries");
                let (source, record) = entries.swap_remove(winner_idx);
                let overridden_by: Vec<PluginId> =
                    entries.iter().map(|(pid, _)| *pid).collect();

                self.conflicts.push(Conflict {
                    form_id,
                    plugins: plugin_ids,
                    resolution: resolution.clone(),
                });

                self.records.insert(
                    form_id,
                    ResolvedRecord {
                        record,
                        source,
                        overridden_by,
                    },
                );
            }
        }
    }

    /// Look up a resolved record by its stable identity.
    ///
    /// Returns `None` if the form ID was never loaded or
    /// [`resolve_conflicts`](Self::resolve_conflicts) hasn't been called yet.
    pub fn get(&self, form_id: &FormIdPair) -> Option<&ResolvedRecord> {
        self.records.get(form_id)
    }

    /// All loaded plugin manifests, in insertion order.
    pub fn plugins(&self) -> &[PluginManifest] {
        &self.plugins
    }
}

impl Default for DataStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Resource for DataStore {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::RecordType;
    use byroredux_core::ecs::components::Transform;
    use byroredux_core::ecs::sparse_set::SparseSetStorage;
    use byroredux_core::ecs::storage::Component;
    use byroredux_core::ecs::world::World;
    use byroredux_core::form_id::{FormIdPool, LocalFormId};
    use byroredux_core::math::Vec3;

    fn plugin_manifest(name: &str, deps: &[&str]) -> PluginManifest {
        PluginManifest {
            id: PluginId::from_filename(name),
            name: name.to_string(),
            version: semver::Version::new(1, 0, 0),
            dependencies: deps.iter().map(|d| PluginId::from_filename(d)).collect(),
        }
    }

    fn make_pair(plugin: &str, local: u32) -> FormIdPair {
        FormIdPair {
            plugin: PluginId::from_filename(plugin),
            local: LocalFormId(local),
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Damage(pub f32);
    impl Component for Damage {
        type Storage = SparseSetStorage<Self>;
    }

    #[test]
    fn single_plugin_no_conflicts() {
        let mut store = DataStore::new();

        let manifest = plugin_manifest("Base.esm", &[]);
        let pair = make_pair("Base.esm", 0x001);
        let mut record = Record::new(pair, RecordType::WEAP);
        record.add_component(Damage(25.0));

        store.add_plugin(manifest, vec![record]);
        store.resolve_conflicts();

        assert!(store.conflicts.is_empty());
        let resolved = store.get(&pair).unwrap();
        assert_eq!(resolved.source, PluginId::from_filename("Base.esm"));
        assert!(resolved.overridden_by.is_empty());
    }

    #[test]
    fn depth_resolved_conflict() {
        // B depends on A, both provide record 0x001 → B wins
        let mut store = DataStore::new();

        let pair = make_pair("A.esm", 0x001);

        let manifest_a = plugin_manifest("A.esm", &[]);
        let mut rec_a = Record::new(pair, RecordType::WEAP);
        rec_a.add_component(Damage(10.0));

        let manifest_b = plugin_manifest("B.esm", &["A.esm"]);
        let mut rec_b = Record::new(pair, RecordType::WEAP);
        rec_b.add_component(Damage(99.0));

        store.add_plugin(manifest_a, vec![rec_a]);
        store.add_plugin(manifest_b, vec![rec_b]);
        store.resolve_conflicts();

        assert_eq!(store.conflicts.len(), 1);
        let resolved = store.get(&pair).unwrap();
        assert_eq!(resolved.source, PluginId::from_filename("B.esm"));
        assert_eq!(resolved.overridden_by.len(), 1);
        assert_eq!(resolved.overridden_by[0], PluginId::from_filename("A.esm"));

        assert!(matches!(
            store.conflicts[0].resolution,
            ConflictResolution::DepthResolved { .. }
        ));
    }

    #[test]
    fn tiebreak_conflict() {
        // A and B are independent, both provide record 0x001 → tiebreak
        let mut store = DataStore::new();

        let pair = make_pair("A.esm", 0x001);

        let manifest_a = plugin_manifest("A.esm", &[]);
        let rec_a = Record::new(pair, RecordType::STAT);

        let manifest_b = plugin_manifest("B.esm", &[]);
        let rec_b = Record::new(pair, RecordType::STAT);

        store.add_plugin(manifest_a, vec![rec_a]);
        store.add_plugin(manifest_b, vec![rec_b]);
        store.resolve_conflicts();

        assert_eq!(store.conflicts.len(), 1);
        assert!(matches!(
            store.conflicts[0].resolution,
            ConflictResolution::TieBreak { .. }
        ));

        // Exactly one resolved record exists
        let resolved = store.get(&pair).unwrap();
        assert_eq!(resolved.overridden_by.len(), 1);
    }

    #[test]
    fn resolved_record_spawns_into_world() {
        let mut store = DataStore::new();

        let pair = make_pair("Base.esm", 0x100);
        let manifest = plugin_manifest("Base.esm", &[]);
        let mut record = Record::new(pair, RecordType::NPC_);
        record.add_component(Transform::from_translation(Vec3::new(10.0, 20.0, 30.0)));
        record.add_component(Damage(75.0));

        store.add_plugin(manifest, vec![record]);
        store.resolve_conflicts();

        let mut world = World::new();
        world.insert_resource(FormIdPool::new());

        let resolved = store.get(&pair).unwrap();
        let entity = resolved.record.spawn(&mut world);

        // Components present
        let t = world.get::<Transform>(entity).unwrap();
        assert_eq!(t.translation, Vec3::new(10.0, 20.0, 30.0));
        let d = world.get::<Damage>(entity).unwrap();
        assert_eq!(d.0, 75.0);

        // Findable by form ID
        let pool = world.resource::<FormIdPool>();
        let fid = pool.get(&pair).unwrap();
        drop(pool);
        assert_eq!(world.find_by_form_id(fid), Some(entity));
    }

    #[test]
    fn datastore_as_world_resource() {
        let mut world = World::new();
        world.insert_resource(DataStore::new());

        let store = world.resource::<DataStore>();
        assert!(store.plugins().is_empty());
        assert!(store.conflicts.is_empty());
    }

    #[test]
    fn plugins_returns_all_manifests() {
        let mut store = DataStore::new();
        store.add_plugin(plugin_manifest("A.esm", &[]), vec![]);
        store.add_plugin(plugin_manifest("B.esm", &["A.esm"]), vec![]);
        store.add_plugin(plugin_manifest("C.esm", &[]), vec![]);

        assert_eq!(store.plugins().len(), 3);
        assert_eq!(store.plugins()[0].name, "A.esm");
        assert_eq!(store.plugins()[1].name, "B.esm");
        assert_eq!(store.plugins()[2].name, "C.esm");
    }
}
