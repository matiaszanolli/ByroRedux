//! `inventory` WIT host interface.

use crate::runtime::*;

impl inventory::Host for HostState {
    fn get(
        &mut self,
        entity: state::EntityRef,
    ) -> wasmtime::Result<Option<inventory::InventorySnapshot>> {
        self.require_inventory_read()?;
        let entity = sdk_entity_ref(entity)?;
        Ok(self
            .entity_projections
            .get(&entity)
            .and_then(EntityProjection::inventory)
            .map(wit_inventory_snapshot))
    }
}
