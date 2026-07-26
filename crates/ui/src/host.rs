//! Scaleform host bridge built on Ruffle's ExternalInterface transport.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;

use ruffle_core::context::UpdateContext;
use ruffle_core::external::{ExternalInterfaceProvider, Value as ExternalValue};

use crate::{ScaleformHostCatalog, ScaleformHostMethodKind, ScaleformProfile};

type ResponseHandler = dyn Fn(&[ScaleformValue]) -> Vec<ScaleformValue>;

/// Value type shared between the engine and ActionScript.
#[derive(Clone, Debug, PartialEq)]
pub enum ScaleformValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Object(BTreeMap<String, ScaleformValue>),
    List(Vec<ScaleformValue>),
}

impl From<&ExternalValue> for ScaleformValue {
    fn from(value: &ExternalValue) -> Self {
        match value {
            ExternalValue::Undefined => Self::Undefined,
            ExternalValue::Null => Self::Null,
            ExternalValue::Bool(value) => Self::Bool(*value),
            ExternalValue::Number(value) => Self::Number(*value),
            ExternalValue::String(value) => Self::String(value.clone()),
            ExternalValue::Object(value) => Self::Object(
                value
                    .iter()
                    .map(|(key, value)| (key.clone(), Self::from(value)))
                    .collect(),
            ),
            ExternalValue::List(value) => {
                Self::List(value.iter().map(Self::from).collect::<Vec<_>>())
            }
        }
    }
}

impl From<ScaleformValue> for ExternalValue {
    fn from(value: ScaleformValue) -> Self {
        match value {
            ScaleformValue::Undefined => Self::Undefined,
            ScaleformValue::Null => Self::Null,
            ScaleformValue::Bool(value) => Self::Bool(value),
            ScaleformValue::Number(value) => Self::Number(value),
            ScaleformValue::String(value) => Self::String(value),
            ScaleformValue::Object(value) => Self::Object(
                value
                    .into_iter()
                    .map(|(key, value)| (key, Self::from(value)))
                    .collect(),
            ),
            ScaleformValue::List(value) => Self::List(value.into_iter().map(Self::from).collect()),
        }
    }
}

impl From<&str> for ScaleformValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<String> for ScaleformValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<bool> for ScaleformValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<f64> for ScaleformValue {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

/// A call made by ActionScript into the engine host.
#[derive(Clone, Debug, PartialEq)]
pub struct ScaleformHostCall {
    /// Monotonic sequence number within this player.
    pub sequence: u64,
    /// Runtime profile that produced the call.
    pub profile: ScaleformProfile,
    /// Raw ExternalInterface method used as the transport.
    pub transport_method: String,
    /// Logical engine method after profile-specific normalization.
    pub method: String,
    /// `GameDelegate` request identifier, when the profile uses one.
    pub request_id: Option<u64>,
    /// Logical method arguments.
    pub arguments: Vec<ScaleformValue>,
    /// How the bridge classified and routed this call.
    pub dispatch: ScaleformHostDispatch,
}

/// Result of routing a menu-to-host call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaleformHostDispatch {
    /// Known command queued for the engine.
    Queued,
    /// Response returned directly from `ExternalInterface.call`.
    ImmediateResponse,
    /// Response prepared for Skyrim's re-entrant `respond` callback.
    GameDelegateResponse,
    /// A cataloged request has no configured response.
    MissingResponse,
    /// Neither the profile catalog nor the application recognizes the method.
    Unknown,
}

#[derive(Default)]
struct BridgeState {
    next_sequence: u64,
    calls: VecDeque<ScaleformHostCall>,
    callbacks: BTreeSet<String>,
    known_methods: BTreeSet<String>,
    unknown_methods: BTreeSet<String>,
    unanswered_methods: BTreeSet<String>,
    responses: BTreeMap<String, Vec<ScaleformValue>>,
    response_handlers: BTreeMap<String, Rc<ResponseHandler>>,
}

