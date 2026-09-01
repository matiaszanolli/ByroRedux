//! Canonical, versioned events delivered to executable extensions.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::identity::{EntityRef, EventId, FormRef, PrincipalId};

/// Maximum opaque payload accepted for one custom/mod event.
pub const MAX_CUSTOM_EVENT_PAYLOAD_BYTES: usize = 4 * 1024;

/// Shared, engine-owned namespace used to replace SKSE's process-global
/// `ModEvent` registry without loading the script extender.
pub const LEGACY_SKSE_MOD_EVENT_PREFIX: &str = "legacy.skse.mod-event.";

/// Maximum UTF-8 event-name length that fits reversibly in an [`EventId`].
pub const MAX_LEGACY_SKSE_MOD_EVENT_NAME_BYTES: usize = 53;

/// Bound for the Papyrus callback identifier retained by a dynamic legacy
/// registration.
pub const MAX_LEGACY_SKSE_CALLBACK_BYTES: usize = 128;

/// Maximum live handle builders retained by one compatibility adapter.
pub const MAX_LEGACY_SKSE_MOD_EVENT_BUILDERS: usize = 64;

/// Maximum typed arguments in one handle-built event.
pub const MAX_LEGACY_SKSE_MOD_EVENT_ARGUMENTS: usize = 128;

/// A principal-authored event queued by one successful guest callback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishEventCommand {
    pub event: EventId,
    pub payload: Vec<u8>,
}

/// Deferred runtime registration mutation for SKSE-compatible mod events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyModEventSubscriptionCommand {
    Subscribe { event: EventId, callback: String },
    Unsubscribe { event: EventId },
    UnsubscribeAll,
}

impl LegacyModEventSubscriptionCommand {
    pub fn subscribe(event_name: &str, callback: String) -> Option<Self> {
        (!callback.is_empty() && callback.len() <= MAX_LEGACY_SKSE_CALLBACK_BYTES).then_some(())?;
        Some(Self::Subscribe {
            event: legacy_skse_mod_event_id(event_name)?,
            callback,
        })
    }

    pub fn unsubscribe(event_name: &str) -> Option<Self> {
        Some(Self::Unsubscribe {
            event: legacy_skse_mod_event_id(event_name)?,
        })
    }

    pub fn is_valid(&self) -> bool {
        match self {
            Self::Subscribe { event, callback } => {
                is_legacy_skse_mod_event_id(event)
                    && !callback.is_empty()
                    && callback.len() <= MAX_LEGACY_SKSE_CALLBACK_BYTES
            }
            Self::Unsubscribe { event } => is_legacy_skse_mod_event_id(event),
            Self::UnsubscribeAll => true,
        }
    }
}

/// Canonical custom/mod event delivered to an exact manifest subscriber.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomEvent {
    pub event: EventId,
    pub sender: PrincipalId,
    pub payload: Vec<u8>,
}

/// Fixed-arity payload of `Form.SendModEvent` after engine adaptation.
///
/// The event name is carried reversibly by the channel ID. The float is kept
/// as IEEE-754 bits so this contract remains equality-comparable and stable
/// across serialization boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacySkseModEventPayload {
    pub string_arg: String,
    pub number_arg_bits: u32,
    pub sender: Option<FormRef>,
}

/// One typed argument pushed through SKSE's handle-based `ModEvent` API.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacySkseModEventValue {
    Bool(bool),
    Int(i32),
    FloatBits(u32),
    String(String),
    Form(Option<FormRef>),
}

impl LegacySkseModEventValue {
    pub fn float(value: f32) -> Self {
        Self::FloatBits(value.to_bits())
    }

    pub fn as_float(&self) -> Option<f32> {
        match self {
            Self::FloatBits(bits) => Some(f32::from_bits(*bits)),
            _ => None,
        }
    }
}

/// Versioned payload for `ModEvent.Create/Push*/Send`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacySkseVariadicModEventPayload {
    pub arguments: Vec<LegacySkseModEventValue>,
}

