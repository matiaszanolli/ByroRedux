//! The intermediate representation both halves share.
//!
//! This is the seam the file was split on (#3852): `lower_program`
//! produces these types and `execute` consumes them, and nothing else
//! in the crate depends on both.

use super::*;

/// Canonical event subset currently executable by the provider runtime.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PapyrusProviderEvent {
    OnInit,
    OnLoad,
    OnActivate,
    OnHit,
    OnObjectEquipped,
    OnObjectUnequipped,
    OnTriggerEnter,
    OnUpdate,
}

/// One conservative instruction in a translated Papyrus handler.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum PapyrusProviderStatement {
    Declare {
        name: String,
        value: ScriptValue,
    },
    AssignCall {
        name: String,
        call: PapyrusProviderInvocation,
    },
    /// Evaluate a bounded scalar expression and assign its result to a local.
    /// Provider calls remain represented by [`Self::AssignCall`] so their
    /// route validation and saved shape stay explicit.
    AssignValue {
        name: String,
        value: PapyrusProviderValue,
        value_type: ScriptValueType,
    },
    /// Execute a native void call whose Papyrus array parameter is mutated by
    /// reference. The host callback returns the filled array as an internal
    /// transport value, which is written back to the named local.
    ArrayWritebackCall {
        name: String,
        call: PapyrusProviderInvocation,
    },
    Call(PapyrusProviderInvocation),
    RegisterModEvent {
        event_name: String,
        callback: String,
    },
    UnregisterModEvent {
        event_name: String,
    },
    UnregisterAllModEvents,
    SendModEvent {
        event_name: PapyrusProviderArgument,
        string_arg: PapyrusProviderArgument,
        number_arg: PapyrusProviderArgument,
        sender: PapyrusModEventSender,
    },
    Wait {
        seconds: f32,
    },
    If {
        condition: Box<PapyrusProviderCondition>,
        then_branch: Vec<PapyrusProviderStatement>,
        else_branch: Vec<PapyrusProviderStatement>,
    },
}

/// Sender projection required by SKSE's three instance-owned SendModEvent
/// surfaces. Form and Alias resolve through the attached entity; an active
/// magic effect intentionally publishes `None`, matching SKSE.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum PapyrusModEventSender {
    Owner,
    Resolved(Option<FormRef>),
}

#[derive(Clone, Debug)]
pub(crate) enum PapyrusModEventRegistrationAction {
    Register {
        event_name: String,
        callback: String,
    },
    Unregister {
        event_name: String,
    },
    UnregisterAll,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub(crate) struct PendingPapyrusProviderContinuation {
    pub(crate) remaining_seconds: f32,
    pub(crate) statements: Vec<PapyrusProviderStatement>,
    pub(crate) locals: BTreeMap<String, ScriptValue>,
    pub(crate) principal: Option<PrincipalId>,
}

/// Bounded latent tails for provider-bearing Papyrus event handlers.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct PapyrusProviderContinuationQueue {
    pub(crate) pending: Vec<PendingPapyrusProviderContinuation>,
}

impl Resource for PapyrusProviderContinuationQueue {}

