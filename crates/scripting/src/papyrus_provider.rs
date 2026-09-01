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
    AssignOp, BinaryOp, CallArg, Event, Expr, Script, ScriptItem, StateItem, Stmt, Type, UnaryOp,
};
use byroredux_sdk::{
    compatibility::{classify_static_call, papyrus_game_content_declarations},
    identity::ExtensionId,
    script_function::{
        ScriptFunctionDeclaration, ScriptFunctionError, ScriptResultDeclaration, ScriptValue,
        ScriptValueType,
    },
};

use crate::events::{
    ActivateEvent, EquipmentEventBatch, HitEvent, OnCellLoadEvent, OnInitEvent, OnTriggerEnterEvent,
};
use crate::recurring_update::OnUpdateEvent;

const MAX_PROVIDER_HANDLER_NESTING: usize = 32;
const MAX_PROVIDER_CONTINUATIONS: usize = 4_096;

/// Host callback shared by Papyrus handlers after all ECS guards are dropped.
pub type PapyrusProviderCallback =
    dyn Fn(&str, &[ScriptValue]) -> Result<ScriptValue, String> + Send + Sync;

/// Live catalog and host callback published atomically by the executable.
#[derive(Clone)]
pub struct PapyrusProviderRuntime {
    catalog: Arc<PapyrusProviderCatalog>,
    callback: Option<Arc<PapyrusProviderCallback>>,
}

impl Resource for PapyrusProviderRuntime {}

