//! Program lowering: Papyrus AST -> the IR in `super::ir`. Pure and
//! table-testable; needs no `World`.

use super::*;

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

pub(crate) fn lower_event_into(
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
    let mut locals = BTreeMap::from([(
        PAPYRUS_SELF_LOCAL.to_owned(),
        PapyrusProviderLocalType::scalar(ScriptValueType::Entity),
    )]);
    let mut parameters = if let Some(canonical) = canonical {
        lower_event_parameters(canonical, event, &mut locals)
    } else {
        lower_mod_event_parameters(event, &mut locals)?
    };
    if !event
        .body
        .iter()
        .any(|statement| statement_mentions_provider(&statement.node, catalog, &locals, 0))
    {
        return Ok(());
    }
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

pub(crate) fn lower_mod_event_parameters(
    event: &Event,
    locals: &mut BTreeMap<String, PapyrusProviderLocalType>,
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
            let local_type = match &parameter.ty.node {
                Type::Object(object) => PapyrusProviderLocalType::object(value_type, &object.0),
                _ => PapyrusProviderLocalType::scalar(value_type),
            };
            locals.insert(name.clone(), local_type);
            Ok(PapyrusProviderParameterBinding {
                name,
                source: PapyrusProviderParameterSource::ModEventArgument { index, value_type },
            })
        })
        .collect()
}

pub(crate) fn lower_event_parameters(
    event_kind: PapyrusProviderEvent,
    event: &Event,
    locals: &mut BTreeMap<String, PapyrusProviderLocalType>,
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
                if let Type::Object(object) = &parameter.ty.node {
                    locals.insert(
                        name.clone(),
                        PapyrusProviderLocalType::object(ScriptValueType::Entity, &object.0),
                    );
                }
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
                if let Type::Object(object) = &parameter.ty.node {
                    locals.insert(
                        name.clone(),
                        PapyrusProviderLocalType::object(ScriptValueType::Form, &object.0),
                    );
                }
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
        locals.insert(
            name.clone(),
            PapyrusProviderLocalType::scalar(ScriptValueType::Boolean),
        );
        bindings.push(PapyrusProviderParameterBinding { name, source });
    }
    bindings
}

