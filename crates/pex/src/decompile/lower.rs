//! Lower the decompiler's node tree to the shared Papyrus AST
//! (`byroredux_papyrus::ast`). This is the retarget that lets a decompiled
//! `.pex` reuse every `.psc` recognizer: instead of Champollion's `.psc`
//! text generation, we emit the same [`Script`] the M30 parser produces.
//!
//! Scope today: structural lowering of expressions, statements, and the
//! object → script assembly (header / variables / properties / states /
//! functions+events). Cosmetic cleanups Champollion does on its way to
//! text — compound-assign recovery, `ElseIf` flattening, name unmangling —
//! are not applied; they don't affect recognizer matching, which keys on
//! names, calls, and condition shape. `Copy` nodes are unwrapped inline.

use byroredux_papyrus::ast::{
    self as past, BinaryOp, CallArg, Event, Expr, Function, Param, Property, PropertyFlags, Script,
    ScriptFlags, ScriptItem, State, StateItem, Stmt, Type, UnaryOp, Variable,
};
use byroredux_papyrus::span::{Span, Spanned};

use crate::model::{Function as PexFunction, Object, Pex, Value};

use super::boolean::rebuild_boolean_operators;
use super::cfg::build_cfg;
use super::control_flow::reconstruct;
use super::event_names::is_event_name;
use super::lift::lift_function;
use super::node::{Node, NodeKind};
use super::DecompileError;

/// All synthesized AST nodes share an empty span (we have no source text).
fn sp<T>(node: T) -> Spanned<T> {
    Spanned::new(node, Span::empty(0))
}

fn ident(name: &str) -> Spanned<past::Identifier> {
    sp(past::Identifier::new(name))
}

/// Map a `.pex` type name to a Papyrus [`Type`]. `X[]` is an array of `X`;
/// the primitives map by name; anything else is an `Object` type.
fn lower_type(name: &str) -> Type {
    if let Some(elem) = name.strip_suffix("[]") {
        return Type::Array(Box::new(lower_type(elem)));
    }
    match name.to_ascii_lowercase().as_str() {
        "bool" => Type::Bool,
        "int" => Type::Int,
        "float" => Type::Float,
        "string" => Type::String,
        "var" => Type::Var,
        _ => Type::Object(past::Identifier::new(name)),
    }
}

/// Map a decompiler operator string to a Papyrus [`BinaryOp`]. `+` covers
/// both integer/float add and string concat (the bytecode op the node came
/// from is lost by this point, and recognizers don't distinguish them).
fn lower_binary_op(op: &str) -> BinaryOp {
    match op {
        "+" => BinaryOp::Add,
        "-" => BinaryOp::Sub,
        "*" => BinaryOp::Mul,
        "/" => BinaryOp::Div,
        "%" => BinaryOp::Mod,
        "==" => BinaryOp::Eq,
        "!=" => BinaryOp::Ne,
        "<" => BinaryOp::Lt,
        "<=" => BinaryOp::Le,
        ">" => BinaryOp::Gt,
        ">=" => BinaryOp::Ge,
        "&&" => BinaryOp::And,
        "||" => BinaryOp::Or,
        // Shouldn't reach here; default keeps lowering total.
        _ => BinaryOp::Eq,
    }
}

/// The type name carried by a node used as a type operand (the right side
/// of `is`, a constant naming a class).
fn node_type_name(node: &Node) -> String {
    match &node.kind {
        NodeKind::Constant(Value::Identifier(s)) => s.clone(),
        NodeKind::IdentifierString(s) => s.clone(),
        _ => "Var".to_string(),
    }
}

