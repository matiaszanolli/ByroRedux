//! Sandboxed executable-mod runtime.
//!
//! This crate is the engine-owned boundary between untrusted community code
//! and semantic host services. Guests are WebAssembly Components, each
//! instance has its own principal and resource store, and host authority is
//! represented by explicit capabilities. No WASI implementation is linked by
//! default, so operating-system access is absent rather than merely promised
//! by convention.

mod bindings;
mod error;
mod identity;
mod limits;
mod runtime;

pub use byroredux_sdk::component::ExtensionCommand;
pub use byroredux_sdk::event::ActivationEvent;
pub use byroredux_sdk::identity::{CapabilityId, CapabilitySet, PrincipalId};
pub use byroredux_sdk::service::LOG_WRITE_CAPABILITY as LOG_CAPABILITY;
pub use error::{Result, SandboxError};
pub use identity::Principal;
pub use limits::SandboxConfig;
pub use runtime::{
    CompiledMod, FaultInfo, FaultKind, InstanceStatus, LifecyclePhase, LogEntry, LogLevel,
    ModInstance, SandboxRuntime,
};

#[cfg(test)]
mod tests;
