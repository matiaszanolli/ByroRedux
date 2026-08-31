//! Canonical, versioned events delivered to executable extensions.

use serde::{Deserialize, Serialize};

use crate::identity::{EntityRef, FormRef};

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

/// Payload of `byro.events.hit`.
///
/// Damage is the producer-resolved value before the target's defenses. Source
/// and projectile handles are absent when the combat producer has no live ECS
/// entity for them.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HitEvent {
    pub subject: EntityRef,
    pub aggressor: Option<EntityRef>,
    pub source: Option<EntityRef>,
    pub projectile: Option<EntityRef>,
    pub damage: f32,
    pub power_attack: bool,
    pub sneak_attack: bool,
    pub bash_attack: bool,
    pub blocked: bool,
}

/// Payload of `byro.events.equipment-change`.
///
/// Items are identified by their load-order-independent authored form rather
/// than by a fabricated ECS entity. `equipped` is false for explicit
/// unequips and for items fully displaced by another equip operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EquipmentEvent {
    pub wearer: EntityRef,
    pub item: FormRef,
    pub equipped: bool,
}

/// Payload of one bounded `byro.events.update` callback.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct UpdateEvent {
    /// Actual engine time accumulated since this subscriber's previous fire.
    pub elapsed_seconds: f32,
}