impl LegacySkseVariadicModEventPayload {
    pub fn encode(&self) -> Option<Vec<u8>> {
        if self.arguments.len() > MAX_LEGACY_SKSE_MOD_EVENT_ARGUMENTS {
            return None;
        }
        let count = u16::try_from(self.arguments.len()).ok()?;
        let mut bytes = Vec::new();
        bytes.push(1);
        bytes.extend_from_slice(&count.to_le_bytes());
        for argument in &self.arguments {
            match argument {
                LegacySkseModEventValue::Bool(value) => {
                    bytes.extend_from_slice(&[0, u8::from(*value)]);
                }
                LegacySkseModEventValue::Int(value) => {
                    bytes.push(1);
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
                LegacySkseModEventValue::FloatBits(bits) => {
                    bytes.push(2);
                    bytes.extend_from_slice(&bits.to_le_bytes());
                }
                LegacySkseModEventValue::String(value) => {
                    bytes.push(3);
                    bytes.extend_from_slice(&u32::try_from(value.len()).ok()?.to_le_bytes());
                    bytes.extend_from_slice(value.as_bytes());
                }
                LegacySkseModEventValue::Form(value) => {
                    bytes.push(4);
                    match value {
                        Some(form) => {
                            bytes.push(1);
                            bytes.extend_from_slice(&form.source());
                            bytes.extend_from_slice(&form.local().to_le_bytes());
                        }
                        None => bytes.push(0),
                    }
                }
            }
            if bytes.len() > MAX_CUSTOM_EVENT_PAYLOAD_BYTES {
                return None;
            }
        }
        Some(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let (&version, rest) = bytes.split_first()?;
        if version != 1 {
            return None;
        }
        let (count, mut rest) = take_u16(rest)?;
        let count = usize::from(count);
        if count > MAX_LEGACY_SKSE_MOD_EVENT_ARGUMENTS {
            return None;
        }
        let mut arguments = Vec::with_capacity(count);
        for _ in 0..count {
            let (&tag, next) = rest.split_first()?;
            rest = next;
            let argument = match tag {
                0 => {
                    let (&value, next) = rest.split_first()?;
                    rest = next;
                    LegacySkseModEventValue::Bool(match value {
                        0 => false,
                        1 => true,
                        _ => return None,
                    })
                }
                1 => {
                    let (value, next) = take_i32(rest)?;
                    rest = next;
                    LegacySkseModEventValue::Int(value)
                }
                2 => {
                    let (bits, next) = take_u32(rest)?;
                    rest = next;
                    LegacySkseModEventValue::FloatBits(bits)
                }
                3 => {
                    let (length, next) = take_u32(rest)?;
                    let length = usize::try_from(length).ok()?;
                    let (value, next) = next.split_at_checked(length)?;
                    rest = next;
                    LegacySkseModEventValue::String(std::str::from_utf8(value).ok()?.to_owned())
                }
                4 => {
                    let (&present, next) = rest.split_first()?;
                    rest = next;
                    let value = match present {
                        0 => None,
                        1 => {
                            let (source, next) = rest.split_at_checked(16)?;
                            let source = source.try_into().ok()?;
                            let (local, next) = take_u32(next)?;
                            rest = next;
                            Some(FormRef::new(source, local))
                        }
                        _ => return None,
                    };
                    LegacySkseModEventValue::Form(value)
                }
                _ => return None,
            };
            arguments.push(argument);
        }
        rest.is_empty().then_some(Self { arguments })
    }
}

/// One in-progress handle event.
#[derive(Clone, Debug, Eq, PartialEq)]
struct LegacySkseModEventBuilder {
    event: EventId,
    arguments: Vec<LegacySkseModEventValue>,
}

/// Engine-owned replacement for SKSE's transient ModEvent handle registry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LegacySkseModEventBuilders {
    next_handle: u32,
    builders: BTreeMap<u32, LegacySkseModEventBuilder>,
}

impl LegacySkseModEventBuilders {
    pub fn new() -> Self {
        Self {
            next_handle: 1,
            builders: BTreeMap::new(),
        }
    }

