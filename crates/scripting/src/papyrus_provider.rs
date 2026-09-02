//! Conservative lowering for manifest-published Papyrus provider functions.
//!
//! This module is intentionally host-neutral. It resolves a legal
//! `Provider.Function(...)` or reserved `self.Function(...)` spelling to the
//! principal-qualified SDK route and validates typed arguments, but it never
//! enters Wasm or touches the ECS while lowering.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::{Resource, World};
use byroredux_papyrus::ast::{
    AssignOp, BinaryOp, CallArg, Event, Expr, Script, ScriptItem, StateItem, Stmt, Type, UnaryOp,
};
use byroredux_sdk::{
    compatibility::{
        adapt_legacy_send_mod_event, classify_static_call, papyrus_game_content_declarations,
        papyrus_input_declarations, papyrus_legacy_container_declarations,
        papyrus_mod_event_declarations, papyrus_storage_util_declarations, papyrus_ui_declarations,
        parse_storage_util_list_route, parse_storage_util_prefix_route, StorageUtilListOperation,
        PAPYRUS_LEGACY_CONTAINERS_ROUTE_PREFIX, PAPYRUS_MOD_EVENT_ROUTE_PREFIX,
        PAPYRUS_STORAGE_UTIL_ADJUST_FLOAT_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_ADJUST_INT_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_GET_FLOAT_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_GET_FORM_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_GET_INT_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_GET_STRING_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_HAS_FLOAT_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_HAS_FORM_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_HAS_INT_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_HAS_STRING_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_PLUCK_FLOAT_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_PLUCK_FORM_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_PLUCK_INT_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_PLUCK_STRING_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_SET_FLOAT_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_SET_FORM_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_SET_INT_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_SET_STRING_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_UNSET_FLOAT_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_UNSET_FORM_VALUE_ROUTE,
        PAPYRUS_STORAGE_UTIL_UNSET_INT_VALUE_ROUTE, PAPYRUS_STORAGE_UTIL_UNSET_STRING_VALUE_ROUTE,
    },
    event::{
        CustomEvent, LegacyModEventSubscriptionCommand, LegacySkseModEventValue,
        LegacySkseVariadicModEventPayload, PublishEventCommand,
    },
    identity::{EntityRef, ExtensionId, FormRef, PrincipalId},
    script_function::{
        ScriptFunctionDeclaration, ScriptFunctionError, ScriptResultDeclaration, ScriptValue,
        ScriptValueType, MAX_SCRIPT_ARRAY_ELEMENTS,
    },
};

use crate::events::{
    ActivateEvent, EquipmentEventBatch, HitEvent, OnCellLoadEvent, OnInitEvent, OnTriggerEnterEvent,
};
use crate::recurring_update::OnUpdateEvent;

const MAX_PROVIDER_HANDLER_NESTING: usize = 32;
const MAX_PROVIDER_CONTINUATIONS: usize = 4_096;
const MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS: usize = 4_096;
const MAX_PENDING_PAPYRUS_MOD_EVENTS: usize = 256;
const PAPYRUS_SELF_PROVIDER: &str = "Self";
const PAPYRUS_SELF_LOCAL: &str = "self";

/// Host callback shared by Papyrus handlers after all ECS guards are dropped.
pub type PapyrusProviderCallback =
    dyn Fn(Option<&PrincipalId>, &str, &[ScriptValue]) -> Result<ScriptValue, String> + Send + Sync;

/// Executable-owned conversion from a raw ECS identity to the same opaque,
/// generational handle used by sandbox callbacks.
pub type PapyrusProviderEntityResolver =
    dyn Fn(EntityId) -> Result<EntityRef, String> + Send + Sync;

/// Executable-owned conversion from a remapped global FormID to portable SDK
/// identity. Unlike entity handles, resolved forms are safe to persist.
pub type PapyrusProviderFormResolver = dyn Fn(u32) -> Result<FormRef, String> + Send + Sync;

/// Executable-owned bridge into the shared custom-event queue. The command is
/// already shaped as the engine SDK event contract; the callback only adds the
/// authenticated legacy-script principal and enforces host queue limits.
pub type PapyrusProviderModEventPublisher =
    dyn Fn(&PrincipalId, PublishEventCommand) -> Result<(), String> + Send + Sync;

/// Live catalog and host callback published atomically by the executable.
#[derive(Clone)]
pub struct PapyrusProviderRuntime {
    catalog: Arc<PapyrusProviderCatalog>,
    callback: Option<Arc<PapyrusProviderCallback>>,
    entity_resolver: Option<Arc<PapyrusProviderEntityResolver>>,
    form_resolver: Option<Arc<PapyrusProviderFormResolver>>,
    mod_event_publisher: Option<Arc<PapyrusProviderModEventPublisher>>,
}

impl Resource for PapyrusProviderRuntime {}

impl Default for PapyrusProviderRuntime {
    fn default() -> Self {
        Self {
            catalog: Arc::new(PapyrusProviderCatalog::engine_compatibility()),
            callback: None,
            entity_resolver: None,
            form_resolver: None,
            mod_event_publisher: None,
        }
    }
}

impl PapyrusProviderRuntime {
    /// Immutable manifest-backed alias catalog used during script lowering.
    pub fn catalog(&self) -> Arc<PapyrusProviderCatalog> {
        Arc::clone(&self.catalog)
    }

    /// Clone the live host callback for guard-free execution.
    pub fn callback(&self) -> Option<Arc<PapyrusProviderCallback>> {
        self.callback.clone()
    }

    pub fn entity_resolver(&self) -> Option<Arc<PapyrusProviderEntityResolver>> {
        self.entity_resolver.clone()
    }

    pub fn form_resolver(&self) -> Option<Arc<PapyrusProviderFormResolver>> {
        self.form_resolver.clone()
    }

    pub fn mod_event_publisher(&self) -> Option<Arc<PapyrusProviderModEventPublisher>> {
        self.mod_event_publisher.clone()
    }
}

/// Install the executable's opaque-handle converter independently of the
/// provider callback so test and headless runtimes can omit reference payloads.
pub fn set_papyrus_provider_entity_resolver(
    world: &World,
    resolver: Option<Arc<PapyrusProviderEntityResolver>>,
) {
    if let Some(mut runtime) = world.try_resource_mut::<PapyrusProviderRuntime>() {
        runtime.entity_resolver = resolver;
    }
}

/// Install the executable's stable authored-form converter independently of
/// the provider callback so built-in compatibility routes can use it too.
pub fn set_papyrus_provider_form_resolver(
    world: &World,
    resolver: Option<Arc<PapyrusProviderFormResolver>>,
) {
    if let Some(mut runtime) = world.try_resource_mut::<PapyrusProviderRuntime>() {
        runtime.form_resolver = resolver;
    }
}

/// Install the executable's shared ModEvent publisher independently of static
/// provider dispatch so headless runtimes can intentionally omit event I/O.
pub fn set_papyrus_provider_mod_event_publisher(
    world: &World,
    publisher: Option<Arc<PapyrusProviderModEventPublisher>>,
) {
    if let Some(mut runtime) = world.try_resource_mut::<PapyrusProviderRuntime>() {
        runtime.mod_event_publisher = publisher;
    }
}

/// Replace the live Papyrus provider surface after extension activation.
pub fn set_papyrus_provider_runtime(
    world: &World,
    catalog: Arc<PapyrusProviderCatalog>,
    callback: Option<Arc<PapyrusProviderCallback>>,
) {
    if let Some(mut runtime) = world.try_resource_mut::<PapyrusProviderRuntime>() {
        runtime.catalog = catalog;
        runtime.callback = callback;
    }
}

/// Register the provider runtime resource before any extension is loaded.
pub(crate) fn register(world: &mut World) {
    world.insert_resource(PapyrusProviderRuntime::default());
    world.insert_resource(PapyrusProviderContinuationQueue::default());
    world.insert_resource(PapyrusModEventRuntime::default());
    world.register::<PapyrusProviderProgram>();
}

/// One manifest-published route addressable by Papyrus source or PEX.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct PapyrusProviderRoute {
    qualified_name: String,
    declaration: ScriptFunctionDeclaration,
}

impl PapyrusProviderRoute {
    /// Principal-qualified engine route used for authenticated dispatch.
    pub fn qualified_name(&self) -> &str {
        &self.qualified_name
    }

    /// Validated SDK declaration backing this route.
    pub fn declaration(&self) -> &ScriptFunctionDeclaration {
        &self.declaration
    }
}

/// Case-insensitive provider/function catalog projected from installed
/// extension manifests.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PapyrusProviderCatalog {
    providers: BTreeSet<String>,
    routes: BTreeMap<(String, String), PapyrusProviderRoute>,
}

impl PapyrusProviderCatalog {
    /// Catalog of exact extender-era aliases implemented by engine services.
    pub fn engine_compatibility() -> Self {
        let mut catalog = Self::default();
        for function in papyrus_game_content_declarations() {
            catalog
                .insert_route(function.route.to_owned(), &function.declaration, false)
                .expect("built-in Papyrus compatibility declaration is valid");
        }
        for function in papyrus_input_declarations() {
            catalog
                .insert_route(function.route.to_owned(), &function.declaration, false)
                .expect("built-in Input compatibility declaration is valid");
        }
        for function in papyrus_ui_declarations() {
            catalog
                .insert_route(function.route.to_owned(), &function.declaration, false)
                .expect("built-in UI compatibility declaration is valid");
        }
        for function in papyrus_storage_util_declarations() {
            catalog
                .insert_route(function.route.to_owned(), &function.declaration, false)
                .expect("built-in StorageUtil compatibility declaration is valid");
        }
        for function in papyrus_legacy_container_declarations() {
            catalog
                .insert_route(function.route, &function.declaration, false)
                .expect("built-in JContainers compatibility declaration is valid");
        }
        for function in papyrus_mod_event_declarations() {
            catalog
                .insert_route(function.route, &function.declaration, false)
                .expect("built-in ModEvent compatibility declaration is valid");
        }
        catalog
    }

    /// Insert one declared function when it publishes a Papyrus alias.
    ///
    /// The operation is atomic: a duplicate alias or invalid declaration does
    /// not modify the catalog.
    pub fn insert(
        &mut self,
        extension: &ExtensionId,
        declaration: &ScriptFunctionDeclaration,
    ) -> Result<(), PapyrusProviderCatalogError> {
        self.insert_route(declaration.qualified_name(extension), declaration, true)
    }

    fn insert_route(
        &mut self,
        qualified_name: String,
        declaration: &ScriptFunctionDeclaration,
        strict_provider: bool,
    ) -> Result<(), PapyrusProviderCatalogError> {
        declaration
            .validate()
            .map_err(PapyrusProviderCatalogError::InvalidDeclaration)?;
        let Some(alias) = declaration.papyrus.as_ref() else {
            return Ok(());
        };
        let key = alias.canonical_key();
        if self.routes.contains_key(&key) {
            return Err(PapyrusProviderCatalogError::DuplicateAlias {
                provider: alias.provider.clone(),
                function: alias.function.clone(),
            });
        }
        let route = PapyrusProviderRoute {
            qualified_name,
            declaration: declaration.clone(),
        };
        if strict_provider {
            self.providers.insert(key.0.clone());
        }
        self.routes.insert(key, route);
        Ok(())
    }

    /// Resolve a Papyrus spelling using the language's case-insensitive rules.
    pub fn resolve(&self, provider: &str, function: &str) -> Option<&PapyrusProviderRoute> {
        self.routes
            .get(&(provider.to_ascii_lowercase(), function.to_ascii_lowercase()))
    }

    fn contains_provider(&self, provider: &str) -> bool {
        self.providers.contains(&provider.to_ascii_lowercase())
    }
}

/// A fully resolved, typed SDK call safe to hand to the extension host.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct TypedPapyrusProviderCall {
    pub route: PapyrusProviderRoute,
    pub arguments: Vec<ScriptValue>,
    pub result: Option<ScriptResultDeclaration>,
}

/// One handler argument resolved either at translation time or from a typed
/// local when the event executes.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum PapyrusProviderArgument {
    Literal(ScriptValue),
    Local {
        name: String,
        value_type: ScriptValueType,
    },
}

/// A provider call embedded in an event handler. Fragment calls continue to
/// use [`TypedPapyrusProviderCall`] and therefore remain literal-only.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct PapyrusProviderInvocation {
    pub route: PapyrusProviderRoute,
    /// Engine-owned receiver for a reserved `self.Method(...)` call. The SDK
    /// declaration includes this as its required first `Entity` parameter.
    pub receiver: Option<Box<PapyrusProviderArgument>>,
    pub arguments: Vec<PapyrusProviderArgument>,
    pub result: Option<ScriptResultDeclaration>,
}

/// Catalog construction failure detected before scripts are attached.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PapyrusProviderCatalogError {
    InvalidDeclaration(ScriptFunctionError),
    DuplicateAlias { provider: String, function: String },
}

/// A recognized provider call whose complete typed lowering was unsafe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PapyrusProviderLowerError {
    UnknownFunction { provider: String, function: String },
    PositionalAfterNamed,
    UnknownParameter(String),
    DuplicateParameter(String),
    MissingParameter(String),
    TooManyArguments,
    UnsupportedArgument { parameter: String },
    InvalidArguments(ScriptFunctionError),
}

/// Lower one exact static call. Non-provider expressions return `Ok(None)`;
/// once a known provider is named, every mismatch is an explicit error so a
/// translator cannot silently install a partial handler.
pub fn lower_provider_call(
    expression: &Expr,
    catalog: &PapyrusProviderCatalog,
) -> Result<Option<TypedPapyrusProviderCall>, PapyrusProviderLowerError> {
    let Expr::Call { callee, args } = expression else {
        return Ok(None);
    };
    let Expr::MemberAccess { object, member } = &callee.node else {
        return Ok(None);
    };
    let Expr::Ident(provider) = &object.node else {
        return Ok(None);
    };
    let Some(route) = catalog.resolve(&provider.0, &member.node.0) else {
        if is_known_provider_call(&provider.0, &member.node.0, catalog) {
            return Err(PapyrusProviderLowerError::UnknownFunction {
                provider: provider.0.clone(),
                function: member.node.0.clone(),
            });
        }
        return Ok(None);
    };

    let arguments = lower_arguments(args, route.declaration())?;
    validate_storage_util_literals(route.qualified_name(), &arguments)?;
    validate_legacy_container_arity(route.qualified_name(), arguments.len())?;
    validate_mod_event_arity(route.qualified_name(), arguments.len())?;
    Ok(Some(TypedPapyrusProviderCall {
        route: route.clone(),
        arguments,
        result: route.declaration().result,
    }))
}

fn storage_util_arity(route: &str) -> Option<(usize, usize)> {
    if parse_storage_util_prefix_route(route).is_some() {
        return Some((1, 1));
    }
    if let Some((_, operation)) = parse_storage_util_list_route(route) {
        return Some(match operation {
            StorageUtilListOperation::Add => (3, 4),
            StorageUtilListOperation::Set
            | StorageUtilListOperation::Insert
            | StorageUtilListOperation::Adjust => (4, 4),
            StorageUtilListOperation::Pluck
            | StorageUtilListOperation::Remove
            | StorageUtilListOperation::CountValue
            | StorageUtilListOperation::Resize
            | StorageUtilListOperation::Slice
            | StorageUtilListOperation::FilterByType
            | StorageUtilListOperation::FilterByTypes => (3, 4),
            StorageUtilListOperation::Get
            | StorageUtilListOperation::RemoveAt
            | StorageUtilListOperation::Find
            | StorageUtilListOperation::Has
            | StorageUtilListOperation::Copy => (3, 3),
            StorageUtilListOperation::Shift
            | StorageUtilListOperation::Pop
            | StorageUtilListOperation::Random
            | StorageUtilListOperation::ToArray
            | StorageUtilListOperation::Sort
            | StorageUtilListOperation::Count
            | StorageUtilListOperation::Clear => (2, 2),
        });
    }
    match route {
        PAPYRUS_STORAGE_UTIL_GET_INT_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_GET_FLOAT_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_GET_STRING_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_GET_FORM_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_PLUCK_INT_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_PLUCK_FLOAT_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_PLUCK_STRING_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_PLUCK_FORM_VALUE_ROUTE => Some((2, 3)),
        PAPYRUS_STORAGE_UTIL_SET_INT_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_SET_FLOAT_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_SET_STRING_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_SET_FORM_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_ADJUST_INT_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_ADJUST_FLOAT_VALUE_ROUTE => Some((3, 3)),
        PAPYRUS_STORAGE_UTIL_HAS_INT_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_HAS_FLOAT_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_HAS_STRING_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_HAS_FORM_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_UNSET_INT_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_UNSET_FLOAT_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_UNSET_STRING_VALUE_ROUTE
        | PAPYRUS_STORAGE_UTIL_UNSET_FORM_VALUE_ROUTE => Some((2, 2)),
        _ => None,
    }
}

fn validate_storage_util_literals(
    route: &str,
    arguments: &[ScriptValue],
) -> Result<(), PapyrusProviderLowerError> {
    let Some((minimum, maximum)) = storage_util_arity(route) else {
        return Ok(());
    };
    if !(minimum..=maximum).contains(&arguments.len()) {
        return Err(PapyrusProviderLowerError::MissingParameter(
            "StorageUtil exact signature".to_owned(),
        ));
    }
    if parse_storage_util_prefix_route(route).is_none()
        && arguments.first() != Some(&ScriptValue::None)
    {
        return Err(PapyrusProviderLowerError::UnsupportedArgument {
            parameter: "object".to_owned(),
        });
    }
    Ok(())
}

fn validate_storage_util_arguments(
    route: &str,
    arguments: &[PapyrusProviderArgument],
) -> Result<(), PapyrusProviderLowerError> {
    let Some((minimum, maximum)) = storage_util_arity(route) else {
        return Ok(());
    };
    if !(minimum..=maximum).contains(&arguments.len()) {
        return Err(PapyrusProviderLowerError::MissingParameter(
            "StorageUtil exact signature".to_owned(),
        ));
    }
    if parse_storage_util_prefix_route(route).is_none()
        && !matches!(
            arguments.first(),
            Some(PapyrusProviderArgument::Literal(ScriptValue::None))
        )
    {
        return Err(PapyrusProviderLowerError::UnsupportedArgument {
            parameter: "object".to_owned(),
        });
    }
    Ok(())
}

fn legacy_container_arity(route: &str) -> Option<(usize, usize)> {
    let function = route.strip_prefix(PAPYRUS_LEGACY_CONTAINERS_ROUTE_PREFIX)?;
    Some(match function {
        "jarray-object" | "jmap-object" => (0, 0),
        "jvalue-is-exists"
        | "jvalue-is-array"
        | "jvalue-is-map"
        | "jvalue-empty"
        | "jvalue-count"
        | "jvalue-clear"
        | "jvalue-shallow-copy"
        | "jvalue-deep-copy"
        | "jvalue-release"
        | "jvalue-release-objects-with-tag"
        | "jarray-count"
        | "jarray-clear"
        | "jmap-count"
        | "jmap-clear" => (1, 1),
        "jvalue-retain" => (1, 2),
        "jvalue-release-and-retain" => (2, 3),
        "jarray-erase-index" | "jmap-has-key" | "jmap-remove-key" => (2, 2),
        function if function.starts_with("jarray-add-") => (2, 3),
        function if function.starts_with("jarray-get-") => (2, 3),
        function if function.starts_with("jarray-set-") => (3, 3),
        function if function.starts_with("jmap-get-") => (2, 3),
        function if function.starts_with("jmap-set-") => (3, 3),
        _ => return None,
    })
}

fn validate_legacy_container_arity(
    route: &str,
    argument_count: usize,
) -> Result<(), PapyrusProviderLowerError> {
    let Some((minimum, maximum)) = legacy_container_arity(route) else {
        return Ok(());
    };
    if !(minimum..=maximum).contains(&argument_count) {
        return Err(PapyrusProviderLowerError::MissingParameter(
            "JContainers exact signature".to_owned(),
        ));
    }
    Ok(())
}

fn validate_mod_event_arity(
    route: &str,
    argument_count: usize,
) -> Result<(), PapyrusProviderLowerError> {
    let Some(function) = route.strip_prefix(PAPYRUS_MOD_EVENT_ROUTE_PREFIX) else {
        return Ok(());
    };
    let expected = match function {
        "mod-event-create" | "mod-event-send" | "mod-event-release" => 1,
        "mod-event-push-bool"
        | "mod-event-push-int"
        | "mod-event-push-float"
        | "mod-event-push-string"
        | "mod-event-push-form" => 2,
        _ => return Ok(()),
    };
    if argument_count != expected {
        return Err(PapyrusProviderLowerError::MissingParameter(
            "ModEvent exact signature".to_owned(),
        ));
    }
    Ok(())
}

fn is_known_provider_call(
    provider: &str,
    function: &str,
    catalog: &PapyrusProviderCatalog,
) -> bool {
    catalog.contains_provider(provider) || classify_static_call(provider, function).is_some()
}

fn lower_arguments(
    args: &[CallArg],
    declaration: &ScriptFunctionDeclaration,
) -> Result<Vec<ScriptValue>, PapyrusProviderLowerError> {
    let ordered = lower_ordered_arguments(args, declaration, |expression, parameter| {
        lower_literal(expression, parameter.value_type, parameter.optional).ok_or_else(|| {
            PapyrusProviderLowerError::UnsupportedArgument {
                parameter: parameter.id.as_str().to_owned(),
            }
        })
    })?;
    declaration
        .validate_arguments(&ordered)
        .map_err(PapyrusProviderLowerError::InvalidArguments)?;
    Ok(ordered)
}

