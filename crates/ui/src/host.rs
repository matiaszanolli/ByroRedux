//! Scaleform host bridge built on Ruffle's ExternalInterface transport.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;

use ruffle_core::context::UpdateContext;
use ruffle_core::external::{ExternalInterfaceProvider, Value as ExternalValue};

use crate::ScaleformProfile;

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
    /// Logical method arguments.
    pub arguments: Vec<ScaleformValue>,
}

#[derive(Default)]
struct BridgeState {
    next_sequence: u64,
    calls: VecDeque<ScaleformHostCall>,
    callbacks: BTreeSet<String>,
    known_methods: BTreeSet<String>,
    unknown_methods: BTreeSet<String>,
    responses: BTreeMap<String, ScaleformValue>,
}

/// Engine-side handle for a Ruffle ExternalInterface provider.
///
/// The handle is intentionally single-threaded: Ruffle's player is owned by
/// the main loop and is not `Send + Sync`.
#[derive(Clone)]
pub struct ScaleformHostBridge {
    profile: ScaleformProfile,
    state: Rc<RefCell<BridgeState>>,
}

impl ScaleformHostBridge {
    pub fn new(profile: ScaleformProfile) -> Self {
        Self {
            profile,
            state: Rc::new(RefCell::new(BridgeState::default())),
        }
    }

    pub const fn profile(&self) -> ScaleformProfile {
        self.profile
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
        let method = method.into();
        let mut state = self.state.borrow_mut();
        state.unknown_methods.remove(&method);
        state.known_methods.insert(method.clone());
        state.responses.insert(method, response);
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

    pub(crate) fn provider(&self) -> Box<dyn ExternalInterfaceProvider> {
        Box::new(BridgeProvider {
            bridge: self.clone(),
        })
    }

    fn record_call(&self, transport_method: &str, args: &[ExternalValue]) -> ExternalValue {
        let converted = args.iter().map(ScaleformValue::from).collect::<Vec<_>>();
        let (method, arguments) = self.normalize_call(transport_method, converted);

        let mut state = self.state.borrow_mut();
        let sequence = state.next_sequence;
        state.next_sequence += 1;

        if !state.known_methods.contains(&method) {
            state.unknown_methods.insert(method.clone());
        }

        state.calls.push_back(ScaleformHostCall {
            sequence,
            profile: self.profile,
            transport_method: transport_method.to_string(),
            method: method.clone(),
            arguments,
        });

        state
            .responses
            .get(&method)
            .cloned()
            .map(ExternalValue::from)
            .unwrap_or(ExternalValue::Null)
    }

    fn normalize_call(
        &self,
        transport_method: &str,
        arguments: Vec<ScaleformValue>,
    ) -> (String, Vec<ScaleformValue>) {
        if self.profile == ScaleformProfile::SkyrimAvm1
            && transport_method.eq_ignore_ascii_case("call")
        {
            if let Some((ScaleformValue::String(method), rest)) = arguments.split_first() {
                return (method.clone(), rest.to_vec());
            }
        }

        (transport_method.to_string(), arguments)
    }
}

struct BridgeProvider {
    bridge: ScaleformHostBridge,
}

impl ExternalInterfaceProvider for BridgeProvider {
    fn call_method(
        &self,
        _context: &mut UpdateContext<'_>,
        name: &str,
        args: &[ExternalValue],
    ) -> ExternalValue {
        self.bridge.record_call(name, args)
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