/// Engine-side handle for a Ruffle ExternalInterface provider.
///
/// The handle is intentionally single-threaded: Ruffle's player is owned by
/// the main loop and is not `Send + Sync`.
#[derive(Clone)]
pub struct ScaleformHostBridge {
    profile: ScaleformProfile,
    catalog: ScaleformHostCatalog,
    state: Rc<RefCell<BridgeState>>,
}

impl ScaleformHostBridge {
    pub fn new(profile: ScaleformProfile) -> Self {
        Self {
            profile,
            catalog: ScaleformHostCatalog::for_profile(profile),
            state: Rc::new(RefCell::new(BridgeState::default())),
        }
    }

    pub const fn profile(&self) -> ScaleformProfile {
        self.profile
    }

    pub const fn catalog(&self) -> ScaleformHostCatalog {
        self.catalog
    }

    /// Register a host method that may be handled asynchronously by the engine.
    pub fn register_method(&self, method: impl Into<String>) {
        let method = method.into();
        let mut state = self.state.borrow_mut();
        state.unknown_methods.remove(&method);
        state.known_methods.insert(method);
    }

    /// Configure a constant synchronous response for a host method.
    ///
    /// Bethesda's menu protocol is mostly callback-driven, but a small number
    /// of ExternalInterface calls inspect their immediate return value.
    pub fn set_response(&self, method: impl Into<String>, response: ScaleformValue) {
        self.set_response_values(method, [response]);
    }

    /// Configure the values passed to a `GameDelegate` response callback.
    pub fn set_response_values(
        &self,
        method: impl Into<String>,
        responses: impl IntoIterator<Item = ScaleformValue>,
    ) {
        let method = method.into();
        let mut state = self.state.borrow_mut();
        state.unknown_methods.remove(&method);
        state.unanswered_methods.remove(&method);
        state.known_methods.insert(method.clone());
        state.response_handlers.remove(&method);
        state
            .responses
            .insert(method, responses.into_iter().collect());
    }

    /// Install a synchronous response handler for a host method.
    ///
    /// The handler runs on Ruffle's owning thread while
    /// `ExternalInterface.call` is active. Its returned values become either
    /// the direct return value or Skyrim's re-entrant `respond` arguments.
    pub fn set_response_handler(
        &self,
        method: impl Into<String>,
        handler: impl Fn(&[ScaleformValue]) -> Vec<ScaleformValue> + 'static,
    ) {
        let method = method.into();
        let mut state = self.state.borrow_mut();
        state.unknown_methods.remove(&method);
        state.unanswered_methods.remove(&method);
        state.known_methods.insert(method.clone());
        state.responses.remove(&method);
        state.response_handlers.insert(method, Rc::new(handler));
    }

    /// Drain calls made by ActionScript since the previous drain.
    pub fn drain_calls(&self) -> Vec<ScaleformHostCall> {
        self.state.borrow_mut().calls.drain(..).collect()
    }

    /// Names registered through `ExternalInterface.addCallback`.
    pub fn available_callbacks(&self) -> Vec<String> {
        self.state.borrow().callbacks.iter().cloned().collect()
    }

    pub fn has_callback(&self, name: &str) -> bool {
        self.state.borrow().callbacks.contains(name)
    }

    /// Host methods observed without a corresponding registration or response.
    pub fn unknown_methods(&self) -> Vec<String> {
        self.state
            .borrow()
            .unknown_methods
            .iter()
            .cloned()
            .collect()
    }

    /// Cataloged requests observed without a configured synchronous response.
    pub fn unanswered_methods(&self) -> Vec<String> {
        self.state
            .borrow()
            .unanswered_methods
            .iter()
            .cloned()
            .collect()
    }

    pub(crate) fn provider(&self) -> Box<dyn ExternalInterfaceProvider> {
        Box::new(BridgeProvider {
            bridge: self.clone(),
        })
    }

