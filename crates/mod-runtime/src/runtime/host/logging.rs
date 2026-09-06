//! `logging` WIT host interface.

use crate::runtime::*;

impl logging::Host for HostState {
    fn log(&mut self, level: logging::Level, message: String) -> wasmtime::Result<()> {
        if !self.grants.contains(LOG_CAPABILITY) {
            wasmtime::bail!(
                "principal {} lacks capability {LOG_CAPABILITY}",
                self.principal.id()
            );
        }
        if message.len() > self.max_log_message_bytes {
            wasmtime::bail!(
                "log message is {} bytes, exceeding the {}-byte limit",
                message.len(),
                self.max_log_message_bytes
            );
        }
        // #3050 — these two are budget, not misbehaviour: they bound what the
        // host is holding undrained, and `ModInstance::take_logs` returns
        // both. Flag the distinction for `enter` before bailing. The
        // per-message size check above is deliberately NOT flagged: an
        // oversized single message is the guest breaking a per-call contract,
        // which no amount of draining changes.
        if self.logs.len() >= self.max_log_entries {
            self.log_budget_exhausted = true;
            wasmtime::bail!(
                "log entry limit of {} exceeded ({} undrained); \
                 drain with ModInstance::take_logs",
                self.max_log_entries,
                self.logs.len()
            );
        }
        let next_log_bytes = self
            .log_bytes
            .checked_add(message.len())
            .ok_or_else(|| wasmtime::Error::msg("log byte count overflow"))?;
        if next_log_bytes > self.max_log_bytes {
            self.log_budget_exhausted = true;
            wasmtime::bail!(
                "log byte limit of {} exceeded ({} undrained); \
                 drain with ModInstance::take_logs",
                self.max_log_bytes,
                self.log_bytes
            );
        }

        self.log_bytes = next_log_bytes;
        self.logs.push(LogEntry {
            principal: self.principal.id().clone(),
            level: match level {
                logging::Level::Debug => LogLevel::Debug,
                logging::Level::Info => LogLevel::Info,
                logging::Level::Warn => LogLevel::Warn,
                logging::Level::Error => LogLevel::Error,
            },
            message,
        });
        Ok(())
    }
}
