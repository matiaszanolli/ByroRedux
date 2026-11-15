//! The SKSE-style mod-event surface.

use super::*;

pub const PAPYRUS_MOD_EVENT_ROUTE_PREFIX: &str = "byro.events.compat.mod-event.";

fn papyrus_mod_event_declaration(
    function: &str,
    parameters: &[(&str, ScriptValueType, bool)],
    result: Option<ScriptValueType>,
) -> EnginePapyrusFunctionDeclaration {
    let operation = match function {
        "Create" => "create",
        "Send" => "send",
        "Release" => "release",
        "PushBool" => "push-bool",
        "PushInt" => "push-int",
        "PushFloat" => "push-float",
        "PushString" => "push-string",
        "PushForm" => "push-form",
        _ => unreachable!("ModEvent declarations are a closed built-in set"),
    };
    let id = format!("mod-event-{operation}");
    BuiltinDeclaration {
        route: format!("{PAPYRUS_MOD_EVENT_ROUTE_PREFIX}{id}"),
        id: &id,
        component: "legacy-mod-events",
        provider: "ModEvent",
        function,
        parameters,
        result,
        description: "Engine-owned principal-private SKSE ModEvent builder compatibility",
        label: "ModEvent",
    }
    .build()
}

/// Exact SKSE `ModEvent` handle-builder calls backed by the shared engine bus.
pub fn papyrus_mod_event_declarations() -> Vec<EnginePapyrusFunctionDeclaration> {
    let integer = ScriptValueType::Integer;
    let handle = ("handle", integer, false);
    vec![
        papyrus_mod_event_declaration(
            "Create",
            &[("event-name", ScriptValueType::String, false)],
            Some(integer),
        ),
        papyrus_mod_event_declaration("Send", &[handle], Some(ScriptValueType::Boolean)),
        papyrus_mod_event_declaration("Release", &[handle], None),
        papyrus_mod_event_declaration(
            "PushBool",
            &[handle, ("value", ScriptValueType::Boolean, false)],
            None,
        ),
        papyrus_mod_event_declaration("PushInt", &[handle, ("value", integer, false)], None),
        papyrus_mod_event_declaration(
            "PushFloat",
            &[handle, ("value", ScriptValueType::Float, false)],
            None,
        ),
        papyrus_mod_event_declaration(
            "PushString",
            &[handle, ("value", ScriptValueType::String, false)],
            None,
        ),
        papyrus_mod_event_declaration(
            "PushForm",
            &[handle, ("value", ScriptValueType::Form, true)],
            None,
        ),
    ]
}

/// Failure to adapt SKSE's fixed-arity `SendModEvent` call.
#[derive(Clone, Debug, Eq, thiserror::Error, PartialEq)]
pub enum LegacyModEventAdapterError {
    #[error("legacy mod-event name is empty or exceeds the reversible engine bound")]
    InvalidEventName,
    #[error("legacy mod-event payload exceeds the bounded engine event contract")]
    PayloadTooLarge,
}

/// Adapt `Form`/`Alias`/`ActiveMagicEffect.SendModEvent` to the shared,
/// engine-owned event service.
pub fn adapt_legacy_send_mod_event(
    event_name: &str,
    string_arg: String,
    number_arg: f32,
    sender: Option<FormRef>,
) -> Result<PublishEventCommand, LegacyModEventAdapterError> {
    let event =
        legacy_skse_mod_event_id(event_name).ok_or(LegacyModEventAdapterError::InvalidEventName)?;
    let payload = LegacySkseModEventPayload::new(string_arg, number_arg, sender)
        .encode()
        .ok_or(LegacyModEventAdapterError::PayloadTooLarge)?;
    PublishEventCommand::new(event, payload).ok_or(LegacyModEventAdapterError::PayloadTooLarge)
}
