use crate::{InstanceStatus, LifecyclePhase};
use byroredux_sdk::identity::{ComponentId, EventId, ExtensionId};
use byroredux_sdk::service::CompatibilityError;
use thiserror::Error;

/// Failures produced while configuring, linking, or executing a sandbox.
#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("invalid sandbox configuration: {0}")]
    InvalidConfig(&'static str),

    #[error("invalid principal id: {0}")]
    InvalidPrincipal(String),

    #[error("extension contract rejected before execution: {0}")]
    ExtensionContract(#[from] CompatibilityError),

    #[error("extension {extension} does not declare component {component}")]
    UndeclaredComponent {
        extension: ExtensionId,
        component: ComponentId,
    },

    #[error("compiled component belongs to {compiled}, not requested extension {requested}")]
    ManifestMismatch { compiled: String, requested: String },

    #[error("extension is not subscribed to event {0}")]
    EventNotSubscribed(EventId),

    #[error("extension lacks capability required to receive event {0}")]
    EventDeliveryDenied(EventId),

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
