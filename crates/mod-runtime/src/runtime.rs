use crate::bindings::byro::mod_host::{context, logging};
use crate::bindings::Extension;
use crate::{CapabilitySet, Principal, Result, SandboxConfig, SandboxError};
use std::fmt;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder};

const LOG_CAPABILITY: &str = crate::LOG_CAPABILITY;

/// One host-attributed diagnostic record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogEntry {
    pub principal: crate::PrincipalId,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Guest entry point currently being executed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecyclePhase {
    Initialize,
    Shutdown,
}

impl fmt::Display for LifecyclePhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Initialize => "initialize",
            Self::Shutdown => "shutdown",
        })
    }
}

/// Why a component was quarantined.
///
/// #3050 — a log-budget overrun and a genuine guest fault both arrive at
/// `enter` as a failed call, and both used to produce an indistinguishable
/// `Quarantined`. They are not the same event: a fault means the guest
/// misbehaved, while a budget overrun means the host never drained what the
/// guest handed it (see [`ModInstance::take_logs`]). An operator triaging a
/// quarantined mod has to be able to tell those apart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultKind {
    /// The guest trapped, ran out of fuel, or a host call rejected it.
    Guest,
    /// The instance's retained-log budget was exhausted. Not misbehaviour on
    /// the guest's part on its own — the budget only fills because nothing
    /// drained it.
    LogBudgetExhausted,
}

impl fmt::Display for FaultKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Guest => "guest fault",
            Self::LogBudgetExhausted => "log budget exhausted",
        })
    }
}

/// Attributed fault retained after a component is quarantined.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FaultInfo {
    pub phase: LifecyclePhase,
    pub kind: FaultKind,
    pub message: String,
}

/// Explicit lifecycle state of one component instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstanceStatus {
    Ready,
    Active,
    Quarantined(FaultInfo),
    Stopped,
}

impl fmt::Display for InstanceStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Ready => "ready",
            Self::Active => "active",
            Self::Quarantined(_) => "quarantined",
            Self::Stopped => "stopped",
        })
    }
}

/// A component compiled for one runtime engine.
pub struct CompiledMod {
    component: Component,
}

impl fmt::Debug for CompiledMod {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompiledMod")
            .finish_non_exhaustive()
    }
}

/// Engine-owned executable-mod runtime with no ambient WASI imports.
pub struct SandboxRuntime {
    engine: Engine,
    linker: Linker<HostState>,
    config: SandboxConfig,
}

impl SandboxRuntime {
    pub fn new(config: SandboxConfig) -> Result<Self> {
        config.validate()?;

        let mut engine_config = Config::new();
        engine_config.wasm_component_model(true);
        engine_config.consume_fuel(true);
        engine_config.max_wasm_stack(config.max_wasm_stack_bytes);

        let engine =
            Engine::new(&engine_config).map_err(|error| SandboxError::Engine(error.to_string()))?;
        let mut linker = Linker::new(&engine);
        Extension::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)
            .map_err(|error| SandboxError::Link(error.to_string()))?;

        Ok(Self {
            engine,
            linker,
            config,
        })
    }

    pub fn config(&self) -> &SandboxConfig {
        &self.config
    }

    pub fn compile(&self, bytes: &[u8]) -> Result<CompiledMod> {
        if bytes.len() > self.config.max_component_bytes {
            return Err(SandboxError::ComponentTooLarge {
                actual: bytes.len(),
                maximum: self.config.max_component_bytes,
            });
        }

        let component = Component::new(&self.engine, bytes)
            .map_err(|error| SandboxError::Compile(format!("{error:#}")))?;
        Ok(CompiledMod { component })
    }

    pub fn instantiate(
        &self,
        compiled: &CompiledMod,
        principal: Principal,
        grants: CapabilitySet,
    ) -> Result<ModInstance> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(self.config.max_memory_bytes)
            .table_elements(self.config.max_table_elements)
            .instances(self.config.max_instances)
            .tables(self.config.max_tables)
            .memories(self.config.max_memories)
            .trap_on_grow_failure(true)
            .build();
        let state = HostState {
            principal,
            grants,
            limits,
            logs: Vec::new(),
            log_bytes: 0,
            max_log_entries: self.config.max_log_entries,
            max_log_message_bytes: self.config.max_log_message_bytes,
            max_log_bytes: self.config.max_log_bytes,
            log_budget_exhausted: false,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.limits);
        store
            .set_fuel(self.config.fuel_per_entry)
            .map_err(|error| SandboxError::Instantiate(error.to_string()))?;

        let bindings = Extension::instantiate(&mut store, &compiled.component, &self.linker)
            .map_err(|error| SandboxError::Instantiate(format!("{error:#}")))?;

        Ok(ModInstance {
            store,
            bindings,
            fuel_per_entry: self.config.fuel_per_entry,
            status: InstanceStatus::Ready,
        })
    }
}

