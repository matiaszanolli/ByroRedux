//! `faction_relationships` WIT host interface.

use crate::runtime::*;

impl faction_relationships::Host for HostState {
    fn get(
        &mut self,
        source: state::FormRef,
        target: state::FormRef,
    ) -> wasmtime::Result<Option<faction_relationships::FactionRelationship>> {
        self.require_faction_relationships_read()?;
        let source = sdk_form_ref(source);
        let target = sdk_form_ref(target);
        Ok(self
            .faction_relationships
            .relationship(source, target)
            .map(|relationship| faction_relationships::FactionRelationship {
                modifier: relationship.modifier(),
                combat_reaction: relationship.combat_reaction_raw(),
            }))
    }

    fn truncated(&mut self) -> wasmtime::Result<bool> {
        self.require_faction_relationships_read()?;
        Ok(self.faction_relationships.truncated())
    }
}
