//! `packages` WIT host interface.

use crate::runtime::*;

impl packages::Host for HostState {
    fn get(
        &mut self,
        entity: state::EntityRef,
    ) -> wasmtime::Result<Option<packages::PackageSnapshot>> {
        self.require_packages_read()?;
        let entity = sdk_entity_ref(entity)?;
        Ok(self
            .entity_projections
            .get(&entity)
            .and_then(EntityProjection::packages)
            .map(wit_package_snapshot))
    }

    fn queue_evaluate(&mut self, entity: state::EntityRef) -> wasmtime::Result<()> {
        if !self.accepting_commands {
            wasmtime::bail!("package reevaluation is only accepted during a callback");
        }
        if !self.grants.contains(PACKAGES_EVALUATE_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {PACKAGES_EVALUATE_CAPABILITY}",
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
            .and_then(EntityProjection::packages)
            .is_none()
        {
            wasmtime::bail!("package target is not visible or has no live package state");
        }
        self.pending_commands
            .push(HostCommand::EvaluatePackage(EvaluatePackageCommand::new(
                entity,
            )));
        Ok(())
    }
}
