//! The crate's trust boundary: every `require_*` capability guard.
//!
//! These were split across two `impl HostState` blocks 1400 lines apart
//! (#3853), which made the enforcement surface impossible to review as a
//! set. They are the whole reason `crates/mod-runtime` is named a trust
//! boundary in `_audit-common.md`, so they get their own module.

use super::*;

impl HostState {
    pub(crate) fn require_legacy_builder_access(&self) -> wasmtime::Result<()> {
        if !self.accepting_commands {
            wasmtime::bail!("mod-event builders are only accessible during an event callback");
        }
        if !self.grants.contains(EVENTS_PUBLISH_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {EVENTS_PUBLISH_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    pub(crate) fn require_legacy_subscription_command(&mut self) -> wasmtime::Result<()> {
        if !self.accepting_commands {
            wasmtime::bail!("event subscriptions are only accepted during an event callback");
        }
        if !self.grants.contains(EVENTS_SUBSCRIBE_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {EVENTS_SUBSCRIBE_CAPABILITY}",
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
        Ok(())
    }
}

impl HostState {
    pub(crate) fn require_actor_values_read(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(ACTOR_VALUES_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {ACTOR_VALUES_READ_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    pub(crate) fn require_inventory_read(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(INVENTORY_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {INVENTORY_READ_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    pub(crate) fn require_factions_read(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(FACTIONS_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {FACTIONS_READ_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    pub(crate) fn require_faction_relationships_read(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(FACTION_RELATIONSHIPS_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {FACTION_RELATIONSHIPS_READ_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    pub(crate) fn require_perks_read(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(PERKS_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {PERKS_READ_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    pub(crate) fn require_packages_read(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(PACKAGES_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {PACKAGES_READ_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    pub(crate) fn require_animation_read(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(ANIMATION_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {ANIMATION_READ_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    pub(crate) fn require_reputation_read(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(REPUTATION_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {REPUTATION_READ_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    pub(crate) fn require_world_spatial_read(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(WORLD_SPATIAL_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {WORLD_SPATIAL_READ_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    pub(crate) fn require_console_context(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(CONSOLE_REGISTER_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {CONSOLE_REGISTER_CAPABILITY}",
                self.principal.id()
            );
        }
        if self.current_console_args.is_none() {
            wasmtime::bail!("console host calls are only available during a console callback");
        }
        Ok(())
    }

    pub(crate) fn require_script_function_context(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(SCRIPT_FUNCTIONS_REGISTER_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {SCRIPT_FUNCTIONS_REGISTER_CAPABILITY}",
                self.principal.id()
            );
        }
        if self.current_script_arguments.is_none() {
            wasmtime::bail!(
                "script function host calls are only available during a script function callback"
            );
        }
        Ok(())
    }

    pub(crate) fn script_argument(&self, index: u32) -> Option<&ScriptValue> {
        self.current_script_arguments
            .as_ref()
            .and_then(|arguments| arguments.get(index as usize))
    }

    pub(crate) fn set_script_result(&mut self, value: ScriptValue) -> wasmtime::Result<()> {
        self.require_script_function_context()?;
        if self.current_script_result.is_some() {
            wasmtime::bail!("script function result was already set");
        }
        self.current_script_result = Some(value);
        Ok(())
    }

    pub(crate) fn require_world_entity_read(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(WORLD_ENTITY_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {WORLD_ENTITY_READ_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    pub(crate) fn require_content_catalog_read(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(CONTENT_CATALOG_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {CONTENT_CATALOG_READ_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    pub(crate) fn require_storage(&self, capability: &str) -> wasmtime::Result<()> {
        if self.principal_storage_schema.is_none() {
            wasmtime::bail!(
                "principal {} did not declare persistent storage",
                self.principal.id()
            );
        }
        if !self.grants.contains(capability) {
            wasmtime::bail!(
                "principal {} lacks capability {capability}",
                self.principal.id()
            );
        }
        Ok(())
    }

    pub(crate) fn require_legacy_container_read(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(STORAGE_READ_OWN_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {STORAGE_READ_OWN_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    pub(crate) fn require_legacy_container_write(&self) -> wasmtime::Result<()> {
        if !self.grants.contains(STORAGE_WRITE_OWN_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {STORAGE_WRITE_OWN_CAPABILITY}",
                self.principal.id()
            );
        }
        Ok(())
    }

    pub(crate) fn require_storage_write(&mut self) -> wasmtime::Result<()> {
        if !self.accepting_commands {
            wasmtime::bail!("storage commands are only accepted during an event callback");
        }
        self.require_storage(STORAGE_WRITE_OWN_CAPABILITY)?;
        if self.pending_commands.len() >= self.max_commands_per_entry {
            self.command_budget_exhausted = true;
            wasmtime::bail!(
                "command limit of {} exceeded in one entry",
                self.max_commands_per_entry
            );
        }
        Ok(())
    }
}
