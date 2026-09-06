//! `context` WIT host interface.

use crate::runtime::*;

impl context::Host for HostState {
    fn principal_id(&mut self) -> wasmtime::Result<String> {
        Ok(self.principal.id().as_str().to_owned())
    }

    fn has_capability(&mut self, capability: String) -> wasmtime::Result<bool> {
        Ok(self.grants.contains(&capability))
    }

    fn sdk_version(&mut self) -> wasmtime::Result<String> {
        Ok(self.catalog.sdk_version().to_string())
    }

    fn service_version(&mut self, service: String) -> wasmtime::Result<Option<String>> {
        Ok(self
            .catalog
            .service_version(&service)
            .map(ToString::to_string))
    }

    fn engine_setting(&mut self, key: String) -> wasmtime::Result<Option<context::SettingValue>> {
        if !self.grants.contains(SETTINGS_READ_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {SETTINGS_READ_CAPABILITY}",
                self.principal.id()
            );
        }
        if key.is_empty() || key.len() > MAX_SETTING_KEY_BYTES || key.chars().any(char::is_control)
        {
            wasmtime::bail!("invalid engine setting key");
        }
        Ok(self.engine_settings.get(&key).map(|value| match value {
            SettingValue::Boolean(value) => context::SettingValue::Boolean(*value),
            SettingValue::Number(value) => context::SettingValue::Number(*value),
            SettingValue::Choice(value) => context::SettingValue::Choice(value.clone()),
        }))
    }

    fn queue_own_setting(
        &mut self,
        declaration_index: u32,
        value: context::SettingValue,
    ) -> wasmtime::Result<()> {
        if !self.accepting_commands {
            wasmtime::bail!("setting writes are only accepted during an event callback");
        }
        if !self.grants.contains(SETTINGS_WRITE_OWN_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {SETTINGS_WRITE_OWN_CAPABILITY}",
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
        let declaration = self
            .setting_declarations
            .get(declaration_index as usize)
            .ok_or_else(|| wasmtime::Error::msg("unknown setting declaration index"))?;
        let value = match value {
            context::SettingValue::Boolean(value) => SettingValue::Boolean(value),
            context::SettingValue::Number(value) => SettingValue::Number(value),
            context::SettingValue::Choice(value) => SettingValue::Choice(value),
        };
        if !declaration.accepts(&value) {
            wasmtime::bail!("setting value does not satisfy its declared control");
        }
        self.pending_commands
            .push(HostCommand::Setting(SettingWriteCommand {
                key: declaration.qualified_name(
                    &ExtensionId::new(self.principal.id().as_str())
                        .expect("extension principals retain a valid extension identity"),
                ),
                value,
            }));
        Ok(())
    }
}
