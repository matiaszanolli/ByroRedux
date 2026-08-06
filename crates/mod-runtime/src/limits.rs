use crate::{Result, SandboxError};

/// Effective ceilings applied independently to every component instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SandboxConfig {
    pub max_component_bytes: usize,
    pub max_memory_bytes: usize,
    pub max_table_elements: usize,
    pub max_instances: usize,
    pub max_tables: usize,
    pub max_memories: usize,
    pub max_wasm_stack_bytes: usize,
    pub fuel_per_entry: u64,
    pub max_log_entries: usize,
    pub max_log_message_bytes: usize,
    pub max_log_bytes: usize,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_component_bytes: 16 * 1024 * 1024,
            max_memory_bytes: 64 * 1024 * 1024,
            max_table_elements: 16_384,
            max_instances: 64,
            max_tables: 32,
            max_memories: 16,
            max_wasm_stack_bytes: 512 * 1024,
            fuel_per_entry: 10_000_000,
            max_log_entries: 1_024,
            max_log_message_bytes: 16 * 1024,
            max_log_bytes: 1024 * 1024,
        }
    }
}

impl SandboxConfig {
    pub fn validate(&self) -> Result<()> {
        if self.max_component_bytes == 0 {
            return Err(SandboxError::InvalidConfig(
                "max_component_bytes must be non-zero",
            ));
        }
        if self.max_memory_bytes < 64 * 1024 {
            return Err(SandboxError::InvalidConfig(
                "max_memory_bytes must allow at least one WebAssembly page",
            ));
        }
        if self.max_table_elements == 0 {
            return Err(SandboxError::InvalidConfig(
                "max_table_elements must be non-zero",
            ));
        }
        if self.max_instances == 0 || self.max_tables == 0 || self.max_memories == 0 {
            return Err(SandboxError::InvalidConfig(
                "instance, table, and memory limits must be non-zero",
            ));
        }
        if self.max_wasm_stack_bytes == 0 {
            return Err(SandboxError::InvalidConfig(
                "max_wasm_stack_bytes must be non-zero",
            ));
        }
        if self.fuel_per_entry == 0 {
            return Err(SandboxError::InvalidConfig(
                "fuel_per_entry must be non-zero",
            ));
        }
        if self.max_log_entries == 0 || self.max_log_message_bytes == 0 || self.max_log_bytes == 0 {
            return Err(SandboxError::InvalidConfig("log limits must be non-zero"));
        }
        if self.max_log_message_bytes > self.max_log_bytes {
            return Err(SandboxError::InvalidConfig(
                "max_log_message_bytes must not exceed max_log_bytes",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        SandboxConfig::default().validate().unwrap();
    }

    #[test]
    fn zero_fuel_is_rejected() {
        let config = SandboxConfig {
            fuel_per_entry: 0,
            ..SandboxConfig::default()
        };
        assert!(matches!(
            config.validate(),
            Err(SandboxError::InvalidConfig(_))
        ));
    }
}