fn lower_ordered_arguments<T>(
    args: &[CallArg],
    declaration: &ScriptFunctionDeclaration,
    lower: impl FnMut(
        &Expr,
        &byroredux_sdk::script_function::ScriptParameterDeclaration,
    ) -> Result<T, PapyrusProviderLowerError>,
) -> Result<Vec<T>, PapyrusProviderLowerError> {
    lower_ordered_arguments_from(args, declaration, 0, lower)
}

fn lower_ordered_arguments_from<T>(
    args: &[CallArg],
    declaration: &ScriptFunctionDeclaration,
    parameter_offset: usize,
    mut lower: impl FnMut(
        &Expr,
        &byroredux_sdk::script_function::ScriptParameterDeclaration,
    ) -> Result<T, PapyrusProviderLowerError>,
) -> Result<Vec<T>, PapyrusProviderLowerError> {
    let parameters = declaration
        .parameters
        .get(parameter_offset..)
        .ok_or(PapyrusProviderLowerError::TooManyArguments)?;
    let mut values = (0..parameters.len())
        .map(|_| None)
        .collect::<Vec<Option<T>>>();
    let mut positional = 0usize;
    let mut named_seen = false;
    for arg in args {
        let index = if let Some(name) = &arg.name {
            named_seen = true;
            parameters
                .iter()
                .position(|parameter| parameter.id.as_str().eq_ignore_ascii_case(&name.node.0))
                .ok_or_else(|| PapyrusProviderLowerError::UnknownParameter(name.node.0.clone()))?
        } else {
            if named_seen {
                return Err(PapyrusProviderLowerError::PositionalAfterNamed);
            }
            let index = positional;
            positional = positional.saturating_add(1);
            index
        };
        let Some(parameter) = parameters.get(index) else {
            return Err(PapyrusProviderLowerError::TooManyArguments);
        };
        if values[index].is_some() {
            return Err(PapyrusProviderLowerError::DuplicateParameter(
                parameter.id.as_str().to_owned(),
            ));
        }
        values[index] = Some(lower(&arg.value.node, parameter)?);
    }

    let last = values.iter().rposition(Option::is_some);
    let mut ordered = Vec::with_capacity(last.map_or(0, |index| index + 1));
    if let Some(last) = last {
        for (index, value) in values.into_iter().take(last + 1).enumerate() {
            let parameter = &parameters[index];
            ordered.push(value.ok_or_else(|| {
                PapyrusProviderLowerError::MissingParameter(parameter.id.as_str().to_owned())
            })?);
        }
    }
    Ok(ordered)
}

fn lower_provider_invocation(
    expression: &Expr,
    catalog: &PapyrusProviderCatalog,
    locals: &BTreeMap<String, ScriptValueType>,
) -> Result<Option<PapyrusProviderInvocation>, PapyrusProviderLowerError> {
    let Expr::Call { callee, args } = expression else {
        return Ok(None);
    };
    let Expr::MemberAccess { object, member } = &callee.node else {
        return Ok(None);
    };
    let Expr::Ident(provider) = &object.node else {
        return Ok(None);
    };
    let (route, receiver, parameter_offset) = if provider.0.eq_ignore_ascii_case(PAPYRUS_SELF_LOCAL)
        && locals.get(PAPYRUS_SELF_LOCAL) == Some(&ScriptValueType::Entity)
    {
        let Some(route) = catalog.resolve(PAPYRUS_SELF_PROVIDER, &member.node.0) else {
            if catalog.contains_provider(PAPYRUS_SELF_PROVIDER) {
                return Err(PapyrusProviderLowerError::UnknownFunction {
                    provider: provider.0.clone(),
                    function: member.node.0.clone(),
                });
            }
            return Ok(None);
        };
        (
            route,
            Some(Box::new(PapyrusProviderArgument::Local {
                name: PAPYRUS_SELF_LOCAL.to_owned(),
                value_type: ScriptValueType::Entity,
            })),
            1,
        )
    } else if let Some(route) = catalog.resolve(&provider.0, &member.node.0) {
        (route, None, 0)
    } else {
        if is_known_provider_call(&provider.0, &member.node.0, catalog) {
            return Err(PapyrusProviderLowerError::UnknownFunction {
                provider: provider.0.clone(),
                function: member.node.0.clone(),
            });
        }
        return Ok(None);
    };
    let declaration = route.declaration();
    if parameter_offset == 1
        && !declaration.parameters.first().is_some_and(|parameter| {
            parameter.value_type == ScriptValueType::Entity && !parameter.optional
        })
    {
        return Err(PapyrusProviderLowerError::UnsupportedArgument {
            parameter: "self receiver".to_owned(),
        });
    }
    let arguments = lower_ordered_arguments_from(
        args,
        declaration,
        parameter_offset,
        |expression, parameter| {
            if let Some(value) = lower_literal(expression, parameter.value_type, parameter.optional)
            {
                return Ok(PapyrusProviderArgument::Literal(value));
            }
            if let Expr::Ident(identifier) = expression {
                let name = identifier.0.to_ascii_lowercase();
                if locals.get(&name) == Some(&parameter.value_type) {
                    return Ok(PapyrusProviderArgument::Local {
                        name,
                        value_type: parameter.value_type,
                    });
                }
            }
            Err(PapyrusProviderLowerError::UnsupportedArgument {
                parameter: parameter.id.as_str().to_owned(),
            })
        },
    )?;
    for (index, argument) in arguments.iter().enumerate() {
        let parameter = &declaration.parameters[index + parameter_offset];
        let valid = match argument {
            PapyrusProviderArgument::Literal(value) => {
                value.matches(parameter.value_type, parameter.optional)
            }
            PapyrusProviderArgument::Local { value_type, .. } => {
                *value_type == parameter.value_type
            }
        };
        if !valid {
            return Err(PapyrusProviderLowerError::UnsupportedArgument {
                parameter: parameter.id.as_str().to_owned(),
            });
        }
    }
    if let Some(parameter) = declaration
        .parameters
        .iter()
        .skip(parameter_offset + arguments.len())
        .find(|parameter| !parameter.optional)
    {
        return Err(PapyrusProviderLowerError::MissingParameter(
            parameter.id.as_str().to_owned(),
        ));
    }
    validate_storage_util_arguments(route.qualified_name(), &arguments)?;
    validate_legacy_container_arity(route.qualified_name(), arguments.len())?;
    validate_mod_event_arity(route.qualified_name(), arguments.len())?;
    Ok(Some(PapyrusProviderInvocation {
        route: route.clone(),
        receiver,
        arguments,
        result: declaration.result,
    }))
}