/// Lower a node used as an expression.
fn lower_expr(node: &Node) -> Spanned<Expr> {
    let expr = match &node.kind {
        NodeKind::Constant(value) => match value {
            Value::None => Expr::NoneLit,
            Value::Integer(n) => Expr::IntLit(*n as i64),
            Value::Float(f) => Expr::FloatLit(*f as f64),
            Value::Bool(b) => Expr::BoolLit(*b),
            Value::Str(s) => Expr::StringLit(s.clone()),
            Value::Identifier(s) => match s.to_ascii_lowercase().as_str() {
                "true" => Expr::BoolLit(true),
                "false" => Expr::BoolLit(false),
                "none" | "::nonevar" => Expr::NoneLit,
                _ => Expr::Ident(past::Identifier::new(s)),
            },
        },
        NodeKind::IdentifierString(s) => {
            if s.eq_ignore_ascii_case("parent") {
                Expr::ParentAccess
            } else {
                Expr::Ident(past::Identifier::new(s))
            }
        }
        NodeKind::BinaryOp { left, op, right } if op == "is" => {
            // Papyrus's `a is T` type-test has no BinaryOp counterpart in
            // the shared AST; lower to the structurally closest `a as T`.
            // Rare and recognizer-irrelevant.
            Expr::Cast {
                expr: Box::new(lower_expr(left)),
                target_type: sp(lower_type(&node_type_name(right))),
            }
        }
        NodeKind::BinaryOp { left, op, right } => Expr::BinaryOp {
            left: Box::new(lower_expr(left)),
            op: lower_binary_op(op),
            right: Box::new(lower_expr(right)),
        },
        NodeKind::UnaryOp { op, operand } => Expr::UnaryOp {
            op: if op == "!" { UnaryOp::Not } else { UnaryOp::Neg },
            operand: Box::new(lower_expr(operand)),
        },
        // `Copy` is a transparent value wrapper — unwrap it.
        NodeKind::Copy { value } => return lower_expr(value),
        NodeKind::Cast { value, target_type } => Expr::Cast {
            expr: Box::new(lower_expr(value)),
            target_type: sp(lower_type(target_type)),
        },
        NodeKind::CallMethod { object, method, params, .. } => Expr::Call {
            callee: Box::new(sp(Expr::MemberAccess {
                object: Box::new(lower_expr(object)),
                member: ident(method),
            })),
            args: params
                .iter()
                .map(|p| CallArg { name: None, value: lower_expr(p) })
                .collect(),
        },
        NodeKind::PropertyAccess { object, property } => Expr::MemberAccess {
            object: Box::new(lower_expr(object)),
            member: ident(property),
        },
        NodeKind::ArrayLength { array } => Expr::MemberAccess {
            object: Box::new(lower_expr(array)),
            member: ident("Length"),
        },
        NodeKind::ArrayAccess { array, index } => Expr::Index {
            object: Box::new(lower_expr(array)),
            index: Box::new(lower_expr(index)),
        },
        NodeKind::ArrayCreate { element_type, size } => Expr::New {
            ty: sp(lower_type(element_type)),
            size: Box::new(lower_expr(size)),
        },
        // Struct creation (`new T`) has no size; the AST's `New` requires
        // one, so we use 0. Rare (FO4+ structs) and recognizer-irrelevant.
        NodeKind::StructCreate { struct_type } => Expr::New {
            ty: sp(lower_type(struct_type)),
            size: Box::new(sp(Expr::IntLit(0))),
        },
        // Statement-shaped nodes shouldn't appear as sub-expressions.
        NodeKind::Assign { .. }
        | NodeKind::Return { .. }
        | NodeKind::IfElse { .. }
        | NodeKind::While { .. } => Expr::NoneLit,
    };
    sp(expr)
}

/// Lower a top-level node (always a statement in this position).
fn lower_stmt(node: &Node) -> Spanned<Stmt> {
    let stmt = match &node.kind {
        NodeKind::Assign { dest, value } => Stmt::Assign {
            target: lower_expr(dest),
            op: past::AssignOp::Eq,
            value: lower_expr(value),
        },
        NodeKind::Return { value } => {
            Stmt::Return(value.as_ref().map(|v| lower_expr(v)))
        }
        NodeKind::IfElse { condition, body, else_body, .. } => Stmt::If {
            condition: lower_expr(condition),
            body: lower_body(body),
            elseif_clauses: Vec::new(),
            else_body: if else_body.is_empty() {
                None
            } else {
                Some(lower_body(else_body))
            },
        },
        NodeKind::While { condition, body } => Stmt::While {
            condition: lower_expr(condition),
            body: lower_body(body),
        },
        _ => Stmt::ExprStmt(lower_expr(node)),
    };
    sp(stmt)
}

