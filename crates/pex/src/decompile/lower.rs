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
//!
//! ## Deliberate Champollion departures
//!
//! Departures 1 and 2 (the boolean pass's no-debug-line guard and
//! termination guard) live in `super::boolean`'s module doc. The third
//! (#4477, `a is T` lowered to `Expr::Cast`) is closed: opcode 36 now
//! lowers to [`Expr::Is`] (#5322 added it to the shared AST), the same
//! shape the `.psc` parser produces for `a is T`. The FO4 / Starfield
//! corpus carries 78 `is` instructions in 37 scripts (#5443).

use std::collections::HashMap;

use byroredux_papyrus::ast::{
    self as past, BinaryOp, CallArg, Event, Expr, Function, Param, Property, PropertyFlags, Script,
    ScriptFlags, ScriptItem, State, StateItem, Stmt, Type, UnaryOp, Variable,
};
use byroredux_papyrus::span::{Span, Spanned};

use crate::model::{Function as PexFunction, Object, Pex, Value, Variable as PexVariable};

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

/// The source initializer a variable's stored default stands for: a
/// numeric / Bool / String literal. `None` for an object default (always
/// `None` in the table) and for an identifier, which no initializer spells.
fn default_literal(value: &Value) -> Option<Spanned<Expr>> {
    Some(sp(match value {
        Value::Integer(n) => Expr::IntLit(*n as i64),
        Value::Float(f) => Expr::FloatLit(*f as f64),
        Value::Bool(b) => Expr::BoolLit(*b),
        Value::Str(s) => Expr::StringLit(s.clone()),
        Value::None | Value::Identifier(_) => return None,
    }))
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
            // Opcode 36: the type-test yields a Bool, unlike `a as T`.
            Expr::Is {
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
            op: if op == "!" {
                UnaryOp::Not
            } else {
                UnaryOp::Neg
            },
            operand: Box::new(lower_expr(operand)),
        },
        // `Copy` is a transparent value wrapper — unwrap it.
        NodeKind::Copy { value } => return lower_expr(value),
        NodeKind::Cast { value, target_type } => Expr::Cast {
            expr: Box::new(lower_expr(value)),
            target_type: sp(lower_type(target_type)),
        },
        NodeKind::CallMethod {
            object,
            method,
            params,
            ..
        } => Expr::Call {
            callee: Box::new(sp(Expr::MemberAccess {
                object: Box::new(lower_expr(object)),
                member: ident(method),
            })),
            args: params
                .iter()
                .map(|p| CallArg {
                    name: None,
                    value: lower_expr(p),
                })
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
        // Struct creation, `new T` (FO4+).
        NodeKind::StructCreate { struct_type } => Expr::NewStruct {
            ty: sp(lower_type(struct_type)),
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
        NodeKind::Return { value } => Stmt::Return(value.as_ref().map(|v| lower_expr(v))),
        NodeKind::IfElse {
            condition,
            body,
            else_body,
            ..
        } => Stmt::If {
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
fn decompile_body(
    object: &Object,
    func: &PexFunction,
) -> Result<Vec<Spanned<Stmt>>, DecompileError> {
    let mut cfg = build_cfg(func)?;
    let mut scopes = lift_function(object, func, &cfg)?;
    // Collapse `&&`/`||` short-circuits before control-flow reconstruction
    // so compound conditions surface as one expression, not nested ifs.
    rebuild_boolean_operators(&mut cfg, &mut scopes, &func.name)?;
    let nodes = reconstruct(cfg, scopes, &func.name)?;
    let mut body = lower_local_decls(func);
    body.extend(lower_body(&nodes));
    Ok(body)
}

/// One bare `VarDecl` per user-declared local of `func`, ahead of its body.
///
/// A `.pex` function carries its locals in a table, not as statements, so
/// the reconstructed body only ever *assigns* them — while the `.psc`
/// frontend the AST consumers were written against declares each local
/// where it is introduced (`Quest k = ...`). Without the declarations every
/// fragment prologue Champollion emits (`__temp = self as Quest`,
/// `akSpeaker = akSpeakerRef as Actor`) read as an assignment to an
/// undeclared name, i.e. a script variable or property, and the fragment
/// lowerer — correctly, #5638 — declined it: 10,021 of the 12,817 declined
/// Skyrim fragments stopped on exactly that statement.
///
/// Compiler temporaries (`::temp0`) are skipped: the lift folds them into
/// the expressions that consume them, and a name no source could spell is
/// not a declaration anyone wrote. Only those, though (#5880): the
/// compiler's `::mangled_<name>_<n>` renaming of a local re-declared in a
/// sibling scope is a real user local the body still assigns and reads
/// under that name, so it is declared like any other.
fn lower_local_decls(func: &PexFunction) -> Vec<Spanned<Stmt>> {
    func.locals
        .iter()
        .filter(|local| !is_temp_var(&local.name))
        .map(|local| {
            sp(Stmt::VarDecl(past::Variable {
                ty: sp(lower_type(&local.type_name)),
                name: ident(&local.name),
                initial_value: None,
                is_conditional: false,
                is_const: false,
                is_hidden: false,
            }))
        })
        .collect()
}

/// Champollion's `isTempVar` (PscDecompiler.cpp:20-24): `::temp*` not ending
/// in `_var` (those are property backing variables), or `::nonevar`.
/// Case-insensitive, as the rest of the format is.
fn is_temp_var(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    (lower.starts_with("::temp") && !lower.ends_with("_var")) || lower == "::nonevar"
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
fn build_handler(
    object: &Object,
    func: &PexFunction,
    name: &str,
) -> Result<Handler, DecompileError> {
    let body = decompile_body(object, func)?;
    let params = lower_params(func);
    let flags = function_flags(func);

    // `name.get(..2)` is boundary-safe: `.pex` names are decoded with
    // `from_utf8_lossy`, so an invalid Win-1252 byte becomes the 3-byte U+FFFD
    // replacement char and a naive `name[..2]` would panic mid-char. `get`
    // returns `None` when index 2 isn't a char boundary (or the name is short).
    // `is_event_name` lowercases the whole name and binary-searches the engine
    // event set, so it already rejects anything ≤ 2 chars — the old length
    // guard was redundant.
    let is_event = (name.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("on"))
        && is_event_name(name))
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
fn build_named_function(
    object: &Object,
    func: &PexFunction,
    name: &str,
) -> Result<Function, DecompileError> {
    Ok(Function {
        return_type: lower_return_type(func),
        name: ident(name),
        params: lower_params(func),
        flags: function_flags(func),
        body: decompile_body(object, func)?,
        doc_comment: None,
    })
}

fn lower_property(
    masks: &UserFlagMasks,
    object: &Object,
    backing_vars: &HashMap<String, &PexVariable>,
    prop: &crate::model::Property,
) -> Result<Property, DecompileError> {
    // The auto property's backing variable (`::P_var`), looked up in the
    // per-object map built once by the caller: scanning `object.variables`
    // per property would be O(properties × variables) on a hostile file.
    let backing = prop
        .auto_var_name
        .as_deref()
        .and_then(|var| backing_vars.get(&var.to_ascii_lowercase()))
        .copied();
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
    // #5685 — the property half of #5398: the same user-flag table the
    // object and variables already consult names a property's `Const` /
    // `Mandatory` / `Hidden` / `Conditional` bits, and the `.psc`
    // frontend parses all four — lowering without them made the two
    // frontends disagree on any flagged property (`Const Auto Hidden`
    // lowered as plain `Auto`).
    //
    // #5882 — but vanilla user-flag tables never name `const`, and the
    // `Auto Const` / `Auto Conditional` bits sit on the auto property's
    // BACKING VARIABLE, not on the property (Champollion `writeProperty`:
    // "The flags defined on the variable must be set on the property"). So
    // the user-flag bits are the property's OR the backing variable's, and
    // the backing variable's `const_flag` is `CONST`.
    let user_flags = prop.user_flags | backing.map_or(0, |v| v.user_flags);
    for (mask, flag) in [
        (masks.constant, PropertyFlags::CONST),
        (masks.mandatory, PropertyFlags::MANDATORY),
        (masks.hidden, PropertyFlags::HIDDEN),
        (masks.conditional, PropertyFlags::CONDITIONAL),
    ] {
        if user_flags & mask != 0 {
            flags |= flag;
        }
    }
    if backing.is_some_and(|v| v.const_flag != 0) {
        flags |= PropertyFlags::CONST;
    }
    Ok(Property {
        ty: sp(lower_type(&prop.type_name)),
        name: ident(&prop.name),
        flags,
        // M47.2 — an auto property's initializer is its backing variable's
        // stored default (`Int Property prereqStageOPT = -1 Auto` keeps the
        // `-1` on `::prereqStageOPT_var`). Dropping it made every compiled
        // script's property defaults read as absent.
        initial_value: backing.and_then(|v| default_literal(&v.default_value)),
        getter,
        setter,
        doc_comment: None,
    })
}

/// Decompile a whole `.pex` into a Papyrus [`Script`].
///
/// The single object becomes the script; its empty-named default state's
/// functions become top-level items, and every named state becomes a `State`
/// item — the one matching `auto_state_name` marked `is_auto` (#4319).
/// Synthetic (`::`-prefixed) variables — temps and property backing
/// stores — are dropped (they're not source-level declarations).
/// Is `state` the object's auto state — the one it boots into?
///
/// Not the script-scope state: that is always the one named `""` (#4319).
///
/// The single source of truth for that question. `88e7dbfc` (#3786) made the
/// comparison case-INSENSITIVE here — Papyrus identifiers are case-insensitive
/// and the compiler's own `.pex` output does not guarantee that a state's
/// `name` and the object's `auto_state_name` agree in case — but the
/// `pex_corpus_smoke` harness kept a hand-copied `==`. A mismatched-casing auto
/// state therefore made the #3017 shape check disagree with `decompile_script`
/// on the very input #3786 fixed, reporting a spurious
/// `decompiled_shape_mismatch` and sending triage at the decompiler (#3943).
///
/// Exported so the harness calls this rather than re-spelling the rule; the
/// predicate has now drifted once, so a second copy is the thing to prevent,
/// not the specific comparison operator.
pub fn is_auto_state(object: &crate::model::Object, state: &crate::model::State) -> bool {
    state.name.eq_ignore_ascii_case(&object.auto_state_name)
}

/// #5398 — the bits the file's user-flag table assigns to the flag names
/// the `.psc` frontend also parses (case-insensitive; Champollion
/// `Pex::UserFlag`). A name may be listed more than once, so a mask is the
/// OR of every matching entry's bit; entries with `flag_index >= 32` carry
/// no bit in a `u32` and are ignored.
///
/// #5683 — resolved ONCE per `Pex` in a single pass over the table. The
/// first form rescanned the whole table per variable, property and flag
/// name: O(F × V) on a hostile `.pex` whose `u16` counts are both 65,535
/// (an 852 KB file stalled `decompile_script` for ~18 s, well inside the
/// parse budget). Now O(F + V).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct UserFlagMasks {
    conditional: u32,
    hidden: u32,
    default: u32,
    constant: u32,
    mandatory: u32,
}

impl UserFlagMasks {
    fn from_pex(pex: &Pex) -> Self {
        let mut masks = Self::default();
        for flag in &pex.user_flags {
            if flag.flag_index >= 32 {
                continue;
            }
            let bit = 1u32 << flag.flag_index;
            let name = flag.name.as_str();
            if name.eq_ignore_ascii_case("conditional") {
                masks.conditional |= bit;
            } else if name.eq_ignore_ascii_case("hidden") {
                masks.hidden |= bit;
            } else if name.eq_ignore_ascii_case("default") {
                masks.default |= bit;
            } else if name.eq_ignore_ascii_case("const") {
                masks.constant |= bit;
            } else if name.eq_ignore_ascii_case("mandatory") {
                masks.mandatory |= bit;
            }
        }
        masks
    }
}

pub fn decompile_script(pex: &Pex) -> Result<Script, DecompileError> {
    let object = pex.main_object().ok_or(DecompileError::EmptyPex)?;
    let masks = UserFlagMasks::from_pex(pex);
    let mut body: Vec<Spanned<ScriptItem>> = Vec::new();

    // FO4+ — the object's struct definitions, each member with the default
    // its table stores.
    for info in &object.struct_infos {
        body.push(sp(ScriptItem::Struct(byroredux_papyrus::ast::Struct {
            name: ident(&info.name),
            members: info
                .members
                .iter()
                .map(|member| Variable {
                    ty: sp(lower_type(&member.type_name)),
                    name: ident(&member.name),
                    initial_value: default_literal(&member.value),
                    // #5882 — a struct member's user flags go through the
                    // same table as a variable's.
                    is_conditional: member.user_flags & masks.conditional != 0,
                    is_const: member.const_flag != 0,
                    is_hidden: member.user_flags & masks.hidden != 0,
                })
                .collect(),
        })));
    }

    for v in &object.variables {
        if v.name.starts_with("::") {
            continue;
        }
        body.push(sp(ScriptItem::Variable(Variable {
            ty: sp(lower_type(&v.type_name)),
            name: ident(&v.name),
            // M47.2 — the table's stored default is the source initializer.
            initial_value: default_literal(&v.default_value),
            // #5398 — `Int x Conditional` round-trips through the same
            // user-flag table as the object's own flags.
            is_conditional: v.user_flags & masks.conditional != 0,
            is_const: v.const_flag != 0,
            is_hidden: false,
        })));
    }

    // First match wins, as the per-property `find` this replaces did.
    let mut backing_vars: HashMap<String, &PexVariable> = HashMap::new();
    for v in &object.variables {
        backing_vars.entry(v.name.to_ascii_lowercase()).or_insert(v);
    }
    for p in &object.properties {
        body.push(sp(ScriptItem::Property(Box::new(lower_property(
            &masks,
            object,
            &backing_vars,
            p,
        )?))));
    }

    // #4473 — at most ONE state is the auto state: the FIRST
    // case-insensitive match of `auto_state_name`. `is_auto_state` matches
    // case-insensitively (#3786, correctly), so a hostile/hand-assembled
    // `.pex` carrying `waiting` + `WAITING` with `auto_state_name =
    // "Waiting"` previously marked BOTH `is_auto: true` — an AST no
    // Papyrus source can express (the compiler rejects duplicate
    // case-insensitive state names). Champollion's case-sensitive compare
    // marks at most one; the first match is the rule here.
    let auto_state_index: Option<usize> = if object.auto_state_name.is_empty() {
        None
    } else {
        object.states.iter().position(|s| is_auto_state(object, s))
    };

    for (state_index, state) in object.states.iter().enumerate() {
        // #4319 — the script-scope state is the one named `""`, always.
        // `auto_state_name` names the state the object *boots into*, which is
        // `""` only when the source declares no `Auto State`. Keying script
        // scope on that match (as this did before) was right for that case
        // alone: `Auto State Waiting` had `Waiting`'s callables hoisted to
        // script scope and the real `""` default demoted to a non-auto
        // `State ''`, with nothing ever marked `Auto` — 983 vanilla scripts.
        // Champollion's `PscCoder::writeStates` and the `.psc` frontend both
        // key script scope on the empty name.
        if state.name.is_empty() {
            for f in &state.functions {
                let item = handler_to_script_item(build_handler(object, f, &f.name)?);
                body.push(sp(item));
            }
        } else {
            let mut items: Vec<Spanned<StateItem>> = Vec::new();
            for f in &state.functions {
                items.push(sp(handler_to_state_item(build_handler(
                    object, f, &f.name,
                )?)));
            }
            body.push(sp(ScriptItem::State(State {
                name: ident(&state.name),
                // #3786 — case-insensitive, through the shared predicate.
                // #4473 — first-match-only under a case collision.
                is_auto: auto_state_index == Some(state_index),
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
    // #5398 — the object's user flags are bit indices named by the file's
    // own user-flag table; map the ones the `.psc` frontend also parses,
    // so both frontends produce the same `ScriptFlags`.
    for (mask, flag) in [
        (masks.conditional, ScriptFlags::CONDITIONAL),
        (masks.hidden, ScriptFlags::HIDDEN),
        (masks.default, ScriptFlags::DEFAULT),
    ] {
        if object.user_flags & mask != 0 {
            flags |= flag;
        }
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
        Instruction {
            op,
            args,
            var_args: Vec::new(),
        }
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
                states: vec![PexState {
                    name: String::new(),
                    functions: vec![func],
                }],
                ..Object::default()
            }],
        }
    }

    /// #5398 — the object's `conditional` / `hidden` / `default` user flags
    /// (bit indices named by the file's user-flag table) lower into the
    /// same `ScriptFlags` the `.psc` frontend parses; a variable's
    /// `conditional` lowers to `is_conditional`.
    #[test]
    fn object_user_flags_lower_into_script_flags() {
        use crate::model::{UserFlag, Variable as PexVariable};
        let mut pex = pex_with_function(PexFunction {
            name: "F".into(),
            return_type_name: "None".into(),
            ..PexFunction::default()
        });
        pex.user_flags = vec![
            UserFlag { name: "hidden".into(), flag_index: 0 },
            UserFlag { name: "conditional".into(), flag_index: 1 },
            UserFlag { name: "default".into(), flag_index: 2 },
        ];
        pex.objects[0].user_flags = 0b010;
        pex.objects[0].variables = vec![PexVariable {
            name: "x".into(),
            type_name: "Int".into(),
            user_flags: 0b010,
            ..PexVariable::default()
        }];
        let script = decompile_script(&pex).expect("decompiles");
        assert_eq!(script.flags, ScriptFlags::CONDITIONAL);
        let conditional_var = script.body.iter().any(|item| {
            matches!(&item.node, ScriptItem::Variable(v) if v.is_conditional)
        });
        assert!(conditional_var, "the variable's conditional user flag lowers");
    }

    /// #5685 / #5882 — a property's `Mandatory` / `Hidden` bits (property
    /// level) and `Const` / `Conditional` (the auto property's BACKING
    /// VARIABLE: `const_flag` and the conditional user bit) lower into the
    /// same `PropertyFlags` the `.psc` frontend parses, alongside the
    /// auto-var-derived `Auto` bit. The user-flag table is the vanilla FO4
    /// layout — no entry is named `const`.
    #[test]
    fn property_user_flags_lower_into_property_flags() {
        use crate::model::{Property as PexProperty, UserFlag, Variable as PexVariable};
        let mut pex = pex_with_function(PexFunction {
            name: "F".into(),
            return_type_name: "None".into(),
            ..PexFunction::default()
        });
        pex.user_flags = [
            "hidden",
            "conditional",
            "default",
            "collapsedonref",
            "collapsedonbase",
            "mandatory",
        ]
        .iter()
        .enumerate()
        .map(|(i, n)| UserFlag { name: (*n).into(), flag_index: i as u8 })
        .collect();
        let flagged = |name: &str, user_flags: u32| PexProperty {
            name: name.into(),
            type_name: "Int".into(),
            user_flags,
            auto_var_name: Some(format!("::{name}_var")),
            flags: crate::model::property_flag::AUTOVAR
                | crate::model::property_flag::READ
                | crate::model::property_flag::WRITE,
            ..PexProperty::default()
        };
        let var = |name: &str, user_flags: u32, const_flag: u8| PexVariable {
            name: format!("::{name}_var"),
            type_name: "Int".into(),
            user_flags,
            const_flag,
            ..PexVariable::default()
        };
        pex.objects[0].variables = vec![
            var("Plain", 0, 0),
            var("AutoConst", 0, 1),
            var("AutoCond", 0b10, 0),
            var("Both", 0b10, 1),
        ];
        pex.objects[0].properties = vec![
            flagged("Plain", 0b100001), // hidden | mandatory, on the property
            flagged("AutoConst", 0),
            flagged("AutoCond", 0),
            flagged("Both", 0),
        ];
        let script = decompile_script(&pex).expect("decompiles");
        let flags_of = |name: &str| {
            script
                .body
                .iter()
                .find_map(|item| match &item.node {
                    ScriptItem::Property(p) if p.name.node.0 == name => Some(p.flags),
                    _ => None,
                })
                .expect("the property lowers")
        };
        assert_eq!(
            flags_of("Plain"),
            PropertyFlags::AUTO | PropertyFlags::HIDDEN | PropertyFlags::MANDATORY,
        );
        assert_eq!(flags_of("AutoConst"), PropertyFlags::AUTO | PropertyFlags::CONST);
        assert_eq!(flags_of("AutoCond"), PropertyFlags::AUTO | PropertyFlags::CONDITIONAL);
        assert_eq!(
            flags_of("Both"),
            PropertyFlags::AUTO | PropertyFlags::CONST | PropertyFlags::CONDITIONAL
        );
    }

    /// #5882 — struct members go through the same user-flag table.
    #[test]
    fn struct_member_user_flags_lower_into_variable_flags() {
        use crate::model::{StructInfo, StructMember, UserFlag};
        let mut pex = pex_with_function(PexFunction::default());
        pex.user_flags = vec![
            UserFlag { name: "hidden".into(), flag_index: 0 },
            UserFlag { name: "conditional".into(), flag_index: 1 },
        ];
        pex.objects[0].struct_infos = vec![StructInfo {
            name: "S".into(),
            members: vec![StructMember {
                name: "m".into(),
                type_name: "Int".into(),
                user_flags: 0b11,
                ..StructMember::default()
            }],
            ..StructInfo::default()
        }];
        let script = decompile_script(&pex).expect("decompiles");
        let member = script
            .body
            .iter()
            .find_map(|item| match &item.node {
                ScriptItem::Struct(st) => st.members.first(),
                _ => None,
            })
            .expect("the struct lowers");
        assert!(member.is_hidden && member.is_conditional);
    }

    /// #5683 — the user-flag masks are one pass over the table: entries
    /// match case-insensitively, a repeated name ORs its bits, and an entry
    /// with `flag_index >= 32` (a hostile table — it names a bit a `u32`
    /// cannot hold) contributes nothing and must not shift-overflow.
    #[test]
    fn user_flag_masks_or_repeated_names_and_ignore_wide_indices() {
        use crate::model::UserFlag;
        let mut pex = pex_with_function(PexFunction::default());
        pex.user_flags = vec![
            UserFlag { name: "Conditional".into(), flag_index: 1 },
            UserFlag { name: "conditional".into(), flag_index: 5 },
            UserFlag { name: "HIDDEN".into(), flag_index: 31 },
            UserFlag { name: "hidden".into(), flag_index: 32 },
            UserFlag { name: "default".into(), flag_index: 255 },
            UserFlag { name: "conditionaX".into(), flag_index: 2 },
        ];
        let masks = UserFlagMasks::from_pex(&pex);
        assert_eq!(masks.conditional, (1 << 1) | (1 << 5));
        assert_eq!(masks.hidden, 1 << 31);
        assert_eq!(masks.default, 0, "flag_index >= 32 carries no bit");
        assert_eq!(masks.constant | masks.mandatory, 0);
    }

    /// #5683 — a hostile table (every flag name a same-length near-miss, 65,535
    /// flags × 65,535 variables, the 852 KB `.pex` of the report) decompiles in
    /// linear time. The pre-fix O(F × V) form took ~18 s here; a quadratic
    /// regression blows far past the generous bound.
    #[test]
    fn a_hostile_user_flag_table_decompiles_in_linear_time() {
        use crate::model::{UserFlag, Variable as PexVariable};
        let mut pex = pex_with_function(PexFunction {
            name: "F".into(),
            return_type_name: "None".into(),
            ..PexFunction::default()
        });
        pex.user_flags = (0..65_535u32)
            .map(|i| UserFlag { name: "conditionaX".into(), flag_index: (i % 256) as u8 })
            .collect();
        pex.objects[0].variables = (0..65_535u32)
            .map(|i| PexVariable {
                name: format!("v{i}"),
                type_name: "Int".into(),
                user_flags: u32::MAX,
                ..PexVariable::default()
            })
            .collect();
        // #5878 — and 65,535 auto properties whose backing name never matches
        // a same-length variable: the per-property `find` it replaced took 39 s.
        pex.objects[0].variables.extend((0..65_535u32).map(|i| PexVariable {
            name: format!("::aaaaaaaaaaX{i}"),
            type_name: "Int".into(),
            ..PexVariable::default()
        }));
        pex.objects[0].properties = (0..65_535u32)
            .map(|i| crate::model::Property {
                name: format!("P{i}"),
                type_name: "Int".into(),
                auto_var_name: Some(format!("::aaaaaaaaaaY{i}")),
                flags: crate::model::property_flag::AUTOVAR
                    | crate::model::property_flag::READ
                    | crate::model::property_flag::WRITE,
                ..crate::model::Property::default()
            })
            .collect();
        let started = std::time::Instant::now();
        let script = decompile_script(&pex).expect("decompiles");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "decompile of a 65,535 × 65,535 user-flag / property table took {:?}",
            started.elapsed()
        );
        assert!(
            script.body.iter().all(|item| !matches!(
                &item.node, ScriptItem::Variable(v) if v.is_conditional)),
            "no table entry is named `conditional`"
        );
    }

    /// #4477 / #5443 — the opcode-36 (`Is`) lowering pin: `a is T` arrives
    /// as `Expr::Is { target_type: T }`, the same shape the `.psc` parser
    /// produces, not as a conversion `Expr::Cast`.
    #[test]
    fn an_is_opcode_lowers_to_an_is_expression() {
        // ; a is Actor
        // Is a Actor  (opcode 36, Bool result → ::NoneVar)
        let func = PexFunction {
            name: "Probe".into(),
            return_type_name: "None".into(),
            params: vec![],
            instructions: vec![
                ins_v(
                    OpCode::Is,
                    vec![id("::NoneVar"), id("a"), Value::Identifier("Actor".to_string())],
                    vec![],
                ),
                ins(OpCode::Return, vec![Value::Identifier("None".to_string())]),
            ],
            ..PexFunction::default()
        };
        let script = crate::decompile::decompile_script(&pex_with_function(func))
            .expect("decompile");

        let body = script
            .body
            .first()
            .and_then(|item| match &item.node {
                ScriptItem::Function(f) => Some(f),
                _ => None,
            })
            .map(|f| &f.body)
            .expect("one function");
        // Walk the statements looking for the Is expression.
        let mut is: Option<&past::Type> = None;
        for stmt in body {
            if let Stmt::ExprStmt(expr) = &stmt.node {
                match &expr.node {
                    Expr::Is { target_type, .. } => is = Some(&target_type.node),
                    Expr::Cast { .. } => panic!("the `is` must not lower to a Cast"),
                    _ => {}
                }
            }
        }
        let is = is.expect("the `is` must lower to an Is");
        assert_eq!(
            is,
            &past::Type::Object(past::Identifier::new("Actor".to_string())),
            "the type-test lowers to an object-typed `Expr::Is` (#5443)"
        );
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
                ins_v(
                    OpCode::CallMethod,
                    vec![id("Foo"), id("self"), id("::NoneVar")],
                    vec![],
                ),
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

    /// #3943 — `is_auto_state` is the one spelling of the auto-state rule.
    ///
    /// #3786 fixed the comparison here; the `pex_corpus_smoke` harness kept a
    /// hand-copied `==` and so disagreed with `decompile_script` on exactly
    /// the input #3786 fixed, reporting a spurious `decompiled_shape_mismatch`.
    /// Both callers now go through this function. The predicate has already
    /// drifted once, so the property worth pinning is case-insensitivity
    /// itself, not any particular call site.
    #[test]
    fn is_auto_state_ignores_identifier_casing() {
        use crate::model::{Object, State};
        let object = Object {
            auto_state_name: "Waiting".into(),
            ..Default::default()
        };
        for spelling in ["Waiting", "waiting", "WAITING", "WaItInG"] {
            let state = State {
                name: spelling.into(),
                ..Default::default()
            };
            assert!(
                is_auto_state(&object, &state),
                "`{spelling}` must match auto_state_name `Waiting` — Papyrus \
                 identifiers are case-insensitive (#3943)"
            );
        }
        let other = State {
            name: "Running".into(),
            ..Default::default()
        };
        assert!(
            !is_auto_state(&object, &other),
            "a genuinely different state name must not match (#3943)"
        );
    }

    #[test]
    fn named_auto_state_stays_an_auto_state_and_the_empty_state_is_script_scope() {
        // #4319 — the layout every vanilla `Auto State` script has: a `""`
        // default state plus the named auto state. Script scope is the `""`
        // state; `auto_state_name` only marks which named state the object
        // boots into. The pre-fix rule hoisted `waiting`'s handler to script
        // scope and demoted `""` to a non-auto `State ''`. The auto state's
        // casing deliberately differs from `auto_state_name`, keeping the
        // #3786 case-insensitive match pinned.
        let handler = |name: &str| PexFunction {
            name: name.into(),
            return_type_name: "None".into(),
            instructions: vec![ins(OpCode::Return, vec![id("::NoneVar")])],
            ..PexFunction::default()
        };
        let pex = Pex {
            script_type: crate::ScriptType::Skyrim,
            header: Default::default(),
            string_table: Vec::new(),
            debug_info: Default::default(),
            user_flags: Vec::new(),
            objects: vec![Object {
                name: "MyScript".into(),
                parent_class_name: "ObjectReference".into(),
                auto_state_name: "Waiting".into(),
                states: vec![
                    PexState {
                        name: String::new(),
                        functions: vec![handler("OnInit")],
                    },
                    PexState {
                        name: "waiting".into(),
                        functions: vec![handler("OnActivate")],
                    },
                ],
                ..Object::default()
            }],
        };
        let script = decompile_script(&pex).unwrap();
        assert_eq!(script.body.len(), 2, "{:?}", script.body);
        let ScriptItem::Event(e) = &script.body[0].node else {
            panic!(
                "the empty-named state's OnInit belongs at script scope, got {:?}",
                script.body[0].node
            );
        };
        assert_eq!(e.name.node.0, "OnInit");
        let ScriptItem::State(state) = &script.body[1].node else {
            panic!(
                "the named auto state must stay a State item, got {:?}",
                script.body[1].node
            );
        };
        assert_eq!(state.name.node.0, "waiting");
        assert!(
            state.is_auto,
            "the state matching auto_state_name is the Auto State"
        );
        assert_eq!(state.body.len(), 1);
        assert!(
            matches!(&state.body[0].node, StateItem::Event(ev) if ev.name.node.0 == "OnActivate")
        );
    }

    /// #4473 — a hand-assembled `.pex` whose states collide
    /// case-insensitively (`waiting` + `WAITING`, `auto_state_name =
    /// "Waiting"`): the FIRST match is the auto state, the duplicate stays
    /// a plain `State`. No Papyrus source can express two auto states (the
    /// compiler rejects duplicate case-insensitive state names), so
    /// marking both produced an AST outside the language.
    #[test]
    fn case_colliding_state_names_mark_exactly_one_auto() {
        let handler = |name: &str| PexFunction {
            name: name.into(),
            return_type_name: "None".into(),
            instructions: vec![ins(OpCode::Return, vec![id("::NoneVar")])],
            ..PexFunction::default()
        };
        let pex = Pex {
            script_type: crate::ScriptType::Skyrim,
            header: Default::default(),
            string_table: Vec::new(),
            debug_info: Default::default(),
            user_flags: Vec::new(),
            objects: vec![Object {
                name: "MyScript".into(),
                parent_class_name: "ObjectReference".into(),
                auto_state_name: "Waiting".into(),
                states: vec![
                    PexState {
                        name: "waiting".into(),
                        functions: vec![handler("OnActivate")],
                    },
                    PexState {
                        name: "WAITING".into(),
                        functions: vec![handler("OnUpdate")],
                    },
                ],
                ..Object::default()
            }],
        };
        let script = decompile_script(&pex).unwrap();
        let autos: Vec<&str> = script
            .body
            .iter()
            .filter_map(|item| match &item.node {
                ScriptItem::State(state) if state.is_auto => Some(&*state.name.node.0),
                _ => None,
            })
            .collect();
        assert_eq!(
            autos,
            ["waiting"],
            "exactly the first case-insensitive match is the auto state (#4473)"
        );
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

    /// D3-NEW-01: a `.pex` function name whose first source byte was an
    /// invalid Win-1252 byte decodes (via `from_utf8_lossy`) to a leading
    /// 3-byte U+FFFD replacement char, so byte index 2 lands mid-char. The
    /// old `name[..2]` byte-slice panicked there; the boundary-safe check
    /// must classify the name cleanly (here a plain Function — U+FFFD… is
    /// no engine event) and return `Ok`, never panic.
    #[test]
    fn non_ascii_function_name_classifies_without_panicking() {
        let bad_name = format!("{}foo", String::from_utf8_lossy(&[0x81]));
        assert!(
            !bad_name.is_char_boundary(2),
            "precondition: byte 2 is inside the leading U+FFFD char",
        );
        let func = PexFunction {
            name: bad_name.clone(),
            return_type_name: "None".into(),
            instructions: vec![ins(OpCode::Return, vec![id("::NoneVar")])],
            ..PexFunction::default()
        };
        let script = decompile_script(&pex_with_function(func)).expect("decompiles without panic");
        let ScriptItem::Function(f) = &script.body[0].node else {
            panic!("a non-event name must lower to a Function, not an Event");
        };
        assert_eq!(f.name.node.0, bad_name);
    }

    /// The locals table surfaces as bare declarations ahead of the body
    /// (compiler temporaries excluded), so the Champollion fragment
    /// prologue `__temp = self as Quest` is an assignment to a *declared*
    /// local — the contract the fragment lowerer's #5638 guard relies on.
    #[test]
    fn user_locals_are_declared_ahead_of_the_body_and_temporaries_are_not() {
        let func = PexFunction {
            name: "Fragment_0".into(),
            return_type_name: "None".into(),
            locals: vec![
                TypedName {
                    name: "::temp0".into(),
                    type_name: "Bool".into(),
                },
                TypedName {
                    name: "__temp".into(),
                    type_name: "Quest".into(),
                },
                TypedName {
                    name: "count".into(),
                    type_name: "Int".into(),
                },
                // #5880 — the compiler's rename of a sibling-scope
                // re-declaration is a user local and IS declared; the
                // `::nonevar` and `::TEMP1` spellings are still temporaries.
                TypedName {
                    name: "::mangled_i_0".into(),
                    type_name: "Int".into(),
                },
                TypedName {
                    name: "::nonevar".into(),
                    type_name: "None".into(),
                },
                TypedName {
                    name: "::TEMP1".into(),
                    type_name: "Int".into(),
                },
            ],
            instructions: vec![
                ins(OpCode::Cast, vec![id("__temp"), id("self")]),
                ins(OpCode::Return, vec![id("::NoneVar")]),
            ],
            ..PexFunction::default()
        };
        let script = decompile_script(&pex_with_function(func)).unwrap();
        let ScriptItem::Function(f) = &script.body[0].node else {
            panic!()
        };
        let declared: Vec<(&str, bool)> = f
            .body
            .iter()
            .filter_map(|stmt| match &stmt.node {
                Stmt::VarDecl(var) => Some((
                    var.name.node.0.as_str(),
                    var.initial_value.is_none(),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(
            declared,
            vec![("__temp", true), ("count", true), ("::mangled_i_0", true)]
        );
        assert!(
            f.body[..3].iter().all(|s| matches!(s.node, Stmt::VarDecl(_))),
            "declarations lead the body"
        );
        assert!(
            f.body.iter().any(|s| matches!(&s.node,
                Stmt::Assign { target, .. } if matches!(&target.node, Expr::Ident(id) if id.0 == "__temp"))),
            "the cast still reads as an assignment to the declared local"
        );
    }

    #[test]
    fn an_if_with_a_call_lowers_to_an_if_statement() {
        // if (a == b)
        //     obj.DoThing()
        let func = PexFunction {
            name: "Check".into(),
            return_type_name: "None".into(),
            locals: vec![TypedName {
                name: "::temp0".into(),
                type_name: "Bool".into(),
            }],
            instructions: vec![
                ins(OpCode::CmpEq, vec![id("::temp0"), id("a"), id("b")]),
                ins(OpCode::JmpF, vec![id("::temp0"), Value::Integer(2)]),
                ins_v(
                    OpCode::CallMethod,
                    vec![id("DoThing"), id("obj"), id("::NoneVar")],
                    vec![],
                ),
                ins(OpCode::Return, vec![id("::NoneVar")]),
            ],
            ..PexFunction::default()
        };
        let script = decompile_script(&pex_with_function(func)).unwrap();
        let ScriptItem::Function(f) = &script.body[0].node else {
            panic!()
        };
        let if_stmt = f
            .body
            .iter()
            .find(|s| matches!(s.node, Stmt::If { .. }))
            .unwrap();
        let Stmt::If {
            condition, body, ..
        } = &if_stmt.node
        else {
            panic!()
        };
        assert!(matches!(
            condition.node,
            Expr::BinaryOp {
                op: BinaryOp::Eq,
                ..
            }
        ));
        // the call inside is `obj.DoThing()`
        let Stmt::ExprStmt(call) = &body[0].node else {
            panic!("expected call stmt")
        };
        let Expr::Call { callee, .. } = &call.node else {
            panic!("expected Call")
        };
        assert!(
            matches!(&callee.node, Expr::MemberAccess { member, .. } if member.node.0 == "DoThing")
        );
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
        let ScriptItem::Property(p) = &script.body[0].node else {
            panic!()
        };
        assert_eq!(p.name.node.0, "MyQuest");
        assert_eq!(p.ty.node, Type::Object(past::Identifier::new("Quest")));
        assert!(p.flags.contains(PropertyFlags::AUTO));
    }

    /// M47.2 — an auto property's initializer is its backing variable's
    /// stored default, and a plain variable keeps its own; an object default
    /// (`None`) stays without an initializer.
    #[test]
    fn variable_defaults_become_source_initializers() {
        let auto_property = |name: &str, ty: &str| crate::model::Property {
            name: name.into(),
            type_name: ty.into(),
            flags: crate::model::property_flag::READ
                | crate::model::property_flag::WRITE
                | crate::model::property_flag::AUTOVAR,
            auto_var_name: Some(format!("::{name}_var")),
            ..crate::model::Property::default()
        };
        let variable = |name: &str, ty: &str, default_value| crate::model::Variable {
            name: name.into(),
            type_name: ty.into(),
            default_value,
            ..Default::default()
        };
        let pex = Pex {
            script_type: crate::ScriptType::Skyrim,
            header: Default::default(),
            string_table: Vec::new(),
            debug_info: Default::default(),
            user_flags: Vec::new(),
            objects: vec![Object {
                name: "defaultSetStageTrigScript".into(),
                variables: vec![
                    variable("::prereqStageOPT_var", "Int", Value::Integer(-1)),
                    variable("::myQuest_var", "Quest", Value::None),
                    variable("count", "Float", Value::Float(2.5)),
                ],
                properties: vec![
                    auto_property("prereqStageOPT", "Int"),
                    auto_property("myQuest", "Quest"),
                ],
                ..Object::default()
            }],
        };
        let script = decompile_script(&pex).unwrap();
        let initial = |wanted: &str| {
            script.body.iter().find_map(|item| match &item.node {
                ScriptItem::Property(p) if p.name.node.0 == wanted => {
                    Some(p.initial_value.as_ref().map(|v| format!("{:?}", v.node)))
                }
                ScriptItem::Variable(v) if v.name.node.0 == wanted => {
                    Some(v.initial_value.as_ref().map(|v| format!("{:?}", v.node)))
                }
                _ => None,
            })
        };
        assert_eq!(initial("prereqStageOPT"), Some(Some("IntLit(-1)".into())));
        assert_eq!(initial("myQuest"), Some(None));
        assert_eq!(initial("count"), Some(Some("FloatLit(2.5)".into())));
    }

    /// #3933 — the two shapes #3783's per-call depth ledger could not see.
    ///
    /// Both are well-formed `.pex` inside every wire-format ceiling, and
    /// both used to decompile to `Ok` with a tree tens of thousands of
    /// levels deep (measured 40 001), which `Node`'s derived `Clone`, its
    /// drop glue and `lower_expr` then walk one frame per level — a stack
    /// overflow, i.e. a `SIGABRT` that `translate_pex`'s `catch_unwind`
    /// cannot intercept. The ledger reset to `vec![1; len]` on every
    /// `rebuild_expression` call, so re-folding an already-folded scope
    /// restarted the count from 1; the depth now lives on the node.
    ///
    /// Sized to fire the cap with a wide margin while staying far below the
    /// depth that would abort the test process if the cap were missing —
    /// the point is that the cap fires first, in *release and debug alike*.
    #[test]
    fn a_temp_chain_split_across_blocks_is_declined_not_aborted() {
        // `::temp{k} = ::temp{k-1} + a` with an unconditional `jmp +1`
        // every `SPLIT` producers. The jump lifts to no statement, so the
        // chain is unbroken — but each block boundary ends one
        // `rebuild_expression` call and starts another, which is exactly
        // what reset the old ledger. `control_flow`'s whole-body re-fold
        // then splices the per-block scopes back together.
        const SPLIT: usize = 250;
        const PRODUCERS: usize = 4_000;

        let mut instructions = Vec::new();
        for k in 0..PRODUCERS {
            let left = if k == 0 {
                id("a")
            } else {
                id(&format!("::temp{}", k - 1))
            };
            instructions.push(ins(
                OpCode::IAdd,
                vec![id(&format!("::temp{k}")), left, id("a")],
            ));
            if k % SPLIT == SPLIT - 1 {
                instructions.push(ins(OpCode::Jmp, vec![Value::Integer(1)]));
            }
        }
        instructions.push(ins(
            OpCode::Assign,
            vec![id("x"), id(&format!("::temp{}", PRODUCERS - 1))],
        ));
        instructions.push(ins(OpCode::Return, vec![id("::NoneVar")]));

        let func = PexFunction {
            name: "Deep".into(),
            return_type_name: "None".into(),
            instructions,
            ..PexFunction::default()
        };
        let err = decompile_script(&pex_with_function(func))
            .expect_err("a cross-block temp chain past the cap must be declined");
        assert!(
            matches!(err, DecompileError::ExpressionTooDeep { .. }),
            "expected ExpressionTooDeep, got {err:?}"
        );
    }

    /// The short-circuit sibling of the test above: the left-associative
    /// `&&` chain a compiler emits for `if x0 && x1 && … && xN`. Each link
    /// is merged by `boolean::combine`, which nests two whole trees under
    /// one new `BinaryOp` — and `rebuild_expression`'s fold loop never runs
    /// on the resulting one-statement scope, so the cap has to be checked
    /// in `combine` itself.
    #[test]
    fn a_short_circuit_chain_is_declined_not_aborted() {
        const LINKS: usize = 4_000;

        let mut instructions = Vec::new();
        for k in 0..LINKS {
            instructions.push(ins(
                OpCode::Assign,
                vec![id("::temp0"), id(&format!("x{k}"))],
            ));
            instructions.push(ins(
                OpCode::JmpF,
                vec![id("::temp0"), Value::Integer(2)],
            ));
        }
        instructions.push(ins(OpCode::Return, vec![id("::NoneVar")]));

        let func = PexFunction {
            name: "Deep".into(),
            return_type_name: "None".into(),
            instructions,
            ..PexFunction::default()
        };
        let err = decompile_script(&pex_with_function(func))
            .expect_err("a short-circuit chain past the cap must be declined");
        assert!(
            matches!(err, DecompileError::ExpressionTooDeep { .. }),
            "expected ExpressionTooDeep, got {err:?}"
        );
    }
}
