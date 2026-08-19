use crate::{Result, SandboxError};

// ── #3049 (SAFE-2026-08-16-02) — validate() ceilings ────────────────
//
// Pre-fix, every check below was a floor (`== 0` / `< minimum`); there
// was no upper bound on anything. A config field near `usize::MAX`
// doesn't crash at `validate()` time, but every one of these fields
// feeds a wasmtime `Config`/`StoreLimitsBuilder` call that then tries
// to actually reserve or permit that much resource per component
// instance — no legitimate mod needs anywhere near these ceilings, so
// they're a sanity backstop against pathological misconfiguration
// (accidental or adversarial), not precisely-derived physical limits.
// Same posture as the physics crate's `MAX_SANE_SHAPE_EXTENT` (#2543).

/// Shared ceiling for every resource-count / byte-size field except
/// `max_wasm_stack_bytes` (needs its own much tighter ceiling — see
/// [`MAX_WASM_STACK_BYTES_CEILING`]) and `fuel_per_entry` (a different
/// unit entirely — see [`MAX_FUEL_PER_ENTRY`]).
const MAX_SANE_LIMIT: usize = 1 << 30; // 1 GiB

/// `max_wasm_stack_bytes` needs a MUCH tighter ceiling than
/// [`MAX_SANE_LIMIT`]: wasmtime's `Config::max_wasm_stack` documents
/// that when the configured wasm stack exceeds what's actually left on
/// the calling thread's stack, the process **aborts** rather than
/// trapping the guest (verified against the vendored wasmtime 47.0.3
/// source — this crate's pinned version) — the exact failure mode this
/// issue is about.
///
/// This crate has no engine consumer yet and doesn't spawn its own
/// thread with a known fixed stack size (see `runtime.rs`), so there
/// is no single "the host thread stack the runtime actually allocates"
/// to derive an exact ceiling from today. Absent that, this uses the
/// smallest commonly-documented default OS thread stack size (Windows:
/// 1 MiB, both the process main thread and `CreateThread`'s default)
/// as the ceiling. That leaves zero margin in the literal worst case
/// (a config at exactly this ceiling, called from a thread that
/// already has a deep call stack before reaching this crate) — a real
/// limitation of validating this in isolation from the calling thread,
/// not a claim that a config passing this check can never abort.
/// Going lower would be an equally unverified guess in the other
/// direction with no more evidence behind it. When a real engine
/// consumer lands, it should either spawn a dedicated thread with an
/// explicit, generous stack size (Rust's own default spawned-thread
/// stack is 2 MiB) and tighten this ceiling to match precisely, or
/// reject configs whose `max_wasm_stack_bytes` exceeds that thread's
/// own known size directly.
const MAX_WASM_STACK_BYTES_CEILING: usize = 1024 * 1024; // 1 MiB

