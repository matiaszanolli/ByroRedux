//! `perks` WIT host interface.

use crate::runtime::*;

impl perks::Host for HostState {
    fn get(&mut self, entity: state::EntityRef) -> wasmtime::Result<Option<perks::PerkSnapshot>> {
        self.require_perks_read()?;
        let entity = sdk_entity_ref(entity)?;
        Ok(self
            .entity_projections
            .get(&entity)
            .and_then(EntityProjection::perks)
            .map(wit_perk_snapshot))
    }
}