    /// Create a builder, returning SKSE's failure sentinel (`0`) on rejection.
    pub fn create(&mut self, event_name: &str) -> u32 {
        let Some(event) = legacy_skse_mod_event_id(event_name) else {
            return 0;
        };
        if self.builders.len() >= MAX_LEGACY_SKSE_MOD_EVENT_BUILDERS {
            return 0;
        }
        for _ in 0..=MAX_LEGACY_SKSE_MOD_EVENT_BUILDERS {
            let handle = self.next_handle.max(1);
            self.next_handle = handle.wrapping_add(1).max(1);
            if let std::collections::btree_map::Entry::Vacant(entry) = self.builders.entry(handle) {
                entry.insert(LegacySkseModEventBuilder {
                    event,
                    arguments: Vec::new(),
                });
                return handle;
            }
        }
        0
    }

    /// Push one argument. Invalid handles or an oversized payload are ignored,
    /// matching the legacy void-return functions.
    pub fn push(&mut self, handle: u32, value: LegacySkseModEventValue) {
        let Some(builder) = self.builders.get_mut(&handle) else {
            return;
        };
        if builder.arguments.len() >= MAX_LEGACY_SKSE_MOD_EVENT_ARGUMENTS {
            return;
        }
        builder.arguments.push(value);
        let encodable = LegacySkseVariadicModEventPayload {
            arguments: builder.arguments.clone(),
        }
        .encode()
        .is_some();
        if !encodable {
            builder.arguments.pop();
        }
    }

    /// Send and release a handle. `None` is SKSE's `false` result.
    pub fn send(&mut self, handle: u32) -> Option<PublishEventCommand> {
        let builder = self.builders.remove(&handle)?;
        let payload = LegacySkseVariadicModEventPayload {
            arguments: builder.arguments,
        }
        .encode()?;
        PublishEventCommand::new(builder.event, payload)
    }

    pub fn release(&mut self, handle: u32) {
        self.builders.remove(&handle);
    }

    pub fn contains(&self, handle: u32) -> bool {
        self.builders.contains_key(&handle)
    }

    pub fn len(&self) -> usize {
        self.builders.len()
    }

    pub fn is_empty(&self) -> bool {
        self.builders.is_empty()
    }
}

impl LegacySkseModEventPayload {
    pub fn new(string_arg: String, number_arg: f32, sender: Option<FormRef>) -> Self {
        Self {
            string_arg,
            number_arg_bits: number_arg.to_bits(),
            sender,
        }
    }

    pub fn number_arg(&self) -> f32 {
        f32::from_bits(self.number_arg_bits)
    }

    /// Encode a versioned, bounded wire payload for the existing event bus.
    pub fn encode(&self) -> Option<Vec<u8>> {
        let string_len = u32::try_from(self.string_arg.len()).ok()?;
        let mut bytes = Vec::with_capacity(
            1 + 4 + self.string_arg.len() + 4 + 1 + self.sender.map_or(0, |_| 20),
        );
        bytes.push(1);
        bytes.extend_from_slice(&string_len.to_le_bytes());
        bytes.extend_from_slice(self.string_arg.as_bytes());
        bytes.extend_from_slice(&self.number_arg_bits.to_le_bytes());
        match self.sender {
            Some(sender) => {
                bytes.push(1);
                bytes.extend_from_slice(&sender.source());
                bytes.extend_from_slice(&sender.local().to_le_bytes());
            }
            None => bytes.push(0),
        }
        (bytes.len() <= MAX_CUSTOM_EVENT_PAYLOAD_BYTES).then_some(bytes)
    }

    /// Decode the exact payload shape; trailing or malformed bytes reject.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let (&version, rest) = bytes.split_first()?;
        if version != 1 {
            return None;
        }
        let (string_len, rest) = take_u32(rest)?;
        let string_len = usize::try_from(string_len).ok()?;
        let (string, rest) = rest.split_at_checked(string_len)?;
        let string_arg = std::str::from_utf8(string).ok()?.to_owned();
        let (number_arg_bits, rest) = take_u32(rest)?;
        let (&has_sender, rest) = rest.split_first()?;
        let (sender, rest) = match has_sender {
            0 => (None, rest),
            1 => {
                let (source, rest) = rest.split_at_checked(16)?;
                let source: [u8; 16] = source.try_into().ok()?;
                let (local, rest) = take_u32(rest)?;
                (Some(FormRef::new(source, local)), rest)
            }
            _ => return None,
        };
        rest.is_empty().then_some(Self {
            string_arg,
            number_arg_bits,
            sender,
        })
    }
}