fn lower_literal(
    expression: &Expr,
    expected: ScriptValueType,
    optional: bool,
) -> Option<ScriptValue> {
    match (expression, expected) {
        (Expr::NoneLit, _) if optional => Some(ScriptValue::None),
        (Expr::BoolLit(value), ScriptValueType::Boolean) => Some(ScriptValue::Boolean(*value)),
        (Expr::IntLit(value), ScriptValueType::Integer) => Some(ScriptValue::Integer(*value)),
        (
            Expr::UnaryOp {
                op: UnaryOp::Neg,
                operand,
            },
            ScriptValueType::Integer,
        ) => match &operand.node {
            Expr::IntLit(value) => value.checked_neg().map(ScriptValue::Integer),
            _ => None,
        },
        (Expr::FloatLit(value), ScriptValueType::Float) => {
            let value = *value as f32;
            value.is_finite().then_some(ScriptValue::Float(value))
        }
        (
            Expr::UnaryOp {
                op: UnaryOp::Neg,
                operand,
            },
            ScriptValueType::Float,
        ) => match &operand.node {
            Expr::FloatLit(value) => {
                let value = -(*value as f32);
                value.is_finite().then_some(ScriptValue::Float(value))
            }
            _ => None,
        },
        (Expr::StringLit(value), ScriptValueType::String) => {
            Some(ScriptValue::String(value.clone()))
        }
        (Expr::New { ty, size }, expected) => {
            let Expr::IntLit(size) = &size.node else {
                return None;
            };
            let size = usize::try_from(*size)
                .ok()
                .filter(|size| *size <= MAX_SCRIPT_ARRAY_ELEMENTS)?;
            match (&ty.node, expected) {
                (Type::Bool, ScriptValueType::BooleanArray) => {
                    Some(ScriptValue::BooleanArray(vec![false; size]))
                }
                (Type::Int, ScriptValueType::IntegerArray) => {
                    Some(ScriptValue::IntegerArray(vec![0; size]))
                }
                (Type::Float, ScriptValueType::FloatArray) => {
                    Some(ScriptValue::FloatArray(vec![0.0; size]))
                }
                (Type::String, ScriptValueType::StringArray) => {
                    Some(ScriptValue::StringArray(vec![String::new(); size]))
                }
                (Type::Object(_), ScriptValueType::FormArray) => {
                    Some(ScriptValue::FormArray(vec![None; size]))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

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
enum PapyrusModEventRegistrationAction {
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
struct PendingPapyrusProviderContinuation {
    remaining_seconds: f32,
    statements: Vec<PapyrusProviderStatement>,
    locals: BTreeMap<String, ScriptValue>,
    principal: Option<PrincipalId>,
}

/// Bounded latent tails for provider-bearing Papyrus event handlers.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct PapyrusProviderContinuationQueue {
    pending: Vec<PendingPapyrusProviderContinuation>,
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
    registrations: BTreeMap<(EntityId, PrincipalId, byroredux_sdk::identity::EventId), String>,
    pending: Vec<CustomEvent>,
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
    handlers: BTreeMap<PapyrusProviderEvent, Vec<PapyrusProviderHandler>>,
    custom_handlers: BTreeMap<String, Vec<PapyrusProviderHandler>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct PapyrusProviderHandler {
    statements: Vec<PapyrusProviderStatement>,
    parameters: Vec<PapyrusProviderParameterBinding>,
    principal: Option<PrincipalId>,
}

#[derive(Clone, Debug, PartialEq)]
struct PapyrusProviderParameterBinding {
    name: String,
    source: PapyrusProviderParameterSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PapyrusProviderParameterSource {
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
struct PapyrusProviderProjectedLocals {
    values: BTreeMap<String, ScriptValue>,
    entities: Vec<(String, EntityId)>,
    forms: Vec<(String, u32)>,
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

    fn handlers_for(
        &self,
        event: PapyrusProviderEvent,
    ) -> impl Iterator<Item = &PapyrusProviderHandler> {
        self.handlers.get(&event).into_iter().flatten()
    }

    fn merge(&mut self, mut other: Self) {
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

    fn set_principal(&mut self, principal: PrincipalId) {
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
    fn projected_locals(
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

    fn projected_mod_event_locals(
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

/// Whole-handler rejection reason. A known provider is never partially run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PapyrusProviderProgramError {
    DuplicateHandler(PapyrusProviderEvent),
    DuplicateCustomHandler(String),
    NestingTooDeep,
    UnsupportedStatement,
    UnsupportedLocal(String),
    UnknownLocal(String),
    ResultTypeMismatch(String),
    Call(PapyrusProviderLowerError),
}

/// Lower supported provider-bearing handlers from source or decompiled PEX.
/// Handlers without a known provider remain available to existing recognizers.
pub fn lower_provider_program(
    script: &Script,
    catalog: &PapyrusProviderCatalog,
) -> Result<Option<PapyrusProviderProgram>, PapyrusProviderProgramError> {
    let mut program = PapyrusProviderProgram::default();
    let mod_event_sender = if script
        .parent
        .as_ref()
        .is_some_and(|parent| parent.node.0.eq_ignore_ascii_case("ActiveMagicEffect"))
    {
        PapyrusModEventSender::Resolved(None)
    } else {
        PapyrusModEventSender::Owner
    };
    for item in &script.body {
        match &item.node {
            ScriptItem::Event(event) => {
                lower_event_into(event, catalog, &mut program, &mod_event_sender)?
            }
            ScriptItem::State(state) => {
                for item in &state.body {
                    if let StateItem::Event(event) = &item.node {
                        lower_event_into(event, catalog, &mut program, &mod_event_sender)?;
                    }
                }
            }
            _ => {}
        }
    }
    if program.is_empty() {
        Ok(None)
    } else {
        Ok(Some(program))
    }
}

fn lower_event_into(
    event: &Event,
    catalog: &PapyrusProviderCatalog,
    program: &mut PapyrusProviderProgram,
    mod_event_sender: &PapyrusModEventSender,
) -> Result<(), PapyrusProviderProgramError> {
    let canonical = if event.name.node.eq_ignore_case("OnInit") {
        Some(PapyrusProviderEvent::OnInit)
    } else if event.name.node.eq_ignore_case("OnLoad") {
        Some(PapyrusProviderEvent::OnLoad)
    } else if event.name.node.eq_ignore_case("OnActivate") {
        Some(PapyrusProviderEvent::OnActivate)
    } else if event.name.node.eq_ignore_case("OnHit") {
        Some(PapyrusProviderEvent::OnHit)
    } else if event.name.node.eq_ignore_case("OnObjectEquipped") {
        Some(PapyrusProviderEvent::OnObjectEquipped)
    } else if event.name.node.eq_ignore_case("OnObjectUnequipped") {
        Some(PapyrusProviderEvent::OnObjectUnequipped)
    } else if event.name.node.eq_ignore_case("OnTriggerEnter") {
        Some(PapyrusProviderEvent::OnTriggerEnter)
    } else if event.name.node.eq_ignore_case("OnUpdate") {
        Some(PapyrusProviderEvent::OnUpdate)
    } else {
        None
    };
    if !event
        .body
        .iter()
        .any(|statement| statement_mentions_provider(&statement.node, catalog, 0))
    {
        return Ok(());
    }
    let mut locals = BTreeMap::from([(PAPYRUS_SELF_LOCAL.to_owned(), ScriptValueType::Entity)]);
    let mut parameters = if let Some(canonical) = canonical {
        lower_event_parameters(canonical, event, &mut locals)
    } else {
        lower_mod_event_parameters(event, &mut locals)?
    };
    let statements = lower_statements(&event.body, catalog, &mut locals, mod_event_sender, 0)?;
    if canonical.is_some() {
        parameters.retain(|parameter| statements_reference_local(&statements, &parameter.name));
    }
    if parameters
        .iter()
        .any(|parameter| parameter.source == PapyrusProviderParameterSource::Entity)
        && statements_contain_wait(&statements)
    {
        return Err(PapyrusProviderProgramError::UnsupportedStatement);
    }
    if statements_reference_local(&statements, PAPYRUS_SELF_LOCAL)
        && statements_contain_wait(&statements)
    {
        return Err(PapyrusProviderProgramError::UnsupportedStatement);
    }
    if statements_contain_wait(&statements)
        && statements_contain_mod_event_registration(&statements)
    {
        return Err(PapyrusProviderProgramError::UnsupportedStatement);
    }
    let handler = PapyrusProviderHandler {
        statements,
        parameters,
        principal: None,
    };
    if let Some(canonical) = canonical {
        if program.handlers.contains_key(&canonical) {
            return Err(PapyrusProviderProgramError::DuplicateHandler(canonical));
        }
        program.handlers.insert(canonical, vec![handler]);
    } else {
        let callback = event.name.node.0.to_ascii_lowercase();
        if program.custom_handlers.contains_key(&callback) {
            return Err(PapyrusProviderProgramError::DuplicateCustomHandler(
                event.name.node.0.clone(),
            ));
        }
        program.custom_handlers.insert(callback, vec![handler]);
    }
    Ok(())
}

fn lower_mod_event_parameters(
    event: &Event,
    locals: &mut BTreeMap<String, ScriptValueType>,
) -> Result<Vec<PapyrusProviderParameterBinding>, PapyrusProviderProgramError> {
    event
        .params
        .iter()
        .enumerate()
        .map(|(index, parameter)| {
            let value_type = match &parameter.ty.node {
                Type::Bool => ScriptValueType::Boolean,
                Type::Int => ScriptValueType::Integer,
                Type::Float => ScriptValueType::Float,
                Type::String => ScriptValueType::String,
                Type::Object(_) => ScriptValueType::Form,
                _ => return Err(PapyrusProviderProgramError::UnsupportedStatement),
            };
            let name = parameter.name.node.0.to_ascii_lowercase();
            locals.insert(name.clone(), value_type);
            Ok(PapyrusProviderParameterBinding {
                name,
                source: PapyrusProviderParameterSource::ModEventArgument { index, value_type },
            })
        })
        .collect()
}

fn lower_event_parameters(
    event_kind: PapyrusProviderEvent,
    event: &Event,
    locals: &mut BTreeMap<String, ScriptValueType>,
) -> Vec<PapyrusProviderParameterBinding> {
    let mut bindings = Vec::new();
    if matches!(
        event_kind,
        PapyrusProviderEvent::OnActivate
            | PapyrusProviderEvent::OnHit
            | PapyrusProviderEvent::OnTriggerEnter
    ) {
        if let Some(parameter) = event.params.first() {
            if matches!(&parameter.ty.node, Type::Object(_)) {
                let name = parameter.name.node.0.to_ascii_lowercase();
                locals.insert(name.clone(), ScriptValueType::Entity);
                bindings.push(PapyrusProviderParameterBinding {
                    name,
                    source: PapyrusProviderParameterSource::Entity,
                });
            }
        }
    }
    if matches!(
        event_kind,
        PapyrusProviderEvent::OnObjectEquipped | PapyrusProviderEvent::OnObjectUnequipped
    ) {
        if let Some(parameter) = event.params.first() {
            if matches!(&parameter.ty.node, Type::Object(_)) {
                let name = parameter.name.node.0.to_ascii_lowercase();
                locals.insert(name.clone(), ScriptValueType::Form);
                bindings.push(PapyrusProviderParameterBinding {
                    name,
                    source: PapyrusProviderParameterSource::Form,
                });
            }
        }
    }
    if event_kind != PapyrusProviderEvent::OnHit {
        return bindings;
    }
    let sources = [
        (3, PapyrusProviderParameterSource::PowerAttack),
        (4, PapyrusProviderParameterSource::SneakAttack),
        (5, PapyrusProviderParameterSource::BashAttack),
        (6, PapyrusProviderParameterSource::Blocked),
    ];
    for (index, source) in sources {
        let Some(parameter) = event.params.get(index) else {
            continue;
        };
        if !matches!(&parameter.ty.node, Type::Bool) {
            continue;
        }
        let name = parameter.name.node.0.to_ascii_lowercase();
        locals.insert(name.clone(), ScriptValueType::Boolean);
        bindings.push(PapyrusProviderParameterBinding { name, source });
    }
    bindings
}

fn statements_reference_local(statements: &[PapyrusProviderStatement], name: &str) -> bool {
    statements.iter().any(|statement| match statement {
        PapyrusProviderStatement::Declare { .. }
        | PapyrusProviderStatement::Wait { .. }
        | PapyrusProviderStatement::RegisterModEvent { .. }
        | PapyrusProviderStatement::UnregisterModEvent { .. }
        | PapyrusProviderStatement::UnregisterAllModEvents => false,
        PapyrusProviderStatement::SendModEvent {
            event_name,
            string_arg,
            number_arg,
            ..
        } => {
            argument_references_local(event_name, name)
                || argument_references_local(string_arg, name)
                || argument_references_local(number_arg, name)
        }
        PapyrusProviderStatement::AssignValue { value, .. } => value_references_local(value, name),
        PapyrusProviderStatement::AssignCall { call, .. }
        | PapyrusProviderStatement::ArrayWritebackCall { call, .. }
        | PapyrusProviderStatement::Call(call) => invocation_references_local(call, name),
        PapyrusProviderStatement::If {
            condition,
            then_branch,
            else_branch,
        } => {
            condition_references_local(condition, name)
                || statements_reference_local(then_branch, name)
                || statements_reference_local(else_branch, name)
        }
    })
}

fn statements_contain_wait(statements: &[PapyrusProviderStatement]) -> bool {
    statements.iter().any(|statement| match statement {
        PapyrusProviderStatement::Wait { .. } => true,
        PapyrusProviderStatement::If {
            then_branch,
            else_branch,
            ..
        } => statements_contain_wait(then_branch) || statements_contain_wait(else_branch),
        PapyrusProviderStatement::Declare { .. }
        | PapyrusProviderStatement::AssignValue { .. }
        | PapyrusProviderStatement::AssignCall { .. }
        | PapyrusProviderStatement::ArrayWritebackCall { .. }
        | PapyrusProviderStatement::Call(_)
        | PapyrusProviderStatement::RegisterModEvent { .. }
        | PapyrusProviderStatement::UnregisterModEvent { .. }
        | PapyrusProviderStatement::UnregisterAllModEvents
        | PapyrusProviderStatement::SendModEvent { .. } => false,
    })
}

fn statements_contain_mod_event_registration(statements: &[PapyrusProviderStatement]) -> bool {
    statements.iter().any(|statement| match statement {
        PapyrusProviderStatement::RegisterModEvent { .. }
        | PapyrusProviderStatement::UnregisterModEvent { .. }
        | PapyrusProviderStatement::UnregisterAllModEvents => true,
        PapyrusProviderStatement::If {
            then_branch,
            else_branch,
            ..
        } => {
            statements_contain_mod_event_registration(then_branch)
                || statements_contain_mod_event_registration(else_branch)
        }
        _ => false,
    })
}

fn invocation_references_local(call: &PapyrusProviderInvocation, name: &str) -> bool {
    call.receiver
        .iter()
        .map(Box::as_ref)
        .chain(call.arguments.iter())
        .any(|argument| {
            matches!(
                argument,
                PapyrusProviderArgument::Local { name: local, .. } if local == name
            )
        })
}

fn argument_references_local(argument: &PapyrusProviderArgument, name: &str) -> bool {
    matches!(
        argument,
        PapyrusProviderArgument::Local { name: local, .. } if local == name
    )
}

fn condition_references_local(condition: &PapyrusProviderCondition, name: &str) -> bool {
    match condition {
        PapyrusProviderCondition::Literal(_) => false,
        PapyrusProviderCondition::Local(local) => local == name,
        PapyrusProviderCondition::Call(call) => invocation_references_local(call, name),
        PapyrusProviderCondition::Not(condition) => condition_references_local(condition, name),
        PapyrusProviderCondition::And(left, right) | PapyrusProviderCondition::Or(left, right) => {
            condition_references_local(left, name) || condition_references_local(right, name)
        }
        PapyrusProviderCondition::Compare { left, right, .. } => {
            value_references_local(left, name) || value_references_local(right, name)
        }
    }
}

fn value_references_local(value: &PapyrusProviderValue, name: &str) -> bool {
    match value {
        PapyrusProviderValue::Literal(_) => false,
        PapyrusProviderValue::Local(local) => local == name,
        PapyrusProviderValue::Call(call) => invocation_references_local(call, name),
        PapyrusProviderValue::Binary { left, right, .. } => {
            value_references_local(left, name) || value_references_local(right, name)
        }
    }
}

fn lower_statements(
    statements: &[byroredux_papyrus::span::Spanned<Stmt>],
    catalog: &PapyrusProviderCatalog,
    locals: &mut BTreeMap<String, ScriptValueType>,
    mod_event_sender: &PapyrusModEventSender,
    depth: usize,
) -> Result<Vec<PapyrusProviderStatement>, PapyrusProviderProgramError> {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return Err(PapyrusProviderProgramError::NestingTooDeep);
    }
    let mut lowered = Vec::with_capacity(statements.len());
    for (statement_index, statement) in statements.iter().enumerate() {
        match &statement.node {
            Stmt::VarDecl(variable) => {
                let Some(value_type) = sdk_type(&variable.ty.node) else {
                    return Err(PapyrusProviderProgramError::UnsupportedLocal(
                        variable.name.node.0.clone(),
                    ));
                };
                let value = if let Some(initial) = &variable.initial_value {
                    lower_literal(&initial.node, value_type, false).ok_or_else(|| {
                        PapyrusProviderProgramError::UnsupportedLocal(variable.name.node.0.clone())
                    })?
                } else {
                    default_value(value_type)
                };
                let key = variable.name.node.0.to_ascii_lowercase();
                locals.insert(key.clone(), value_type);
                lowered.push(PapyrusProviderStatement::Declare { name: key, value });
            }
            Stmt::Assign { target, op, value } if *op == AssignOp::Eq => {
                let Expr::Ident(target) = &target.node else {
                    return Err(PapyrusProviderProgramError::UnsupportedStatement);
                };
                let key = target.0.to_ascii_lowercase();
                let expected = locals
                    .get(&key)
                    .copied()
                    .ok_or_else(|| PapyrusProviderProgramError::UnknownLocal(target.0.clone()))?;
                if let Some(call) = lower_provider_invocation(&value.node, catalog, locals)
                    .map_err(PapyrusProviderProgramError::Call)?
                {
                    require_result(&call, expected, &target.0)?;
                    lowered.push(PapyrusProviderStatement::AssignCall { name: key, call });
                } else {
                    let (value, value_type) =
                        lower_provider_value(&value.node, catalog, locals, 0)?;
                    if value_type != expected {
                        return Err(PapyrusProviderProgramError::ResultTypeMismatch(
                            target.0.clone(),
                        ));
                    }
                    lowered.push(PapyrusProviderStatement::AssignValue {
                        name: key,
                        value,
                        value_type,
                    });
                }
            }
            Stmt::ExprStmt(expression) => {
                if let Some(seconds) = lower_wait(&expression.node)? {
                    lowered.push(PapyrusProviderStatement::Wait { seconds });
                    continue;
                }
                if let Some(registration) = lower_mod_event_registration(&expression.node)? {
                    lowered.push(registration);
                    continue;
                }
                if let Some(send) =
                    lower_send_mod_event(&expression.node, locals, mod_event_sender)?
                {
                    lowered.push(send);
                    continue;
                }
                let call = lower_provider_invocation(&expression.node, catalog, locals)
                    .map_err(PapyrusProviderProgramError::Call)?
                    .ok_or(PapyrusProviderProgramError::UnsupportedStatement)?;
                if parse_storage_util_list_route(call.route.qualified_name())
                    .is_some_and(|(_, operation)| operation == StorageUtilListOperation::Slice)
                {
                    let Some(PapyrusProviderArgument::Local { name, .. }) = call.arguments.get(2)
                    else {
                        return Err(PapyrusProviderProgramError::UnsupportedStatement);
                    };
                    lowered.push(PapyrusProviderStatement::ArrayWritebackCall {
                        name: name.clone(),
                        call,
                    });
                } else {
                    lowered.push(PapyrusProviderStatement::Call(call));
                }
            }
            Stmt::If {
                condition,
                body,
                elseif_clauses,
                else_body,
            } => {
                let condition = lower_condition(&condition.node, catalog, locals)?;
                let mut branch_locals = locals.clone();
                let then_branch = lower_statements(
                    body,
                    catalog,
                    &mut branch_locals,
                    mod_event_sender,
                    depth + 1,
                )?;
                let mut else_branch = if let Some(body) = else_body {
                    let mut branch_locals = locals.clone();
                    lower_statements(
                        body,
                        catalog,
                        &mut branch_locals,
                        mod_event_sender,
                        depth + 1,
                    )?
                } else {
                    Vec::new()
                };
                for (condition, body) in elseif_clauses.iter().rev() {
                    let condition = lower_condition(&condition.node, catalog, locals)?;
                    let mut branch_locals = locals.clone();
                    let then_branch = lower_statements(
                        body,
                        catalog,
                        &mut branch_locals,
                        mod_event_sender,
                        depth + 1,
                    )?;
                    else_branch = vec![PapyrusProviderStatement::If {
                        condition: Box::new(condition),
                        then_branch,
                        else_branch,
                    }];
                }
                lowered.push(PapyrusProviderStatement::If {
                    condition: Box::new(condition),
                    then_branch,
                    else_branch,
                });
            }
            // The PEX decompiler preserves the compiler-emitted terminal
            // `Return None`. It has no observable effect at handler tail, but
            // returns inside branches remain unsupported because skipping one
            // there would change control flow.
            Stmt::Return(None) if depth == 0 && statement_index + 1 == statements.len() => {}
            _ => return Err(PapyrusProviderProgramError::UnsupportedStatement),
        }
    }
    Ok(lowered)
}

fn lower_mod_event_registration(
    expression: &Expr,
) -> Result<Option<PapyrusProviderStatement>, PapyrusProviderProgramError> {
    let Expr::Call { callee, args } = expression else {
        return Ok(None);
    };
    let function = match &callee.node {
        Expr::Ident(identifier) => &identifier.0,
        Expr::MemberAccess { member, .. } => &member.node.0,
        _ => return Ok(None),
    };
    if function.eq_ignore_ascii_case("RegisterForModEvent") {
        let [event, callback] = args.as_slice() else {
            return Err(PapyrusProviderProgramError::UnsupportedStatement);
        };
        if event.name.is_some() || callback.name.is_some() {
            return Err(PapyrusProviderProgramError::UnsupportedStatement);
        }
        let (Expr::StringLit(event_name), Expr::StringLit(callback)) =
            (&event.value.node, &callback.value.node)
        else {
            return Err(PapyrusProviderProgramError::UnsupportedStatement);
        };
        LegacyModEventSubscriptionCommand::subscribe(event_name, callback.clone())
            .ok_or(PapyrusProviderProgramError::UnsupportedStatement)?;
        return Ok(Some(PapyrusProviderStatement::RegisterModEvent {
            event_name: event_name.clone(),
            callback: callback.to_ascii_lowercase(),
        }));
    }
    if function.eq_ignore_ascii_case("UnregisterForModEvent") {
        let [event] = args.as_slice() else {
            return Err(PapyrusProviderProgramError::UnsupportedStatement);
        };
        if event.name.is_some() {
            return Err(PapyrusProviderProgramError::UnsupportedStatement);
        }
        let Expr::StringLit(event_name) = &event.value.node else {
            return Err(PapyrusProviderProgramError::UnsupportedStatement);
        };
        LegacyModEventSubscriptionCommand::unsubscribe(event_name)
            .ok_or(PapyrusProviderProgramError::UnsupportedStatement)?;
        return Ok(Some(PapyrusProviderStatement::UnregisterModEvent {
            event_name: event_name.clone(),
        }));
    }
    if function.eq_ignore_ascii_case("UnregisterForAllModEvents") {
        if !args.is_empty() {
            return Err(PapyrusProviderProgramError::UnsupportedStatement);
        }
        return Ok(Some(PapyrusProviderStatement::UnregisterAllModEvents));
    }
    Ok(None)
}

fn lower_send_mod_event(
    expression: &Expr,
    locals: &BTreeMap<String, ScriptValueType>,
    sender: &PapyrusModEventSender,
) -> Result<Option<PapyrusProviderStatement>, PapyrusProviderProgramError> {
    let Expr::Call { callee, args } = expression else {
        return Ok(None);
    };
    let is_send = match &callee.node {
        Expr::Ident(function) => function.0.eq_ignore_ascii_case("SendModEvent"),
        Expr::MemberAccess { object, member } => {
            member.node.0.eq_ignore_ascii_case("SendModEvent")
                && matches!(&object.node, Expr::Ident(receiver) if receiver.0.eq_ignore_ascii_case("self"))
        }
        _ => false,
    };
    if !is_send {
        return Ok(None);
    }

    let mut ordered: [Option<&CallArg>; 3] = [None, None, None];
    let mut next_positional = 0;
    for argument in args {
        let index = if let Some(name) = &argument.name {
            ["eventName", "strArg", "numArg"]
                .iter()
                .position(|candidate| name.node.0.eq_ignore_ascii_case(candidate))
                .ok_or(PapyrusProviderProgramError::UnsupportedStatement)?
        } else {
            while next_positional < ordered.len() && ordered[next_positional].is_some() {
                next_positional += 1;
            }
            if next_positional == ordered.len() {
                return Err(PapyrusProviderProgramError::UnsupportedStatement);
            }
            let index = next_positional;
            next_positional += 1;
            index
        };
        if ordered[index].replace(argument).is_some() {
            return Err(PapyrusProviderProgramError::UnsupportedStatement);
        }
    }
    let event_name = ordered[0]
        .ok_or(PapyrusProviderProgramError::UnsupportedStatement)
        .and_then(|argument| {
            lower_mod_event_argument(&argument.value.node, ScriptValueType::String, locals)
        })?;
    let string_arg = ordered[1].map_or_else(
        || {
            Ok(PapyrusProviderArgument::Literal(ScriptValue::String(
                String::new(),
            )))
        },
        |argument| lower_mod_event_argument(&argument.value.node, ScriptValueType::String, locals),
    )?;
    let number_arg = ordered[2].map_or_else(
        || Ok(PapyrusProviderArgument::Literal(ScriptValue::Float(0.0))),
        |argument| lower_mod_event_argument(&argument.value.node, ScriptValueType::Float, locals),
    )?;
    Ok(Some(PapyrusProviderStatement::SendModEvent {
        event_name,
        string_arg,
        number_arg,
        sender: sender.clone(),
    }))
}

fn lower_mod_event_argument(
    expression: &Expr,
    expected: ScriptValueType,
    locals: &BTreeMap<String, ScriptValueType>,
) -> Result<PapyrusProviderArgument, PapyrusProviderProgramError> {
    if let Some(value) = lower_literal(expression, expected, false) {
        return Ok(PapyrusProviderArgument::Literal(value));
    }
    if let Expr::Ident(identifier) = expression {
        let name = identifier.0.to_ascii_lowercase();
        if locals.get(&name) == Some(&expected) {
            return Ok(PapyrusProviderArgument::Local {
                name,
                value_type: expected,
            });
        }
    }
    Err(PapyrusProviderProgramError::UnsupportedStatement)
}

fn lower_wait(expression: &Expr) -> Result<Option<f32>, PapyrusProviderProgramError> {
    let Expr::Call { callee, args } = expression else {
        return Ok(None);
    };
    let Expr::MemberAccess { object, member } = &callee.node else {
        return Ok(None);
    };
    let Expr::Ident(provider) = &object.node else {
        return Ok(None);
    };
    if !provider.0.eq_ignore_ascii_case("Utility") || !member.node.0.eq_ignore_ascii_case("Wait") {
        return Ok(None);
    }
    let [argument] = args.as_slice() else {
        return Err(PapyrusProviderProgramError::UnsupportedStatement);
    };
    if argument.name.is_some() {
        return Err(PapyrusProviderProgramError::UnsupportedStatement);
    }
    let seconds = match &argument.value.node {
        Expr::IntLit(value) => *value as f32,
        Expr::FloatLit(value) => *value as f32,
        _ => return Err(PapyrusProviderProgramError::UnsupportedStatement),
    };
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(PapyrusProviderProgramError::UnsupportedStatement);
    }
    Ok(Some(seconds))
}

fn lower_condition(
    expression: &Expr,
    catalog: &PapyrusProviderCatalog,
    locals: &BTreeMap<String, ScriptValueType>,
) -> Result<PapyrusProviderCondition, PapyrusProviderProgramError> {
    lower_condition_at_depth(expression, catalog, locals, 0)
}

fn lower_condition_at_depth(
    expression: &Expr,
    catalog: &PapyrusProviderCatalog,
    locals: &BTreeMap<String, ScriptValueType>,
    depth: usize,
) -> Result<PapyrusProviderCondition, PapyrusProviderProgramError> {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return Err(PapyrusProviderProgramError::NestingTooDeep);
    }
    match expression {
        Expr::BoolLit(value) => Ok(PapyrusProviderCondition::Literal(*value)),
        Expr::Ident(identifier) => {
            let key = identifier.0.to_ascii_lowercase();
            if locals.get(&key) == Some(&ScriptValueType::Boolean) {
                Ok(PapyrusProviderCondition::Local(key))
            } else {
                Err(PapyrusProviderProgramError::UnknownLocal(
                    identifier.0.clone(),
                ))
            }
        }
        Expr::UnaryOp {
            op: UnaryOp::Not,
            operand,
        } => Ok(PapyrusProviderCondition::Not(Box::new(
            lower_condition_at_depth(&operand.node, catalog, locals, depth + 1)?,
        ))),
        Expr::BinaryOp {
            left,
            op: BinaryOp::And,
            right,
        } => Ok(PapyrusProviderCondition::And(
            Box::new(lower_condition_at_depth(
                &left.node,
                catalog,
                locals,
                depth + 1,
            )?),
            Box::new(lower_condition_at_depth(
                &right.node,
                catalog,
                locals,
                depth + 1,
            )?),
        )),
        Expr::BinaryOp {
            left,
            op: BinaryOp::Or,
            right,
        } => Ok(PapyrusProviderCondition::Or(
            Box::new(lower_condition_at_depth(
                &left.node,
                catalog,
                locals,
                depth + 1,
            )?),
            Box::new(lower_condition_at_depth(
                &right.node,
                catalog,
                locals,
                depth + 1,
            )?),
        )),
        Expr::BinaryOp { left, op, right } if comparison_operator(*op).is_some() => {
            let operator = comparison_operator(*op).expect("comparison operator was matched");
            let (left, left_type) = lower_condition_value(&left.node, catalog, locals)?;
            let (right, right_type) = lower_condition_value(&right.node, catalog, locals)?;
            if left_type != right_type || !comparison_is_supported(operator, left_type) {
                return Err(PapyrusProviderProgramError::UnsupportedStatement);
            }
            Ok(PapyrusProviderCondition::Compare {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            })
        }
        _ => {
            let call = lower_provider_invocation(expression, catalog, locals)
                .map_err(PapyrusProviderProgramError::Call)?
                .ok_or(PapyrusProviderProgramError::UnsupportedStatement)?;
            require_result(&call, ScriptValueType::Boolean, "if condition")?;
            Ok(PapyrusProviderCondition::Call(call))
        }
    }
}

fn lower_condition_value(
    expression: &Expr,
    catalog: &PapyrusProviderCatalog,
    locals: &BTreeMap<String, ScriptValueType>,
) -> Result<(PapyrusProviderValue, ScriptValueType), PapyrusProviderProgramError> {
    lower_provider_value(expression, catalog, locals, 0)
}

fn lower_provider_value(
    expression: &Expr,
    catalog: &PapyrusProviderCatalog,
    locals: &BTreeMap<String, ScriptValueType>,
    depth: usize,
) -> Result<(PapyrusProviderValue, ScriptValueType), PapyrusProviderProgramError> {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return Err(PapyrusProviderProgramError::NestingTooDeep);
    }
    let literal = match expression {
        // Papyrus uses `None` as the null value for object references. Keep
        // the lowered type Entity so it can participate in identity checks;
        // assignment validation still rejects it for non-optional results.
        Expr::NoneLit => Some((ScriptValue::None, ScriptValueType::Entity)),
        Expr::BoolLit(value) => Some((ScriptValue::Boolean(*value), ScriptValueType::Boolean)),
        Expr::IntLit(value) => Some((ScriptValue::Integer(*value), ScriptValueType::Integer)),
        Expr::FloatLit(value) => {
            let value = *value as f32;
            value
                .is_finite()
                .then_some((ScriptValue::Float(value), ScriptValueType::Float))
        }
        Expr::StringLit(value) => {
            Some((ScriptValue::String(value.clone()), ScriptValueType::String))
        }
        _ => None,
    };
    if let Some((value, value_type)) = literal {
        return Ok((PapyrusProviderValue::Literal(value), value_type));
    }
    if let Expr::Ident(identifier) = expression {
        let key = identifier.0.to_ascii_lowercase();
        let value_type = locals
            .get(&key)
            .copied()
            .ok_or_else(|| PapyrusProviderProgramError::UnknownLocal(identifier.0.clone()))?;
        return Ok((PapyrusProviderValue::Local(key), value_type));
    }
    if let Expr::BinaryOp { left, op, right } = expression {
        let Some(operator) = provider_arithmetic(*op) else {
            return Err(PapyrusProviderProgramError::UnsupportedStatement);
        };
        let (left, left_type) = lower_provider_value(&left.node, catalog, locals, depth + 1)?;
        let (right, right_type) = lower_provider_value(&right.node, catalog, locals, depth + 1)?;
        let (operator, value_type) = match operator {
            PapyrusProviderArithmetic::Add
                if left_type == ScriptValueType::String
                    && right_type == ScriptValueType::String =>
            {
                (PapyrusProviderArithmetic::StrCat, ScriptValueType::String)
            }
            PapyrusProviderArithmetic::StrCat
                if left_type == ScriptValueType::String
                    && right_type == ScriptValueType::String =>
            {
                (PapyrusProviderArithmetic::StrCat, ScriptValueType::String)
            }
            PapyrusProviderArithmetic::Add
            | PapyrusProviderArithmetic::Sub
            | PapyrusProviderArithmetic::Mul
            | PapyrusProviderArithmetic::Div
            | PapyrusProviderArithmetic::Mod
                if left_type == right_type
                    && matches!(left_type, ScriptValueType::Integer | ScriptValueType::Float) =>
            {
                (operator, left_type)
            }
            _ => return Err(PapyrusProviderProgramError::UnsupportedStatement),
        };
        return Ok((
            PapyrusProviderValue::Binary {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            },
            value_type,
        ));
    }
    let call = lower_provider_invocation(expression, catalog, locals)
        .map_err(PapyrusProviderProgramError::Call)?
        .ok_or(PapyrusProviderProgramError::UnsupportedStatement)?;
    let result = call
        .result
        .as_ref()
        .filter(|result| !result.optional)
        .ok_or_else(|| PapyrusProviderProgramError::ResultTypeMismatch("comparison".to_owned()))?;
    let value_type = result.value_type;
    Ok((PapyrusProviderValue::Call(call), value_type))
}

fn provider_arithmetic(operator: BinaryOp) -> Option<PapyrusProviderArithmetic> {
    match operator {
        BinaryOp::Add => Some(PapyrusProviderArithmetic::Add),
        BinaryOp::Sub => Some(PapyrusProviderArithmetic::Sub),
        BinaryOp::Mul => Some(PapyrusProviderArithmetic::Mul),
        BinaryOp::Div => Some(PapyrusProviderArithmetic::Div),
        BinaryOp::Mod => Some(PapyrusProviderArithmetic::Mod),
        BinaryOp::StrCat => Some(PapyrusProviderArithmetic::StrCat),
        _ => None,
    }
}

fn comparison_operator(operator: BinaryOp) -> Option<PapyrusProviderComparison> {
    match operator {
        BinaryOp::Eq => Some(PapyrusProviderComparison::Equal),
        BinaryOp::Ne => Some(PapyrusProviderComparison::NotEqual),
        BinaryOp::Lt => Some(PapyrusProviderComparison::Less),
        BinaryOp::Le => Some(PapyrusProviderComparison::LessOrEqual),
        BinaryOp::Gt => Some(PapyrusProviderComparison::Greater),
        BinaryOp::Ge => Some(PapyrusProviderComparison::GreaterOrEqual),
        _ => None,
    }
}

fn comparison_is_supported(
    operator: PapyrusProviderComparison,
    value_type: ScriptValueType,
) -> bool {
    match value_type {
        ScriptValueType::Boolean => matches!(
            operator,
            PapyrusProviderComparison::Equal | PapyrusProviderComparison::NotEqual
        ),
        ScriptValueType::Integer | ScriptValueType::Float => true,
        ScriptValueType::String => matches!(
            operator,
            PapyrusProviderComparison::Equal | PapyrusProviderComparison::NotEqual
        ),
        ScriptValueType::Form
        | ScriptValueType::BooleanArray
        | ScriptValueType::IntegerArray
        | ScriptValueType::FloatArray
        | ScriptValueType::StringArray
        | ScriptValueType::FormArray
        | ScriptValueType::EntityArray => false,
        ScriptValueType::Entity => matches!(
            operator,
            PapyrusProviderComparison::Equal | PapyrusProviderComparison::NotEqual
        ),
    }
}

fn require_result(
    call: &PapyrusProviderInvocation,
    expected: ScriptValueType,
    target: &str,
) -> Result<(), PapyrusProviderProgramError> {
    if call.result.is_some_and(|result| {
        result.value_type == expected
            && (!result.optional
                || matches!(expected, ScriptValueType::Form | ScriptValueType::Entity))
    }) {
        Ok(())
    } else {
        Err(PapyrusProviderProgramError::ResultTypeMismatch(
            target.to_owned(),
        ))
    }
}

fn sdk_type(value: &Type) -> Option<ScriptValueType> {
    match value {
        Type::Bool => Some(ScriptValueType::Boolean),
        Type::Int => Some(ScriptValueType::Integer),
        Type::Float => Some(ScriptValueType::Float),
        Type::String => Some(ScriptValueType::String),
        Type::Object(_) => Some(ScriptValueType::Entity),
        Type::Array(element) => match element.as_ref() {
            Type::Bool => Some(ScriptValueType::BooleanArray),
            Type::Int => Some(ScriptValueType::IntegerArray),
            Type::Float => Some(ScriptValueType::FloatArray),
            Type::String => Some(ScriptValueType::StringArray),
            Type::Object(_) => Some(ScriptValueType::FormArray),
            _ => None,
        },
        _ => None,
    }
}

fn default_value(value_type: ScriptValueType) -> ScriptValue {
    match value_type {
        ScriptValueType::Boolean => ScriptValue::Boolean(false),
        ScriptValueType::Integer => ScriptValue::Integer(0),
        ScriptValueType::Float => ScriptValue::Float(0.0),
        ScriptValueType::String => ScriptValue::String(String::new()),
        ScriptValueType::Form | ScriptValueType::Entity => ScriptValue::None,
        ScriptValueType::BooleanArray => ScriptValue::BooleanArray(Vec::new()),
        ScriptValueType::IntegerArray => ScriptValue::IntegerArray(Vec::new()),
        ScriptValueType::FloatArray => ScriptValue::FloatArray(Vec::new()),
        ScriptValueType::StringArray => ScriptValue::StringArray(Vec::new()),
        ScriptValueType::FormArray => ScriptValue::FormArray(Vec::new()),
        ScriptValueType::EntityArray => ScriptValue::EntityArray(Vec::new()),
    }
}

fn statement_mentions_provider(
    statement: &Stmt,
    catalog: &PapyrusProviderCatalog,
    depth: usize,
) -> bool {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return true;
    }
    match statement {
        Stmt::Assign { target, value, .. } => {
            expression_mentions_provider(&target.node, catalog, depth + 1)
                || expression_mentions_provider(&value.node, catalog, depth + 1)
        }
        Stmt::Return(value) => value
            .as_ref()
            .is_some_and(|value| expression_mentions_provider(&value.node, catalog, depth + 1)),
        Stmt::If {
            condition,
            body,
            elseif_clauses,
            else_body,
        } => {
            expression_mentions_provider(&condition.node, catalog, depth + 1)
                || body
                    .iter()
                    .any(|stmt| statement_mentions_provider(&stmt.node, catalog, depth + 1))
                || elseif_clauses.iter().any(|(condition, body)| {
                    expression_mentions_provider(&condition.node, catalog, depth + 1)
                        || body
                            .iter()
                            .any(|stmt| statement_mentions_provider(&stmt.node, catalog, depth + 1))
                })
                || else_body.as_ref().is_some_and(|body| {
                    body.iter()
                        .any(|stmt| statement_mentions_provider(&stmt.node, catalog, depth + 1))
                })
        }
        Stmt::While { condition, body } => {
            expression_mentions_provider(&condition.node, catalog, depth + 1)
                || body
                    .iter()
                    .any(|stmt| statement_mentions_provider(&stmt.node, catalog, depth + 1))
        }
        Stmt::ExprStmt(expression) => {
            expression_mentions_provider(&expression.node, catalog, depth + 1)
        }
        Stmt::VarDecl(variable) => variable
            .initial_value
            .as_ref()
            .is_some_and(|value| expression_mentions_provider(&value.node, catalog, depth + 1)),
    }
}

fn expression_mentions_provider(
    expression: &Expr,
    catalog: &PapyrusProviderCatalog,
    depth: usize,
) -> bool {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return true;
    }
    match expression {
        Expr::Call { callee, args } => {
            let direct = matches!(
                &callee.node,
                Expr::MemberAccess { object, member }
                    if matches!(
                        &object.node,
                        Expr::Ident(provider)
                            if is_known_provider_call(&provider.0, &member.node.0, catalog)
                    )
            ) || matches!(
                &callee.node,
                Expr::Ident(function)
                    if byroredux_sdk::compatibility::method_source_alias(&function.0).is_some()
            ) || matches!(
                &callee.node,
                Expr::MemberAccess { member, .. }
                    if byroredux_sdk::compatibility::method_source_alias(&member.node.0).is_some()
            );
            direct
                || expression_mentions_provider(&callee.node, catalog, depth + 1)
                || args
                    .iter()
                    .any(|arg| expression_mentions_provider(&arg.value.node, catalog, depth + 1))
        }
        Expr::MemberAccess { object, .. } => {
            expression_mentions_provider(&object.node, catalog, depth + 1)
        }
        Expr::Index { object, index } => {
            expression_mentions_provider(&object.node, catalog, depth + 1)
                || expression_mentions_provider(&index.node, catalog, depth + 1)
        }
        Expr::UnaryOp { operand, .. } => {
            expression_mentions_provider(&operand.node, catalog, depth + 1)
        }
        Expr::BinaryOp { left, right, .. } => {
            expression_mentions_provider(&left.node, catalog, depth + 1)
                || expression_mentions_provider(&right.node, catalog, depth + 1)
        }
        Expr::Cast { expr, .. } => expression_mentions_provider(&expr.node, catalog, depth + 1),
        Expr::New { size, .. } => expression_mentions_provider(&size.node, catalog, depth + 1),
        Expr::ArrayLit(values) => values
            .iter()
            .any(|value| expression_mentions_provider(&value.node, catalog, depth + 1)),
        _ => false,
    }
}

/// Attach one already-lowered static program to its scripted entity.
pub fn attach_papyrus_provider_program(
    world: &mut World,
    entity: EntityId,
    program: PapyrusProviderProgram,
) {
    if let Some(existing) = world.get_mut::<PapyrusProviderProgram>(entity) {
        existing.merge(program);
    } else {
        world.insert(entity, program);
    }
    world.insert(entity, OnInitEvent);
}

/// Attach a translated program with the stable package principal that supplied
/// its compiled script. The owner is retained independently for every merged
/// handler and for any latent continuation it creates.
pub fn attach_owned_papyrus_provider_program(
    world: &mut World,
    entity: EntityId,
    mut program: PapyrusProviderProgram,
    principal: PrincipalId,
) {
    program.set_principal(principal);
    attach_papyrus_provider_program(world, entity, program);
}

fn statements_need_owner_sender(statements: &[PapyrusProviderStatement]) -> bool {
    statements.iter().any(|statement| match statement {
        PapyrusProviderStatement::SendModEvent {
            sender: PapyrusModEventSender::Owner,
            ..
        } => true,
        PapyrusProviderStatement::If {
            then_branch,
            else_branch,
            ..
        } => statements_need_owner_sender(then_branch) || statements_need_owner_sender(else_branch),
        _ => false,
    })
}

fn resolve_mod_event_senders(statements: &mut [PapyrusProviderStatement], owner: Option<FormRef>) {
    for statement in statements {
        match statement {
            PapyrusProviderStatement::SendModEvent { sender, .. }
                if matches!(sender, PapyrusModEventSender::Owner) =>
            {
                *sender = PapyrusModEventSender::Resolved(owner);
            }
            PapyrusProviderStatement::If {
                then_branch,
                else_branch,
                ..
            } => {
                resolve_mod_event_senders(then_branch, owner);
                resolve_mod_event_senders(else_branch, owner);
            }
            _ => {}
        }
    }
}

/// Execute provider handlers only after snapshotting programs and event
/// markers. No ECS query or resource guard survives the host callback.
pub fn papyrus_provider_system(world: &World, dt: f32) {
    let runtime = world
        .try_resource::<PapyrusProviderRuntime>()
        .and_then(|runtime| {
            runtime.callback().map(|callback| {
                (
                    runtime.catalog(),
                    callback,
                    runtime.entity_resolver(),
                    runtime.form_resolver(),
                    runtime.mod_event_publisher(),
                )
            })
        });
    let Some((catalog, callback, entity_resolver, form_resolver, mod_event_publisher)) = runtime
    else {
        return;
    };
    let pending = {
        let mut queue = world.resource_mut::<PapyrusProviderContinuationQueue>();
        std::mem::take(&mut queue.pending)
    };
    let (mod_event_registrations, pending_mod_events) = {
        let mut runtime = world.resource_mut::<PapyrusModEventRuntime>();
        (
            runtime.registrations.clone(),
            std::mem::take(&mut runtime.pending),
        )
    };
    let pending_mod_events = pending_mod_events
        .into_iter()
        .filter_map(|event| {
            LegacySkseVariadicModEventPayload::decode(&event.payload)
                .map(|payload| (event, payload))
        })
        .collect::<Vec<_>>();
    let mut still_pending = Vec::new();
    let mut handlers = Vec::new();
    for mut continuation in pending {
        if !continuation.remaining_seconds.is_finite() || continuation.remaining_seconds < 0.0 {
            log::warn!("Papyrus provider continuation dropped: invalid remaining wait");
            continue;
        }
        continuation.remaining_seconds -= dt.max(0.0);
        if continuation.remaining_seconds > 0.0 {
            still_pending.push(continuation);
        } else {
            handlers.push((
                continuation.statements,
                continuation.locals,
                Vec::new(),
                Vec::new(),
                continuation.principal,
                None,
            ));
        }
    }

    let initialized = world
        .query::<OnInitEvent>()
        .map(|events| {
            events
                .iter()
                .map(|(entity, _)| entity)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let loaded = world
        .query::<OnCellLoadEvent>()
        .map(|events| {
            events
                .iter()
                .map(|(entity, _)| entity)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let activated = world
        .query::<ActivateEvent>()
        .map(|events| {
            events
                .iter()
                .map(|(entity, event)| (entity, event.activator))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let hits = world
        .query::<HitEvent>()
        .map(|events| {
            events
                .iter()
                .map(|(entity, event)| (entity, *event))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let equipment_changes = world
        .query::<EquipmentEventBatch>()
        .map(|events| {
            events
                .iter()
                .map(|(entity, batch)| {
                    (
                        entity,
                        batch
                            .0
                            .iter()
                            .map(|change| (change.item_form_id, change.equipped))
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let trigger_entries = world
        .query::<OnTriggerEnterEvent>()
        .map(|events| {
            events
                .iter()
                .map(|(entity, event)| (entity, event.triggerers.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let updated = world
        .query::<OnUpdateEvent>()
        .map(|events| {
            events
                .iter()
                .map(|(entity, _)| entity)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let owner_form_ids = {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::FormIdPool;

        match (
            world.query::<FormIdComponent>(),
            world.try_resource::<FormIdPool>(),
        ) {
            (Some(forms), Some(pool)) => forms
                .iter()
                .filter_map(|(entity, form)| {
                    pool.resolve(form.0).map(|pair| (entity, pair.local.0))
                })
                .collect::<BTreeMap<_, _>>(),
            _ => BTreeMap::new(),
        }
    };
    let Some(programs) = world.query::<PapyrusProviderProgram>() else {
        return;
    };
    for (entity, program) in programs.iter() {
        let mut enqueue = |event, projected_entity, hit: Option<&HitEvent>, form| {
            for handler in program.handlers_for(event) {
                let projected = handler.projected_locals(projected_entity, hit, form);
                handlers.push((
                    handler.statements.clone(),
                    projected.values,
                    projected.entities,
                    projected.forms,
                    handler.principal.clone(),
                    Some(entity),
                ));
            }
        };
        if initialized.contains(&entity) {
            enqueue(PapyrusProviderEvent::OnInit, None, None, None);
        }
        if loaded.contains(&entity) {
            enqueue(PapyrusProviderEvent::OnLoad, None, None, None);
        }
        if let Some(activator) = activated.get(&entity) {
            enqueue(
                PapyrusProviderEvent::OnActivate,
                Some(*activator),
                None,
                None,
            );
        }
        if let Some(hit) = hits.get(&entity) {
            enqueue(
                PapyrusProviderEvent::OnHit,
                Some(hit.aggressor),
                Some(hit),
                None,
            );
        }
        if let Some(triggerers) = trigger_entries.get(&entity) {
            for triggerer in triggerers {
                enqueue(
                    PapyrusProviderEvent::OnTriggerEnter,
                    Some(*triggerer),
                    None,
                    None,
                );
            }
        }
        if let Some(changes) = equipment_changes.get(&entity) {
            for (form_id, equipped) in changes {
                let event = if *equipped {
                    PapyrusProviderEvent::OnObjectEquipped
                } else {
                    PapyrusProviderEvent::OnObjectUnequipped
                };
                enqueue(event, None, None, Some(*form_id));
            }
        }
        if updated.contains(&entity) {
            enqueue(PapyrusProviderEvent::OnUpdate, None, None, None);
        }
        for (event, payload) in &pending_mod_events {
            for ((registered_entity, principal, registered_event), callback_name) in
                &mod_event_registrations
            {
                if *registered_entity != entity || registered_event != &event.event {
                    continue;
                }
                let Some(custom_handlers) = program.custom_handlers.get(callback_name) else {
                    continue;
                };
                for handler in custom_handlers {
                    if handler.principal.as_ref() != Some(principal) {
                        continue;
                    }
                    let Some(locals) = handler.projected_mod_event_locals(payload) else {
                        log::warn!(
                            "Papyrus ModEvent callback {callback_name} rejected a mismatched typed payload"
                        );
                        continue;
                    };
                    handlers.push((
                        handler.statements.clone(),
                        locals,
                        Vec::new(),
                        Vec::new(),
                        handler.principal.clone(),
                        Some(entity),
                    ));
                }
            }
        }
    }
    drop(programs);

    for (mut statements, mut locals, entity_locals, form_locals, principal, owner) in handlers {
        if statements_reference_local(&statements, PAPYRUS_SELF_LOCAL) {
            let Some(owner) = owner else {
                log::warn!("Papyrus provider handler aborted: self receiver has no owner");
                continue;
            };
            let Some(resolver) = entity_resolver.as_ref() else {
                log::warn!("Papyrus provider handler aborted: entity resolver is unavailable");
                continue;
            };
            match resolver(owner) {
                Ok(entity) => {
                    locals.insert(PAPYRUS_SELF_LOCAL.to_owned(), ScriptValue::Entity(entity));
                }
                Err(error) => {
                    log::warn!("Papyrus provider handler aborted: {error}");
                    continue;
                }
            }
        }
        if statements_need_owner_sender(&statements) {
            let Some(owner_form_id) = owner.and_then(|owner| owner_form_ids.get(&owner).copied())
            else {
                log::warn!("Papyrus SendModEvent aborted: script owner has no stable FormID");
                continue;
            };
            let Some(resolver) = form_resolver.as_ref() else {
                log::warn!("Papyrus SendModEvent aborted: form resolver is unavailable");
                continue;
            };
            let owner_form = match resolver(owner_form_id) {
                Ok(form) => form,
                Err(error) => {
                    log::warn!("Papyrus SendModEvent aborted: {error}");
                    continue;
                }
            };
            resolve_mod_event_senders(&mut statements, Some(owner_form));
        }
        if let Err(error) = validate_provider_statements(&statements, catalog.as_ref(), 0) {
            log::warn!("Papyrus provider handler aborted before dispatch: {error}");
            continue;
        }
        let mut projection_failed = false;
        for (name, entity) in entity_locals {
            let Some(resolver) = entity_resolver.as_ref() else {
                log::warn!("Papyrus provider handler aborted: entity resolver is unavailable");
                projection_failed = true;
                break;
            };
            match resolver(entity) {
                Ok(entity) => {
                    locals.insert(name, ScriptValue::Entity(entity));
                }
                Err(error) => {
                    log::warn!("Papyrus provider handler aborted: {error}");
                    projection_failed = true;
                    break;
                }
            }
        }
        if projection_failed {
            continue;
        }
        for (name, form_id) in form_locals {
            let Some(resolver) = form_resolver.as_ref() else {
                log::warn!("Papyrus provider handler aborted: form resolver is unavailable");
                projection_failed = true;
                break;
            };
            match resolver(form_id) {
                Ok(form) => {
                    locals.insert(name, ScriptValue::Form(form));
                }
                Err(error) => {
                    log::warn!("Papyrus provider handler aborted: {error}");
                    projection_failed = true;
                    break;
                }
            }
        }
        if projection_failed {
            continue;
        }
        let mut registrations = Vec::new();
        match execute_statements(
            &statements,
            callback.as_ref(),
            mod_event_publisher.as_deref(),
            principal.as_ref(),
            &mut locals,
            &mut registrations,
        ) {
            Ok(Some((remaining_seconds, statements))) => {
                apply_mod_event_registrations(world, owner, principal.as_ref(), registrations);
                still_pending.push(PendingPapyrusProviderContinuation {
                    remaining_seconds,
                    statements,
                    locals,
                    principal,
                });
            }
            Ok(None) => {
                apply_mod_event_registrations(world, owner, principal.as_ref(), registrations)
            }
            Err(error) => log::warn!("Papyrus provider handler aborted: {error}"),
        }
    }
    if still_pending.len() > MAX_PROVIDER_CONTINUATIONS {
        log::warn!(
            "Papyrus provider continuation queue exceeded {MAX_PROVIDER_CONTINUATIONS}; dropping newest tails"
        );
        still_pending.truncate(MAX_PROVIDER_CONTINUATIONS);
    }
    world
        .resource_mut::<PapyrusProviderContinuationQueue>()
        .pending = still_pending;
}

fn apply_mod_event_registrations(
    world: &World,
    owner: Option<EntityId>,
    principal: Option<&PrincipalId>,
    actions: Vec<PapyrusModEventRegistrationAction>,
) {
    if actions.is_empty() {
        return;
    }
    let (Some(owner), Some(principal)) = (owner, principal) else {
        log::warn!("Papyrus ModEvent registration ignored without an owned script instance");
        return;
    };
    let mut runtime = world.resource_mut::<PapyrusModEventRuntime>();
    for action in actions {
        match action {
            PapyrusModEventRegistrationAction::Register {
                event_name,
                callback,
            } => {
                let Some(LegacyModEventSubscriptionCommand::Subscribe { event, .. }) =
                    LegacyModEventSubscriptionCommand::subscribe(&event_name, callback.clone())
                else {
                    continue;
                };
                let key = (owner, principal.clone(), event);
                if runtime.registrations.contains_key(&key)
                    || runtime.registrations.len() < MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS
                {
                    runtime.registrations.insert(key, callback);
                } else {
                    log::warn!(
                        "Papyrus ModEvent registration limit of {MAX_PAPYRUS_MOD_EVENT_REGISTRATIONS} exceeded"
                    );
                }
            }
            PapyrusModEventRegistrationAction::Unregister { event_name } => {
                let Some(LegacyModEventSubscriptionCommand::Unsubscribe { event }) =
                    LegacyModEventSubscriptionCommand::unsubscribe(&event_name)
                else {
                    continue;
                };
                runtime
                    .registrations
                    .remove(&(owner, principal.clone(), event));
            }
            PapyrusModEventRegistrationAction::UnregisterAll => {
                runtime
                    .registrations
                    .retain(|(entity, owner_principal, _), _| {
                        *entity != owner || owner_principal != principal
                    });
            }
        }
    }
}

fn validate_provider_statements(
    statements: &[PapyrusProviderStatement],
    catalog: &PapyrusProviderCatalog,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return Err("saved provider continuation nesting exceeds the runtime bound".to_owned());
    }
    for statement in statements {
        match statement {
            PapyrusProviderStatement::Declare { .. }
            | PapyrusProviderStatement::UnregisterAllModEvents => {}
            PapyrusProviderStatement::AssignValue {
                value, value_type, ..
            } => {
                if !matches!(
                    value_type,
                    ScriptValueType::Boolean
                        | ScriptValueType::Integer
                        | ScriptValueType::Float
                        | ScriptValueType::String
                ) {
                    return Err("saved provider expression has a non-scalar result".to_owned());
                }
                validate_provider_value(value, catalog)?;
            }
            PapyrusProviderStatement::RegisterModEvent {
                event_name,
                callback,
            } => {
                LegacyModEventSubscriptionCommand::subscribe(event_name, callback.clone())
                    .ok_or_else(|| "saved ModEvent registration is invalid".to_owned())?;
            }
            PapyrusProviderStatement::UnregisterModEvent { event_name } => {
                LegacyModEventSubscriptionCommand::unsubscribe(event_name)
                    .ok_or_else(|| "saved ModEvent unregistration is invalid".to_owned())?;
            }
            PapyrusProviderStatement::SendModEvent {
                event_name,
                string_arg,
                number_arg,
                sender: _,
            } => {
                validate_mod_event_send_argument(event_name, ScriptValueType::String)?;
                validate_mod_event_send_argument(string_arg, ScriptValueType::String)?;
                validate_mod_event_send_argument(number_arg, ScriptValueType::Float)?;
            }
            PapyrusProviderStatement::AssignCall { call, .. }
            | PapyrusProviderStatement::ArrayWritebackCall { call, .. }
            | PapyrusProviderStatement::Call(call) => validate_provider_call(call, catalog)?,
            PapyrusProviderStatement::Wait { seconds } => {
                if !seconds.is_finite() || *seconds < 0.0 {
                    return Err("saved provider continuation contains an invalid wait".to_owned());
                }
            }
            PapyrusProviderStatement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                validate_provider_condition(condition, catalog, depth + 1)?;
                validate_provider_statements(then_branch, catalog, depth + 1)?;
                validate_provider_statements(else_branch, catalog, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn validate_mod_event_send_argument(
    argument: &PapyrusProviderArgument,
    expected: ScriptValueType,
) -> Result<(), String> {
    match argument {
        PapyrusProviderArgument::Literal(value) if value.matches(expected, false) => Ok(()),
        PapyrusProviderArgument::Local { name, value_type }
            if !name.is_empty()
                && *name == name.to_ascii_lowercase()
                && *value_type == expected =>
        {
            Ok(())
        }
        _ => Err("saved SendModEvent argument is invalid".to_owned()),
    }
}

fn validate_provider_call(
    call: &PapyrusProviderInvocation,
    catalog: &PapyrusProviderCatalog,
) -> Result<(), String> {
    let alias = call
        .route
        .declaration
        .papyrus
        .as_ref()
        .ok_or_else(|| "saved provider route has no Papyrus alias".to_owned())?;
    let live = catalog
        .resolve(&alias.provider, &alias.function)
        .ok_or_else(|| "saved provider route is no longer published".to_owned())?;
    let saved = call.route.declaration();
    let current = live.declaration();
    if live.qualified_name() != call.route.qualified_name()
        || saved.id != current.id
        || saved.component != current.component
        || saved.parameters != current.parameters
        || saved.result != current.result
        || saved.papyrus != current.papyrus
        || call.result != current.result
    {
        return Err("saved provider route does not match the live catalog".to_owned());
    }
    let parameter_offset = if let Some(receiver) = &call.receiver {
        if !alias.provider.eq_ignore_ascii_case(PAPYRUS_SELF_PROVIDER) {
            return Err("saved provider receiver is not an engine self route".to_owned());
        }
        let parameter = current
            .parameters
            .first()
            .filter(|parameter| {
                parameter.value_type == ScriptValueType::Entity && !parameter.optional
            })
            .ok_or_else(|| "saved self route has no required Entity receiver".to_owned())?;
        match &**receiver {
            PapyrusProviderArgument::Local { name, value_type }
                if name == PAPYRUS_SELF_LOCAL && *value_type == parameter.value_type => {}
            _ => return Err("saved provider receiver is invalid".to_owned()),
        }
        1
    } else {
        0
    };
    call.arguments
        .iter()
        .enumerate()
        .map(|(index, argument)| {
            let parameter = current
                .parameters
                .get(index + parameter_offset)
                .ok_or_else(|| "saved provider call has too many arguments".to_owned())?;
            match argument {
                PapyrusProviderArgument::Literal(value) => {
                    if !value.matches(parameter.value_type, parameter.optional) {
                        return Err("saved provider literal argument changed type".to_owned());
                    }
                    Ok(())
                }
                PapyrusProviderArgument::Local { name, value_type } => {
                    if name.is_empty()
                        || *name != name.to_ascii_lowercase()
                        || *value_type != parameter.value_type
                    {
                        return Err("saved provider local argument is invalid".to_owned());
                    }
                    Ok(())
                }
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    if current
        .parameters
        .iter()
        .skip(parameter_offset + call.arguments.len())
        .any(|parameter| !parameter.optional)
    {
        return Err("saved provider call omits a required argument".to_owned());
    }
    validate_storage_util_arguments(call.route.qualified_name(), &call.arguments)
        .map_err(|_| "saved StorageUtil call has an invalid exact signature".to_owned())?;
    validate_legacy_container_arity(call.route.qualified_name(), call.arguments.len())
        .map_err(|_| "saved JContainers call has an invalid exact signature".to_owned())?;
    validate_mod_event_arity(call.route.qualified_name(), call.arguments.len())
        .map_err(|_| "saved ModEvent call has an invalid exact signature".to_owned())?;
    Ok(())
}

fn materialize_provider_arguments(
    call: &PapyrusProviderInvocation,
    locals: &BTreeMap<String, ScriptValue>,
) -> Result<Vec<ScriptValue>, String> {
    let parameter_offset = if call.receiver.is_some() { 1 } else { 0 };
    let mut arguments = Vec::with_capacity(call.arguments.len() + parameter_offset);
    if let Some(receiver) = &call.receiver {
        let parameter = call
            .route
            .declaration()
            .parameters
            .first()
            .filter(|parameter| {
                parameter.value_type == ScriptValueType::Entity && !parameter.optional
            })
            .ok_or_else(|| "provider self receiver declaration is invalid".to_owned())?;
        let PapyrusProviderArgument::Local { name, value_type } = receiver.as_ref() else {
            return Err("provider self receiver must be a local".to_owned());
        };
        if name != PAPYRUS_SELF_LOCAL || *value_type != parameter.value_type {
            return Err("provider self receiver local changed type".to_owned());
        }
        let value = locals
            .get(name)
            .cloned()
            .ok_or_else(|| "translated self receiver was not initialized".to_owned())?;
        if !value.matches(parameter.value_type, parameter.optional) {
            return Err("translated self receiver changed type at execution".to_owned());
        }
        arguments.push(value);
    }
    for (index, argument) in call.arguments.iter().enumerate() {
        let parameter = call
            .route
            .declaration()
            .parameters
            .get(index + parameter_offset)
            .ok_or_else(|| "provider call has too many arguments".to_owned())?;
        let value = match argument {
            PapyrusProviderArgument::Literal(value) => value.clone(),
            PapyrusProviderArgument::Local { name, value_type } => {
                if *value_type != parameter.value_type {
                    return Err("provider local argument declaration changed type".to_owned());
                }
                locals
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("translated local {name} was not initialized"))?
            }
        };
        if !value.matches(parameter.value_type, parameter.optional) {
            return Err(format!(
                "translated argument {} changed type at execution",
                parameter.id.as_str()
            ));
        }
        arguments.push(value);
    }
    call.route
        .declaration()
        .validate_arguments(&arguments)
        .map_err(|error| format!("provider arguments are invalid at execution: {error:?}"))?;
    Ok(arguments)
}

fn validate_provider_condition(
    condition: &PapyrusProviderCondition,
    catalog: &PapyrusProviderCatalog,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return Err("saved provider condition nesting exceeds the runtime bound".to_owned());
    }
    match condition {
        PapyrusProviderCondition::Literal(_) | PapyrusProviderCondition::Local(_) => Ok(()),
        PapyrusProviderCondition::Call(call) => validate_provider_call(call, catalog),
        PapyrusProviderCondition::Not(condition) => {
            validate_provider_condition(condition, catalog, depth + 1)
        }
        PapyrusProviderCondition::And(left, right) | PapyrusProviderCondition::Or(left, right) => {
            validate_provider_condition(left, catalog, depth + 1)?;
            validate_provider_condition(right, catalog, depth + 1)
        }
        PapyrusProviderCondition::Compare { left, right, .. } => {
            validate_provider_value(left, catalog)?;
            validate_provider_value(right, catalog)
        }
    }
}

fn validate_provider_value(
    value: &PapyrusProviderValue,
    catalog: &PapyrusProviderCatalog,
) -> Result<(), String> {
    validate_provider_value_at_depth(value, catalog, 0)
}

fn validate_provider_value_at_depth(
    value: &PapyrusProviderValue,
    catalog: &PapyrusProviderCatalog,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return Err("saved provider value nesting exceeds the runtime bound".to_owned());
    }
    match value {
        PapyrusProviderValue::Call(call) => validate_provider_call(call, catalog),
        PapyrusProviderValue::Binary { left, right, .. } => {
            validate_provider_value_at_depth(left, catalog, depth + 1)?;
            validate_provider_value_at_depth(right, catalog, depth + 1)
        }
        PapyrusProviderValue::Literal(_) | PapyrusProviderValue::Local(_) => Ok(()),
    }
}

fn execute_statements(
    statements: &[PapyrusProviderStatement],
    callback: &PapyrusProviderCallback,
    mod_event_publisher: Option<&PapyrusProviderModEventPublisher>,
    principal: Option<&PrincipalId>,
    locals: &mut BTreeMap<String, ScriptValue>,
    registrations: &mut Vec<PapyrusModEventRegistrationAction>,
) -> Result<Option<(f32, Vec<PapyrusProviderStatement>)>, String> {
    for (index, statement) in statements.iter().enumerate() {
        match statement {
            PapyrusProviderStatement::Declare { name, value } => {
                locals.insert(name.clone(), value.clone());
            }
            PapyrusProviderStatement::AssignCall { name, call } => {
                let arguments = materialize_provider_arguments(call, locals)?;
                let value = callback(principal, call.route.qualified_name(), &arguments)?;
                locals.insert(name.clone(), value);
            }
            PapyrusProviderStatement::AssignValue {
                name,
                value,
                value_type,
            } => {
                let value = evaluate_provider_value(value, callback, principal, locals, 0)?;
                if !value.matches(*value_type, false) {
                    return Err(format!(
                        "provider expression assigned an invalid {value_type:?} value"
                    ));
                }
                locals.insert(name.clone(), value);
            }
            PapyrusProviderStatement::ArrayWritebackCall { name, call } => {
                let arguments = materialize_provider_arguments(call, locals)?;
                let value = callback(principal, call.route.qualified_name(), &arguments)?;
                let expected = call
                    .route
                    .declaration()
                    .parameters
                    .get(2)
                    .map(|parameter| parameter.value_type)
                    .ok_or_else(|| "StorageUtil ListSlice array parameter is missing".to_owned())?;
                if !value.matches(expected, false) {
                    return Err(
                        "StorageUtil ListSlice callback returned an invalid array type".to_owned(),
                    );
                }
                locals.insert(name.clone(), value);
            }
            PapyrusProviderStatement::Call(call) => {
                let arguments = materialize_provider_arguments(call, locals)?;
                callback(principal, call.route.qualified_name(), &arguments)?;
            }
            PapyrusProviderStatement::RegisterModEvent {
                event_name,
                callback,
            } => registrations.push(PapyrusModEventRegistrationAction::Register {
                event_name: event_name.clone(),
                callback: callback.clone(),
            }),
            PapyrusProviderStatement::UnregisterModEvent { event_name } => {
                registrations.push(PapyrusModEventRegistrationAction::Unregister {
                    event_name: event_name.clone(),
                });
            }
            PapyrusProviderStatement::UnregisterAllModEvents => {
                registrations.push(PapyrusModEventRegistrationAction::UnregisterAll);
            }
            PapyrusProviderStatement::SendModEvent {
                event_name,
                string_arg,
                number_arg,
                sender,
            } => {
                let Some(principal) = principal else {
                    return Err(
                        "SendModEvent has no authenticated legacy-script principal".to_owned()
                    );
                };
                let Some(publisher) = mod_event_publisher else {
                    return Err("SendModEvent publisher is unavailable".to_owned());
                };
                let ScriptValue::String(event_name) =
                    materialize_mod_event_argument(event_name, ScriptValueType::String, locals)?
                else {
                    unreachable!("validated SendModEvent event name type")
                };
                let ScriptValue::String(string_arg) =
                    materialize_mod_event_argument(string_arg, ScriptValueType::String, locals)?
                else {
                    unreachable!("validated SendModEvent string argument type")
                };
                let ScriptValue::Float(number_arg) =
                    materialize_mod_event_argument(number_arg, ScriptValueType::Float, locals)?
                else {
                    unreachable!("validated SendModEvent number argument type")
                };
                let PapyrusModEventSender::Resolved(sender) = sender else {
                    return Err("SendModEvent sender was not resolved before execution".to_owned());
                };
                let command =
                    adapt_legacy_send_mod_event(&event_name, string_arg, number_arg, *sender)
                        .map_err(|error| {
                            format!("SendModEvent arguments are invalid: {error:?}")
                        })?;
                publisher(principal, command)?;
            }
            PapyrusProviderStatement::Wait { seconds } => {
                return Ok(Some((*seconds, statements[index + 1..].to_vec())));
            }
            PapyrusProviderStatement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let selected = if evaluate_condition(condition, callback, principal, locals)? {
                    then_branch
                } else {
                    else_branch
                };
                let mut ordered_tail =
                    Vec::with_capacity(selected.len() + statements.len().saturating_sub(index + 1));
                ordered_tail.extend_from_slice(selected);
                ordered_tail.extend_from_slice(&statements[index + 1..]);
                return execute_statements(
                    &ordered_tail,
                    callback,
                    mod_event_publisher,
                    principal,
                    locals,
                    registrations,
                );
            }
        }
    }
    Ok(None)
}

fn materialize_mod_event_argument(
    argument: &PapyrusProviderArgument,
    expected: ScriptValueType,
    locals: &BTreeMap<String, ScriptValue>,
) -> Result<ScriptValue, String> {
    let value = match argument {
        PapyrusProviderArgument::Literal(value) => value.clone(),
        PapyrusProviderArgument::Local { name, value_type } => {
            if *value_type != expected {
                return Err("SendModEvent local declaration changed type".to_owned());
            }
            locals
                .get(name)
                .cloned()
                .ok_or_else(|| format!("translated local {name} was not initialized"))?
        }
    };
    value
        .matches(expected, false)
        .then_some(value)
        .ok_or_else(|| "SendModEvent argument changed type at execution".to_owned())
}

fn evaluate_condition(
    condition: &PapyrusProviderCondition,
    callback: &PapyrusProviderCallback,
    principal: Option<&PrincipalId>,
    locals: &BTreeMap<String, ScriptValue>,
) -> Result<bool, String> {
    match condition {
        PapyrusProviderCondition::Not(condition) => {
            return Ok(!evaluate_condition(condition, callback, principal, locals)?);
        }
        PapyrusProviderCondition::And(left, right) => {
            return Ok(evaluate_condition(left, callback, principal, locals)?
                && evaluate_condition(right, callback, principal, locals)?);
        }
        PapyrusProviderCondition::Or(left, right) => {
            return Ok(evaluate_condition(left, callback, principal, locals)?
                || evaluate_condition(right, callback, principal, locals)?);
        }
        PapyrusProviderCondition::Compare {
            left,
            operator,
            right,
        } => {
            let left = evaluate_condition_value(left, callback, principal, locals)?;
            let right = evaluate_condition_value(right, callback, principal, locals)?;
            return compare_condition_values(&left, *operator, &right);
        }
        _ => {}
    }
    let value = match condition {
        PapyrusProviderCondition::Literal(value) => return Ok(*value),
        PapyrusProviderCondition::Local(name) => locals
            .get(name)
            .cloned()
            .ok_or_else(|| format!("translated local {name} was not initialized"))?,
        PapyrusProviderCondition::Call(call) => {
            let arguments = materialize_provider_arguments(call, locals)?;
            callback(principal, call.route.qualified_name(), &arguments)?
        }
        PapyrusProviderCondition::Not(_)
        | PapyrusProviderCondition::And(_, _)
        | PapyrusProviderCondition::Or(_, _)
        | PapyrusProviderCondition::Compare { .. } => unreachable!("handled above"),
    };
    match value {
        ScriptValue::Boolean(value) => Ok(value),
        _ => Err("provider returned a non-boolean condition result".to_owned()),
    }
}

fn evaluate_condition_value(
    value: &PapyrusProviderValue,
    callback: &PapyrusProviderCallback,
    principal: Option<&PrincipalId>,
    locals: &BTreeMap<String, ScriptValue>,
) -> Result<ScriptValue, String> {
    evaluate_provider_value(value, callback, principal, locals, 0)
}

fn evaluate_provider_value(
    value: &PapyrusProviderValue,
    callback: &PapyrusProviderCallback,
    principal: Option<&PrincipalId>,
    locals: &BTreeMap<String, ScriptValue>,
    depth: usize,
) -> Result<ScriptValue, String> {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return Err("provider expression nesting exceeds the runtime bound".to_owned());
    }
    match value {
        PapyrusProviderValue::Literal(value) => Ok(value.clone()),
        PapyrusProviderValue::Local(name) => locals
            .get(name)
            .cloned()
            .ok_or_else(|| format!("translated local {name} was not initialized")),
        PapyrusProviderValue::Call(call) => {
            let arguments = materialize_provider_arguments(call, locals)?;
            callback(principal, call.route.qualified_name(), &arguments)
        }
        PapyrusProviderValue::Binary {
            left,
            operator,
            right,
        } => {
            let left = evaluate_provider_value(left, callback, principal, locals, depth + 1)?;
            let right = evaluate_provider_value(right, callback, principal, locals, depth + 1)?;
            apply_provider_arithmetic(left, *operator, right)
        }
    }
}

fn apply_provider_arithmetic(
    left: ScriptValue,
    operator: PapyrusProviderArithmetic,
    right: ScriptValue,
) -> Result<ScriptValue, String> {
    match (left, operator, right) {
        (
            ScriptValue::Integer(left),
            PapyrusProviderArithmetic::Add,
            ScriptValue::Integer(right),
        ) => left
            .checked_add(right)
            .map(ScriptValue::Integer)
            .ok_or_else(|| "provider integer addition overflowed".to_owned()),
        (
            ScriptValue::Integer(left),
            PapyrusProviderArithmetic::Sub,
            ScriptValue::Integer(right),
        ) => left
            .checked_sub(right)
            .map(ScriptValue::Integer)
            .ok_or_else(|| "provider integer subtraction overflowed".to_owned()),
        (
            ScriptValue::Integer(left),
            PapyrusProviderArithmetic::Mul,
            ScriptValue::Integer(right),
        ) => left
            .checked_mul(right)
            .map(ScriptValue::Integer)
            .ok_or_else(|| "provider integer multiplication overflowed".to_owned()),
        (
            ScriptValue::Integer(left),
            PapyrusProviderArithmetic::Div,
            ScriptValue::Integer(right),
        ) => {
            if right == 0 {
                return Err("provider integer division by zero".to_owned());
            }
            left.checked_div(right)
                .map(ScriptValue::Integer)
                .ok_or_else(|| "provider integer division overflowed".to_owned())
        }
        (
            ScriptValue::Integer(left),
            PapyrusProviderArithmetic::Mod,
            ScriptValue::Integer(right),
        ) => {
            if right == 0 {
                return Err("provider integer remainder by zero".to_owned());
            }
            left.checked_rem(right)
                .map(ScriptValue::Integer)
                .ok_or_else(|| "provider integer remainder overflowed".to_owned())
        }
        (ScriptValue::Float(left), PapyrusProviderArithmetic::Add, ScriptValue::Float(right)) => {
            finite_float_result(left + right, "addition")
        }
        (ScriptValue::Float(left), PapyrusProviderArithmetic::Sub, ScriptValue::Float(right)) => {
            finite_float_result(left - right, "subtraction")
        }
        (ScriptValue::Float(left), PapyrusProviderArithmetic::Mul, ScriptValue::Float(right)) => {
            finite_float_result(left * right, "multiplication")
        }
        (ScriptValue::Float(left), PapyrusProviderArithmetic::Div, ScriptValue::Float(right)) => {
            if right == 0.0 {
                return Err("provider float division by zero".to_owned());
            }
            finite_float_result(left / right, "division")
        }
        (ScriptValue::Float(left), PapyrusProviderArithmetic::Mod, ScriptValue::Float(right)) => {
            if right == 0.0 {
                return Err("provider float remainder by zero".to_owned());
            }
            finite_float_result(left % right, "remainder")
        }
        (
            ScriptValue::String(left),
            PapyrusProviderArithmetic::StrCat,
            ScriptValue::String(right),
        ) => {
            let value = format!("{left}{right}");
            let result = ScriptValue::String(value);
            result
                .matches(ScriptValueType::String, false)
                .then_some(result)
                .ok_or_else(|| "provider string concatenation exceeded the script limit".to_owned())
        }
        _ => Err("provider expression operands changed type at execution".to_owned()),
    }
}

fn finite_float_result(value: f32, operation: &str) -> Result<ScriptValue, String> {
    value
        .is_finite()
        .then_some(ScriptValue::Float(value))
        .ok_or_else(|| format!("provider float {operation} produced a non-finite result"))
}

fn compare_condition_values(
    left: &ScriptValue,
    operator: PapyrusProviderComparison,
    right: &ScriptValue,
) -> Result<bool, String> {
    match (left, right) {
        (ScriptValue::Boolean(left), ScriptValue::Boolean(right)) => match operator {
            PapyrusProviderComparison::Equal => Ok(left == right),
            PapyrusProviderComparison::NotEqual => Ok(left != right),
            _ => Err("ordered boolean provider comparison reached execution".to_owned()),
        },
        (ScriptValue::Integer(left), ScriptValue::Integer(right)) => {
            Ok(compare_ordered(*left, operator, *right))
        }
        (ScriptValue::Float(left), ScriptValue::Float(right)) => {
            Ok(compare_ordered(*left, operator, *right))
        }
        (ScriptValue::String(left), ScriptValue::String(right)) => match operator {
            PapyrusProviderComparison::Equal => Ok(left == right),
            PapyrusProviderComparison::NotEqual => Ok(left != right),
            _ => Err("ordered string provider comparison reached execution".to_owned()),
        },
        (ScriptValue::Entity(left), ScriptValue::Entity(right)) => match operator {
            PapyrusProviderComparison::Equal => Ok(left == right),
            PapyrusProviderComparison::NotEqual => Ok(left != right),
            _ => Err("ordered entity provider comparison reached execution".to_owned()),
        },
        (ScriptValue::None, ScriptValue::None) => match operator {
            PapyrusProviderComparison::Equal => Ok(true),
            PapyrusProviderComparison::NotEqual => Ok(false),
            _ => Err("ordered null entity provider comparison reached execution".to_owned()),
        },
        (ScriptValue::Entity(_), ScriptValue::None)
        | (ScriptValue::None, ScriptValue::Entity(_)) => match operator {
            PapyrusProviderComparison::Equal => Ok(false),
            PapyrusProviderComparison::NotEqual => Ok(true),
            _ => Err("ordered nullable entity provider comparison reached execution".to_owned()),
        },
        _ => Err("provider comparison operands changed type at execution".to_owned()),
    }
}

fn compare_ordered<T: PartialOrd + PartialEq>(
    left: T,
    operator: PapyrusProviderComparison,
    right: T,
) -> bool {
    match operator {
        PapyrusProviderComparison::Equal => left == right,
        PapyrusProviderComparison::NotEqual => left != right,
        PapyrusProviderComparison::Less => left < right,
        PapyrusProviderComparison::LessOrEqual => left <= right,
        PapyrusProviderComparison::Greater => left > right,
        PapyrusProviderComparison::GreaterOrEqual => left >= right,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use byroredux_papyrus::{ast::ScriptItem, parse_script};
    use byroredux_sdk::{
        identity::{ComponentId, ScriptFunctionId, ScriptParameterId},
        script_function::{PapyrusFunctionAlias, ScriptParameterDeclaration},
    };

    fn declaration() -> ScriptFunctionDeclaration {
        ScriptFunctionDeclaration {
            id: ScriptFunctionId::new("weather-at").unwrap(),
            component: ComponentId::new("runtime").unwrap(),
            parameters: vec![
                ScriptParameterDeclaration {
                    id: ScriptParameterId::new("day").unwrap(),
                    value_type: ScriptValueType::Integer,
                    optional: false,
                },
                ScriptParameterDeclaration {
                    id: ScriptParameterId::new("fallback").unwrap(),
                    value_type: ScriptValueType::String,
                    optional: true,
                },
            ],
            result: Some(ScriptResultDeclaration {
                value_type: ScriptValueType::String,
                optional: false,
            }),
            papyrus: Some(PapyrusFunctionAlias {
                provider: "WeatherNative".to_owned(),
                function: "WeatherAt".to_owned(),
            }),
            description: "Return weather at a day index".to_owned(),
        }
    }

    fn boolean_declaration() -> ScriptFunctionDeclaration {
        ScriptFunctionDeclaration {
            id: ScriptFunctionId::new("is-storm").unwrap(),
            component: ComponentId::new("runtime").unwrap(),
            parameters: Vec::new(),
            result: Some(ScriptResultDeclaration {
                value_type: ScriptValueType::Boolean,
                optional: false,
            }),
            papyrus: Some(PapyrusFunctionAlias {
                provider: "WeatherNative".to_owned(),
                function: "IsStorm".to_owned(),
            }),
            description: "Whether the current weather is a storm".to_owned(),
        }
    }

    fn entity_declaration() -> ScriptFunctionDeclaration {
        ScriptFunctionDeclaration {
            id: ScriptFunctionId::new("inspect-entity").unwrap(),
            component: ComponentId::new("runtime").unwrap(),
            parameters: vec![ScriptParameterDeclaration {
                id: ScriptParameterId::new("target").unwrap(),
                value_type: ScriptValueType::Entity,
                optional: false,
            }],
            result: Some(ScriptResultDeclaration {
                value_type: ScriptValueType::String,
                optional: false,
            }),
            papyrus: Some(PapyrusFunctionAlias {
                provider: "WeatherNative".to_owned(),
                function: "InspectEntity".to_owned(),
            }),
            description: "Inspect one opaque entity handle".to_owned(),
        }
    }

    fn self_declaration() -> ScriptFunctionDeclaration {
        ScriptFunctionDeclaration {
            id: ScriptFunctionId::new("touch-self").unwrap(),
            component: ComponentId::new("runtime").unwrap(),
            parameters: vec![
                ScriptParameterDeclaration {
                    id: ScriptParameterId::new("receiver").unwrap(),
                    value_type: ScriptValueType::Entity,
                    optional: false,
                },
                ScriptParameterDeclaration {
                    id: ScriptParameterId::new("value").unwrap(),
                    value_type: ScriptValueType::Integer,
                    optional: false,
                },
            ],
            result: None,
            papyrus: Some(PapyrusFunctionAlias {
                provider: PAPYRUS_SELF_PROVIDER.to_owned(),
                function: "Touch".to_owned(),
            }),
            description: "Touch the current script owner".to_owned(),
        }
    }

    fn form_declaration() -> ScriptFunctionDeclaration {
        ScriptFunctionDeclaration {
            id: ScriptFunctionId::new("inspect-form").unwrap(),
            component: ComponentId::new("runtime").unwrap(),
            parameters: vec![ScriptParameterDeclaration {
                id: ScriptParameterId::new("form").unwrap(),
                value_type: ScriptValueType::Form,
                optional: false,
            }],
            result: Some(ScriptResultDeclaration {
                value_type: ScriptValueType::String,
                optional: false,
            }),
            papyrus: Some(PapyrusFunctionAlias {
                provider: "WeatherNative".to_owned(),
                function: "InspectForm".to_owned(),
            }),
            description: "Inspect one stable authored form".to_owned(),
        }
    }

    fn expression(source: &str) -> Expr {
        let source = format!("ScriptName Fixture\nEvent OnInit()\n  {source}\nEndEvent\n");
        let (script, errors) = parse_script(&source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let ScriptItem::Event(event) = &script.body[0].node else {
            panic!("expected event");
        };
        let byroredux_papyrus::ast::Stmt::ExprStmt(expression) = &event.body[0].node else {
            panic!("expected expression statement");
        };
        expression.node.clone()
    }

    fn catalog() -> PapyrusProviderCatalog {
        let mut catalog = PapyrusProviderCatalog::default();
        catalog
            .insert(
                &ExtensionId::new("org.example.weather").unwrap(),
                &declaration(),
            )
            .unwrap();
        catalog
            .insert(
                &ExtensionId::new("org.example.weather").unwrap(),
                &boolean_declaration(),
            )
            .unwrap();
        catalog
            .insert(
                &ExtensionId::new("org.example.weather").unwrap(),
                &entity_declaration(),
            )
            .unwrap();
        catalog
            .insert(
                &ExtensionId::new("org.example.weather").unwrap(),
                &form_declaration(),
            )
            .unwrap();
        catalog
    }

    fn self_catalog() -> PapyrusProviderCatalog {
        let mut catalog = catalog();
        catalog
            .insert(
                &ExtensionId::new("org.example.self").unwrap(),
                &self_declaration(),
            )
            .unwrap();
        catalog
    }

    #[test]
    fn self_receiver_lowers_to_an_explicit_entity_argument() {
        let source = r#"
            ScriptName Fixture
            Event OnInit()
                self.Touch(7)
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let program = lower_provider_program(&script, &self_catalog())
            .unwrap()
            .unwrap();
        let [PapyrusProviderStatement::Call(call)] = program.handler(PapyrusProviderEvent::OnInit)
        else {
            panic!("expected one self provider call");
        };
        assert_eq!(
            call.receiver,
            Some(Box::new(PapyrusProviderArgument::Local {
                name: PAPYRUS_SELF_LOCAL.to_owned(),
                value_type: ScriptValueType::Entity,
            }))
        );
        assert_eq!(
            call.arguments,
            [PapyrusProviderArgument::Literal(ScriptValue::Integer(7))]
        );
    }

    #[test]
    fn self_receiver_dispatch_resolves_the_current_owner_handle() {
        let source = r#"
            ScriptName Fixture
            Event OnInit()
                self.Touch(7)
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let program = lower_provider_program(&script, &self_catalog())
            .unwrap()
            .unwrap();
        let mut world = World::new();
        crate::register(&mut world);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_for_callback = Arc::clone(&calls);
        let callback = Arc::new(
            move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
                calls_for_callback
                    .lock()
                    .unwrap()
                    .push((route.to_owned(), arguments.to_vec()));
                Ok(ScriptValue::None)
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(self_catalog()), Some(callback));
        let resolver = Arc::new(|entity: EntityId| {
            EntityRef::new(9, u64::from(entity) + 1).ok_or_else(|| "invalid test entity".to_owned())
        }) as Arc<PapyrusProviderEntityResolver>;
        set_papyrus_provider_entity_resolver(&world, Some(resolver));
        let owner = world.spawn();
        attach_papyrus_provider_program(&mut world, owner, program);
        world.insert(owner, OnInitEvent);
        papyrus_provider_system(&world, 0.0);

        assert_eq!(
            calls.lock().unwrap().as_slice(),
            [(
                "ext.org.example.self.touch-self".to_owned(),
                vec![
                    ScriptValue::Entity(EntityRef::new(9, u64::from(owner) + 1).unwrap()),
                    ScriptValue::Integer(7),
                ],
            )]
        );
    }

    #[test]
    fn self_receiver_handlers_reject_latent_owner_use_until_continuations_persist_it() {
        let source = r#"
            ScriptName Fixture
            Event OnInit()
                self.Touch(7)
                Utility.Wait(1.0)
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            lower_provider_program(&script, &self_catalog()),
            Err(PapyrusProviderProgramError::UnsupportedStatement)
        );
    }

    struct PexBytes {
        bytes: Vec<u8>,
        strings: Vec<String>,
    }

    impl PexBytes {
        fn new() -> Self {
            Self {
                bytes: Vec::new(),
                strings: Vec::new(),
            }
        }

        fn u8(&mut self, value: u8) {
            self.bytes.push(value);
        }

        fn u16(&mut self, value: u16) {
            self.bytes.extend_from_slice(&value.to_be_bytes());
        }

        fn u32(&mut self, value: u32) {
            self.bytes.extend_from_slice(&value.to_be_bytes());
        }

        fn i64(&mut self, value: i64) {
            self.bytes.extend_from_slice(&value.to_be_bytes());
        }

        fn string(&mut self, value: &str) {
            self.u16(value.len() as u16);
            self.bytes.extend_from_slice(value.as_bytes());
        }

        fn intern(&mut self, value: &str) {
            if !self.strings.iter().any(|candidate| candidate == value) {
                self.strings.push(value.to_owned());
            }
        }

        fn string_index(&mut self, value: &str) {
            let index = self
                .strings
                .iter()
                .position(|candidate| candidate == value)
                .expect("PEX fixture string was pre-interned");
            self.u16(index as u16);
        }
    }

    fn provider_call_pex_bytes() -> Vec<u8> {
        use byroredux_pex::OpCode;

        let mut writer = PexBytes::new();
        for value in [
            "ProviderFixture",
            "ObjectReference",
            "",
            "None",
            "OnLoad",
            "WeatherNative",
            "WeatherAt",
            "::nonevar",
            "clear",
        ] {
            writer.intern(value);
        }

        // PEX magic is always little-endian; this marker selects Skyrim's
        // big-endian layout for every later multi-byte field.
        writer
            .bytes
            .extend_from_slice(&0xDEC0_57FA_u32.to_le_bytes());
        writer.u8(3);
        writer.u8(2);
        writer.u16(0);
        writer.i64(1_700_000_000);
        writer.string("ProviderFixture.psc");
        writer.string("byroredux");
        writer.string("provider conformance");

        let strings = writer.strings.clone();
        writer.u16(strings.len() as u16);
        for value in &strings {
            writer.string(value);
        }

        writer.u8(0); // no debug metadata
        writer.u16(0); // no user flags
        writer.u16(1); // one object
        writer.string_index("ProviderFixture");
        writer.u32(0); // ignored object size
        writer.string_index("ObjectReference");
        writer.string_index("");
        writer.u32(0);
        writer.string_index(""); // auto state
        writer.u16(0); // variables
        writer.u16(0); // properties
        writer.u16(1); // states
        writer.string_index("");
        writer.u16(1); // functions
        writer.string_index("OnLoad");
        writer.string_index("None");
        writer.string_index("");
        writer.u32(0);
        writer.u8(0);
        writer.u16(0); // parameters
        writer.u16(0); // locals
        writer.u16(2); // instructions

        writer.u8(OpCode::CallStatic as u8);
        for value in ["WeatherNative", "WeatherAt", "::nonevar"] {
            writer.u8(1); // identifier
            writer.string_index(value);
        }
        writer.u8(3); // integer vararg count
        writer.u32(2);
        writer.u8(3); // integer literal
        writer.u32(4);
        writer.u8(2); // string literal
        writer.string_index("clear");
        writer.u8(OpCode::Return as u8);
        writer.u8(0); // None

        writer.bytes
    }

    fn send_mod_event_pex_bytes() -> Vec<u8> {
        use byroredux_pex::OpCode;

        let mut writer = PexBytes::new();
        for value in [
            "SendFixture",
            "ObjectReference",
            "",
            "None",
            "OnLoad",
            "SendModEvent",
            "self",
            "::nonevar",
            "ByroReady",
        ] {
            writer.intern(value);
        }

        writer
            .bytes
            .extend_from_slice(&0xDEC0_57FA_u32.to_le_bytes());
        writer.u8(3);
        writer.u8(2);
        writer.u16(0);
        writer.i64(1_700_000_000);
        writer.string("SendFixture.psc");
        writer.string("byroredux");
        writer.string("instance ModEvent conformance");

        let strings = writer.strings.clone();
        writer.u16(strings.len() as u16);
        for value in &strings {
            writer.string(value);
        }

        writer.u8(0);
        writer.u16(0);
        writer.u16(1);
        writer.string_index("SendFixture");
        writer.u32(0);
        writer.string_index("ObjectReference");
        writer.string_index("");
        writer.u32(0);
        writer.string_index("");
        writer.u16(0);
        writer.u16(0);
        writer.u16(1);
        writer.string_index("");
        writer.u16(1);
        writer.string_index("OnLoad");
        writer.string_index("None");
        writer.string_index("");
        writer.u32(0);
        writer.u8(0);
        writer.u16(0);
        writer.u16(0);
        writer.u16(2);

        writer.u8(OpCode::CallMethod as u8);
        for value in ["SendModEvent", "self", "::nonevar"] {
            writer.u8(1);
            writer.string_index(value);
        }
        writer.u8(3);
        writer.u32(1);
        writer.u8(2);
        writer.string_index("ByroReady");
        writer.u8(OpCode::Return as u8);
        writer.u8(0);

        writer.bytes
    }

    #[test]
    fn static_calls_resolve_case_insensitively_and_reorder_named_arguments() {
        let call = lower_provider_call(
            &expression("WEATHERNATIVE.weatherat(fallback = \"clear\", day = 4)"),
            &catalog(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            call.route.qualified_name(),
            "ext.org.example.weather.weather-at"
        );
        assert_eq!(
            call.arguments,
            [
                ScriptValue::Integer(4),
                ScriptValue::String("clear".to_owned())
            ]
        );
    }

    #[test]
    fn known_providers_fail_closed_on_unknown_functions_and_bad_arguments() {
        assert!(matches!(
            lower_provider_call(&expression("WeatherNative.Missing(4)"), &catalog()),
            Err(PapyrusProviderLowerError::UnknownFunction { .. })
        ));
        assert!(matches!(
            lower_provider_call(&expression("WeatherNative.WeatherAt(\"four\")"), &catalog()),
            Err(PapyrusProviderLowerError::UnsupportedArgument { .. })
        ));
        assert!(matches!(
            lower_provider_call(
                &expression("WeatherNative.WeatherAt(fallback = \"x\")"),
                &catalog()
            ),
            Err(PapyrusProviderLowerError::MissingParameter(_))
                | Err(PapyrusProviderLowerError::InvalidArguments(_))
        ));
    }

    #[test]
    fn recognized_extender_call_without_an_executable_route_fails_closed() {
        let empty = PapyrusProviderCatalog::default();
        assert_eq!(
            lower_provider_call(&expression("SKSE.GetVersion()"), &empty),
            Err(PapyrusProviderLowerError::UnknownFunction {
                provider: "SKSE".to_owned(),
                function: "GetVersion".to_owned(),
            })
        );

        let source = r#"
            ScriptName Fixture
            Event OnLoad()
                SKSE.UnknownNative()
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            lower_provider_program(&script, &empty),
            Err(PapyrusProviderProgramError::Call(
                PapyrusProviderLowerError::UnknownFunction {
                    provider: "SKSE".to_owned(),
                    function: "UnknownNative".to_owned(),
                }
            ))
        );
    }

    #[test]
    fn engine_compatibility_catalog_lowers_read_only_input_aliases() {
        let catalog = PapyrusProviderCatalog::engine_compatibility();
        let key = lower_provider_call(&expression("Input.GetMappedKey(\"Forward\")"), &catalog)
            .unwrap()
            .unwrap();
        assert_eq!(
            key.route.qualified_name(),
            byroredux_sdk::compatibility::PAPYRUS_INPUT_GET_MAPPED_KEY_ROUTE
        );
        assert_eq!(key.arguments, [ScriptValue::String("Forward".to_owned())]);
        let control = lower_provider_call(&expression("Input.GetMappedControl(17)"), &catalog)
            .unwrap()
            .unwrap();
        assert_eq!(
            control.route.qualified_name(),
            byroredux_sdk::compatibility::PAPYRUS_INPUT_GET_MAPPED_CONTROL_ROUTE
        );
        assert_eq!(control.arguments, [ScriptValue::Integer(17)]);
        assert_eq!(
            catalog
                .resolve("Input", "GetMappedKey")
                .unwrap()
                .declaration()
                .parameters
                .len(),
            2
        );
        assert!(matches!(
            lower_provider_call(&expression("Input.TapKey(17)"), &catalog),
            Err(PapyrusProviderLowerError::UnknownFunction { .. })
        ));
        let menu = lower_provider_call(&expression("UI.IsMenuOpen(\"InventoryMenu\")"), &catalog)
            .unwrap()
            .unwrap();
        assert_eq!(
            menu.route.qualified_name(),
            byroredux_sdk::compatibility::PAPYRUS_UI_IS_MENU_OPEN_ROUTE
        );
        assert_eq!(
            menu.arguments,
            [ScriptValue::String("InventoryMenu".to_owned())]
        );
        assert!(matches!(
            lower_provider_call(
                &expression("UI.IsMenuRegistered(\"InventoryMenu\")"),
                &catalog
            ),
            Err(PapyrusProviderLowerError::UnknownFunction { .. })
        ));
    }

    #[test]
    fn engine_compatibility_catalog_lowers_exact_game_storage_and_container_aliases() {
        let mut catalog = PapyrusProviderCatalog::engine_compatibility();
        assert!(PapyrusProviderRuntime::default()
            .catalog()
            .resolve("Game", "GetModCount")
            .is_some());
        let call = lower_provider_call(&expression("Game.GetModByName(\"Update.esm\")"), &catalog)
            .unwrap()
            .unwrap();
        assert_eq!(
            call.route.qualified_name(),
            byroredux_sdk::compatibility::PAPYRUS_GAME_GET_MOD_BY_NAME_ROUTE
        );
        assert_eq!(
            call.arguments,
            [ScriptValue::String("Update.esm".to_owned())]
        );
        let form_from_file = lower_provider_call(
            &expression("Game.GetFormFromFile(4660, \"Update.esm\")"),
            &catalog,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            form_from_file.route.qualified_name(),
            byroredux_sdk::compatibility::PAPYRUS_GAME_GET_FORM_FROM_FILE_ROUTE
        );
        assert_eq!(
            form_from_file.arguments,
            [
                ScriptValue::Integer(4660),
                ScriptValue::String("Update.esm".to_owned()),
            ]
        );
        let player = lower_provider_call(&expression("Game.GetPlayer()"), &catalog)
            .unwrap()
            .unwrap();
        assert_eq!(
            player.route.qualified_name(),
            byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE
        );
        assert!(player.arguments.is_empty());
        assert_eq!(
            player.result,
            Some(ScriptResultDeclaration {
                value_type: ScriptValueType::Entity,
                optional: true,
            })
        );
        let storage = lower_provider_call(
            &expression("StorageUtil.GetIntValue(None, \"Score\", -1)"),
            &catalog,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            storage.route.qualified_name(),
            byroredux_sdk::compatibility::PAPYRUS_STORAGE_UTIL_GET_INT_VALUE_ROUTE
        );
        assert_eq!(
            storage.arguments,
            [
                ScriptValue::None,
                ScriptValue::String("Score".to_owned()),
                ScriptValue::Integer(-1),
            ]
        );
        let float = lower_provider_call(
            &expression("StorageUtil.AdjustFloatValue(None, \"Weight\", 0.5)"),
            &catalog,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            float.route.qualified_name(),
            byroredux_sdk::compatibility::PAPYRUS_STORAGE_UTIL_ADJUST_FLOAT_VALUE_ROUTE
        );
        assert_eq!(
            float.arguments,
            [
                ScriptValue::None,
                ScriptValue::String("Weight".to_owned()),
                ScriptValue::Float(0.5),
            ]
        );
        let form = lower_provider_call(
            &expression("StorageUtil.SetFormValue(None, \"Owner\", None)"),
            &catalog,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            form.route.qualified_name(),
            byroredux_sdk::compatibility::PAPYRUS_STORAGE_UTIL_SET_FORM_VALUE_ROUTE
        );
        assert_eq!(
            form.arguments,
            [
                ScriptValue::None,
                ScriptValue::String("Owner".to_owned()),
                ScriptValue::None,
            ]
        );
        let pluck = lower_provider_call(
            &expression("StorageUtil.PluckFormValue(None, \"Owner\", None)"),
            &catalog,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            pluck.route.qualified_name(),
            byroredux_sdk::compatibility::PAPYRUS_STORAGE_UTIL_PLUCK_FORM_VALUE_ROUTE
        );
        let list = lower_provider_call(
            &expression("StorageUtil.IntListAdd(None, \"Recent\", 7, false)"),
            &catalog,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            list.route.qualified_name(),
            "byro.storage.compat.storage-util.list-int-add"
        );
        assert_eq!(
            list.arguments,
            [
                ScriptValue::None,
                ScriptValue::String("Recent".to_owned()),
                ScriptValue::Integer(7),
                ScriptValue::Boolean(false),
            ]
        );
        let list_pluck = lower_provider_call(
            &expression("StorageUtil.StringListPluck(None, \"Labels\", 2, \"missing\")"),
            &catalog,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            list_pluck.route.qualified_name(),
            "byro.storage.compat.storage-util.list-string-pluck"
        );
        assert_eq!(
            list_pluck.arguments,
            [
                ScriptValue::None,
                ScriptValue::String("Labels".to_owned()),
                ScriptValue::Integer(2),
                ScriptValue::String("missing".to_owned()),
            ]
        );
        let list_remove = lower_provider_call(
            &expression("StorageUtil.FormListRemove(None, \"Owners\", None, true)"),
            &catalog,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            list_remove.route.qualified_name(),
            "byro.storage.compat.storage-util.list-form-remove"
        );
        assert_eq!(
            list_remove.arguments,
            [
                ScriptValue::None,
                ScriptValue::String("Owners".to_owned()),
                ScriptValue::None,
                ScriptValue::Boolean(true),
            ]
        );
        let list_resize = lower_provider_call(
            &expression("StorageUtil.FloatListResize(None, \"Ratios\", 4, 1.5)"),
            &catalog,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            list_resize.route.qualified_name(),
            "byro.storage.compat.storage-util.list-float-resize"
        );
        assert_eq!(
            list_resize.arguments,
            [
                ScriptValue::None,
                ScriptValue::String("Ratios".to_owned()),
                ScriptValue::Integer(4),
                ScriptValue::Float(1.5),
            ]
        );
        let list_sort = lower_provider_call(
            &expression("StorageUtil.FormListSort(None, \"Owners\")"),
            &catalog,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            list_sort.route.qualified_name(),
            "byro.storage.compat.storage-util.list-form-sort"
        );
        assert!(list_sort.result.is_none());
        let list_random = lower_provider_call(
            &expression("StorageUtil.FormListRandom(None, \"Owners\")"),
            &catalog,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            list_random.route.qualified_name(),
            "byro.storage.compat.storage-util.list-form-random"
        );
        assert_eq!(
            list_random.arguments,
            [ScriptValue::None, ScriptValue::String("Owners".to_owned()),]
        );
        let list_array = lower_provider_call(
            &expression("StorageUtil.IntListToArray(None, \"Numbers\")"),
            &catalog,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            list_array.route.qualified_name(),
            "byro.storage.compat.storage-util.list-int-to-array"
        );
        assert_eq!(
            list_array.result.unwrap().value_type,
            ScriptValueType::IntegerArray
        );
        let list_filter = lower_provider_call(
            &expression("StorageUtil.FormListFilterByType(None, \"Owners\", 41, false)"),
            &catalog,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            list_filter.route.qualified_name(),
            byroredux_sdk::compatibility::PAPYRUS_STORAGE_UTIL_FORM_FILTER_BY_TYPE_ROUTE
        );
        assert_eq!(
            list_filter.arguments,
            [
                ScriptValue::None,
                ScriptValue::String("Owners".to_owned()),
                ScriptValue::Integer(41),
                ScriptValue::Boolean(false),
            ]
        );
        assert_eq!(
            list_filter.result.unwrap().value_type,
            ScriptValueType::FormArray
        );
        let prefix_route = catalog.resolve("StorageUtil", "CountAllPrefix").unwrap();
        assert_eq!(
            prefix_route.declaration().parameters[0].id.as_str(),
            "prefix"
        );
        let prefix_expression = expression("StorageUtil.CountAllPrefix(\"my_mod.\")");
        let prefix = lower_provider_call(&prefix_expression, &catalog)
            .unwrap()
            .unwrap();
        assert_eq!(
            prefix.route.qualified_name(),
            "byro.storage.compat.storage-util.prefix-count-all"
        );
        assert_eq!(
            prefix.arguments,
            [ScriptValue::String("my_mod.".to_owned())]
        );
        let container = lower_provider_call(&expression("JArray.getInt(4, -1, 7)"), &catalog)
            .unwrap()
            .unwrap();
        assert_eq!(
            container.route.qualified_name(),
            "byro.legacy-containers.compat.jarray-get-int"
        );
        assert_eq!(
            container.arguments,
            [
                ScriptValue::Integer(4),
                ScriptValue::Integer(-1),
                ScriptValue::Integer(7),
            ]
        );
        let mod_event =
            lower_provider_call(&expression("ModEvent.PushString(7, \"ready\")"), &catalog)
                .unwrap()
                .unwrap();
        assert_eq!(
            mod_event.route.qualified_name(),
            "byro.events.compat.mod-event.mod-event-push-string"
        );
        assert_eq!(
            mod_event.arguments,
            [
                ScriptValue::Integer(7),
                ScriptValue::String("ready".to_owned()),
            ]
        );
        assert!(matches!(
            lower_provider_call(&expression("ModEvent.PushForm(7)"), &catalog),
            Err(PapyrusProviderLowerError::MissingParameter(_))
        ));
        assert!(matches!(
            catalog.insert(
                &ExtensionId::new("org.example.shadow").unwrap(),
                &papyrus_game_content_declarations()
                    .into_iter()
                    .find(|function| {
                        function.route
                            == byroredux_sdk::compatibility::PAPYRUS_GAME_GET_MOD_BY_NAME_ROUTE
                    })
                    .unwrap()
                    .declaration,
            ),
            Err(PapyrusProviderCatalogError::DuplicateAlias { .. })
        ));
    }

    #[test]
    fn unrelated_calls_are_left_for_other_translators() {
        assert_eq!(
            lower_provider_call(&expression("Utility.Wait(1.0)"), &catalog()).unwrap(),
            None
        );
    }

    #[test]
    fn aliases_are_unique_across_principals() {
        let mut catalog = catalog();
        let error = catalog
            .insert(
                &ExtensionId::new("org.example.other").unwrap(),
                &declaration(),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            PapyrusProviderCatalogError::DuplicateAlias { .. }
        ));
    }

    #[test]
    fn attached_program_dispatches_on_init_exactly_once() {
        let source = r#"
            ScriptName Fixture
            Event OnInit()
                WeatherNative.WeatherAt(4, "initialized")
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let program = lower_provider_program(&script, &catalog())
            .unwrap()
            .unwrap();
        assert_eq!(program.handler(PapyrusProviderEvent::OnInit).len(), 1);

        let mut world = World::new();
        crate::register(&mut world);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_for_callback = Arc::clone(&calls);
        let callback = Arc::new(
            move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
                calls_for_callback
                    .lock()
                    .unwrap()
                    .push((route.to_owned(), arguments.to_vec()));
                Ok(ScriptValue::String("clear".to_owned()))
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
        let entity = world.spawn();

        attach_papyrus_provider_program(&mut world, entity, program);
        assert!(world.has::<OnInitEvent>(entity));
        papyrus_provider_system(&world, 0.0);
        crate::event_cleanup_system(&world, 0.0);
        papyrus_provider_system(&world, 0.0);

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "ext.org.example.weather.weather-at");
        assert_eq!(
            calls[0].1,
            [
                ScriptValue::Integer(4),
                ScriptValue::String("initialized".to_owned())
            ]
        );
    }

    #[test]
    fn dynamic_mod_event_registration_delivers_typed_callback_and_unregisters() {
        let source = r#"
            ScriptName Fixture
            Event OnInit()
                RegisterForModEvent("ByroReady", "OnByroReady")
            EndEvent
            Event OnByroReady(String status, Int count)
                WeatherNative.WeatherAt(count, status)
                UnregisterForModEvent("ByroReady")
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let program = lower_provider_program(&script, &catalog())
            .unwrap()
            .unwrap();
        let principal = PrincipalId::new("legacy.scripts.receiver").unwrap();

        let mut world = World::new();
        crate::register(&mut world);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&calls);
        let callback = Arc::new(
            move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
                observed
                    .lock()
                    .unwrap()
                    .push((route.to_owned(), arguments.to_vec()));
                Ok(ScriptValue::String("clear".to_owned()))
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
        let entity = world.spawn();
        attach_owned_papyrus_provider_program(&mut world, entity, program, principal);
        papyrus_provider_system(&world, 0.0);
        crate::event_cleanup_system(&world, 0.0);

        let sender = PrincipalId::new("legacy.scripts.sender").unwrap();
        let mut malformed = byroredux_sdk::event::LegacySkseModEventBuilders::new();
        let malformed_handle = malformed.create("ByroReady");
        malformed.push(malformed_handle, LegacySkseModEventValue::Int(7));
        let malformed = malformed.send(malformed_handle).unwrap();
        queue_papyrus_mod_event(
            &world,
            CustomEvent {
                event: malformed.event,
                sender: sender.clone(),
                payload: malformed.payload,
            },
        );
        papyrus_provider_system(&world, 0.0);
        assert!(calls.lock().unwrap().is_empty());

        let mut builders = byroredux_sdk::event::LegacySkseModEventBuilders::new();
        let handle = builders.create("ByroReady");
        builders.push(handle, LegacySkseModEventValue::String("ready".to_owned()));
        builders.push(handle, LegacySkseModEventValue::Int(7));
        let command = builders.send(handle).unwrap();
        let event = CustomEvent {
            event: command.event,
            sender,
            payload: command.payload,
        };
        queue_papyrus_mod_event(&world, event.clone());
        papyrus_provider_system(&world, 0.0);
        assert_eq!(
            *calls.lock().unwrap(),
            [(
                "ext.org.example.weather.weather-at".to_owned(),
                vec![
                    ScriptValue::Integer(7),
                    ScriptValue::String("ready".to_owned()),
                ],
            )]
        );

        queue_papyrus_mod_event(&world, event);
        papyrus_provider_system(&world, 0.0);
        assert_eq!(calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn form_send_mod_event_preserves_stable_sender_across_wait() {
        use byroredux_core::ecs::components::FormIdComponent;
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};

        let source = r#"
            ScriptName Fixture extends Quest
            Event OnInit()
                String eventName = "ByroReady"
                Utility.Wait(1.0)
                self.SendModEvent(eventName, "ready", 7.0)
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let program = lower_provider_program(&script, &catalog())
            .unwrap()
            .unwrap();
        let principal = PrincipalId::new("legacy.scripts.sender").unwrap();
        let expected_sender = FormRef::new([9; 16], 0x1234);

        let mut world = World::new();
        crate::register(&mut world);
        let mut pool = FormIdPool::new();
        let form_id = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Fixture.esm"),
            local: LocalFormId(0x1234),
        });
        world.insert_resource(pool);
        let callback = Arc::new(
            |_principal: Option<&PrincipalId>, _route: &str, _arguments: &[ScriptValue]| {
                Ok(ScriptValue::None)
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
        set_papyrus_provider_form_resolver(
            &world,
            Some(Arc::new(move |_form_id| Ok(expected_sender))),
        );
        let published = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&published);
        set_papyrus_provider_mod_event_publisher(
            &world,
            Some(Arc::new(move |principal, command| {
                observed.lock().unwrap().push((principal.clone(), command));
                Ok(())
            })),
        );
        let entity = world.spawn();
        world.insert(entity, FormIdComponent(form_id));
        attach_owned_papyrus_provider_program(&mut world, entity, program, principal.clone());

        papyrus_provider_system(&world, 0.0);
        crate::event_cleanup_system(&world, 0.0);
        assert!(published.lock().unwrap().is_empty());
        papyrus_provider_system(&world, 1.0);

        let published = published.lock().unwrap();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].0, principal);
        let payload =
            byroredux_sdk::event::LegacySkseModEventPayload::decode(&published[0].1.payload)
                .unwrap();
        assert_eq!(payload.string_arg, "ready");
        assert_eq!(payload.number_arg(), 7.0);
        assert_eq!(payload.sender, Some(expected_sender));
    }

    #[test]
    fn active_magic_effect_send_mod_event_uses_none_sender_and_defaults() {
        let source = r#"
            ScriptName Fixture extends ActiveMagicEffect
            Event OnInit()
                SendModEvent("EffectReady")
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let program = lower_provider_program(&script, &catalog())
            .unwrap()
            .unwrap();

        let mut world = World::new();
        crate::register(&mut world);
        let callback = Arc::new(
            |_principal: Option<&PrincipalId>, _route: &str, _arguments: &[ScriptValue]| {
                Ok(ScriptValue::None)
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
        let published = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&published);
        set_papyrus_provider_mod_event_publisher(
            &world,
            Some(Arc::new(move |_principal, command| {
                observed.lock().unwrap().push(command);
                Ok(())
            })),
        );
        let entity = world.spawn();
        attach_owned_papyrus_provider_program(
            &mut world,
            entity,
            program,
            PrincipalId::new("legacy.scripts.effect").unwrap(),
        );

        papyrus_provider_system(&world, 0.0);

        let published = published.lock().unwrap();
        assert_eq!(published.len(), 1);
        let payload =
            byroredux_sdk::event::LegacySkseModEventPayload::decode(&published[0].payload).unwrap();
        assert_eq!(payload.string_arg, "");
        assert_eq!(payload.number_arg(), 0.0);
        assert_eq!(payload.sender, None);
    }

    #[test]
    fn owned_program_preserves_its_principal_across_a_latent_tail() {
        let source = r#"
            ScriptName Fixture
            Event OnInit()
                WeatherNative.WeatherAt(1, "before")
                Utility.Wait(0.0)
                WeatherNative.WeatherAt(2, "after")
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let program = lower_provider_program(&script, &catalog())
            .unwrap()
            .unwrap();
        let principal = PrincipalId::new("legacy.scripts.fixture").unwrap();

        let mut world = World::new();
        crate::register(&mut world);
        let owners = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&owners);
        let callback = Arc::new(
            move |principal: Option<&PrincipalId>, _route: &str, _arguments: &[ScriptValue]| {
                observed
                    .lock()
                    .unwrap()
                    .push(principal.map(ToString::to_string));
                Ok(ScriptValue::String("ok".to_owned()))
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
        let entity = world.spawn();
        attach_owned_papyrus_provider_program(&mut world, entity, program, principal.clone());

        papyrus_provider_system(&world, 0.0);
        assert_eq!(
            world.resource::<PapyrusProviderContinuationQueue>().len(),
            1
        );
        crate::event_cleanup_system(&world, 0.0);
        papyrus_provider_system(&world, 0.0);

        assert_eq!(
            *owners.lock().unwrap(),
            [Some(principal.to_string()), Some(principal.to_string())]
        );
    }

    #[test]
    fn multiple_attached_scripts_preserve_handler_order_without_overwrite() {
        let first_source = r#"
            ScriptName FirstFixture
            Event OnInit()
                WeatherNative.WeatherAt(1, "first-init")
            EndEvent
            Event OnLoad()
                WeatherNative.WeatherAt(2, "first-load")
            EndEvent
        "#;
        let second_source = r#"
            ScriptName SecondFixture
            Event OnInit()
                WeatherNative.WeatherAt(3, "second-init")
            EndEvent
            Event OnLoad()
                WeatherNative.WeatherAt(4, "second-load")
            EndEvent
            Event OnActivate()
                WeatherNative.WeatherAt(5, "second-activate")
            EndEvent
        "#;
        let lower = |source| {
            let (script, errors) = parse_script(source).unwrap();
            assert!(errors.is_empty(), "{errors:?}");
            lower_provider_program(&script, &catalog())
                .unwrap()
                .unwrap()
        };

        let mut world = World::new();
        crate::register(&mut world);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&calls);
        let callback = Arc::new(
            move |_principal: Option<&PrincipalId>, _route: &str, arguments: &[ScriptValue]| {
                observed.lock().unwrap().push(arguments.to_vec());
                Ok(ScriptValue::String("ok".to_owned()))
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
        let entity = world.spawn();
        attach_papyrus_provider_program(&mut world, entity, lower(first_source));
        attach_papyrus_provider_program(&mut world, entity, lower(second_source));
        assert_eq!(
            world
                .get::<PapyrusProviderProgram>(entity)
                .unwrap()
                .handlers_for(PapyrusProviderEvent::OnLoad)
                .count(),
            2
        );
        world.insert(entity, OnCellLoadEvent);
        world.insert(entity, ActivateEvent { activator: entity });

        papyrus_provider_system(&world, 0.0);

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 5);
        assert_eq!(
            calls
                .iter()
                .map(|arguments| arguments[0].clone())
                .collect::<Vec<_>>(),
            [1, 3, 2, 4, 5]
                .into_iter()
                .map(ScriptValue::Integer)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn combat_and_equipment_events_dispatch_in_batch_order() {
        let source = r#"
            ScriptName Fixture
            Event OnHit(ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked)
                If abPowerAttack && !abHitBlocked
                    WeatherNative.WeatherAt(1, "hit")
                EndIf
            EndEvent
            Event OnObjectEquipped(Form akBaseObject, ObjectReference akReference)
                WeatherNative.InspectForm(akBaseObject)
            EndEvent
            Event OnObjectUnequipped(Form akBaseObject, ObjectReference akReference)
                WeatherNative.InspectForm(akBaseObject)
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let program = lower_provider_program(&script, &catalog())
            .unwrap()
            .unwrap();

        let mut world = World::new();
        crate::register(&mut world);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_for_callback = Arc::clone(&calls);
        let callback = Arc::new(
            move |_principal: Option<&PrincipalId>, _route: &str, arguments: &[ScriptValue]| {
                calls_for_callback.lock().unwrap().push(arguments.to_vec());
                Ok(ScriptValue::String("clear".to_owned()))
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
        let form_resolver = Arc::new(|form_id: u32| Ok(FormRef::new([7; 16], form_id)))
            as Arc<PapyrusProviderFormResolver>;
        set_papyrus_provider_form_resolver(&world, Some(form_resolver));
        let entity = world.spawn();
        let aggressor = world.spawn();
        attach_papyrus_provider_program(&mut world, entity, program);
        world.insert(
            entity,
            HitEvent {
                aggressor,
                source: aggressor,
                projectile: 0,
                damage: 10.0,
                power_attack: true,
                sneak_attack: false,
                bash_attack: false,
                blocked: false,
            },
        );
        world.insert(
            entity,
            EquipmentEventBatch(vec![
                crate::EquipmentChange {
                    item_form_id: 1,
                    equipped: false,
                },
                crate::EquipmentChange {
                    item_form_id: 2,
                    equipped: true,
                },
            ]),
        );

        papyrus_provider_system(&world, 0.0);

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0][0], ScriptValue::Integer(1));
        assert_eq!(calls[1], [ScriptValue::Form(FormRef::new([7; 16], 1))]);
        assert_eq!(calls[2], [ScriptValue::Form(FormRef::new([7; 16], 2))]);
    }

    #[test]
    fn equipment_form_identity_survives_a_latent_handler_tail() {
        let source = r#"
            ScriptName Fixture
            Event OnObjectEquipped(Form akBaseObject, ObjectReference akReference)
                Utility.Wait(0.5)
                WeatherNative.InspectForm(akBaseObject)
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let program = lower_provider_program(&script, &catalog())
            .unwrap()
            .unwrap();

        let mut world = World::new();
        crate::register(&mut world);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_for_callback = Arc::clone(&calls);
        let callback = Arc::new(
            move |_principal: Option<&PrincipalId>, _route: &str, arguments: &[ScriptValue]| {
                calls_for_callback.lock().unwrap().push(arguments.to_vec());
                Ok(ScriptValue::String("ok".to_owned()))
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
        let form_resolver = Arc::new(|form_id: u32| Ok(FormRef::new([8; 16], form_id)))
            as Arc<PapyrusProviderFormResolver>;
        set_papyrus_provider_form_resolver(&world, Some(form_resolver));
        let entity = world.spawn();
        attach_papyrus_provider_program(&mut world, entity, program);
        world.insert(
            entity,
            EquipmentEventBatch(vec![crate::EquipmentChange {
                item_form_id: 0x44,
                equipped: true,
            }]),
        );

        papyrus_provider_system(&world, 0.0);
        assert!(calls.lock().unwrap().is_empty());
        assert_eq!(
            world.resource::<PapyrusProviderContinuationQueue>().len(),
            1
        );
        crate::event_cleanup_system(&world, 0.0);

        papyrus_provider_system(&world, 0.5);

        assert_eq!(
            *calls.lock().unwrap(),
            vec![vec![ScriptValue::Form(FormRef::new([8; 16], 0x44))]]
        );
        assert!(world
            .resource::<PapyrusProviderContinuationQueue>()
            .is_empty());
    }

    #[test]
    fn source_handler_assigns_a_typed_result_and_selects_one_branch() {
        let source = r#"
            ScriptName Fixture
            Event OnLoad()
                Bool storm
                storm = WeatherNative.IsStorm()
                If storm
                    WeatherNative.WeatherAt(4, "clear")
                Else
                    WeatherNative.WeatherAt(5, "cloudy")
                EndIf
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let program = lower_provider_program(&script, &catalog())
            .unwrap()
            .unwrap();

        let mut world = World::new();
        crate::register(&mut world);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_for_callback = Arc::clone(&calls);
        let callback = Arc::new(
            move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
                calls_for_callback
                    .lock()
                    .unwrap()
                    .push((route.to_owned(), arguments.to_vec()));
                if route.ends_with("is-storm") {
                    Ok(ScriptValue::Boolean(true))
                } else {
                    Ok(ScriptValue::String("rain".to_owned()))
                }
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
        let entity = world.spawn();
        attach_papyrus_provider_program(&mut world, entity, program);
        world.insert(entity, OnCellLoadEvent);

        papyrus_provider_system(&world, 0.0);

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, "ext.org.example.weather.is-storm");
        assert_eq!(calls[1].0, "ext.org.example.weather.weather-at");
        assert_eq!(
            calls[1].1,
            [
                ScriptValue::Integer(4),
                ScriptValue::String("clear".to_owned())
            ]
        );
    }

    #[test]
    fn latent_wait_preserves_locals_and_branch_and_handler_tails() {
        let source = r#"
            ScriptName Fixture
            Event OnLoad()
                Bool storm
                String branchLabel = "after-branch-wait"
                storm = WeatherNative.IsStorm()
                If storm
                    Utility.Wait(0.5)
                    WeatherNative.WeatherAt(4, branchLabel)
                EndIf
                WeatherNative.WeatherAt(5, "handler-tail")
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let provider_catalog = catalog();
        let program = lower_provider_program(&script, &provider_catalog)
            .unwrap()
            .unwrap();

        let mut world = World::new();
        crate::register(&mut world);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_for_callback = Arc::clone(&calls);
        let callback = Arc::new(
            move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
                calls_for_callback
                    .lock()
                    .unwrap()
                    .push((route.to_owned(), arguments.to_vec()));
                if route.ends_with("is-storm") {
                    Ok(ScriptValue::Boolean(true))
                } else {
                    Ok(ScriptValue::String("ok".to_owned()))
                }
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(provider_catalog), Some(callback));
        let entity = world.spawn();
        attach_papyrus_provider_program(&mut world, entity, program);
        world.insert(entity, OnCellLoadEvent);

        papyrus_provider_system(&world, 0.0);
        assert_eq!(calls.lock().unwrap().len(), 1);
        assert_eq!(
            world.resource::<PapyrusProviderContinuationQueue>().len(),
            1
        );
        world.query_mut::<OnCellLoadEvent>().unwrap().remove(entity);

        papyrus_provider_system(&world, 0.25);
        assert_eq!(calls.lock().unwrap().len(), 1);
        assert_eq!(
            world.resource::<PapyrusProviderContinuationQueue>().len(),
            1
        );

        papyrus_provider_system(&world, 0.25);
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[1].1[0], ScriptValue::Integer(4));
        assert_eq!(
            calls[1].1[1],
            ScriptValue::String("after-branch-wait".to_owned())
        );
        assert_eq!(calls[2].1[0], ScriptValue::Integer(5));
        assert!(world
            .resource::<PapyrusProviderContinuationQueue>()
            .is_empty());
    }

    #[test]
    fn restored_continuation_rejects_a_route_not_in_the_live_catalog() {
        let source = r#"
            ScriptName Fixture
            Event OnLoad()
                Game.GetModCount()
                Utility.Wait(0.0)
                Game.IsPluginInstalled("Update.esm")
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let provider_catalog = PapyrusProviderCatalog::engine_compatibility();
        let program = lower_provider_program(&script, &provider_catalog)
            .unwrap()
            .unwrap();

        let mut world = World::new();
        crate::register(&mut world);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_for_callback = Arc::clone(&calls);
        let callback = Arc::new(
            move |_principal: Option<&PrincipalId>, route: &str, _arguments: &[ScriptValue]| {
                calls_for_callback.lock().unwrap().push(route.to_owned());
                Ok(ScriptValue::None)
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(provider_catalog), Some(callback));
        let entity = world.spawn();
        attach_papyrus_provider_program(&mut world, entity, program);
        world.insert(entity, OnCellLoadEvent);

        papyrus_provider_system(&world, 0.0);
        world.query_mut::<OnCellLoadEvent>().unwrap().remove(entity);
        {
            let mut queue = world.resource_mut::<PapyrusProviderContinuationQueue>();
            let PapyrusProviderStatement::Call(call) = &mut queue.pending[0].statements[0] else {
                panic!("expected saved provider call tail");
            };
            call.route.qualified_name = "ext.attacker.privileged".to_owned();
        }

        papyrus_provider_system(&world, 0.0);

        assert_eq!(calls.lock().unwrap().len(), 1);
        assert!(world
            .resource::<PapyrusProviderContinuationQueue>()
            .is_empty());
    }

    #[test]
    fn provider_results_support_comparisons_and_short_circuit_conditions() {
        let source = r#"
            ScriptName Fixture
            Event OnLoad()
                Int count
                count = Game.GetModCount()
                If count >= 2 && !Game.IsPluginInstalled("Missing.esp")
                    WeatherNative.WeatherAt(4, "matched")
                Else
                    WeatherNative.WeatherAt(5, "missed")
                EndIf
                If true || Game.IsPluginInstalled("MustNotRun.esp")
                    WeatherNative.WeatherAt(6, "short-circuited")
                EndIf
                String weather
                weather = WeatherNative.WeatherAt(0, "probe")
                If weather == "rain"
                    WeatherNative.WeatherAt(7, weather)
                EndIf
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let mut provider_catalog = PapyrusProviderCatalog::engine_compatibility();
        let extension = ExtensionId::new("org.example.weather").unwrap();
        provider_catalog.insert(&extension, &declaration()).unwrap();
        provider_catalog
            .insert(&extension, &boolean_declaration())
            .unwrap();
        let program = lower_provider_program(&script, &provider_catalog)
            .unwrap()
            .unwrap();

        let mut world = World::new();
        crate::register(&mut world);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_for_callback = Arc::clone(&calls);
        let callback = Arc::new(
            move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
                calls_for_callback
                    .lock()
                    .unwrap()
                    .push((route.to_owned(), arguments.to_vec()));
                if route.ends_with("get-mod-count") {
                    Ok(ScriptValue::Integer(2))
                } else if route.ends_with("is-plugin-installed") {
                    Ok(ScriptValue::Boolean(false))
                } else if arguments.first() == Some(&ScriptValue::Integer(0)) {
                    Ok(ScriptValue::String("rain".to_owned()))
                } else {
                    Ok(ScriptValue::String("ok".to_owned()))
                }
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(provider_catalog), Some(callback));
        let entity = world.spawn();
        attach_papyrus_provider_program(&mut world, entity, program);
        world.insert(entity, OnCellLoadEvent);

        papyrus_provider_system(&world, 0.0);

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 6);
        assert!(calls[0].0.ends_with("get-mod-count"));
        assert!(calls[1].0.ends_with("is-plugin-installed"));
        assert_eq!(calls[2].1[0], ScriptValue::Integer(4));
        assert_eq!(calls[3].1[0], ScriptValue::Integer(6));
        assert_eq!(calls[4].1[0], ScriptValue::Integer(0));
        assert_eq!(calls[5].1[0], ScriptValue::Integer(7));
        assert_eq!(calls[5].1[1], ScriptValue::String("rain".to_owned()));
        assert!(calls.iter().all(|(_, arguments)| arguments.first()
            != Some(&ScriptValue::String("MustNotRun.esp".to_owned()))));
    }

    #[test]
    fn provider_expressions_execute_typed_arithmetic_and_string_concatenation() {
        let source = r#"
            ScriptName Fixture
            Event OnLoad()
                Int count
                count = Game.GetModCount() + 2
                String label
                label = "prefix-" + WeatherNative.WeatherAt(count, "fallback")
                If count * 2 >= 10
                    WeatherNative.WeatherAt(count, label)
                EndIf
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let mut provider_catalog = PapyrusProviderCatalog::engine_compatibility();
        let extension = ExtensionId::new("org.example.weather").unwrap();
        provider_catalog.insert(&extension, &declaration()).unwrap();
        let program = lower_provider_program(&script, &provider_catalog)
            .unwrap()
            .unwrap();
        assert!(program
            .handler(PapyrusProviderEvent::OnLoad)
            .iter()
            .any(|statement| matches!(
                statement,
                PapyrusProviderStatement::AssignValue {
                    value: PapyrusProviderValue::Binary {
                        operator: PapyrusProviderArithmetic::StrCat,
                        ..
                    },
                    value_type: ScriptValueType::String,
                    ..
                }
            )));

        let mut world = World::new();
        crate::register(&mut world);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&calls);
        let callback = Arc::new(
            move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
                observed
                    .lock()
                    .unwrap()
                    .push((route.to_owned(), arguments.to_vec()));
                if route.ends_with("get-mod-count") {
                    Ok(ScriptValue::Integer(3))
                } else {
                    Ok(ScriptValue::String("rain".to_owned()))
                }
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(provider_catalog), Some(callback));
        let entity = world.spawn();
        attach_papyrus_provider_program(&mut world, entity, program);
        world.insert(entity, OnCellLoadEvent);

        papyrus_provider_system(&world, 0.0);

        assert_eq!(
            *calls.lock().unwrap(),
            vec![
                (
                    byroredux_sdk::compatibility::PAPYRUS_GAME_GET_MOD_COUNT_ROUTE.to_owned(),
                    Vec::new(),
                ),
                (
                    "ext.org.example.weather.weather-at".to_owned(),
                    vec![
                        ScriptValue::Integer(5),
                        ScriptValue::String("fallback".to_owned()),
                    ],
                ),
                (
                    "ext.org.example.weather.weather-at".to_owned(),
                    vec![
                        ScriptValue::Integer(5),
                        ScriptValue::String("prefix-rain".to_owned()),
                    ],
                ),
            ]
        );
    }

    #[test]
    fn game_get_player_binds_to_an_opaque_object_local() {
        let source = r#"
            ScriptName Fixture
            Event OnLoad()
                ObjectReference player
                player = Game.GetPlayer()
                WeatherNative.InspectEntity(player)
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let mut provider_catalog = PapyrusProviderCatalog::engine_compatibility();
        let extension = ExtensionId::new("org.example.weather").unwrap();
        provider_catalog
            .insert(&extension, &entity_declaration())
            .unwrap();
        let program = lower_provider_program(&script, &provider_catalog)
            .unwrap()
            .unwrap();

        let mut world = World::new();
        crate::register(&mut world);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&calls);
        let callback = Arc::new(
            move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
                observed
                    .lock()
                    .unwrap()
                    .push((route.to_owned(), arguments.to_vec()));
                if route == byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE {
                    Ok(ScriptValue::Entity(EntityRef::new(1, 7).unwrap()))
                } else {
                    Ok(ScriptValue::String("inspected".to_owned()))
                }
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(provider_catalog), Some(callback));
        let entity = world.spawn();
        attach_papyrus_provider_program(&mut world, entity, program);
        world.insert(entity, OnCellLoadEvent);

        papyrus_provider_system(&world, 0.0);

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0],
            (
                byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE.to_owned(),
                Vec::new()
            )
        );
        assert_eq!(calls[1].0, "ext.org.example.weather.inspect-entity");
        assert_eq!(
            calls[1].1,
            vec![ScriptValue::Entity(EntityRef::new(1, 7).unwrap())]
        );
    }

    #[test]
    fn entity_conditions_support_identity_and_nullable_none() {
        let source = r#"
            ScriptName Fixture
            Event OnLoad()
                ObjectReference player
                player = Game.GetPlayer()
                If player == player
                    WeatherNative.WeatherAt(1, "same")
                EndIf
                If player != None
                    WeatherNative.WeatherAt(2, "present")
                EndIf
                If player == None
                    WeatherNative.WeatherAt(3, "unexpected")
                EndIf
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let mut provider_catalog = PapyrusProviderCatalog::engine_compatibility();
        let extension = ExtensionId::new("org.example.weather").unwrap();
        provider_catalog.insert(&extension, &declaration()).unwrap();
        let program = lower_provider_program(&script, &provider_catalog)
            .unwrap()
            .unwrap();

        let mut world = World::new();
        crate::register(&mut world);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&calls);
        let callback = Arc::new(
            move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
                observed
                    .lock()
                    .unwrap()
                    .push((route.to_owned(), arguments.to_vec()));
                if route == byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE {
                    Ok(ScriptValue::Entity(EntityRef::new(1, 7).unwrap()))
                } else {
                    Ok(ScriptValue::String("ok".to_owned()))
                }
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(provider_catalog), Some(callback));
        let entity = world.spawn();
        attach_papyrus_provider_program(&mut world, entity, program);
        world.insert(entity, OnCellLoadEvent);

        papyrus_provider_system(&world, 0.0);

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert_eq!(
            calls[0],
            (
                byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE.to_owned(),
                Vec::new()
            )
        );
        assert_eq!(calls[1].0, "ext.org.example.weather.weather-at");
        assert_eq!(calls[1].1[0], ScriptValue::Integer(1));
        assert_eq!(calls[2].0, "ext.org.example.weather.weather-at");
        assert_eq!(calls[2].1[0], ScriptValue::Integer(2));
    }

    #[test]
    fn entity_conditions_match_none_when_engine_player_is_missing() {
        let source = r#"
            ScriptName Fixture
            Event OnLoad()
                ObjectReference player
                player = Game.GetPlayer()
                If player == None
                    WeatherNative.WeatherAt(3, "none")
                EndIf
                If player != None
                    WeatherNative.WeatherAt(4, "unexpected")
                EndIf
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let mut provider_catalog = PapyrusProviderCatalog::engine_compatibility();
        let extension = ExtensionId::new("org.example.weather").unwrap();
        provider_catalog.insert(&extension, &declaration()).unwrap();
        let program = lower_provider_program(&script, &provider_catalog)
            .unwrap()
            .unwrap();

        let mut world = World::new();
        crate::register(&mut world);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&calls);
        let callback = Arc::new(
            move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
                observed
                    .lock()
                    .unwrap()
                    .push((route.to_owned(), arguments.to_vec()));
                if route == byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE {
                    Ok(ScriptValue::None)
                } else {
                    Ok(ScriptValue::String("ok".to_owned()))
                }
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(provider_catalog), Some(callback));
        let entity = world.spawn();
        attach_papyrus_provider_program(&mut world, entity, program);
        world.insert(entity, OnCellLoadEvent);

        papyrus_provider_system(&world, 0.0);

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0],
            (
                byroredux_sdk::compatibility::PAPYRUS_GAME_GET_PLAYER_ROUTE.to_owned(),
                Vec::new()
            )
        );
        assert_eq!(calls[1].0, "ext.org.example.weather.weather-at");
        assert_eq!(calls[1].1[0], ScriptValue::Integer(3));
    }

    #[test]
    fn entity_conditions_reject_ordered_comparisons() {
        let source = r#"
            ScriptName Fixture
            Event OnLoad()
                ObjectReference player
                player = Game.GetPlayer()
                If player < player
                    WeatherNative.WeatherAt(1, "unexpected")
                EndIf
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            lower_provider_program(&script, &PapyrusProviderCatalog::engine_compatibility()),
            Err(PapyrusProviderProgramError::UnsupportedStatement)
        );
    }

    #[test]
    fn provider_expressions_reject_mixed_numeric_types_and_runtime_faults() {
        let source = r#"
            ScriptName Fixture
            Event OnLoad()
                Int count
                count = Game.GetModCount() + 1.5
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            lower_provider_program(&script, &PapyrusProviderCatalog::engine_compatibility()),
            Err(PapyrusProviderProgramError::UnsupportedStatement)
        );

        assert!(apply_provider_arithmetic(
            ScriptValue::Integer(i64::MAX),
            PapyrusProviderArithmetic::Add,
            ScriptValue::Integer(1),
        )
        .is_err());
        assert!(apply_provider_arithmetic(
            ScriptValue::Integer(1),
            PapyrusProviderArithmetic::Div,
            ScriptValue::Integer(0),
        )
        .is_err());
        assert!(apply_provider_arithmetic(
            ScriptValue::String("x".repeat(4 * 1024)),
            PapyrusProviderArithmetic::StrCat,
            ScriptValue::String("y".to_owned()),
        )
        .is_err());
    }

    #[test]
    fn trigger_enter_and_update_events_dispatch_provider_handlers() {
        let source = r#"
            ScriptName Fixture
            Event OnTriggerEnter(ObjectReference akActionRef)
                WeatherNative.InspectEntity(akActionRef)
            EndEvent
            Event OnUpdate()
                WeatherNative.WeatherAt(8, "update")
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        let program = lower_provider_program(&script, &catalog())
            .unwrap()
            .unwrap();

        let mut world = World::new();
        crate::register(&mut world);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_for_callback = Arc::clone(&calls);
        let callback = Arc::new(
            move |_principal: Option<&PrincipalId>, route: &str, arguments: &[ScriptValue]| {
                calls_for_callback
                    .lock()
                    .unwrap()
                    .push((route.to_owned(), arguments.to_vec()));
                Ok(ScriptValue::String("ok".to_owned()))
            },
        ) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
        let resolver = Arc::new(|entity: EntityId| {
            EntityRef::new(9, u64::from(entity) + 1).ok_or_else(|| "invalid test entity".to_owned())
        }) as Arc<PapyrusProviderEntityResolver>;
        set_papyrus_provider_entity_resolver(&world, Some(resolver));
        let entity = world.spawn();
        let first_triggerer = world.spawn();
        let second_triggerer = world.spawn();
        attach_papyrus_provider_program(&mut world, entity, program);
        world.insert(
            entity,
            OnTriggerEnterEvent {
                triggerers: vec![first_triggerer, second_triggerer],
            },
        );
        world.insert(entity, OnUpdateEvent);

        papyrus_provider_system(&world, 0.0);

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 3);
        assert_eq!(
            calls[0].1[0],
            ScriptValue::Entity(EntityRef::new(9, u64::from(first_triggerer) + 1).unwrap())
        );
        assert_eq!(
            calls[1].1[0],
            ScriptValue::Entity(EntityRef::new(9, u64::from(second_triggerer) + 1).unwrap())
        );
        assert_eq!(calls[2].1[0], ScriptValue::Integer(8));
    }

    #[test]
    fn provider_bearing_handler_rejects_unsupported_statements_as_a_unit() {
        let source = r#"
            ScriptName Fixture
            Event OnLoad()
                While WeatherNative.IsStorm()
                    WeatherNative.WeatherAt(4)
                EndWhile
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            lower_provider_program(&script, &catalog()),
            Err(PapyrusProviderProgramError::UnsupportedStatement)
        );
    }

    #[test]
    fn reference_event_parameters_do_not_cross_latent_waits() {
        let source = r#"
            ScriptName Fixture
            Event OnTriggerEnter(ObjectReference akActionRef)
                WeatherNative.InspectEntity(akActionRef)
                Utility.Wait(1.0)
                WeatherNative.WeatherAt(1, "after")
            EndEvent
        "#;
        let (script, errors) = parse_script(source).unwrap();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            lower_provider_program(&script, &catalog()),
            Err(PapyrusProviderProgramError::UnsupportedStatement)
        );
    }

    #[test]
    fn byte_level_pex_static_call_lowers_to_the_same_provider_route() {
        let translation = crate::translate_pex_detailed_with_providers(
            &provider_call_pex_bytes(),
            byroredux_plugin::esm::reader::GameKind::Skyrim,
            None,
            None,
            &catalog(),
        );
        assert_eq!(translation.provider_error, None);
        let program = translation.provider_program.unwrap();
        let [PapyrusProviderStatement::Call(call)] = program.handler(PapyrusProviderEvent::OnLoad)
        else {
            panic!("expected one lowered provider call");
        };
        assert_eq!(
            call.route.qualified_name(),
            "ext.org.example.weather.weather-at"
        );
        assert_eq!(
            call.arguments,
            [
                PapyrusProviderArgument::Literal(ScriptValue::Integer(4)),
                PapyrusProviderArgument::Literal(ScriptValue::String("clear".to_owned()))
            ]
        );
    }

    #[test]
    fn byte_level_pex_instance_send_mod_event_lowers_with_defaults() {
        let translation = crate::translate_pex_detailed_with_providers(
            &send_mod_event_pex_bytes(),
            byroredux_plugin::esm::reader::GameKind::Skyrim,
            None,
            None,
            &catalog(),
        );
        assert_eq!(translation.provider_error, None);
        let program = translation.provider_program.unwrap();
        let [PapyrusProviderStatement::SendModEvent {
            event_name,
            string_arg,
            number_arg,
            sender,
        }] = program.handler(PapyrusProviderEvent::OnLoad)
        else {
            panic!("expected one lowered instance SendModEvent call");
        };
        assert_eq!(
            event_name,
            &PapyrusProviderArgument::Literal(ScriptValue::String("ByroReady".to_owned()))
        );
        assert_eq!(
            string_arg,
            &PapyrusProviderArgument::Literal(ScriptValue::String(String::new()))
        );
        assert_eq!(
            number_arg,
            &PapyrusProviderArgument::Literal(ScriptValue::Float(0.0))
        );
        assert_eq!(sender, &PapyrusModEventSender::Owner);
    }
}