impl Default for PapyrusProviderRuntime {
    fn default() -> Self {
        Self {
            catalog: Arc::new(PapyrusProviderCatalog::engine_compatibility()),
            callback: None,
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
        call: TypedPapyrusProviderCall,
    },
    Call(TypedPapyrusProviderCall),
    Wait {
        seconds: f32,
    },
    If {
        condition: Box<PapyrusProviderCondition>,
        then_branch: Vec<PapyrusProviderStatement>,
        else_branch: Vec<PapyrusProviderStatement>,
    },
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
struct PendingPapyrusProviderContinuation {
    remaining_seconds: f32,
    statements: Vec<PapyrusProviderStatement>,
    locals: BTreeMap<String, ScriptValue>,
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

/// Boolean expression subset used to select a translated branch.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum PapyrusProviderCondition {
    Literal(bool),
    Local(String),
    Call(TypedPapyrusProviderCall),
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
    Call(TypedPapyrusProviderCall),
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
    handlers: BTreeMap<PapyrusProviderEvent, PapyrusProviderHandler>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct PapyrusProviderHandler {
    statements: Vec<PapyrusProviderStatement>,
    parameters: Vec<PapyrusProviderParameterBinding>,
}

#[derive(Clone, Debug, PartialEq)]
struct PapyrusProviderParameterBinding {
    name: String,
    source: PapyrusProviderParameterSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PapyrusProviderParameterSource {
    PowerAttack,
    SneakAttack,
    BashAttack,
    Blocked,
}

impl Component for PapyrusProviderProgram {
    type Storage = SparseSetStorage<Self>;
}

impl PapyrusProviderProgram {
    /// Instructions for one canonical event.
    pub fn handler(&self, event: PapyrusProviderEvent) -> &[PapyrusProviderStatement] {
        self.handlers
            .get(&event)
            .map_or(&[], |handler| handler.statements.as_slice())
    }

    /// Whether no supported handler was present in the source unit.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    fn hit_locals(&self, event: &HitEvent) -> BTreeMap<String, ScriptValue> {
        let mut locals = BTreeMap::new();
        let Some(handler) = self.handlers.get(&PapyrusProviderEvent::OnHit) else {
            return locals;
        };
        for parameter in &handler.parameters {
            let value = match parameter.source {
                PapyrusProviderParameterSource::PowerAttack => event.power_attack,
                PapyrusProviderParameterSource::SneakAttack => event.sneak_attack,
                PapyrusProviderParameterSource::BashAttack => event.bash_attack,
                PapyrusProviderParameterSource::Blocked => event.blocked,
            };
            locals.insert(parameter.name.clone(), ScriptValue::Boolean(value));
        }
        locals
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
    let canonical = if event.name.node.eq_ignore_case("OnInit") {
        PapyrusProviderEvent::OnInit
    } else if event.name.node.eq_ignore_case("OnLoad") {
        PapyrusProviderEvent::OnLoad
    } else if event.name.node.eq_ignore_case("OnActivate") {
        PapyrusProviderEvent::OnActivate
    } else if event.name.node.eq_ignore_case("OnHit") {
        PapyrusProviderEvent::OnHit
    } else if event.name.node.eq_ignore_case("OnObjectEquipped") {
        PapyrusProviderEvent::OnObjectEquipped
    } else if event.name.node.eq_ignore_case("OnObjectUnequipped") {
        PapyrusProviderEvent::OnObjectUnequipped
    } else if event.name.node.eq_ignore_case("OnTriggerEnter") {
        PapyrusProviderEvent::OnTriggerEnter
    } else if event.name.node.eq_ignore_case("OnUpdate") {
        PapyrusProviderEvent::OnUpdate
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
    let parameters = lower_event_parameters(canonical, event, &mut locals);
    let statements = lower_statements(&event.body, catalog, &mut locals, 0)?;
    program.handlers.insert(
        canonical,
        PapyrusProviderHandler {
            statements,
            parameters,
        },
    );
    Ok(())
}

fn lower_event_parameters(
    event_kind: PapyrusProviderEvent,
    event: &Event,
    locals: &mut BTreeMap<String, ScriptValueType>,
) -> Vec<PapyrusProviderParameterBinding> {
    if event_kind != PapyrusProviderEvent::OnHit {
        return Vec::new();
    }
    let sources = [
        (3, PapyrusProviderParameterSource::PowerAttack),
        (4, PapyrusProviderParameterSource::SneakAttack),
        (5, PapyrusProviderParameterSource::BashAttack),
        (6, PapyrusProviderParameterSource::Blocked),
    ];
    let mut bindings = Vec::new();
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
                if let Some(seconds) = lower_wait(&expression.node)? {
                    lowered.push(PapyrusProviderStatement::Wait { seconds });
                    continue;
                }
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
            let call = lower_provider_call(expression, catalog)
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
    let literal = match expression {
        Expr::BoolLit(value) => Some((ScriptValue::Boolean(*value), ScriptValueType::Boolean)),
        Expr::IntLit(value) => Some((ScriptValue::Integer(*value), ScriptValueType::Integer)),
        Expr::FloatLit(value) => {
            let value = *value as f32;
            value
                .is_finite()
                .then_some((ScriptValue::Float(value), ScriptValueType::Float))
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
    let call = lower_provider_call(expression, catalog)
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
        _ => false,
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
    world.insert(entity, OnInitEvent);
}

/// Execute provider handlers only after snapshotting programs and event
/// markers. No ECS query or resource guard survives the host callback.
pub fn papyrus_provider_system(world: &World, dt: f32) {
    let runtime = world
        .try_resource::<PapyrusProviderRuntime>()
        .and_then(|runtime| {
            runtime
                .callback()
                .map(|callback| (runtime.catalog(), callback))
        });
    let Some((catalog, callback)) = runtime else {
        return;
    };
    let pending = {
        let mut queue = world.resource_mut::<PapyrusProviderContinuationQueue>();
        std::mem::take(&mut queue.pending)
    };
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
            handlers.push((continuation.statements, continuation.locals));
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
                .map(|(entity, _)| entity)
                .collect::<BTreeSet<_>>()
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
                            .map(|change| change.equipped)
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
                .map(|(entity, event)| (entity, event.triggerers.len()))
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
    let Some(programs) = world.query::<PapyrusProviderProgram>() else {
        return;
    };
    for (entity, program) in programs.iter() {
        if initialized.contains(&entity) {
            handlers.push((
                program.handler(PapyrusProviderEvent::OnInit).to_vec(),
                BTreeMap::new(),
            ));
        }
        if loaded.contains(&entity) {
            handlers.push((
                program.handler(PapyrusProviderEvent::OnLoad).to_vec(),
                BTreeMap::new(),
            ));
        }
        if activated.contains(&entity) {
            handlers.push((
                program.handler(PapyrusProviderEvent::OnActivate).to_vec(),
                BTreeMap::new(),
            ));
        }
        if let Some(hit) = hits.get(&entity) {
            handlers.push((
                program.handler(PapyrusProviderEvent::OnHit).to_vec(),
                program.hit_locals(hit),
            ));
        }
        if let Some(entry_count) = trigger_entries.get(&entity) {
            for _ in 0..*entry_count {
                handlers.push((
                    program
                        .handler(PapyrusProviderEvent::OnTriggerEnter)
                        .to_vec(),
                    BTreeMap::new(),
                ));
            }
        }
        if let Some(changes) = equipment_changes.get(&entity) {
            for equipped in changes {
                let event = if *equipped {
                    PapyrusProviderEvent::OnObjectEquipped
                } else {
                    PapyrusProviderEvent::OnObjectUnequipped
                };
                handlers.push((program.handler(event).to_vec(), BTreeMap::new()));
            }
        }
        if updated.contains(&entity) {
            handlers.push((
                program.handler(PapyrusProviderEvent::OnUpdate).to_vec(),
                BTreeMap::new(),
            ));
        }
    }
    drop(programs);

    for (statements, mut locals) in handlers {
        if let Err(error) = validate_provider_statements(&statements, catalog.as_ref(), 0) {
            log::warn!("Papyrus provider handler aborted before dispatch: {error}");
            continue;
        }
        match execute_statements(&statements, callback.as_ref(), &mut locals) {
            Ok(Some((remaining_seconds, statements))) => {
                still_pending.push(PendingPapyrusProviderContinuation {
                    remaining_seconds,
                    statements,
                    locals,
                });
            }
            Ok(None) => {}
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
            PapyrusProviderStatement::Declare { .. } => {}
            PapyrusProviderStatement::AssignCall { call, .. }
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

fn validate_provider_call(
    call: &TypedPapyrusProviderCall,
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
    current
        .validate_arguments(&call.arguments)
        .map_err(|error| format!("saved provider arguments are invalid: {error:?}"))
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
    match value {
        PapyrusProviderValue::Call(call) => validate_provider_call(call, catalog),
        PapyrusProviderValue::Literal(_) | PapyrusProviderValue::Local(_) => Ok(()),
    }
}

fn execute_statements(
    statements: &[PapyrusProviderStatement],
    callback: &PapyrusProviderCallback,
    locals: &mut BTreeMap<String, ScriptValue>,
) -> Result<Option<(f32, Vec<PapyrusProviderStatement>)>, String> {
    for (index, statement) in statements.iter().enumerate() {
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
            PapyrusProviderStatement::Wait { seconds } => {
                return Ok(Some((*seconds, statements[index + 1..].to_vec())));
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
                let mut ordered_tail =
                    Vec::with_capacity(selected.len() + statements.len().saturating_sub(index + 1));
                ordered_tail.extend_from_slice(selected);
                ordered_tail.extend_from_slice(&statements[index + 1..]);
                return execute_statements(&ordered_tail, callback, locals);
            }
        }
    }
    Ok(None)
}

fn evaluate_condition(
    condition: &PapyrusProviderCondition,
    callback: &PapyrusProviderCallback,
    locals: &BTreeMap<String, ScriptValue>,
) -> Result<bool, String> {
    match condition {
        PapyrusProviderCondition::Not(condition) => {
            return Ok(!evaluate_condition(condition, callback, locals)?);
        }
        PapyrusProviderCondition::And(left, right) => {
            return Ok(evaluate_condition(left, callback, locals)?
                && evaluate_condition(right, callback, locals)?);
        }
        PapyrusProviderCondition::Or(left, right) => {
            return Ok(evaluate_condition(left, callback, locals)?
                || evaluate_condition(right, callback, locals)?);
        }
        PapyrusProviderCondition::Compare {
            left,
            operator,
            right,
        } => {
            let left = evaluate_condition_value(left, callback, locals)?;
            let right = evaluate_condition_value(right, callback, locals)?;
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
            callback(call.route.qualified_name(), &call.arguments)?
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
    locals: &BTreeMap<String, ScriptValue>,
) -> Result<ScriptValue, String> {
    match value {
        PapyrusProviderValue::Literal(value) => Ok(value.clone()),
        PapyrusProviderValue::Local(name) => locals
            .get(name)
            .cloned()
            .ok_or_else(|| format!("translated local {name} was not initialized")),
        PapyrusProviderValue::Call(call) => callback(call.route.qualified_name(), &call.arguments),
    }
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
    fn engine_compatibility_catalog_lowers_only_the_exact_game_alias() {
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
        assert_eq!(
            lower_provider_call(&expression("Game.GetPlayer()"), &catalog),
            Ok(None)
        );
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
        let callback = Arc::new(move |route: &str, arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            Ok(ScriptValue::String("clear".to_owned()))
        }) as Arc<PapyrusProviderCallback>;
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
    fn combat_and_equipment_events_dispatch_in_batch_order() {
        let source = r#"
            ScriptName Fixture
            Event OnHit(ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked)
                If abPowerAttack && !abHitBlocked
                    WeatherNative.WeatherAt(1, "hit")
                EndIf
            EndEvent
            Event OnObjectEquipped()
                WeatherNative.WeatherAt(2, "equipped")
            EndEvent
            Event OnObjectUnequipped()
                WeatherNative.WeatherAt(3, "unequipped")
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
        let callback = Arc::new(move |_route: &str, arguments: &[ScriptValue]| {
            calls_for_callback.lock().unwrap().push(arguments.to_vec());
            Ok(ScriptValue::String("clear".to_owned()))
        }) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
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
        assert_eq!(calls[1][0], ScriptValue::Integer(3));
        assert_eq!(calls[2][0], ScriptValue::Integer(2));
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
    fn latent_wait_preserves_locals_and_branch_and_handler_tails() {
        let source = r#"
            ScriptName Fixture
            Event OnLoad()
                Bool storm
                storm = WeatherNative.IsStorm()
                If storm
                    Utility.Wait(0.5)
                    WeatherNative.WeatherAt(4, "after-branch-wait")
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
        let callback = Arc::new(move |route: &str, arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            if route.ends_with("is-storm") {
                Ok(ScriptValue::Boolean(true))
            } else {
                Ok(ScriptValue::String("ok".to_owned()))
            }
        }) as Arc<PapyrusProviderCallback>;
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
        let callback = Arc::new(move |route: &str, _arguments: &[ScriptValue]| {
            calls_for_callback.lock().unwrap().push(route.to_owned());
            Ok(ScriptValue::None)
        }) as Arc<PapyrusProviderCallback>;
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
        let callback = Arc::new(move |route: &str, arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            if route.ends_with("get-mod-count") {
                Ok(ScriptValue::Integer(2))
            } else if route.ends_with("is-plugin-installed") {
                Ok(ScriptValue::Boolean(false))
            } else {
                Ok(ScriptValue::String("ok".to_owned()))
            }
        }) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(provider_catalog), Some(callback));
        let entity = world.spawn();
        attach_papyrus_provider_program(&mut world, entity, program);
        world.insert(entity, OnCellLoadEvent);

        papyrus_provider_system(&world, 0.0);

        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 4);
        assert!(calls[0].0.ends_with("get-mod-count"));
        assert!(calls[1].0.ends_with("is-plugin-installed"));
        assert_eq!(calls[2].1[0], ScriptValue::Integer(4));
        assert_eq!(calls[3].1[0], ScriptValue::Integer(6));
        assert!(calls.iter().all(|(_, arguments)| arguments.first()
            != Some(&ScriptValue::String("MustNotRun.esp".to_owned()))));
    }

    #[test]
    fn trigger_enter_and_update_events_dispatch_provider_handlers() {
        let source = r#"
            ScriptName Fixture
            Event OnTriggerEnter(ObjectReference akActionRef)
                WeatherNative.WeatherAt(7, "trigger")
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
        let callback = Arc::new(move |route: &str, arguments: &[ScriptValue]| {
            calls_for_callback
                .lock()
                .unwrap()
                .push((route.to_owned(), arguments.to_vec()));
            Ok(ScriptValue::String("ok".to_owned()))
        }) as Arc<PapyrusProviderCallback>;
        set_papyrus_provider_runtime(&world, Arc::new(catalog()), Some(callback));
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
        assert_eq!(calls[0].1[0], ScriptValue::Integer(7));
        assert_eq!(calls[1].1[0], ScriptValue::Integer(7));
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
