//! Call lowering (front-end): a legal `Provider.Function(...)` and its
//! typed arguments -> a principal-qualified SDK route. Pure — never
//! enters Wasm, never touches the ECS.

use super::*;

/// A fully resolved, typed SDK call safe to hand to the extension host.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct TypedPapyrusProviderCall {
    pub route: PapyrusProviderRoute,
    pub arguments: Vec<ScriptValue>,
    pub result: Option<ScriptResultDeclaration>,
}

/// One handler argument resolved either at translation time, from a typed
/// local, or from a bounded provider expression when the event executes.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum PapyrusProviderArgument {
    Literal(ScriptValue),
    Local {
        name: String,
        value_type: ScriptValueType,
    },
    /// A bounded provider expression evaluated immediately before the call.
    /// This is used for receiver-producing expressions such as
    /// `Game.GetPlayer().Method(...)`.
    Value {
        value: Box<PapyrusProviderValue>,
        value_type: ScriptValueType,
    },
}

/// Papyrus object types that the provider runtime can prove for a returned
/// entity value. This is intentionally a compact enum so saved expression
/// values do not inflate the surrounding recursive AST enums.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum PapyrusProviderObjectType {
    ObjectReference,
}

impl PapyrusProviderObjectType {
    fn as_provider_name(self) -> &'static str {
        match self {
            Self::ObjectReference => "ObjectReference",
        }
    }
}

/// A provider call embedded in an event handler. Fragment calls continue to
/// use [`TypedPapyrusProviderCall`] and therefore remain literal-only.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct PapyrusProviderInvocation {
    pub route: PapyrusProviderRoute,
    /// Engine-owned receiver for a reserved `self.Method(...)` call or a
    /// typed object receiver. The SDK declaration includes this as its
    /// required first `Entity` parameter.
    pub receiver: Option<Box<PapyrusProviderArgument>>,
    pub arguments: Vec<PapyrusProviderArgument>,
    pub result: Option<ScriptResultDeclaration>,
    /// Papyrus object type produced by this call when the SDK can prove one.
    /// This field is required in the current save format because a nested
    /// receiver-producing call must retain its proven object type across a
    /// continuation boundary.
    pub result_object_type: Option<PapyrusProviderObjectType>,
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

