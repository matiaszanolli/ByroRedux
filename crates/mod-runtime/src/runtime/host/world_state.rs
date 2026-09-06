//! `world_state` WIT host interface.

use crate::runtime::*;

impl world_state::Host for HostState {
    fn contains_entity(&mut self, entity: state::EntityRef) -> wasmtime::Result<bool> {
        self.require_world_entity_read()?;
        let entity = sdk_entity_ref(entity)?;
        Ok(self.entity_projections.contains_key(&entity))
    }

    fn get_entity(
        &mut self,
        entity: state::EntityRef,
    ) -> wasmtime::Result<Option<world_state::EntityProjection>> {
        self.require_world_entity_read()?;
        let entity = sdk_entity_ref(entity)?;
        let Some(projection) = self.entity_projections.get(&entity) else {
            return Ok(None);
        };
        Ok(Some(wit_entity_projection(
            projection,
            self.grants.contains(WORLD_TRANSFORM_READ_CAPABILITY),
        )))
    }
}