fn lower_body(nodes: &[Node]) -> Vec<Spanned<Stmt>> {
    nodes.iter().map(lower_stmt).collect()
}

/// Decompile one `.pex` function to a Papyrus body. The public seam the
/// tests and the script assembly share.
fn decompile_body(object: &Object, func: &PexFunction) -> Result<Vec<Spanned<Stmt>>, DecompileError> {
    let mut cfg = build_cfg(func)?;
    let mut scopes = lift_function(object, func, &cfg)?;
    // Collapse `&&`/`||` short-circuits before control-flow reconstruction
    // so compound conditions surface as one expression, not nested ifs.
    rebuild_boolean_operators(&mut cfg, &mut scopes, &func.name)?;
    let nodes = reconstruct(cfg, scopes, &func.name)?;
    Ok(lower_body(&nodes))
}

fn lower_params(func: &PexFunction) -> Vec<Param> {
    func.params
        .iter()
        .map(|p| Param {
            ty: sp(lower_type(&p.type_name)),
            name: ident(&p.name),
            default: None,
        })
        .collect()
}

fn lower_return_type(func: &PexFunction) -> Option<Spanned<Type>> {
    if func.return_type_name.is_empty() || func.return_type_name.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(sp(lower_type(&func.return_type_name)))
    }
}

fn function_flags(func: &PexFunction) -> past::FunctionFlags {
    let mut flags = past::FunctionFlags::empty();
    if func.is_global() {
        flags |= past::FunctionFlags::GLOBAL;
    }
    if func.is_native() {
        flags |= past::FunctionFlags::NATIVE;
    }
    flags
}

/// A decompiled callable, before it's slotted into a script-level or
/// state-level item.
enum Handler {
    Event(Event),
    Function(Function),
}

/// Decompile a function and classify it as a Papyrus `Event` or `Function`
/// (Champollion's rule: an `on…`-prefixed name in the built-in event set,
/// or an `::remote_` custom-event thunk, is an event).
fn build_handler(object: &Object, func: &PexFunction, name: &str) -> Result<Handler, DecompileError> {
    let body = decompile_body(object, func)?;
    let params = lower_params(func);
    let flags = function_flags(func);

    let is_event = (name.len() > 2 && name[..2].eq_ignore_ascii_case("on") && is_event_name(name))
        || name.starts_with("::remote_");
    let display = name.strip_prefix("::remote_").unwrap_or(name);

    Ok(if is_event {
        Handler::Event(Event {
            name: ident(display),
            params,
            flags,
            body,
            doc_comment: None,
        })
    } else {
        Handler::Function(Function {
            return_type: lower_return_type(func),
            name: ident(display),
            params,
            flags,
            body,
            doc_comment: None,
        })
    })
}

fn handler_to_script_item(h: Handler) -> ScriptItem {
    match h {
        Handler::Event(e) => ScriptItem::Event(e),
        Handler::Function(f) => ScriptItem::Function(f),
    }
}

fn handler_to_state_item(h: Handler) -> StateItem {
    match h {
        Handler::Event(e) => StateItem::Event(e),
        Handler::Function(f) => StateItem::Function(f),
    }
}

/// Build a named Papyrus `Function` (used for property getters/setters,
/// which carry no name of their own in the `.pex`).
fn build_named_function(object: &Object, func: &PexFunction, name: &str) -> Result<Function, DecompileError> {
    Ok(Function {
        return_type: lower_return_type(func),
        name: ident(name),
        params: lower_params(func),
        flags: function_flags(func),
        body: decompile_body(object, func)?,
        doc_comment: None,
    })
}

fn lower_property(object: &Object, prop: &crate::model::Property) -> Result<Property, DecompileError> {
    let mut flags = PropertyFlags::empty();
    let (getter, setter) = if prop.has_auto_var() {
        flags |= if prop.is_readable() && !prop.is_writable() {
            PropertyFlags::AUTO_READ_ONLY
        } else {
            PropertyFlags::AUTO
        };
        (None, None)
    } else {
        let getter = match &prop.read_function {
            Some(f) => Some(build_named_function(object, f, "Get")?),
            None => None,
        };
        let setter = match &prop.write_function {
            Some(f) => Some(build_named_function(object, f, "Set")?),
            None => None,
        };
        (getter, setter)
    };
    Ok(Property {
        ty: sp(lower_type(&prop.type_name)),
        name: ident(&prop.name),
        flags,
        initial_value: None,
        getter,
        setter,
        doc_comment: None,
    })
}

