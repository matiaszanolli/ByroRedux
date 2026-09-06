//! `state` WIT host interface.

use crate::runtime::*;

impl state::Host for HostState {
    fn queue_increment_own_i64(
        &mut self,
        entity: state::EntityRef,
        schema_index: u32,
        field_index: u32,
        delta: i64,
    ) -> wasmtime::Result<()> {
        if !self.accepting_commands {
            wasmtime::bail!("state commands are only accepted during an event callback");
        }
        if !self.grants.contains(COMPONENTS_WRITE_OWN_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {COMPONENTS_WRITE_OWN_CAPABILITY}",
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
        let schema = self
            .schemas
            .get(schema_index as usize)
            .ok_or_else(|| wasmtime::Error::msg(format!("unknown schema index {schema_index}")))?;
        let field = schema
            .fields
            .get(field_index as usize)
            .ok_or_else(|| wasmtime::Error::msg(format!("unknown field index {field_index}")))?;
        if field.value_type != ExtensionValueType::I64 {
            wasmtime::bail!("field {} in schema {} is not an i64", field.id, schema.id);
        }
        let entity =
            byroredux_sdk::identity::EntityRef::new(entity.world_generation, entity.object)
                .ok_or_else(|| {
                    wasmtime::Error::msg("entity reference contains a reserved zero value")
                })?;
        self.pending_commands
            .push(HostCommand::Component(ExtensionCommand::IncrementI64 {
                entity,
                schema: schema.id.clone(),
                field: field.id.clone(),
                delta,
            }));
        Ok(())
    }
}
