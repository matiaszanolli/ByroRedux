//! ECS runtime for Skyrim+ `SCEN` records.
//!
//! Parsed scene records are immutable definitions held by [`SceneRegistry`].
//! Each definition owns one persistent [`ScenePlayer`] entity containing only
//! playback state.  Actions communicate with their eventual dialogue/package
//! executors through transient [`SceneEventBatch`] components, while compiled
//! Papyrus bindings are surfaced as [`SceneFragmentInvocationBatch`] events.

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::ecs::components::{FactionRanks, Inventory};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::world::World;
use byroredux_plugin::esm::records::ScenRecord;

use crate::quest_stages::QuestFormId;

/// Immutable scene definitions and the ECS entity assigned to each one.
#[derive(Debug, Clone, Default)]
pub struct SceneRegistry {
    definitions: HashMap<u32, Arc<ScenRecord>>,
    entities: HashMap<u32, EntityId>,
}

impl Resource for SceneRegistry {}

impl SceneRegistry {
    /// Build a definition-only registry, useful for tools and conformance
    /// probes that do not own an ECS [`World`].
    pub fn from_records(records: impl IntoIterator<Item = ScenRecord>) -> Self {
        let mut registry = Self::default();
        for record in records {
            registry
                .definitions
                .insert(record.form_id, Arc::new(record));
        }
        registry
    }

    pub fn definition(&self, form_id: u32) -> Option<&ScenRecord> {
        self.definitions.get(&form_id).map(Arc::as_ref)
    }

    pub fn scene_entity(&self, form_id: u32) -> Option<EntityId> {
        self.entities.get(&form_id).copied()
    }

    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    fn definition_arc(&self, form_id: u32) -> Option<Arc<ScenRecord>> {
        self.definitions.get(&form_id).cloned()
    }
}

/// Runtime resolution of `(owning quest, alias id)` to a live reference.
///
/// The historical name is retained because SCEN actor slots were the first
/// consumer. The table is now the canonical quest-alias surface used by
/// conditions, fragment object resolution, packages, dialogue, and scenes.
#[derive(Debug, Clone, Default)]
pub struct SceneActorBindings {
    actors: HashMap<(QuestFormId, i32), EntityId>,
    dirty: bool,
}

impl Resource for SceneActorBindings {}

impl SceneActorBindings {
    pub fn bind(&mut self, quest: QuestFormId, alias_id: i32, entity: EntityId) {
        self.actors.insert((quest, alias_id), entity);
    }

    pub fn unbind(&mut self, quest: QuestFormId, alias_id: i32) -> Option<EntityId> {
        self.actors.remove(&(quest, alias_id))
    }

    pub fn resolve(&self, quest: QuestFormId, alias_id: i32) -> Option<EntityId> {
        self.actors.get(&(quest, alias_id)).copied()
    }

    /// Whether cell/lifecycle changes have invalidated the cached table and
    /// the scheduled alias refresh has not run yet.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

// Split into topic submodules (#2408 / TD1-005); glob-re-exported so
// every `crate::scene::*` path — including `lib.rs`'s public surface —
// is unchanged.
mod playback;
mod quest_alias;

pub use playback::*;
pub use quest_alias::*;
// Crate-private; `register` seeds it, `install_scene_quest_aliases` fills it.
use quest_alias::SceneQuestAliasRegistry;

/// Register scene components/resources without replacing an already-populated
/// registry. Called by the scripting subsystem's top-level `register` hook.
pub fn register(world: &mut World) {
    world.register::<FactionRanks>();
    world.register::<Inventory>();
    world.register::<ScenePlayer>();
    world.register::<SceneAliasCandidate>();
    world.register::<RemoteSceneActorStub>();
    world.register::<QuestAliasInjectedOverlays>();
    world.register::<QuestAliasRuntimeOverlays>();
    world.register::<SceneStartRequest>();
    world.register::<SceneStopRequest>();
    world.register::<SceneActionCompletionBatch>();
    world.register::<SceneEventBatch>();
    world.register::<SceneFragmentInvocationBatch>();
    if world.try_resource::<SceneRegistry>().is_none() {
        world.insert_resource(SceneRegistry::default());
    }
    if world.try_resource::<SceneActorBindings>().is_none() {
        world.insert_resource(SceneActorBindings::default());
    }
    if world.try_resource::<SceneQuestAliasRegistry>().is_none() {
        world.insert_resource(SceneQuestAliasRegistry::default());
    }
    if world.try_resource::<QuestAliasInjectionState>().is_none() {
        world.insert_resource(QuestAliasInjectionState::default());
    }
}

/// Install parsed definitions and create one persistent scene-player entity
/// per new FormID. Re-installing a load order refreshes definitions in place
/// without resetting live playback state or creating duplicate entities.
pub fn install_scene_records(
    world: &mut World,
    records: impl IntoIterator<Item = ScenRecord>,
) -> usize {
    if world.try_resource::<SceneRegistry>().is_none() {
        register(world);
    }

    let records: Vec<ScenRecord> = records.into_iter().collect();
    let existing = {
        let registry = world.resource::<SceneRegistry>();
        registry.entities.clone()
    };

    let mut installed = Vec::with_capacity(records.len());
    for record in records {
        let form_id = record.form_id;
        let entity = existing.get(&form_id).copied().unwrap_or_else(|| {
            let entity = world.spawn();
            world.insert(entity, ScenePlayer::new(form_id));
            entity
        });
        installed.push((form_id, entity, record));
    }

    let count = installed.len();
    let mut registry = world.resource_mut::<SceneRegistry>();
    for (form_id, entity, record) in installed {
        registry.definitions.insert(form_id, Arc::new(record));
        registry.entities.insert(form_id, entity);
    }
    count
}

/// Notify the alias resolver that the live candidate set changed.
pub fn mark_scene_actor_bindings_dirty(world: &World) {
    if let Some(mut bindings) = world.try_resource_mut::<SceneActorBindings>() {
        bindings.dirty = true;
    }
}

// Tests live in sibling files by topic (#2408 / TD1-005).
#[cfg(test)]
mod playback_tests;
#[cfg(test)]
mod quest_alias_tests;
