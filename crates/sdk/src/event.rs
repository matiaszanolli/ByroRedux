//! Canonical, versioned events delivered to executable extensions.

use serde::{Deserialize, Serialize};

use crate::identity::{EntityRef, FormRef};

/// Canonical gameplay actions exposed after physical input rebinding.
///
/// Extensions observe these semantic intents, never platform key codes or
/// device-specific button numbers.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InputAction {
    MoveForward,
    MoveBackward,
    StrafeLeft,
    StrafeRight,
    Jump,
    Sprint,
    Activate,
    Attack,
    Block,
    Inventory,
    Quicksave,
    Quickload,
    Pause,
}

impl InputAction {
    /// Stable manifest spelling used by `byro.input.action` filters.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MoveForward => "move-forward",
            Self::MoveBackward => "move-backward",
            Self::StrafeLeft => "strafe-left",
            Self::StrafeRight => "strafe-right",
            Self::Jump => "jump",
            Self::Sprint => "sprint",
            Self::Activate => "activate",
            Self::Attack => "attack",
            Self::Block => "block",
            Self::Inventory => "inventory",
            Self::Quicksave => "quicksave",
            Self::Quickload => "quickload",
            Self::Pause => "pause",
        }
    }

    /// Parse a stable manifest filter value.
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "move-forward" => Self::MoveForward,
            "move-backward" => Self::MoveBackward,
            "strafe-left" => Self::StrafeLeft,
            "strafe-right" => Self::StrafeRight,
            "jump" => Self::Jump,
            "sprint" => Self::Sprint,
            "activate" => Self::Activate,
            "attack" => Self::Attack,
            "block" => Self::Block,
            "inventory" => Self::Inventory,
            "quicksave" => Self::Quicksave,
            "quickload" => Self::Quickload,
            "pause" => Self::Pause,
            _ => return None,
        })
    }
}

/// Edge of one normalized input action.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InputPhase {
    Pressed,
    Released,
}

/// Payload of `byro.events.input-action`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InputActionEvent {
    pub action: InputAction,
    pub phase: InputPhase,
}

/// Canonical normalized input-action event identifier.
pub const INPUT_ACTION_EVENT: &str = "byro.events.input-action";
/// Manifest filter field selecting one or more normalized actions.
pub const INPUT_ACTION_FILTER_FIELD: &str = "byro.input.action";

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