pub(crate) fn statements_reference_local(
    statements: &[PapyrusProviderStatement],
    name: &str,
) -> bool {
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

pub(crate) fn statements_contain_wait(statements: &[PapyrusProviderStatement]) -> bool {
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

pub(crate) fn statements_contain_mod_event_registration(
    statements: &[PapyrusProviderStatement],
) -> bool {
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

pub(crate) fn invocation_references_local(call: &PapyrusProviderInvocation, name: &str) -> bool {
    call.receiver
        .iter()
        .map(Box::as_ref)
        .chain(call.arguments.iter())
        .any(|argument| argument_references_local(argument, name))
}

pub(crate) fn argument_references_local(argument: &PapyrusProviderArgument, name: &str) -> bool {
    match argument {
        PapyrusProviderArgument::Local { name: local, .. } => local == name,
        PapyrusProviderArgument::Value { value, .. } => value_references_local(value, name),
        PapyrusProviderArgument::Literal(_) => false,
    }
}

pub(crate) fn condition_references_local(condition: &PapyrusProviderCondition, name: &str) -> bool {
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

pub(crate) fn value_references_local(value: &PapyrusProviderValue, name: &str) -> bool {
    match value {
        PapyrusProviderValue::Literal(_) => false,
        PapyrusProviderValue::Local(local) => local == name,
        PapyrusProviderValue::Call(call) => invocation_references_local(call, name),
        PapyrusProviderValue::Binary { left, right, .. } => {
            value_references_local(left, name) || value_references_local(right, name)
        }
    }
}

pub(crate) fn lower_statements(
    statements: &[byroredux_papyrus::span::Spanned<Stmt>],
    catalog: &PapyrusProviderCatalog,
    locals: &mut BTreeMap<String, PapyrusProviderLocalType>,
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
                let Some(local_type) = sdk_local_type(&variable.ty.node) else {
                    return Err(PapyrusProviderProgramError::UnsupportedLocal(
                        variable.name.node.0.clone(),
                    ));
                };
                let value_type = local_type.value_type;
                let value = if let Some(initial) = &variable.initial_value {
                    lower_literal(&initial.node, value_type, false).ok_or_else(|| {
                        PapyrusProviderProgramError::UnsupportedLocal(variable.name.node.0.clone())
                    })?
                } else {
                    default_value(value_type)
                };
                let key = variable.name.node.0.to_ascii_lowercase();
                locals.insert(key.clone(), local_type);
                lowered.push(PapyrusProviderStatement::Declare { name: key, value });
            }
            Stmt::Assign { target, op, value } if *op == AssignOp::Eq => {
                let Expr::Ident(target) = &target.node else {
                    return Err(PapyrusProviderProgramError::UnsupportedStatement);
                };
                let key = target.0.to_ascii_lowercase();
                let expected = locals
                    .get(&key)
                    .map(|local| local.value_type)
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

pub(crate) fn lower_mod_event_registration(
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

pub(crate) fn lower_send_mod_event(
    expression: &Expr,
    locals: &BTreeMap<String, PapyrusProviderLocalType>,
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

pub(crate) fn lower_mod_event_argument(
    expression: &Expr,
    expected: ScriptValueType,
    locals: &BTreeMap<String, PapyrusProviderLocalType>,
) -> Result<PapyrusProviderArgument, PapyrusProviderProgramError> {
    if let Some(value) = lower_literal(expression, expected, false) {
        return Ok(PapyrusProviderArgument::Literal(value));
    }
    if let Expr::Ident(identifier) = expression {
        let name = identifier.0.to_ascii_lowercase();
        if locals
            .get(&name)
            .is_some_and(|local| local.value_type == expected)
        {
            return Ok(PapyrusProviderArgument::Local {
                name,
                value_type: expected,
            });
        }
    }
    Err(PapyrusProviderProgramError::UnsupportedStatement)
}

pub(crate) fn lower_wait(expression: &Expr) -> Result<Option<f32>, PapyrusProviderProgramError> {
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

pub(crate) fn lower_condition(
    expression: &Expr,
    catalog: &PapyrusProviderCatalog,
    locals: &BTreeMap<String, PapyrusProviderLocalType>,
) -> Result<PapyrusProviderCondition, PapyrusProviderProgramError> {
    lower_condition_at_depth(expression, catalog, locals, 0)
}

pub(crate) fn lower_condition_at_depth(
    expression: &Expr,
    catalog: &PapyrusProviderCatalog,
    locals: &BTreeMap<String, PapyrusProviderLocalType>,
    depth: usize,
) -> Result<PapyrusProviderCondition, PapyrusProviderProgramError> {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return Err(PapyrusProviderProgramError::NestingTooDeep);
    }
    match expression {
        Expr::BoolLit(value) => Ok(PapyrusProviderCondition::Literal(*value)),
        Expr::Ident(identifier) => {
            let key = identifier.0.to_ascii_lowercase();
            if locals
                .get(&key)
                .is_some_and(|local| local.value_type == ScriptValueType::Boolean)
            {
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

pub(crate) fn lower_condition_value(
    expression: &Expr,
    catalog: &PapyrusProviderCatalog,
    locals: &BTreeMap<String, PapyrusProviderLocalType>,
) -> Result<(PapyrusProviderValue, ScriptValueType), PapyrusProviderProgramError> {
    lower_provider_value(expression, catalog, locals, 0)
}

pub(crate) fn lower_provider_value(
    expression: &Expr,
    catalog: &PapyrusProviderCatalog,
    locals: &BTreeMap<String, PapyrusProviderLocalType>,
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
            .map(|local| local.value_type)
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
        .filter(|result| !result.optional || matches!(result.value_type, ScriptValueType::Entity))
        .ok_or_else(|| PapyrusProviderProgramError::ResultTypeMismatch("comparison".to_owned()))?;
    let value_type = result.value_type;
    Ok((PapyrusProviderValue::Call(call), value_type))
}

pub(crate) fn provider_arithmetic(operator: BinaryOp) -> Option<PapyrusProviderArithmetic> {
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

pub(crate) fn comparison_operator(operator: BinaryOp) -> Option<PapyrusProviderComparison> {
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

pub(crate) fn comparison_is_supported(
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

pub(crate) fn require_result(
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

pub(crate) fn sdk_type(value: &Type) -> Option<ScriptValueType> {
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

pub(crate) fn sdk_local_type(value: &Type) -> Option<PapyrusProviderLocalType> {
    match value {
        Type::Object(object) => Some(PapyrusProviderLocalType::object(
            ScriptValueType::Entity,
            &object.0,
        )),
        _ => sdk_type(value).map(PapyrusProviderLocalType::scalar),
    }
}

pub(crate) fn provider_value_object_type(
    value: &PapyrusProviderValue,
) -> Option<PapyrusProviderObjectType> {
    match value {
        PapyrusProviderValue::Call(call) => call.result_object_type,
        PapyrusProviderValue::Literal(_)
        | PapyrusProviderValue::Local(_)
        | PapyrusProviderValue::Binary { .. } => None,
    }
}

pub(crate) fn provider_result_object_type(
    route: &PapyrusProviderRoute,
) -> Option<PapyrusProviderObjectType> {
    let declaration = route.declaration();
    if declaration
        .result
        .as_ref()
        .is_none_or(|result| result.value_type != ScriptValueType::Entity)
    {
        return None;
    }
    let alias = declaration.papyrus.as_ref()?;
    if route.qualified_name() == PAPYRUS_GAME_GET_PLAYER_ROUTE
        || alias.provider.eq_ignore_ascii_case("ObjectReference")
    {
        Some(PapyrusProviderObjectType::ObjectReference)
    } else {
        None
    }
}

pub(crate) fn default_value(value_type: ScriptValueType) -> ScriptValue {
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

pub(crate) fn statement_mentions_provider(
    statement: &Stmt,
    catalog: &PapyrusProviderCatalog,
    locals: &BTreeMap<String, PapyrusProviderLocalType>,
    depth: usize,
) -> bool {
    if depth > MAX_PROVIDER_HANDLER_NESTING {
        return true;
    }
    match statement {
        Stmt::Assign { target, value, .. } => {
            expression_mentions_provider(&target.node, catalog, locals, depth + 1)
                || expression_mentions_provider(&value.node, catalog, locals, depth + 1)
        }
        Stmt::Return(value) => value.as_ref().is_some_and(|value| {
            expression_mentions_provider(&value.node, catalog, locals, depth + 1)
        }),
        Stmt::If {
            condition,
            body,
            elseif_clauses,
            else_body,
        } => {
            expression_mentions_provider(&condition.node, catalog, locals, depth + 1)
                || body
                    .iter()
                    .any(|stmt| statement_mentions_provider(&stmt.node, catalog, locals, depth + 1))
                || elseif_clauses.iter().any(|(condition, body)| {
                    expression_mentions_provider(&condition.node, catalog, locals, depth + 1)
                        || body.iter().any(|stmt| {
                            statement_mentions_provider(&stmt.node, catalog, locals, depth + 1)
                        })
                })
                || else_body.as_ref().is_some_and(|body| {
                    body.iter().any(|stmt| {
                        statement_mentions_provider(&stmt.node, catalog, locals, depth + 1)
                    })
                })
        }
        Stmt::While { condition, body } => {
            expression_mentions_provider(&condition.node, catalog, locals, depth + 1)
                || body
                    .iter()
                    .any(|stmt| statement_mentions_provider(&stmt.node, catalog, locals, depth + 1))
        }
        Stmt::ExprStmt(expression) => {
            expression_mentions_provider(&expression.node, catalog, locals, depth + 1)
        }
        Stmt::VarDecl(variable) => variable.initial_value.as_ref().is_some_and(|value| {
            expression_mentions_provider(&value.node, catalog, locals, depth + 1)
        }),
    }
}

pub(crate) fn expression_mentions_provider(
    expression: &Expr,
    catalog: &PapyrusProviderCatalog,
    locals: &BTreeMap<String, PapyrusProviderLocalType>,
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
                                || locals
                                    .get(&provider.0.to_ascii_lowercase())
                                    .and_then(|local| local.object_type.as_deref())
                                    .is_some_and(|object_type| {
                                        catalog.resolve(object_type, &member.node.0).is_some()
                                    })
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
                || expression_mentions_provider(&callee.node, catalog, locals, depth + 1)
                || args.iter().any(|arg| {
                    expression_mentions_provider(&arg.value.node, catalog, locals, depth + 1)
                })
        }
        Expr::MemberAccess { object, .. } => {
            expression_mentions_provider(&object.node, catalog, locals, depth + 1)
        }
        Expr::Index { object, index } => {
            expression_mentions_provider(&object.node, catalog, locals, depth + 1)
                || expression_mentions_provider(&index.node, catalog, locals, depth + 1)
        }
        Expr::UnaryOp { operand, .. } => {
            expression_mentions_provider(&operand.node, catalog, locals, depth + 1)
        }
        Expr::BinaryOp { left, right, .. } => {
            expression_mentions_provider(&left.node, catalog, locals, depth + 1)
                || expression_mentions_provider(&right.node, catalog, locals, depth + 1)
        }
        Expr::Cast { expr, .. } => {
            expression_mentions_provider(&expr.node, catalog, locals, depth + 1)
        }
        Expr::New { size, .. } => {
            expression_mentions_provider(&size.node, catalog, locals, depth + 1)
        }
        Expr::ArrayLit(values) => values
            .iter()
            .any(|value| expression_mentions_provider(&value.node, catalog, locals, depth + 1)),
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

pub(crate) fn statements_need_owner_sender(statements: &[PapyrusProviderStatement]) -> bool {
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

pub(crate) fn resolve_mod_event_senders(
    statements: &mut [PapyrusProviderStatement],
    owner: Option<FormRef>,
) {
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