/// `fuel_per_entry` is a different unit (wasmtime fuel, not bytes) and
/// a different risk category from the others: an oversized value
/// doesn't crash the host, it removes the compute-time safety valve
/// the fuel mechanism exists to provide (a guest could run for a very
/// long time before exhausting its budget) — an availability/DoS
/// concern, not the host-abort concern `MAX_WASM_STACK_BYTES_CEILING`
/// guards against. Same "no legitimate use needs this much headroom
/// above the default" reasoning: 1e12 is 100,000x the default
/// `fuel_per_entry` (10e6).
const MAX_FUEL_PER_ENTRY: u64 = 1_000_000_000_000;

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
        if self.max_component_bytes > MAX_SANE_LIMIT {
            return Err(SandboxError::InvalidConfig(
                "max_component_bytes exceeds the sane ceiling",
            ));
        }
        if self.max_memory_bytes < 64 * 1024 {
            return Err(SandboxError::InvalidConfig(
                "max_memory_bytes must allow at least one WebAssembly page",
            ));
        }
        if self.max_memory_bytes > MAX_SANE_LIMIT {
            return Err(SandboxError::InvalidConfig(
                "max_memory_bytes exceeds the sane ceiling",
            ));
        }
        if self.max_table_elements == 0 {
            return Err(SandboxError::InvalidConfig(
                "max_table_elements must be non-zero",
            ));
        }
        if self.max_table_elements > MAX_SANE_LIMIT {
            return Err(SandboxError::InvalidConfig(
                "max_table_elements exceeds the sane ceiling",
            ));
        }
        if self.max_instances == 0 || self.max_tables == 0 || self.max_memories == 0 {
            return Err(SandboxError::InvalidConfig(
                "instance, table, and memory limits must be non-zero",
            ));
        }
        if self.max_instances > MAX_SANE_LIMIT
            || self.max_tables > MAX_SANE_LIMIT
            || self.max_memories > MAX_SANE_LIMIT
        {
            return Err(SandboxError::InvalidConfig(
                "instance, table, or memory limit exceeds the sane ceiling",
            ));
        }
        if self.max_wasm_stack_bytes == 0 {
            return Err(SandboxError::InvalidConfig(
                "max_wasm_stack_bytes must be non-zero",
            ));
        }
        // #3049 — the ceiling this issue is actually about: wasmtime
        // aborts the host process (rather than trapping the guest) when
        // the configured wasm stack exceeds what's left on the calling
        // thread's stack. See MAX_WASM_STACK_BYTES_CEILING's doc for
        // the reasoning behind the specific value.
        if self.max_wasm_stack_bytes > MAX_WASM_STACK_BYTES_CEILING {
            return Err(SandboxError::InvalidConfig(
                "max_wasm_stack_bytes exceeds the host-thread-safe ceiling",
            ));
        }
        if self.fuel_per_entry == 0 {
            return Err(SandboxError::InvalidConfig(
                "fuel_per_entry must be non-zero",
            ));
        }
        if self.fuel_per_entry > MAX_FUEL_PER_ENTRY {
            return Err(SandboxError::InvalidConfig(
                "fuel_per_entry exceeds the sane ceiling",
            ));
        }
        if self.max_log_entries == 0 || self.max_log_message_bytes == 0 || self.max_log_bytes == 0 {
            return Err(SandboxError::InvalidConfig("log limits must be non-zero"));
        }
        if self.max_log_entries > MAX_SANE_LIMIT
            || self.max_log_message_bytes > MAX_SANE_LIMIT
            || self.max_log_bytes > MAX_SANE_LIMIT
        {
            return Err(SandboxError::InvalidConfig(
                "a log limit exceeds the sane ceiling",
            ));
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

    // ── #3049 (SAFE-2026-08-16-02) — ceiling regression tests ──────

    /// The issue's headline case: an oversized `max_wasm_stack_bytes`
    /// must be rejected at config time, not left to abort the host
    /// process when a guest's recursion exceeds the calling thread's
    /// remaining stack.
    #[test]
    fn oversized_wasm_stack_is_rejected() {
        let config = SandboxConfig {
            max_wasm_stack_bytes: MAX_WASM_STACK_BYTES_CEILING + 1,
            ..SandboxConfig::default()
        };
        assert!(matches!(
            config.validate(),
            Err(SandboxError::InvalidConfig(_))
        ));
    }

    /// Boundary check alongside the rejection test above: a config
    /// sitting exactly AT the ceiling must still validate — the guard
    /// is `>`, not `>=`.
    #[test]
    fn wasm_stack_at_the_ceiling_is_accepted() {
        let config = SandboxConfig {
            max_wasm_stack_bytes: MAX_WASM_STACK_BYTES_CEILING,
            ..SandboxConfig::default()
        };
        config.validate().unwrap();
    }

    /// CEILINGS completeness check: every floor-only field the audit
    /// named must now also reject a pathologically large value, not
    /// just the wasm stack. One table-driven test instead of N
    /// near-duplicate ones — each row sets exactly one field to
    /// `usize::MAX` (or `u64::MAX` for fuel) and expects rejection.
    #[test]
    fn every_resource_field_rejects_usize_max() {
        let cases: [(&str, SandboxConfig); 8] = [
            (
                "max_component_bytes",
                SandboxConfig {
                    max_component_bytes: usize::MAX,
                    ..SandboxConfig::default()
                },
            ),
            (
                "max_memory_bytes",
                SandboxConfig {
                    max_memory_bytes: usize::MAX,
                    ..SandboxConfig::default()
                },
            ),
            (
                "max_table_elements",
                SandboxConfig {
                    max_table_elements: usize::MAX,
                    ..SandboxConfig::default()
                },
            ),
            (
                "max_instances",
                SandboxConfig {
                    max_instances: usize::MAX,
                    ..SandboxConfig::default()
                },
            ),
            (
                "max_tables",
                SandboxConfig {
                    max_tables: usize::MAX,
                    ..SandboxConfig::default()
                },
            ),
            (
                "max_memories",
                SandboxConfig {
                    max_memories: usize::MAX,
                    ..SandboxConfig::default()
                },
            ),
            (
                "fuel_per_entry",
                SandboxConfig {
                    fuel_per_entry: u64::MAX,
                    ..SandboxConfig::default()
                },
            ),
            (
                "max_log_entries",
                SandboxConfig {
                    max_log_entries: usize::MAX,
                    ..SandboxConfig::default()
                },
            ),
        ];
        for (field, config) in cases {
            assert!(
                matches!(config.validate(), Err(SandboxError::InvalidConfig(_))),
                "{field} = MAX must be rejected by validate(), but it passed"
            );
        }
    }

    /// `max_log_bytes` shares `MAX_SANE_LIMIT` with the other resource
    /// fields but also participates in the pre-existing
    /// message-must-not-exceed-total cross-check. Keep
    /// `max_log_message_bytes` comfortably under both limits so this
    /// specifically exercises the new ceiling, not the older
    /// cross-check.
    #[test]
    fn oversized_max_log_bytes_is_rejected() {
        let config = SandboxConfig {
            max_log_bytes: MAX_SANE_LIMIT + 1,
            ..SandboxConfig::default()
        };
        assert!(matches!(
            config.validate(),
            Err(SandboxError::InvalidConfig(_))
        ));
    }

    /// Same as above for `max_log_message_bytes`, with `max_log_bytes`
    /// raised alongside it so the pre-existing cross-check doesn't
    /// fire for an unrelated reason and mask which guard actually
    /// caught it.
    #[test]
    fn oversized_max_log_message_bytes_is_rejected() {
        let config = SandboxConfig {
            max_log_message_bytes: MAX_SANE_LIMIT + 1,
            max_log_bytes: MAX_SANE_LIMIT + 2,
            ..SandboxConfig::default()
        };
        assert!(matches!(
            config.validate(),
            Err(SandboxError::InvalidConfig(_))
        ));
    }
}