fn take_u32(bytes: &[u8]) -> Option<(u32, &[u8])> {
    let (value, rest) = bytes.split_at_checked(4)?;
    Some((u32::from_le_bytes(value.try_into().ok()?), rest))
}

fn take_u16(bytes: &[u8]) -> Option<(u16, &[u8])> {
    let (value, rest) = bytes.split_at_checked(2)?;
    Some((u16::from_le_bytes(value.try_into().ok()?), rest))
}

fn take_i32(bytes: &[u8]) -> Option<(i32, &[u8])> {
    let (value, rest) = bytes.split_at_checked(4)?;
    Some((i32::from_le_bytes(value.try_into().ok()?), rest))
}

/// Reversibly map one legacy shared event name to an engine channel.
pub fn legacy_skse_mod_event_id(name: &str) -> Option<EventId> {
    if name.is_empty() || name.len() > MAX_LEGACY_SKSE_MOD_EVENT_NAME_BYTES {
        return None;
    }
    let mut id = String::with_capacity(LEGACY_SKSE_MOD_EVENT_PREFIX.len() + name.len() * 2);
    id.push_str(LEGACY_SKSE_MOD_EVENT_PREFIX);
    for byte in name.as_bytes() {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        id.push(char::from(HEX[usize::from(byte >> 4)]));
        id.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    EventId::new(id).ok()
}

/// Recover the original case-sensitive UTF-8 name from a compatibility ID.
pub fn legacy_skse_mod_event_name(event: &EventId) -> Option<String> {
    let encoded = event.as_str().strip_prefix(LEGACY_SKSE_MOD_EVENT_PREFIX)?;
    if encoded.is_empty() || !encoded.len().is_multiple_of(2) {
        return None;
    }
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        decoded.push((hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?);
    }
    String::from_utf8(decoded).ok()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    Some(match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => return None,
    })
}

/// Whether the ID belongs to the shared engine-level SKSE compatibility bus.
pub fn is_legacy_skse_mod_event_id(event: &EventId) -> bool {
    legacy_skse_mod_event_name(event).is_some()
}

/// Whether an ID follows the reserved `mod.<principal>.event.<channel>` shape.
pub fn is_custom_event_id(event: &EventId) -> bool {
    if is_legacy_skse_mod_event_id(event) {
        return true;
    }
    let Some(rest) = event.as_str().strip_prefix("mod.") else {
        return false;
    };
    let Some((owner, channel)) = rest.split_once(".event.") else {
        return false;
    };
    !channel.is_empty() && PrincipalId::new(owner).is_ok()
}

