use crate::{InstanceStatus, LifecyclePhase};
use thiserror::Error;

/// Failures produced while configuring, linking, or executing a sandbox.
#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("invalid sandbox configuration: {0}")]
    InvalidConfig(&'static str),

    #[error("invalid principal id: {0}")]
    InvalidPrincipal(String),

    #[error("invalid capability id: {0}")]
    InvalidCapability(String),

    #[error("component is {actual} bytes, exceeding the {maximum}-byte limit")]
    ComponentTooLarge { actual: usize, maximum: usize },

    #[error("failed to create the sandbox engine: {0}")]
    Engine(String),

    #[error("failed to compile the component: {0}")]
    Compile(String),

    #[error("failed to link the host contract: {0}")]
    Link(String),

    #[error("failed to instantiate the component: {0}")]
    Instantiate(String),

    #[error("cannot enter {phase} while instance is {status}")]
    InvalidLifecycle {
        phase: LifecyclePhase,
        status: InstanceStatus,
    },

    #[error("guest fault during {phase}: {message}")]
    GuestFault {
        phase: LifecyclePhase,
        message: String,
    },
}

pub type Result<T> = std::result::Result<T, SandboxError>;