/// Decompile a whole `.pex` into a Papyrus [`Script`].
///
/// The single object becomes the script; its auto/default state's
/// functions become top-level items, named states become `State` items.
/// Synthetic (`::`-prefixed) variables — temps and property backing
/// stores — are dropped (they're not source-level declarations).
pub fn decompile_script(pex: &Pex) -> Result<Script, DecompileError> {
    let object = pex.main_object().ok_or(DecompileError::EmptyPex)?;
    let mut body: Vec<Spanned<ScriptItem>> = Vec::new();

    for v in &object.variables {
        if v.name.starts_with("::") {
            continue;
        }
        body.push(sp(ScriptItem::Variable(Variable {
            ty: sp(lower_type(&v.type_name)),
            name: ident(&v.name),
            initial_value: None,
            is_conditional: false,
            is_const: v.const_flag != 0,
        })));
    }

    for p in &object.properties {
        body.push(sp(ScriptItem::Property(Box::new(lower_property(object, p)?))));
    }

    for state in &object.states {
        if state.name == object.auto_state_name {
            // Auto/default state: its callables live at script scope.
            for f in &state.functions {
                let item = handler_to_script_item(build_handler(object, f, &f.name)?);
                body.push(sp(item));
            }
        } else {
            let mut items: Vec<Spanned<StateItem>> = Vec::new();
            for f in &state.functions {
                items.push(sp(handler_to_state_item(build_handler(object, f, &f.name)?)));
            }
            body.push(sp(ScriptItem::State(State {
                name: ident(&state.name),
                is_auto: false,
                body: items,
            })));
        }
    }

    let parent = if object.parent_class_name.is_empty()
        || object.parent_class_name.eq_ignore_ascii_case("none")
    {
        None
    } else {
        Some(ident(&object.parent_class_name))
    };

    let mut flags = ScriptFlags::empty();
    if object.const_flag != 0 {
        flags |= ScriptFlags::CONST;
    }

    Ok(Script {
        name: ident(&object.name),
        parent,
        flags,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Instruction, Object, State as PexState, TypedName};
    use crate::OpCode;

    fn ins(op: OpCode, args: Vec<Value>) -> Instruction {
        Instruction { op, args, var_args: Vec::new() }
    }
    fn ins_v(op: OpCode, args: Vec<Value>, var_args: Vec<Value>) -> Instruction {
        Instruction { op, args, var_args }
    }
    fn id(s: &str) -> Value {
        Value::Identifier(s.to_string())
    }

    /// A one-object `.pex` model wrapping a single default-state function.
    fn pex_with_function(func: PexFunction) -> Pex {
        Pex {
            script_type: crate::ScriptType::Skyrim,
            header: Default::default(),
            string_table: Vec::new(),
            debug_info: Default::default(),
            user_flags: Vec::new(),
            objects: vec![Object {
                name: "MyScript".into(),
                parent_class_name: "ObjectReference".into(),
                auto_state_name: String::new(),
                states: vec![PexState { name: String::new(), functions: vec![func] }],
                ..Object::default()
            }],
        }
    }

    #[test]
    fn an_on_activate_function_lowers_to_an_event() {
        // Event OnActivate(ObjectReference akActivator)
        //     Foo()
        // EndEvent
        let func = PexFunction {
            name: "OnActivate".into(),
            return_type_name: "None".into(),
            params: vec![TypedName {
                name: "akActivator".into(),
                type_name: "ObjectReference".into(),
            }],
            instructions: vec![
                ins_v(OpCode::CallMethod, vec![id("Foo"), id("self"), id("::NoneVar")], vec![]),
                ins(OpCode::Return, vec![id("::NoneVar")]),
            ],
            ..PexFunction::default()
        };
        let script = decompile_script(&pex_with_function(func)).unwrap();
        assert_eq!(script.name.node.0, "MyScript");
        assert_eq!(script.parent.as_ref().unwrap().node.0, "ObjectReference");

        let item = &script.body[0].node;
        let ScriptItem::Event(e) = item else {
            panic!("OnActivate should be an Event, got {item:?}");
        };
        assert_eq!(e.name.node.0, "OnActivate");
        assert_eq!(e.params.len(), 1);
        assert_eq!(e.params[0].name.node.0, "akActivator");
        // body: Foo() as an expression statement.
        assert!(matches!(e.body[0].node, Stmt::ExprStmt(_)));
    }

    #[test]
    fn a_plain_function_stays_a_function() {
        let func = PexFunction {
            name: "ComputeThing".into(),
            return_type_name: "Int".into(),
            instructions: vec![ins(OpCode::Return, vec![Value::Integer(7)])],
            ..PexFunction::default()
        };
        let script = decompile_script(&pex_with_function(func)).unwrap();
        let ScriptItem::Function(f) = &script.body[0].node else {
            panic!("ComputeThing should be a Function");
        };
        assert_eq!(f.name.node.0, "ComputeThing");
        assert_eq!(f.return_type.as_ref().unwrap().node, Type::Int);
        // body: Return 7
        assert!(matches!(
            &f.body[0].node,
            Stmt::Return(Some(v)) if matches!(v.node, Expr::IntLit(7))
        ));
    }

    #[test]
    fn an_if_with_a_call_lowers_to_an_if_statement() {
        // if (a == b)
        //     obj.DoThing()
        let func = PexFunction {
            name: "Check".into(),
            return_type_name: "None".into(),
            locals: vec![TypedName { name: "::temp0".into(), type_name: "Bool".into() }],
            instructions: vec![
                ins(OpCode::CmpEq, vec![id("::temp0"), id("a"), id("b")]),
                ins(OpCode::JmpF, vec![id("::temp0"), Value::Integer(2)]),
                ins_v(OpCode::CallMethod, vec![id("DoThing"), id("obj"), id("::NoneVar")], vec![]),
                ins(OpCode::Return, vec![id("::NoneVar")]),
            ],
            ..PexFunction::default()
        };
        let script = decompile_script(&pex_with_function(func)).unwrap();
        let ScriptItem::Function(f) = &script.body[0].node else { panic!() };
        let if_stmt = f.body.iter().find(|s| matches!(s.node, Stmt::If { .. })).unwrap();
        let Stmt::If { condition, body, .. } = &if_stmt.node else { panic!() };
        assert!(matches!(condition.node, Expr::BinaryOp { op: BinaryOp::Eq, .. }));
        // the call inside is `obj.DoThing()`
        let Stmt::ExprStmt(call) = &body[0].node else { panic!("expected call stmt") };
        let Expr::Call { callee, .. } = &call.node else { panic!("expected Call") };
        assert!(matches!(&callee.node, Expr::MemberAccess { member, .. } if member.node.0 == "DoThing"));
    }

    #[test]
    fn auto_property_lowers_with_auto_flag() {
        let pex = Pex {
            script_type: crate::ScriptType::Skyrim,
            header: Default::default(),
            string_table: Vec::new(),
            debug_info: Default::default(),
            user_flags: Vec::new(),
            objects: vec![Object {
                name: "S".into(),
                properties: vec![crate::model::Property {
                    name: "MyQuest".into(),
                    type_name: "Quest".into(),
                    flags: crate::model::property_flag::READ
                        | crate::model::property_flag::WRITE
                        | crate::model::property_flag::AUTOVAR,
                    auto_var_name: Some("::MyQuest_var".into()),
                    ..crate::model::Property::default()
                }],
                ..Object::default()
            }],
        };
        let script = decompile_script(&pex).unwrap();
        let ScriptItem::Property(p) = &script.body[0].node else { panic!() };
        assert_eq!(p.name.node.0, "MyQuest");
        assert_eq!(p.ty.node, Type::Object(past::Identifier::new("Quest")));
        assert!(p.flags.contains(PropertyFlags::AUTO));
    }
}
