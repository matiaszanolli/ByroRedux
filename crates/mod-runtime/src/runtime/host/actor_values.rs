//! `actor_values` WIT host interface.

use crate::runtime::*;

impl actor_values::Host for HostState {
    fn get(
        &mut self,
        entity: state::EntityRef,
        actor_value: state::FormRef,
    ) -> wasmtime::Result<Option<actor_values::ActorValueState>> {
        self.require_actor_values_read()?;
        let entity = sdk_entity_ref(entity)?;
        let actor_value = sdk_form_ref(actor_value);
        let Some(projection) = self.entity_projections.get(&entity) else {
            return Ok(None);
        };
        let Some(values) = projection.actor_values() else {
            return Ok(None);
        };
        if self
            .content_catalog
            .record(actor_value)
            .is_none_or(|record| record.record_type() != *b"AVIF")
        {
            return Ok(None);
        }
        let value = values.get(&actor_value).copied().unwrap_or_default();
        Ok(Some(wit_actor_value_state(value)))
    }

    fn queue(
        &mut self,
        entity: state::EntityRef,
        actor_value: state::FormRef,
        operation: actor_values::Operation,
        value: f32,
    ) -> wasmtime::Result<()> {
        if !self.accepting_commands {
            wasmtime::bail!("actor-value writes are only accepted during an event callback");
        }
        if !self.grants.contains(ACTOR_VALUES_WRITE_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {ACTOR_VALUES_WRITE_CAPABILITY}",
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
        let actor_value = sdk_form_ref(actor_value);
        let Some(projection) = self.entity_projections.get(&entity) else {
            wasmtime::bail!("actor-value target is not visible in this callback");
        };
        if projection.actor_values().is_none() {
            wasmtime::bail!("actor-value target does not carry canonical actor values");
        }
        if self
            .content_catalog
            .record(actor_value)
            .is_none_or(|record| record.record_type() != *b"AVIF")
        {
            wasmtime::bail!("actor-value identity does not resolve to an AVIF record");
        }
        let operation = match operation {
            actor_values::Operation::SetBase => ActorValueOperation::SetBase,
            actor_values::Operation::ModifyPermanent => ActorValueOperation::ModifyPermanent,
            actor_values::Operation::ModifyTemporary => ActorValueOperation::ModifyTemporary,
            actor_values::Operation::Damage => ActorValueOperation::Damage,
            actor_values::Operation::Restore => ActorValueOperation::Restore,
        };
        let command = ActorValueCommand::new(entity, actor_value, operation, value)
            .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
        self.pending_commands.push(HostCommand::ActorValue(command));
        Ok(())
    }
}
