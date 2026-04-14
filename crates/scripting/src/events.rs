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

/// A single text key event crossed during animation playback.
#[derive(Debug, Clone)]
pub struct AnimationTextKeyEvent {
    /// The text key label from the NIF (e.g., "hit", "sound: wpn_swing").
    pub label: String,
    /// The clip time at which this event was defined.
    pub time: f32,
}

/// Fired when animation text keys are crossed during playback.
///
/// Text keys are timed markers in .kf files (e.g., "hit", "sound: wpn_swing",
/// "FootLeft", "FootRight", "start", "end"). They fire each time the
/// animation's local time crosses the key's timestamp, including on loop.
/// Multiple keys can fire in a single frame, so this holds a Vec.
///
/// Systems can query for this component to trigger sounds, hit detection,
/// footstep effects, or state transitions.
#[derive(Debug, Clone)]
pub struct AnimationTextKeyEvents(pub Vec<AnimationTextKeyEvent>);

impl Component for AnimationTextKeyEvents {
    type Storage = SparseSetStorage<Self>;
}
