//! Canonical, versioned events delivered to executable extensions.

use serde::{Deserialize, Serialize};

use crate::identity::EntityRef;

/// Payload of `byro.events.activate`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ActivationEvent {
    /// Stable handle for the activated target.
    pub subject: EntityRef,
    /// Stable handle for the activating entity, when one is representable.
    pub activator: Option<EntityRef>,
}

/// Payload of `byro.events.cell-load`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CellLoadEvent {
    /// Newly loaded script-bearing entity.
    pub subject: EntityRef,
}
