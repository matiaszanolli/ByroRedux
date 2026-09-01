//! Conservative lowering for manifest-published Papyrus static functions.
//!
//! This module is intentionally host-neutral. It resolves a legal
//! `Provider.Function(...)` spelling to the principal-qualified SDK route and
//! validates literal arguments, but it never enters Wasm or touches the ECS.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::{Resource, World};
use byroredux_papyrus::ast::{
    AssignOp, CallArg, Event, Expr, Script, ScriptItem, StateItem, Stmt, Type,
};
use byroredux_sdk::{
    compatibility::classify_static_call,
    identity::ExtensionId,
    script_function::{
        ScriptFunctionDeclaration, ScriptFunctionError, ScriptResultDeclaration, ScriptValue,
        ScriptValueType,
    },
};

use crate::events::{ActivateEvent, OnCellLoadEvent};

const MAX_PROVIDER_HANDLER_NESTING: usize = 32;

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
    world.register::<PapyrusProviderProgram>();
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
        if is_known_provider_call(&provider.0, &member.node.0, catalog) {
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

/// Canonical event subset currently executable by the provider runtime.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PapyrusProviderEvent {
    OnLoad,
    OnActivate,
}

/// One conservative instruction in a translated Papyrus handler.
#[derive(Clone, Debug, PartialEq)]
pub enum PapyrusProviderStatement {
    Declare {
        name: String,
        value: ScriptValue,
    },
    AssignCall {
        name: String,
        call: TypedPapyrusProviderCall,
    },
    Call(TypedPapyrusProviderCall),
    If {
        condition: PapyrusProviderCondition,
        then_branch: Vec<PapyrusProviderStatement>,
        else_branch: Vec<PapyrusProviderStatement>,
    },
}

/// Boolean expression subset used to select a translated branch.
#[derive(Clone, Debug, PartialEq)]
pub enum PapyrusProviderCondition {
    Literal(bool),
    Local(String),
    Call(TypedPapyrusProviderCall),
}

/// Static translated handlers attached to one scripted entity.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PapyrusProviderProgram {
    handlers: BTreeMap<PapyrusProviderEvent, Vec<PapyrusProviderStatement>>,
}

impl Component for PapyrusProviderProgram {
    type Storage = SparseSetStorage<Self>;
}

impl PapyrusProviderProgram {
    /// Instructions for one canonical event.
    pub fn handler(&self, event: PapyrusProviderEvent) -> &[PapyrusProviderStatement] {
        self.handlers.get(&event).map_or(&[], Vec::as_slice)
    }

