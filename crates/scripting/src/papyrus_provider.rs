//! Conservative lowering for manifest-published Papyrus static functions.
//!
//! This module is intentionally host-neutral. It resolves a legal
//! `Provider.Function(...)` spelling to the principal-qualified SDK route and
//! validates literal arguments, but it never enters Wasm or touches the ECS.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use byroredux_core::ecs::{Resource, World};
use byroredux_papyrus::ast::{CallArg, Expr};
use byroredux_sdk::{
    identity::ExtensionId,
    script_function::{
        ScriptFunctionDeclaration, ScriptFunctionError, ScriptResultDeclaration, ScriptValue,
        ScriptValueType,
    },
};

/// Host callback shared by Papyrus handlers after all ECS guards are dropped.
pub type PapyrusProviderCallback =
    dyn Fn(&str, &[ScriptValue]) -> Result<ScriptValue, String> + Send + Sync;

/// Live catalog and host callback published atomically by the executable.
#[derive(Clone, Default)]
pub struct PapyrusProviderRuntime {
    catalog: Arc<PapyrusProviderCatalog>,
    callback: Option<Arc<PapyrusProviderCallback>>,
}

impl Resource for PapyrusProviderRuntime {}

impl PapyrusProviderRuntime {
    /// Immutable manifest-backed alias catalog used during script lowering.
    pub fn catalog(&self) -> Arc<PapyrusProviderCatalog> {
        Arc::clone(&self.catalog)
    }

    /// Clone the live host callback for guard-free execution.
    pub fn callback(&self) -> Option<Arc<PapyrusProviderCallback>> {
        self.callback.clone()
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
}

/// One manifest-published route addressable by Papyrus source or PEX.
#[derive(Clone, Debug, Eq, PartialEq)]
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
    /// Insert one declared function when it publishes a Papyrus alias.
    ///
    /// The operation is atomic: a duplicate alias or invalid declaration does
    /// not modify the catalog.
    pub fn insert(
        &mut self,
        extension: &ExtensionId,
        declaration: &ScriptFunctionDeclaration,
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
            qualified_name: declaration.qualified_name(extension),
            declaration: declaration.clone(),
        };
        self.providers.insert(key.0.clone());
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
pub struct TypedPapyrusProviderCall {
    pub route: PapyrusProviderRoute,
    pub arguments: Vec<ScriptValue>,
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
        if catalog.contains_provider(&provider.0) {
            return Err(PapyrusProviderLowerError::UnknownFunction {
                provider: provider.0.clone(),
                function: member.node.0.clone(),
            });
        }
        return Ok(None);
    };

    let arguments = lower_arguments(args, route.declaration())?;
    Ok(Some(TypedPapyrusProviderCall {
        route: route.clone(),
        arguments,
        result: route.declaration().result,
    }))
}

fn lower_arguments(
    args: &[CallArg],
    declaration: &ScriptFunctionDeclaration,
) -> Result<Vec<ScriptValue>, PapyrusProviderLowerError> {
    let mut values = vec![None; declaration.parameters.len()];
    let mut positional = 0usize;
    let mut named_seen = false;
    for arg in args {
        let index = if let Some(name) = &arg.name {
            named_seen = true;
            declaration
                .parameters
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
        let Some(parameter) = declaration.parameters.get(index) else {
            return Err(PapyrusProviderLowerError::TooManyArguments);
        };
        if values[index].is_some() {
            return Err(PapyrusProviderLowerError::DuplicateParameter(
                parameter.id.as_str().to_owned(),
            ));
        }
        values[index] = Some(
            lower_literal(&arg.value.node, parameter.value_type, parameter.optional).ok_or_else(
                || PapyrusProviderLowerError::UnsupportedArgument {
                    parameter: parameter.id.as_str().to_owned(),
                },
            )?,
        );
    }

    let last = values.iter().rposition(Option::is_some);
    let mut ordered = Vec::with_capacity(last.map_or(0, |index| index + 1));
    if let Some(last) = last {
        for (index, value) in values.into_iter().take(last + 1).enumerate() {
            let parameter = &declaration.parameters[index];
            ordered.push(value.ok_or_else(|| {
                PapyrusProviderLowerError::MissingParameter(parameter.id.as_str().to_owned())
            })?);
        }
    }
    declaration
        .validate_arguments(&ordered)
        .map_err(PapyrusProviderLowerError::InvalidArguments)?;
    Ok(ordered)
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
        (Expr::FloatLit(value), ScriptValueType::Float) => {
            let value = *value as f32;
            value.is_finite().then_some(ScriptValue::Float(value))
        }
        (Expr::StringLit(value), ScriptValueType::String) => {
            Some(ScriptValue::String(value.clone()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
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
}