impl PapyrusProviderContinuationQueue {
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// Transient per-script-instance SKSE-compatible ModEvent registrations and
/// deliveries. Scripts refresh registrations from `OnInit`/`OnLoad` after a
/// world replacement, matching the lifecycle contract documented by SKSE.
#[derive(Clone, Debug, Default)]
pub struct PapyrusModEventRuntime {
    pub(crate) registrations:
        BTreeMap<(EntityId, PrincipalId, byroredux_sdk::identity::EventId), String>,
    pub(crate) pending: Vec<CustomEvent>,
}

impl Resource for PapyrusModEventRuntime {}

/// Queue one already-validated shared ModEvent for Papyrus delivery.
pub fn queue_papyrus_mod_event(world: &World, event: CustomEvent) {
    if !event.is_valid() {
        log::warn!("invalid Papyrus ModEvent delivery was rejected");
        return;
    }
    if let Some(mut runtime) = world.try_resource_mut::<PapyrusModEventRuntime>() {
        if runtime.pending.len() < MAX_PENDING_PAPYRUS_MOD_EVENTS {
            runtime.pending.push(event);
        } else {
            log::warn!(
                "Papyrus ModEvent delivery limit of {MAX_PENDING_PAPYRUS_MOD_EVENTS} exceeded"
            );
        }
    }
}

/// Boolean expression subset used to select a translated branch.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum PapyrusProviderCondition {
    Literal(bool),
    Local(String),
    Call(PapyrusProviderInvocation),
    Not(Box<PapyrusProviderCondition>),
    And(Box<PapyrusProviderCondition>, Box<PapyrusProviderCondition>),
    Or(Box<PapyrusProviderCondition>, Box<PapyrusProviderCondition>),
    Compare {
        left: Box<PapyrusProviderValue>,
        operator: PapyrusProviderComparison,
        right: Box<PapyrusProviderValue>,
    },
}

/// Scalar expression accepted on either side of a translated comparison.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum PapyrusProviderValue {
    Literal(ScriptValue),
    Local(String),
    Call(PapyrusProviderInvocation),
    Binary {
        left: Box<PapyrusProviderValue>,
        operator: PapyrusProviderArithmetic,
        right: Box<PapyrusProviderValue>,
    },
}

/// Same-type scalar operations that can execute inside a provider-bearing
/// handler. Numeric operands are deliberately not coerced across integer and
/// float domains; the Papyrus source type must be unambiguous at lowering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum PapyrusProviderArithmetic {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    StrCat,
}

/// Same-type comparison operations executable by the provider runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum PapyrusProviderComparison {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

