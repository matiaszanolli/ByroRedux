//! `reputation` WIT host interface.

use crate::runtime::*;

impl reputation::Host for HostState {
    fn get(
        &mut self,
        entity: state::EntityRef,
    ) -> wasmtime::Result<Option<reputation::ReputationSnapshot>> {
        self.require_reputation_read()?;
        let entity = sdk_entity_ref(entity)?;
        Ok(self
            .entity_projections
            .get(&entity)
            .and_then(EntityProjection::reputation)
            .map(wit_reputation_snapshot))
    }

    fn queue(
        &mut self,
        entity: state::EntityRef,
        reputation: state::FormRef,
        operation: reputation::Operation,
        points: u16,
    ) -> wasmtime::Result<()> {
        if !self.accepting_commands {
            wasmtime::bail!("reputation mutations are only accepted during a callback");
        }
        if !self.grants.contains(REPUTATION_WRITE_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {REPUTATION_WRITE_CAPABILITY}",
                self.principal.id()
            );
        }
        if self.pending_commands.len() >= self.max_commands_per_entry {
            self.command_budget_exhausted = true;
            wasmtime::bail!(
                "command limit of {} exceeded in one entry",
                self.max_commands_per_entry
            );
        }
        let entity = sdk_entity_ref(entity)?;
        if self
            .entity_projections
            .get(&entity)
            .and_then(EntityProjection::reputation)
            .is_none()
        {
            wasmtime::bail!("reputation target is not visible or has no reputation state");
        }
        let operation = match operation {
            reputation::Operation::AddFame => ReputationOperation::AddFame,
            reputation::Operation::AddInfamy => ReputationOperation::AddInfamy,
            reputation::Operation::Reset => ReputationOperation::Reset,
        };
        let command = ReputationCommand::new(entity, sdk_form_ref(reputation), operation, points)
            .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
        self.pending_commands.push(HostCommand::Reputation(command));
        Ok(())
    }
}