pub(crate) fn storage_util_arity(route: &str) -> Option<(usize, usize)> {
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

pub(crate) fn validate_storage_util_literals(
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

pub(crate) fn validate_storage_util_arguments(
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

pub(crate) fn legacy_container_arity(route: &str) -> Option<(usize, usize)> {
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

pub(crate) fn validate_legacy_container_arity(
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

pub(crate) fn validate_mod_event_arity(
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

pub(crate) fn is_known_provider_call(
    provider: &str,
    function: &str,
    catalog: &PapyrusProviderCatalog,
) -> bool {
    catalog.contains_provider(provider) || classify_static_call(provider, function).is_some()
}

pub(crate) fn lower_arguments(
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

pub(crate) fn lower_ordered_arguments<T>(
    args: &[CallArg],
    declaration: &ScriptFunctionDeclaration,
    lower: impl FnMut(
        &Expr,
        &byroredux_sdk::script_function::ScriptParameterDeclaration,
    ) -> Result<T, PapyrusProviderLowerError>,
) -> Result<Vec<T>, PapyrusProviderLowerError> {
    lower_ordered_arguments_from(args, declaration, 0, lower)
}

pub(crate) fn lower_ordered_arguments_from<T>(
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

pub(crate) fn lower_provider_invocation(
    expression: &Expr,
    catalog: &PapyrusProviderCatalog,
    locals: &BTreeMap<String, PapyrusProviderLocalType>,
) -> Result<Option<PapyrusProviderInvocation>, PapyrusProviderLowerError> {
    let Expr::Call { callee, args } = expression else {
        return Ok(None);
    };
    let Expr::MemberAccess { object, member } = &callee.node else {
        return Ok(None);
    };
    let (route, receiver, parameter_offset) =
        if let Expr::Ident(provider) = &object.node {
            let provider_name = provider.0.to_ascii_lowercase();
            if provider.0.eq_ignore_ascii_case(PAPYRUS_SELF_LOCAL)
                && locals
                    .get(PAPYRUS_SELF_LOCAL)
                    .is_some_and(|local| local.value_type == ScriptValueType::Entity)
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
            } else if let Some(local) = locals.get(&provider_name) {
                if local.value_type != ScriptValueType::Entity {
                    if let Some(route) = catalog.resolve(&provider.0, &member.node.0) {
                        (route, None, 0)
                    } else {
                        if is_known_provider_call(&provider.0, &member.node.0, catalog) {
                            return Err(PapyrusProviderLowerError::UnknownFunction {
                                provider: provider.0.clone(),
                                function: member.node.0.clone(),
                            });
                        }
                        return Ok(None);
                    }
                } else if let Some(object_type) = local.object_type.as_deref() {
                    if let Some(route) = catalog.resolve(object_type, &member.node.0) {
                        (
                            route,
                            Some(Box::new(PapyrusProviderArgument::Local {
                                name: provider_name,
                                value_type: ScriptValueType::Entity,
                            })),
                            1,
                        )
                    } else if catalog.contains_provider(object_type) {
                        return Err(PapyrusProviderLowerError::UnknownFunction {
                            provider: object_type.to_owned(),
                            function: member.node.0.clone(),
                        });
                    } else if let Some(route) = catalog.resolve(&provider.0, &member.node.0) {
                        (route, None, 0)
                    } else {
                        return Ok(None);
                    }
                } else if let Some(route) = catalog.resolve(&provider.0, &member.node.0) {
                    (route, None, 0)
                } else {
                    return Ok(None);
                }
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
            }
        } else {
            let (value, value_type) = lower_provider_value(&object.node, catalog, locals, 1)
                .map_err(|error| PapyrusProviderLowerError::UnsupportedArgument {
                    parameter: format!("receiver expression: {error:?}"),
                })?;
            if value_type != ScriptValueType::Entity {
                return Err(PapyrusProviderLowerError::UnsupportedArgument {
                    parameter: "receiver expression".to_owned(),
                });
            }
            let Some(object_type) = provider_value_object_type(&value) else {
                return Err(PapyrusProviderLowerError::UnsupportedArgument {
                    parameter: "receiver object type".to_owned(),
                });
            };
            let object_type_name = object_type.as_provider_name();
            let Some(route) = catalog.resolve(object_type_name, &member.node.0) else {
                if catalog.contains_provider(object_type_name) {
                    return Err(PapyrusProviderLowerError::UnknownFunction {
                        provider: object_type_name.to_owned(),
                        function: member.node.0.clone(),
                    });
                }
                return Ok(None);
            };
            (
                route,
                Some(Box::new(PapyrusProviderArgument::Value {
                    value: Box::new(value),
                    value_type,
                })),
                1,
            )
        };
    let declaration = route.declaration();
    if parameter_offset == 1
        && !declaration.parameters.first().is_some_and(|parameter| {
            parameter.value_type == ScriptValueType::Entity && !parameter.optional
        })
    {
        return Err(PapyrusProviderLowerError::UnsupportedArgument {
            parameter: "provider receiver".to_owned(),
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
                if locals
                    .get(&name)
                    .is_some_and(|local| local.value_type == parameter.value_type)
                {
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
            PapyrusProviderArgument::Local { value_type, .. }
            | PapyrusProviderArgument::Value { value_type, .. } => {
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
    let result_object_type = provider_result_object_type(route);
    Ok(Some(PapyrusProviderInvocation {
        route: route.clone(),
        receiver,
        arguments,
        result: declaration.result,
        result_object_type,
    }))
}

pub(crate) fn lower_literal(
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
