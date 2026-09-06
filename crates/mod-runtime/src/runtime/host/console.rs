//! `console` WIT host interface.

use crate::runtime::*;

impl console::Host for HostState {
    fn args_len(&mut self) -> wasmtime::Result<u32> {
        self.require_console_context()?;
        Ok(
            u32::try_from(self.current_console_args.as_ref().map_or(0, Vec::len))
                .expect("console arguments are bounded below u32::MAX"),
        )
    }

    fn args_byte(&mut self, index: u32) -> wasmtime::Result<Option<u8>> {
        self.require_console_context()?;
        Ok(self
            .current_console_args
            .as_ref()
            .and_then(|args| args.get(index as usize).copied()))
    }

    fn write_line(&mut self, message: String) -> wasmtime::Result<()> {
        self.require_console_context()?;
        let next_bytes = self
            .console_output_bytes
            .checked_add(message.len())
            .ok_or_else(|| wasmtime::Error::msg("console output byte count overflow"))?;
        if message.len() > MAX_CONSOLE_OUTPUT_LINE_BYTES
            || message.chars().any(char::is_control)
            || self.console_output.len() >= MAX_CONSOLE_OUTPUT_LINES
            || next_bytes > MAX_CONSOLE_OUTPUT_BYTES
        {
            self.console_output_budget_exhausted = true;
            wasmtime::bail!("console output exceeds its bounded line or byte contract");
        }
        self.console_output_bytes = next_bytes;
        self.console_output.push(message);
        Ok(())
    }

    fn set_failed(&mut self) -> wasmtime::Result<()> {
        self.require_console_context()?;
        self.console_failed = true;
        Ok(())
    }
}
