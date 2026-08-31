//! Runtime metadata needed by Papyrus equipment effects.
//!
//! `Actor.EquipItem` names a base item FormID, while the canonical ECS
//! equipment store needs that armor's biped-slot mask. Plugin parsing already
//! owns this mapping; this small read-only resource carries only the masks the
//! scripting runtime needs instead of retaining the entire `EsmIndex`.

use std::collections::HashMap;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;

use crate::events::{EquipmentChange, EquipmentEventBatch};

#[derive(Debug, Clone, Default)]
pub struct EquipItemCatalog {
    biped_slot_masks: HashMap<u32, u32>,
}

impl EquipItemCatalog {
    pub fn slot_mask(&self, form_id: u32) -> Option<u32> {
        self.biped_slot_masks.get(&form_id).copied()
    }

    pub fn len(&self) -> usize {
        self.biped_slot_masks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.biped_slot_masks.is_empty()
    }
}

impl Resource for EquipItemCatalog {}

/// Replace the live item-to-biped-mask lookup with metadata from the current
/// plugin load order. Callers filter the full item index to armor records.
pub fn install_equip_item_catalog(
    world: &mut World,
    items: impl IntoIterator<Item = (u32, u32)>,
) -> usize {
    let catalog = EquipItemCatalog {
        biped_slot_masks: items.into_iter().filter(|(_, mask)| *mask != 0).collect(),
    };
    let count = catalog.len();
    world.insert_resource(catalog);
    count
}

/// Append ordered equipment transitions to the wearer's one-frame batch.
pub fn emit_equipment_changes(
    world: &World,
    wearer: byroredux_core::ecs::EntityId,
    changes: impl IntoIterator<Item = EquipmentChange>,
) {
    let changes = changes.into_iter().collect::<Vec<_>>();
    if changes.is_empty() {
        return;
    }
    let Some(mut events) = world.query_mut::<EquipmentEventBatch>() else {
        return;
    };
    if let Some(batch) = events.get_mut(wearer) {
        batch.0.extend(changes);
    } else {
        events.insert(wearer, EquipmentEventBatch(changes));
    }
}

pub(crate) fn register(world: &mut World) {
    if world.try_resource::<EquipItemCatalog>().is_none() {
        world.insert_resource(EquipItemCatalog::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_replaces_catalog_and_drops_zero_masks() {
        let mut world = World::new();
        register(&mut world);
        let count = install_equip_item_catalog(
            &mut world,
            [(0x1234, 1 << 12), (0x5678, 0), (0x9abc, 1 << 15)],
        );

        assert_eq!(count, 2);
        let catalog = world.resource::<EquipItemCatalog>();
        assert_eq!(catalog.slot_mask(0x1234), Some(1 << 12));
        assert_eq!(catalog.slot_mask(0x5678), None);
    }
}
