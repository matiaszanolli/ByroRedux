//! IDLE records by editor ID (M47.3).
//!
//! ObScript's `PlayIdle` takes "the actual editorID string of the idle as parameter, not the
//! converted formID" (GECK): the compiled argument is a length-prefixed string. The VM asks the host
//! which form the name stands for, and this resource answers it from the load order's IDLE records.

use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::records::EsmIndex;

/// Lowercased IDLE editor ID → its FormID. `NOT_SAVED_BY_DESIGN`: rebuilt from the load order.
#[derive(Debug, Default)]
pub struct IdleNames(crate::name_table::EditorIdTable<u32>);

impl Resource for IdleNames {}

impl IdleNames {
    pub fn from_index(index: &EsmIndex) -> Self {
        Self(crate::name_table::EditorIdTable::from_records(
            index.idle_animations.values().map(|idle| (idle.editor_id.as_str(), idle.form_id)),
        ))
    }

    pub fn resolve(&self, name: &str) -> Option<u32> {
        self.0.get(name).copied()
    }
}

/// The IDLE record an editor ID names, if the load order has one.
pub fn idle_by_editor_id(world: &World, name: &str) -> Option<u32> {
    world.try_resource::<IdleNames>()?.resolve(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::IdleRecord;

    #[test]
    fn an_editor_id_names_its_idle_in_any_case() {
        let mut index = EsmIndex::default();
        index.idle_animations.insert(0xC1991, IdleRecord { form_id: 0xC1991, editor_id: "LooseDogGrowl".into(), ..Default::default() });
        let mut world = World::new();
        world.insert_resource(IdleNames::from_index(&index));
        assert_eq!(idle_by_editor_id(&world, "loosedoggrowl"), Some(0xC1991));
        assert_eq!(idle_by_editor_id(&world, "Nope"), None);
    }
}