/// Whether a capability-authorized principal may publish this channel.
///
/// Native custom channels remain owner-only. SKSE compatibility channels are
/// deliberately shared, matching the extender's process-global registry.
pub fn custom_event_publishable_by(event: &EventId, principal: &PrincipalId) -> bool {
    custom_event_owned_by(event, principal) || is_legacy_skse_mod_event_id(event)
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
    /// Validate channel authority and payload bounds before guest delivery.
    pub fn is_valid(&self) -> bool {
        custom_event_publishable_by(&self.event, &self.sender)
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

    #[test]
    fn legacy_skse_channels_are_shared_reversible_and_case_sensitive() {
        let upper = legacy_skse_mod_event_id("SKICP_configManagerReady").unwrap();
        let lower = legacy_skse_mod_event_id("skicp_configmanagerready").unwrap();
        assert_ne!(upper, lower);
        assert_eq!(
            legacy_skse_mod_event_name(&upper).as_deref(),
            Some("SKICP_configManagerReady")
        );
        assert!(is_custom_event_id(&upper));
        assert!(custom_event_publishable_by(&upper, &principal()));
        assert!(!custom_event_owned_by(&upper, &principal()));
        assert!(legacy_skse_mod_event_id("").is_none());
        assert!(legacy_skse_mod_event_id(&"x".repeat(54)).is_none());
    }

    #[test]
    fn legacy_skse_fixed_payload_round_trips_exactly() {
        let payload = LegacySkseModEventPayload::new(
            "page:selected".to_owned(),
            -13.25,
            Some(FormRef::new([0x5a; 16], 0x123456)),
        );
        let encoded = payload.encode().unwrap();
        assert_eq!(LegacySkseModEventPayload::decode(&encoded), Some(payload));
        assert!(LegacySkseModEventPayload::decode(&encoded[..encoded.len() - 1]).is_none());
        assert!(LegacySkseModEventPayload::decode(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 0]).is_none());
    }

    #[test]
    fn legacy_skse_dynamic_subscription_commands_are_bounded() {
        let subscribe = LegacyModEventSubscriptionCommand::subscribe(
            "SKICP_configManagerReady",
            "OnConfigManagerReady".to_owned(),
        )
        .unwrap();
        assert!(matches!(
            subscribe,
            LegacyModEventSubscriptionCommand::Subscribe { .. }
        ));
        assert!(LegacyModEventSubscriptionCommand::subscribe("ready", String::new()).is_none());
        assert!(LegacyModEventSubscriptionCommand::subscribe(
            "ready",
            "x".repeat(MAX_LEGACY_SKSE_CALLBACK_BYTES + 1),
        )
        .is_none());
        assert!(LegacyModEventSubscriptionCommand::unsubscribe("ready").is_some());
    }

    #[test]
    fn legacy_skse_handle_builder_sends_typed_arguments_and_releases() {
        let mut builders = LegacySkseModEventBuilders::new();
        let handle = builders.create("SKIWF_widgetLoaded");
        assert_ne!(handle, 0);
        builders.push(handle, LegacySkseModEventValue::Bool(true));
        builders.push(handle, LegacySkseModEventValue::Int(-12));
        builders.push(handle, LegacySkseModEventValue::float(3.25));
        builders.push(handle, LegacySkseModEventValue::String("ready".to_owned()));
        builders.push(
            handle,
            LegacySkseModEventValue::Form(Some(FormRef::new([7; 16], 0x123))),
        );

        let command = builders.send(handle).unwrap();
        assert!(builders.is_empty());
        assert_eq!(
            legacy_skse_mod_event_name(&command.event).as_deref(),
            Some("SKIWF_widgetLoaded")
        );
        let payload = LegacySkseVariadicModEventPayload::decode(&command.payload).unwrap();
        assert_eq!(payload.arguments.len(), 5);
        assert_eq!(payload.arguments[0], LegacySkseModEventValue::Bool(true));
        assert_eq!(payload.arguments[1], LegacySkseModEventValue::Int(-12));
        assert_eq!(payload.arguments[2].as_float(), Some(3.25));
        assert_eq!(
            payload.arguments[4],
            LegacySkseModEventValue::Form(Some(FormRef::new([7; 16], 0x123)))
        );
        assert!(builders.send(handle).is_none());
    }

    #[test]
    fn legacy_skse_handle_builder_bounds_handles_and_payloads() {
        let mut builders = LegacySkseModEventBuilders::new();
        assert_eq!(builders.create(""), 0);
        let handles = (0..MAX_LEGACY_SKSE_MOD_EVENT_BUILDERS)
            .map(|index| builders.create(&format!("event-{index}")))
            .collect::<Vec<_>>();
        assert!(handles.iter().all(|handle| *handle != 0));
        assert_eq!(builders.create("overflow"), 0);

        let handle = handles[0];
        builders.push(
            handle,
            LegacySkseModEventValue::String("x".repeat(MAX_CUSTOM_EVENT_PAYLOAD_BYTES)),
        );
        let command = builders.send(handle).unwrap();
        let payload = LegacySkseVariadicModEventPayload::decode(&command.payload).unwrap();
        assert!(payload.arguments.is_empty());

        let released = handles[1];
        builders.release(released);
        assert_eq!(builders.len(), MAX_LEGACY_SKSE_MOD_EVENT_BUILDERS - 2);
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
