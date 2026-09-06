//! `factions` WIT host interface.

use crate::runtime::*;

impl factions::Host for HostState {
    fn get(
        &mut self,
        entity: state::EntityRef,
    ) -> wasmtime::Result<Option<factions::FactionSnapshot>> {
        self.require_factions_read()?;
        let entity = sdk_entity_ref(entity)?;
        Ok(self
            .entity_projections
            .get(&entity)
            .and_then(EntityProjection::factions)
            .map(wit_faction_snapshot))
    }
}