/// Static translated handlers attached to one scripted entity.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PapyrusProviderProgram {
    pub(crate) handlers: BTreeMap<PapyrusProviderEvent, Vec<PapyrusProviderHandler>>,
    pub(crate) custom_handlers: BTreeMap<String, Vec<PapyrusProviderHandler>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PapyrusProviderHandler {
    pub(crate) statements: Vec<PapyrusProviderStatement>,
    pub(crate) parameters: Vec<PapyrusProviderParameterBinding>,
    pub(crate) principal: Option<PrincipalId>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PapyrusProviderParameterBinding {
    pub(crate) name: String,
    pub(crate) source: PapyrusProviderParameterSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PapyrusProviderLocalType {
    pub(crate) value_type: ScriptValueType,
    /// Case-folded Papyrus object name when this local came from `Type::Object`.
    pub(crate) object_type: Option<String>,
}

impl PapyrusProviderLocalType {
    pub(crate) fn scalar(value_type: ScriptValueType) -> Self {
        Self {
            value_type,
            object_type: None,
        }
    }

    pub(crate) fn object(value_type: ScriptValueType, object_type: &str) -> Self {
        Self {
            value_type,
            object_type: Some(object_type.to_ascii_lowercase()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PapyrusProviderParameterSource {
    Entity,
    Form,
    PowerAttack,
    SneakAttack,
    BashAttack,
    Blocked,
    ModEventArgument {
        index: usize,
        value_type: ScriptValueType,
    },
}

#[derive(Default)]
pub(crate) struct PapyrusProviderProjectedLocals {
    pub(crate) values: BTreeMap<String, ScriptValue>,
    pub(crate) entities: Vec<(String, EntityId)>,
    pub(crate) forms: Vec<(String, u32)>,
}

impl Component for PapyrusProviderProgram {
    type Storage = SparseSetStorage<Self>;
}

impl PapyrusProviderProgram {
    /// Instructions for one canonical event.
    pub fn handler(&self, event: PapyrusProviderEvent) -> &[PapyrusProviderStatement] {
        self.handlers
            .get(&event)
            .and_then(|handlers| handlers.first())
            .map_or(&[], |handler| handler.statements.as_slice())
    }

    /// Whether no supported handler was present in the source unit.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty() && self.custom_handlers.is_empty()
    }

    pub(crate) fn handlers_for(
        &self,
        event: PapyrusProviderEvent,
    ) -> impl Iterator<Item = &PapyrusProviderHandler> {
        self.handlers.get(&event).into_iter().flatten()
    }

    pub(crate) fn merge(&mut self, mut other: Self) {
        for (event, mut handlers) in std::mem::take(&mut other.handlers) {
            self.handlers
                .entry(event)
                .or_default()
                .append(&mut handlers);
        }
        for (callback, mut handlers) in std::mem::take(&mut other.custom_handlers) {
            self.custom_handlers
                .entry(callback)
                .or_default()
                .append(&mut handlers);
        }
    }

    pub(crate) fn set_principal(&mut self, principal: PrincipalId) {
        for handlers in self.handlers.values_mut() {
            for handler in handlers {
                handler.principal = Some(principal.clone());
            }
        }
        for handlers in self.custom_handlers.values_mut() {
            for handler in handlers {
                handler.principal = Some(principal.clone());
            }
        }
    }
}

impl PapyrusProviderHandler {
    pub(crate) fn projected_locals(
        &self,
        entity: Option<EntityId>,
        hit: Option<&HitEvent>,
        form: Option<u32>,
    ) -> PapyrusProviderProjectedLocals {
        let mut projected = PapyrusProviderProjectedLocals::default();
        for parameter in &self.parameters {
            let value = match parameter.source {
                PapyrusProviderParameterSource::Entity => {
                    if let Some(entity) = entity {
                        projected.entities.push((parameter.name.clone(), entity));
                    }
                    continue;
                }
                PapyrusProviderParameterSource::Form => {
                    if let Some(form) = form {
                        projected.forms.push((parameter.name.clone(), form));
                    }
                    continue;
                }
                PapyrusProviderParameterSource::PowerAttack => {
                    hit.is_some_and(|hit| hit.power_attack)
                }
                PapyrusProviderParameterSource::SneakAttack => {
                    hit.is_some_and(|hit| hit.sneak_attack)
                }
                PapyrusProviderParameterSource::BashAttack => {
                    hit.is_some_and(|hit| hit.bash_attack)
                }
                PapyrusProviderParameterSource::Blocked => hit.is_some_and(|hit| hit.blocked),
                PapyrusProviderParameterSource::ModEventArgument { .. } => continue,
            };
            projected
                .values
                .insert(parameter.name.clone(), ScriptValue::Boolean(value));
        }
        projected
    }

    pub(crate) fn projected_mod_event_locals(
        &self,
        payload: &LegacySkseVariadicModEventPayload,
    ) -> Option<BTreeMap<String, ScriptValue>> {
        let mut locals = BTreeMap::new();
        for parameter in &self.parameters {
            let PapyrusProviderParameterSource::ModEventArgument { index, value_type } =
                parameter.source
            else {
                return None;
            };
            let argument = payload.arguments.get(index)?;
            let value = match (argument, value_type) {
                (LegacySkseModEventValue::Bool(value), ScriptValueType::Boolean) => {
                    ScriptValue::Boolean(*value)
                }
                (LegacySkseModEventValue::Int(value), ScriptValueType::Integer) => {
                    ScriptValue::Integer(i64::from(*value))
                }
                (LegacySkseModEventValue::FloatBits(bits), ScriptValueType::Float) => {
                    ScriptValue::Float(f32::from_bits(*bits))
                }
                (LegacySkseModEventValue::String(value), ScriptValueType::String) => {
                    ScriptValue::String(value.clone())
                }
                (LegacySkseModEventValue::Form(Some(value)), ScriptValueType::Form) => {
                    ScriptValue::Form(*value)
                }
                (LegacySkseModEventValue::Form(None), ScriptValueType::Form) => ScriptValue::None,
                _ => return None,
            };
            locals.insert(parameter.name.clone(), value);
        }
        (payload.arguments.len() == self.parameters.len()).then_some(locals)
    }
}