    fn record_call(&self, transport_method: &str, args: &[ExternalValue]) -> HostCallOutcome {
        let converted = args.iter().map(ScaleformValue::from).collect::<Vec<_>>();
        let normalized = self.normalize_call(transport_method, converted);
        let response_handler = self
            .state
            .borrow()
            .response_handlers
            .get(&normalized.method)
            .cloned();
        let dynamic_response = response_handler.map(|handler| handler(&normalized.arguments));

        let mut state = self.state.borrow_mut();
        let sequence = state.next_sequence;
        state.next_sequence += 1;

        let catalog_method = self.catalog.find(&normalized.method);
        let response =
            dynamic_response.or_else(|| state.responses.get(&normalized.method).cloned());
        let is_known = catalog_method.is_some() || state.known_methods.contains(&normalized.method);
        let dispatch = if response.is_some() {
            if normalized.request_id.is_some() {
                ScaleformHostDispatch::GameDelegateResponse
            } else {
                ScaleformHostDispatch::ImmediateResponse
            }
        } else if catalog_method
            .is_some_and(|method| method.kind == ScaleformHostMethodKind::Request)
        {
            state.unanswered_methods.insert(normalized.method.clone());
            ScaleformHostDispatch::MissingResponse
        } else if is_known {
            ScaleformHostDispatch::Queued
        } else {
            state.unknown_methods.insert(normalized.method.clone());
            ScaleformHostDispatch::Unknown
        };

        state.calls.push_back(ScaleformHostCall {
            sequence,
            profile: self.profile,
            transport_method: transport_method.to_string(),
            method: normalized.method.clone(),
            request_id: normalized.request_id,
            arguments: normalized.arguments,
            dispatch,
        });

        let mut response = response.map(|values| {
            values
                .into_iter()
                .map(ExternalValue::from)
                .collect::<Vec<_>>()
        });
        let callback_response = normalized.request_id.and_then(|request_id| {
            response.take().map(|mut values| {
                values.insert(0, ExternalValue::Number(request_id as f64));
                values
            })
        });
        let return_value = match response {
            Some(mut values) if values.len() == 1 => values.pop().unwrap(),
            Some(values) => ExternalValue::List(values),
            None => ExternalValue::Null,
        };

        HostCallOutcome {
            return_value,
            callback_response,
        }
    }

    fn normalize_call(
        &self,
        transport_method: &str,
        arguments: Vec<ScaleformValue>,
    ) -> NormalizedCall {
        if self.profile == ScaleformProfile::SkyrimAvm1 {
            if let Some((ScaleformValue::Number(request_id), rest)) = arguments.split_first() {
                if let Some(request_id) = numeric_request_id(*request_id) {
                    return NormalizedCall {
                        method: transport_method.to_string(),
                        request_id: Some(request_id),
                        arguments: rest.to_vec(),
                    };
                }
            }
        }

        NormalizedCall {
            method: transport_method.to_string(),
            request_id: None,
            arguments,
        }
    }
}

fn numeric_request_id(value: f64) -> Option<u64> {
    if value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value <= u64::MAX as f64 {
        Some(value as u64)
    } else {
        None
    }
}

struct NormalizedCall {
    method: String,
    request_id: Option<u64>,
    arguments: Vec<ScaleformValue>,
}

struct HostCallOutcome {
    return_value: ExternalValue,
    callback_response: Option<Vec<ExternalValue>>,
}

struct BridgeProvider {
    bridge: ScaleformHostBridge,
}

impl ExternalInterfaceProvider for BridgeProvider {
    fn call_method(
        &self,
        context: &mut UpdateContext<'_>,
        name: &str,
        args: &[ExternalValue],
    ) -> ExternalValue {
        let outcome = self.bridge.record_call(name, args);
        if let Some(arguments) = outcome.callback_response {
            if let Some(callback) = context.external_interface.get_callback("respond") {
                callback.call(context, "respond", arguments);
            }
        }
        outcome.return_value
    }

    fn on_callback_available(&self, name: &str) {
        self.bridge
            .state
            .borrow_mut()
            .callbacks
            .insert(name.to_string());
    }

    fn get_id(&self) -> Option<String> {
        Some(self.bridge.profile.external_interface_id().to_string())
    }
}

#[cfg(test)]
mod tests;
