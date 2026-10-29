//! Papyrus AST predicates the QF/player-callback contract checks are built from.

use super::*;

pub(super) fn expression_call_name(expression: &Expr) -> Option<String> {
    let Expr::Call { callee, .. } = expression else {
        return None;
    };
    match &callee.node {
        Expr::MemberAccess { member, .. } => Some(member.node.0.to_ascii_lowercase()),
        Expr::Ident(name) => Some(name.0.to_ascii_lowercase()),
        _ => Some("<dynamic-call>".to_owned()),
    }
}

pub(super) fn statement_shape(statement: &Stmt) -> String {
    match statement {
        Stmt::ExprStmt(expression) => expression_call_name(&expression.node)
            .map_or_else(|| "expr".to_owned(), |name| format!("call:{name}")),
        Stmt::VarDecl(variable) => variable
            .initial_value
            .as_ref()
            .and_then(|value| expression_call_name(&value.node))
            .map_or_else(|| "let".to_owned(), |name| format!("let:{name}")),
        Stmt::Assign { value, .. } => expression_call_name(&value.node)
            .map_or_else(|| "assign".to_owned(), |name| format!("assign:{name}")),
        Stmt::If { .. } => "if".to_owned(),
        Stmt::While { .. } => "while".to_owned(),
        Stmt::Return(_) => "return".to_owned(),
    }
}

pub(super) fn expression_contains_string(expression: &Expr, needle: &str) -> bool {
    match expression {
        Expr::StringLit(value) => value.eq_ignore_ascii_case(needle),
        Expr::MemberAccess { object, .. } => expression_contains_string(&object.node, needle),
        Expr::Index { object, index } => {
            expression_contains_string(&object.node, needle)
                || expression_contains_string(&index.node, needle)
        }
        Expr::Call { callee, args } => {
            expression_contains_string(&callee.node, needle)
                || args
                    .iter()
                    .any(|arg| expression_contains_string(&arg.value.node, needle))
        }
        Expr::UnaryOp { operand, .. } => expression_contains_string(&operand.node, needle),
        Expr::BinaryOp { left, right, .. } => {
            expression_contains_string(&left.node, needle)
                || expression_contains_string(&right.node, needle)
        }
        Expr::Cast { expr, .. } => expression_contains_string(&expr.node, needle),
        Expr::Is { expr, .. } => expression_contains_string(&expr.node, needle),
        Expr::New { size, .. } => expression_contains_string(&size.node, needle),
        Expr::NewStruct { .. } => false,
        Expr::ArrayLit(values) => values
            .iter()
            .any(|value| expression_contains_string(&value.node, needle)),
        Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::BoolLit(_)
        | Expr::NoneLit
        | Expr::Ident(_)
        | Expr::ParentAccess => false,
    }
}

pub(super) fn expression_contains_call(expression: &Expr, receiver: &str, method: &str) -> bool {
    let is_match = match expression {
        Expr::Call { callee, .. } => match &callee.node {
            Expr::MemberAccess { object, member } => {
                matches!(&object.node, Expr::Ident(name) if name.0.eq_ignore_ascii_case(receiver))
                    && member.node.0.eq_ignore_ascii_case(method)
            }
            _ => false,
        },
        _ => false,
    };
    if is_match {
        return true;
    }
    match expression {
        Expr::MemberAccess { object, .. } => {
            expression_contains_call(&object.node, receiver, method)
        }
        Expr::Index { object, index } => {
            expression_contains_call(&object.node, receiver, method)
                || expression_contains_call(&index.node, receiver, method)
        }
        Expr::Call { callee, args } => {
            expression_contains_call(&callee.node, receiver, method)
                || args
                    .iter()
                    .any(|arg| expression_contains_call(&arg.value.node, receiver, method))
        }
        Expr::UnaryOp { operand, .. } => expression_contains_call(&operand.node, receiver, method),
        Expr::BinaryOp { left, right, .. } => {
            expression_contains_call(&left.node, receiver, method)
                || expression_contains_call(&right.node, receiver, method)
        }
        Expr::Cast { expr, .. } => expression_contains_call(&expr.node, receiver, method),
        Expr::Is { expr, .. } => expression_contains_call(&expr.node, receiver, method),
        Expr::New { size, .. } => expression_contains_call(&size.node, receiver, method),
        Expr::NewStruct { .. } => false,
        Expr::ArrayLit(values) => values
            .iter()
            .any(|value| expression_contains_call(&value.node, receiver, method)),
        Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::BoolLit(_)
        | Expr::StringLit(_)
        | Expr::NoneLit
        | Expr::Ident(_)
        | Expr::ParentAccess => false,
    }
}