    /// Whether no supported handler was present in the source unit.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

/// Whole-handler rejection reason. A known provider is never partially run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PapyrusProviderProgramError {
    DuplicateHandler(PapyrusProviderEvent),
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
    for item in &script.body {
        match &item.node {
            ScriptItem::Event(event) => lower_event_into(event, catalog, &mut program)?,
            ScriptItem::State(state) => {
                for item in &state.body {
                    if let StateItem::Event(event) = &item.node {
                        lower_event_into(event, catalog, &mut program)?;
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
) -> Result<(), PapyrusProviderProgramError> {
    let canonical = if event.name.node.eq_ignore_case("OnLoad") {
        PapyrusProviderEvent::OnLoad
    } else if event.name.node.eq_ignore_case("OnActivate") {
        PapyrusProviderEvent::OnActivate
    } else {
        return Ok(());
    };
    if !event
        .body
        .iter()
        .any(|statement| statement_mentions_provider(&statement.node, catalog, 0))
    {
        return Ok(());
    }
    if program.handlers.contains_key(&canonical) {
        return Err(PapyrusProviderProgramError::DuplicateHandler(canonical));
    }
    let mut locals = BTreeMap::new();
    let statements = lower_statements(&event.body, catalog, &mut locals, 0)?;
    program.handlers.insert(canonical, statements);
    Ok(())
}

fn lower_statements(
    statements: &[byroredux_papyrus::span::Spanned<Stmt>],
    catalog: &PapyrusProviderCatalog,
    locals: &mut BTreeMap<String, ScriptValueType>,
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
                let call = lower_provider_call(&value.node, catalog)
                    .map_err(PapyrusProviderProgramError::Call)?
                    .ok_or(PapyrusProviderProgramError::UnsupportedStatement)?;
                require_result(&call, expected, &target.0)?;
                lowered.push(PapyrusProviderStatement::AssignCall { name: key, call });
            }
            Stmt::ExprStmt(expression) => {
                let call = lower_provider_call(&expression.node, catalog)
                    .map_err(PapyrusProviderProgramError::Call)?
                    .ok_or(PapyrusProviderProgramError::UnsupportedStatement)?;
                lowered.push(PapyrusProviderStatement::Call(call));
            }
            Stmt::If {
                condition,
                body,
                elseif_clauses,
                else_body,
            } => {
                let condition = lower_condition(&condition.node, catalog, locals)?;
                let mut branch_locals = locals.clone();
                let then_branch = lower_statements(body, catalog, &mut branch_locals, depth + 1)?;
                let mut else_branch = if let Some(body) = else_body {
                    let mut branch_locals = locals.clone();
                    lower_statements(body, catalog, &mut branch_locals, depth + 1)?
                } else {
                    Vec::new()
                };
                for (condition, body) in elseif_clauses.iter().rev() {
                    let condition = lower_condition(&condition.node, catalog, locals)?;
                    let mut branch_locals = locals.clone();
                    let then_branch =
                        lower_statements(body, catalog, &mut branch_locals, depth + 1)?;
                    else_branch = vec![PapyrusProviderStatement::If {
                        condition,
                        then_branch,
                        else_branch,
                    }];
                }
                lowered.push(PapyrusProviderStatement::If {
                    condition,
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

fn lower_condition(
    expression: &Expr,
    catalog: &PapyrusProviderCatalog,
    locals: &BTreeMap<String, ScriptValueType>,
) -> Result<PapyrusProviderCondition, PapyrusProviderProgramError> {
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
        _ => {
            let call = lower_provider_call(expression, catalog)
                .map_err(PapyrusProviderProgramError::Call)?
                .ok_or(PapyrusProviderProgramError::UnsupportedStatement)?;
            require_result(&call, ScriptValueType::Boolean, "if condition")?;
            Ok(PapyrusProviderCondition::Call(call))
        }
    }
}

fn require_result(
    call: &TypedPapyrusProviderCall,
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
    world.insert(entity, program);
}

/// Execute provider handlers only after snapshotting programs and event
/// markers. No ECS query or resource guard survives the host callback.
pub fn papyrus_provider_system(world: &World, _dt: f32) {
    let callback = world
        .try_resource::<PapyrusProviderRuntime>()
        .and_then(|runtime| runtime.callback());
    let Some(callback) = callback else {
        return;
    };
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
                .map(|(entity, _)| entity)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let Some(programs) = world.query::<PapyrusProviderProgram>() else {
        return;
    };
    let mut handlers = Vec::new();
    for (entity, program) in programs.iter() {
        if loaded.contains(&entity) {
            handlers.push(program.handler(PapyrusProviderEvent::OnLoad).to_vec());
        }
        if activated.contains(&entity) {
            handlers.push(program.handler(PapyrusProviderEvent::OnActivate).to_vec());
        }
    }
    drop(programs);

    for statements in handlers {
        let mut locals = BTreeMap::new();
        if let Err(error) = execute_statements(&statements, callback.as_ref(), &mut locals) {
            log::warn!("Papyrus provider handler aborted: {error}");
        }
    }
}

fn execute_statements(
    statements: &[PapyrusProviderStatement],
    callback: &PapyrusProviderCallback,
    locals: &mut BTreeMap<String, ScriptValue>,
) -> Result<(), String> {
    for statement in statements {
        match statement {
            PapyrusProviderStatement::Declare { name, value } => {
                locals.insert(name.clone(), value.clone());
            }
            PapyrusProviderStatement::AssignCall { name, call } => {
                let value = callback(call.route.qualified_name(), &call.arguments)?;
                locals.insert(name.clone(), value);
            }
            PapyrusProviderStatement::Call(call) => {
                callback(call.route.qualified_name(), &call.arguments)?;
            }
            PapyrusProviderStatement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let selected = if evaluate_condition(condition, callback, locals)? {
                    then_branch
                } else {
                    else_branch
                };
                execute_statements(selected, callback, locals)?;
            }
        }
    }
    Ok(())
}

fn evaluate_condition(
    condition: &PapyrusProviderCondition,
    callback: &PapyrusProviderCallback,
    locals: &BTreeMap<String, ScriptValue>,
) -> Result<bool, String> {
    let value = match condition {
        PapyrusProviderCondition::Literal(value) => return Ok(*value),
        PapyrusProviderCondition::Local(name) => locals
            .get(name)
            .cloned()
            .ok_or_else(|| format!("translated local {name} was not initialized"))?,
        PapyrusProviderCondition::Call(call) => {
            callback(call.route.qualified_name(), &call.arguments)?
        }
    };
    match value {
        ScriptValue::Boolean(value) => Ok(value),
        _ => Err("provider returned a non-boolean condition result".to_owned()),
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
        let callback = Arc::new(move |route: &str, arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            if route.ends_with("is-storm") {
                Ok(ScriptValue::Boolean(true))
            } else {
                Ok(ScriptValue::String("rain".to_owned()))
            }
        }) as Arc<PapyrusProviderCallback>;
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
                ScriptValue::Integer(4),
                ScriptValue::String("clear".to_owned())
            ]
        );
    }
}