/// A linked component isolated in its own Wasmtime store and principal state.
pub struct ModInstance {
    store: Store<HostState>,
    bindings: Extension,
    fuel_per_entry: u64,
    status: InstanceStatus,
}

impl ModInstance {
    pub fn principal(&self) -> &Principal {
        &self.store.data().principal
    }

    pub fn grants(&self) -> &CapabilitySet {
        &self.store.data().grants
    }

    /// Peek at the diagnostics the guest has produced and the host has not
    /// yet consumed. Does **not** free budget — see [`Self::take_logs`].
    pub fn logs(&self) -> &[LogEntry] {
        &self.store.data().logs
    }

    /// Remove and return every retained diagnostic, returning its budget to
    /// the guest.
    ///
    /// #3050 — `max_log_entries` / `max_log_bytes` used to be lifetime totals
    /// with no drain, so a well-behaved mod that logged at any steady rate was
    /// eventually quarantined for having run long enough rather than for
    /// misbehaving. They are a bound on what the host is *holding*, not on
    /// what the guest may ever say: draining hands the entries to the owner
    /// and the budget back to the guest. Mirrors the `take_errors` /
    /// `resource_errors` split in `crates/ui`, and is why [`Self::logs`] stays
    /// a non-consuming peek.
    ///
    /// A consumer that never calls this still gets the old behaviour, which is
    /// the correct backstop: undrained diagnostics cannot grow without bound.
    pub fn take_logs(&mut self) -> Vec<LogEntry> {
        let state = self.store.data_mut();
        state.log_bytes = 0;
        std::mem::take(&mut state.logs)
    }

    pub fn status(&self) -> &InstanceStatus {
        &self.status
    }

    pub fn fuel_remaining(&self) -> u64 {
        self.store.get_fuel().unwrap_or(0)
    }

    pub fn initialize(&mut self) -> Result<()> {
        if self.status != InstanceStatus::Ready {
            return Err(SandboxError::InvalidLifecycle {
                phase: LifecyclePhase::Initialize,
                status: self.status.clone(),
            });
        }

        self.enter(LifecyclePhase::Initialize, |bindings, store| {
            bindings.call_initialize(store)
        })?;
        self.status = InstanceStatus::Active;
        Ok(())
    }

    pub fn shutdown(&mut self) -> Result<()> {
        if self.status != InstanceStatus::Active {
            return Err(SandboxError::InvalidLifecycle {
                phase: LifecyclePhase::Shutdown,
                status: self.status.clone(),
            });
        }

        self.enter(LifecyclePhase::Shutdown, |bindings, store| {
            bindings.call_shutdown(store)
        })?;
        self.status = InstanceStatus::Stopped;
        Ok(())
    }

    fn enter(
        &mut self,
        phase: LifecyclePhase,
        call: impl FnOnce(&Extension, &mut Store<HostState>) -> wasmtime::Result<()>,
    ) -> Result<()> {
        if let Err(error) = self.store.set_fuel(self.fuel_per_entry) {
            return self.quarantine(phase, FaultKind::Guest, error.to_string());
        }
        if let Err(error) = call(&self.bindings, &mut self.store) {
            // #3050 — the host sets this flag at the point it refuses a log
            // for budget, so the trap that propagates back here can be
            // attributed to the budget rather than to the guest. Read (and
            // cleared) here so a later, genuine fault is not mislabelled.
            let kind = if std::mem::take(&mut self.store.data_mut().log_budget_exhausted) {
                FaultKind::LogBudgetExhausted
            } else {
                FaultKind::Guest
            };
            return self.quarantine(phase, kind, format!("{error:#}"));
        }
        Ok(())
    }

    fn quarantine<T>(
        &mut self,
        phase: LifecyclePhase,
        kind: FaultKind,
        message: String,
    ) -> Result<T> {
        self.status = InstanceStatus::Quarantined(FaultInfo {
            phase,
            kind,
            message: message.clone(),
        });
        Err(SandboxError::GuestFault { phase, message })
    }
}

struct HostState {
    principal: Principal,
    grants: CapabilitySet,
    limits: StoreLimits,
    logs: Vec<LogEntry>,
    log_bytes: usize,
    max_log_entries: usize,
    max_log_message_bytes: usize,
    max_log_bytes: usize,
    /// Set when a log was refused for budget rather than for content (#3050),
    /// so `ModInstance::enter` can attribute the resulting trap. Cleared by
    /// the reader.
    log_budget_exhausted: bool,
}

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

impl context::Host for HostState {
    fn principal_id(&mut self) -> wasmtime::Result<String> {
        Ok(self.principal.id().as_str().to_owned())
    }

    fn has_capability(&mut self, capability: String) -> wasmtime::Result<bool> {
        Ok(self.grants.contains(&capability))
    }
}
