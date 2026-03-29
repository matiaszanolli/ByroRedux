//! Script event marker components.
//!
//! Events are transient components: added when something happens,
//! processed by script systems during the frame, then removed by
//! the cleanup system at the end of the frame.
//!
//! This is the ECS replacement for Papyrus's event queue. Instead of
//! enqueueing events in a VM dispatcher (which adds latency), events
//! are immediate component mutations visible to all systems in the
//! same frame.

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};

/// Fired when an entity is activated by another entity (e.g., player uses a door).
/// Replaces Papyrus `OnActivate`.
#[derive(Debug, Clone, Copy)]
pub struct ActivateEvent {
    pub activator: EntityId,
}

impl Component for ActivateEvent {
    type Storage = SparseSetStorage<Self>;
}

/// Fired when an entity is hit in combat.
/// Replaces Papyrus `OnHit`.
#[derive(Debug, Clone, Copy)]
pub struct HitEvent {
    pub aggressor: EntityId,
    pub source: EntityId,
    pub projectile: EntityId,
    pub power_attack: bool,
    pub sneak_attack: bool,
    pub bash_attack: bool,
    pub blocked: bool,
}

impl Component for HitEvent {
    type Storage = SparseSetStorage<Self>;
}

/// Fired when a timer expires. Added by the timer tick system.
/// Replaces Papyrus `OnTimer`.
#[derive(Debug, Clone, Copy)]
pub struct TimerExpired {
    pub timer_id: u32,
}

impl Component for TimerExpired {
    type Storage = SparseSetStorage<Self>;
}