pub(super) fn statement_call(statement: &Stmt) -> Option<(&Expr, &str, &[CallArg])> {
    let Stmt::ExprStmt(expression) = statement else {
        return None;
    };
    let Expr::Call { callee, args } = &expression.node else {
        return None;
    };
    let Expr::MemberAccess { object, member } = &callee.node else {
        return None;
    };
    Some((&object.node, member.node.0.as_str(), args))
}

pub(super) fn ident_is(expression: &Expr, expected: &str) -> bool {
    matches!(expression, Expr::Ident(name) if name.0.eq_ignore_ascii_case(expected))
}

pub(super) fn single_float_arg(args: &[CallArg], expected: f64) -> bool {
    matches!(args, [arg] if matches!(arg.value.node, Expr::FloatLit(value) if value == expected))
}

pub(super) fn single_int_arg(args: &[CallArg], expected: i64) -> bool {
    matches!(args, [arg] if matches!(arg.value.node, Expr::IntLit(value) if value == expected))
}

pub(super) fn unregister_call(statement: &Stmt, event_name: &str) -> bool {
    let Some((receiver, method, args)) = statement_call(statement) else {
        return false;
    };
    ident_is(receiver, "self")
        && method.eq_ignore_ascii_case("UnregisterForAnimationEvent")
        && matches!(args, [source, event]
            if expression_contains_call(&source.value.node, "game", "GetPlayer")
                && matches!(&event.value.node, Expr::StringLit(value)
                    if value.eq_ignore_ascii_case(event_name)))
}

pub(super) fn callback_branch<'a>(event: &'a Event, event_name: &str) -> Option<&'a [Spanned<Stmt>]> {
    event.body.iter().find_map(|statement| {
        let Stmt::If {
            condition, body, ..
        } = &statement.node
        else {
            return None;
        };
        (expression_contains_string(&condition.node, event_name)
            && expression_contains_call(&condition.node, "game", "GetPlayer"))
        .then_some(body.as_slice())
    })
}

pub(super) fn mq101_player_callback_contract(script: &Script) -> bool {
    let Some(event) = script.body.iter().find_map(|item| match &item.node {
        ScriptItem::Event(event) if event.name.node.0.eq_ignore_ascii_case("OnAnimationEvent") => {
            Some(event)
        }
        _ => None,
    }) else {
        return false;
    };
    let Some(play_imod) = callback_branch(event, "PlayImod") else {
        return false;
    };
    let Some(furniture_exit) = callback_branch(event, "IdleFurnitureExit") else {
        return false;
    };

    let play_imod_ok = matches!(play_imod, [player_imod, blur, stage, unregister]
        if matches!(statement_call(&player_imod.node), Some((receiver, method, args))
            if ident_is(receiver, "::PlayerAlduinIMOD_var")
                && method.eq_ignore_ascii_case("Apply")
                && single_float_arg(args, 1.0))
        && matches!(statement_call(&blur.node), Some((receiver, method, args))
            if ident_is(receiver, "::CGDragonAttackBlurLong_var")
                && method.eq_ignore_ascii_case("Apply")
                && single_float_arg(args, 1.0))
        && matches!(statement_call(&stage.node), Some((receiver, method, args))
            if ident_is(receiver, "self")
                && method.eq_ignore_ascii_case("SetStage")
                && single_int_arg(args, 145))
        && unregister_call(&unregister.node, "PlayImod"));
    let furniture_exit_ok = matches!(furniture_exit, [stage, unregister]
        if matches!(statement_call(&stage.node), Some((receiver, method, args))
            if ident_is(receiver, "self")
                && method.eq_ignore_ascii_case("SetStage")
                && single_int_arg(args, 160))
        && unregister_call(&unregister.node, "IdleFurnitureExit"));
    play_imod_ok && furniture_exit_ok
}
