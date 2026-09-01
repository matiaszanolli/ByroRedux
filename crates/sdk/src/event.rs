//! Canonical, versioned events delivered to executable extensions.

use serde::{Deserialize, Serialize};

use crate::identity::{EntityRef, EventId, FormRef, PrincipalId};

/// Maximum opaque payload accepted for one custom/mod event.
pub const MAX_CUSTOM_EVENT_PAYLOAD_BYTES: usize = 4 * 1024;

/// A principal-authored event queued by one successful guest callback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishEventCommand {
    pub event: EventId,
    pub payload: Vec<u8>,
}

/// Canonical custom/mod event delivered to an exact manifest subscriber.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomEvent {
    pub event: EventId,
    pub sender: PrincipalId,
    pub payload: Vec<u8>,
}

/// Whether an ID follows the reserved `mod.<principal>.event.<channel>` shape.
pub fn is_custom_event_id(event: &EventId) -> bool {
    let Some(rest) = event.as_str().strip_prefix("mod.") else {
        return false;
    };
    let Some((owner, channel)) = rest.split_once(".event.") else {
        return false;
    };
    !channel.is_empty() && PrincipalId::new(owner).is_ok()
}

/// Whether the authenticated principal owns this custom event namespace.
pub fn custom_event_owned_by(event: &EventId, principal: &PrincipalId) -> bool {
    event
        .as_str()
        .strip_prefix("mod.")
        .and_then(|rest| rest.split_once(".event."))
        .is_some_and(|(owner, channel)| owner == principal.as_str() && !channel.is_empty())
}

impl PublishEventCommand {
    /// Validate payload bounds; namespace ownership is checked by the host.
    pub fn new(event: EventId, payload: Vec<u8>) -> Option<Self> {
        (is_custom_event_id(&event) && payload.len() <= MAX_CUSTOM_EVENT_PAYLOAD_BYTES)
            .then_some(Self { event, payload })
    }
}

impl CustomEvent {
    /// Validate channel ownership and payload bounds before guest delivery.
    pub fn is_valid(&self) -> bool {
        custom_event_owned_by(&self.event, &self.sender)
            && self.payload.len() <= MAX_CUSTOM_EVENT_PAYLOAD_BYTES
    }
}

#[cfg(test)]
mod custom_event_tests {
    use super::*;

    fn principal() -> PrincipalId {
        PrincipalId::new("org.example.weather").unwrap()
    }

    #[test]
    fn custom_channels_are_reserved_exact_and_principal_owned() {
        let owned = EventId::new("mod.org.example.weather.event.front-arrived").unwrap();
        assert!(is_custom_event_id(&owned));
        assert!(custom_event_owned_by(&owned, &principal()));

        let foreign = EventId::new("mod.org.example.climate.event.front-arrived").unwrap();
        assert!(is_custom_event_id(&foreign));
        assert!(!custom_event_owned_by(&foreign, &principal()));

        let nested = EventId::new("mod.org.example.weather.event.front.event.arrived").unwrap();
        assert!(custom_event_owned_by(&nested, &principal()));
        assert!(!custom_event_owned_by(
            &nested,
            &PrincipalId::new("org.example.weather.event.front").unwrap()
        ));

        for invalid in [
            "byro.events.front-arrived",
            "mod.org.example.weather.front-arrived",
            "mod.org.example.weather.event.",
        ] {
            assert!(!is_custom_event_id(&EventId::new(invalid).unwrap()));
        }
    }

    #[test]
    fn publication_payload_is_bounded() {
        let event = EventId::new("mod.org.example.weather.event.front-arrived").unwrap();
        assert!(
            PublishEventCommand::new(event.clone(), vec![0; MAX_CUSTOM_EVENT_PAYLOAD_BYTES])
                .is_some()
        );
        assert!(
            PublishEventCommand::new(event, vec![0; MAX_CUSTOM_EVENT_PAYLOAD_BYTES + 1]).is_none()
        );
    }
}

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

/// Engine-owned session transition delivered after the operation commits.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionPhase {
    /// A fresh game session became active without restoring a save.
    NewGame,
    /// A save slot was committed successfully.
    SaveComplete,
    /// A save slot was restored successfully, including extension state.
    LoadComplete,
}

impl SessionPhase {
    /// Stable manifest spelling used by `byro.session.phase` filters.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NewGame => "new-game",
            Self::SaveComplete => "save-complete",
            Self::LoadComplete => "load-complete",
        }
    }

    /// Parse a stable manifest filter value.
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "new-game" => Self::NewGame,
            "save-complete" => Self::SaveComplete,
            "load-complete" => Self::LoadComplete,
            _ => return None,
        })
    }
}

/// Payload of `byro.events.session`.
///
/// `slot` is present for successful save/load transitions and absent for a
/// new game. Hosts reject any other combination before guest delivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionEvent {
    pub phase: SessionPhase,
    pub slot: Option<u32>,
}

impl SessionEvent {
    /// Whether phase and optional slot form one canonical engine payload.
    pub const fn is_valid(self) -> bool {
        matches!(
            (self.phase, self.slot),
            (SessionPhase::NewGame, None)
                | (
                    SessionPhase::SaveComplete | SessionPhase::LoadComplete,
                    Some(_)
                )
        )
    }
}

/// Canonical game-session lifecycle event identifier.
pub const SESSION_EVENT: &str = "byro.events.session";
/// Manifest filter field selecting one or more lifecycle phases.
pub const SESSION_PHASE_FILTER_FIELD: &str = "byro.session.phase";

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
