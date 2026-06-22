//! Opcode → node-tree lifting and copy-propagation. Port of Champollion's
//! `PscDecompiler::createNodesForBlocks`, `checkAssign`, `fromValue`,
//! `typeOfVar`/`findVarTypes`, and `rebuildExpression`.
//!
//! For each basic block (from [`super::cfg`]) this turns the block's
//! instructions into a flat list of statement nodes — every opcode result
//! initially landing in its own temp — then folds each temp-producing node
//! into its single consumer, rebuilding nested expression trees. The
//! output is one scope (`Vec<Node>`) per block, ready for the control-flow
//! reconstruction pass.

use std::collections::BTreeMap;

use crate::model::{Function, Instruction, Object, Value};
use crate::opcode::OpCode;

use super::cfg::Cfg;
use super::node::{is_temp_var, Node, NodeKind};
use super::DecompileError;

/// Per-function lifting context: the variable→type map and whether the
/// function returns `None` (void).
struct LiftCtx {
    var_types: BTreeMap<String, String>,
    return_none: bool,
}

impl LiftCtx {
    /// Declared type of a variable (Champollion `typeOfVar`); empty string
    /// when unknown.
    fn type_of(&self, name: &str) -> String {
        self.var_types.get(name).cloned().unwrap_or_default()
    }
}

/// Build the `name → type` map from the owning object's variables plus the
/// function's params and locals (Champollion `findVarTypes`).
fn build_var_types(object: &Object, function: &Function) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    for v in &object.variables {
        m.insert(v.name.clone(), v.type_name.clone());
    }
    for p in &function.params {
        m.insert(p.name.clone(), p.type_name.clone());
    }
    for l in &function.locals {
        m.insert(l.name.clone(), l.type_name.clone());
    }
    m
}

/// `Pex::Value::getId()` — the identifier text of an operand that must be
/// an identifier (a destination, method name, or property name).
fn id_of(v: &Value, ip: usize) -> Result<String, DecompileError> {
    match v {
        Value::Identifier(s) => Ok(s.clone()),
        _ => Err(DecompileError::ExpectedIdentifier { ip }),
    }
}

/// `fromValue` — wrap an operand as a constant node.
fn from_value(ip: usize, v: &Value) -> Node {
    Node::constant(ip, v.clone())
}

/// Lift every block's instructions into a rebuilt scope, keyed by block.
///
/// `object` supplies object-level variable types; `function` owns the
/// instruction stream `cfg` was built from. Bodyless functions yield an
/// empty map.
pub fn lift_function(
    object: &Object,
    function: &Function,
    cfg: &Cfg,
) -> Result<BTreeMap<usize, Vec<Node>>, DecompileError> {
    let ctx = LiftCtx {
        var_types: build_var_types(object, function),
        return_none: function.return_type_name.is_empty()
            || function.return_type_name.eq_ignore_ascii_case("none"),
    };

    let count = function.instructions.len();
    let mut scopes = BTreeMap::new();
    for (&key, block) in &cfg.blocks {
        // The synthetic exit anchor (begin == instruction count) has no code.
        if block.begin >= count {
            scopes.insert(key, Vec::new());
            continue;
        }
        let mut scope = Vec::new();
        for ip in block.begin..=block.end {
            if let Some(node) = ctx.create_node(ip, &function.instructions[ip])? {
                scope.push(check_assign(node));
            }
        }
        rebuild_expression(&mut scope, function)?;
        scopes.insert(key, scope);
    }
    Ok(scopes)
}

impl LiftCtx {
    /// Lift one instruction to a node, or `None` for opcodes that emit no
    /// node (`nop`, the jumps). Port of the `createNodesForBlocks` switch.
    fn create_node(&self, ip: usize, ins: &Instruction) -> Result<Option<Node>, DecompileError> {
        let a = &ins.args;
        // Convenience: identifier of fixed arg N (destination/name operands).
        let id = |n: usize| -> Result<String, DecompileError> { id_of(&a[n], ip) };
        // Convenience: constant node from fixed arg N.
        let val = |n: usize| -> Node { from_value(ip, &a[n]) };

        let node = match ins.op {
            OpCode::Nop | OpCode::Jmp | OpCode::JmpT | OpCode::JmpF => return Ok(None),

            // Arithmetic / string-concat — `result = left <op> right`.
            OpCode::IAdd | OpCode::FAdd | OpCode::StrCat => {
                Node::binary_op(ip, 5, Some(id(0)?), val(1), "+", val(2))
            }
            OpCode::ISub | OpCode::FSub => {
                Node::binary_op(ip, 5, Some(id(0)?), val(1), "-", val(2))
            }
            OpCode::IMul | OpCode::FMul => {
                Node::binary_op(ip, 4, Some(id(0)?), val(1), "*", val(2))
            }
            OpCode::IDiv | OpCode::FDiv => {
                Node::binary_op(ip, 4, Some(id(0)?), val(1), "/", val(2))
            }
            OpCode::IMod => Node::binary_op(ip, 4, Some(id(0)?), val(1), "%", val(2)),

            OpCode::Not => Node::unary_op(ip, 3, Some(id(0)?), "!", val(1)),
            OpCode::INeg | OpCode::FNeg => Node::unary_op(ip, 3, Some(id(0)?), "-", val(1)),

            OpCode::Assign => Node::copy(ip, Some(id(0)?), val(1)),

            OpCode::Cast => {
                let result = id(0)?;
                let src = &a[1];
                if src.is_none() {
                    Node::copy(ip, Some(result), val(1))
                } else if !matches!(src, Value::Identifier(_))
                    || (self.type_of(&result) != self.type_of(src.as_identifier().unwrap())
                        && !src.as_identifier().unwrap().eq_ignore_ascii_case("::nonevar"))
                {
                    let ty = self.type_of(&result);
                    Node::cast(ip, Some(result), val(1), ty)
                } else {
                    // Two same-typed identifiers — really just an assign.
                    Node::copy(ip, Some(result), val(1))
                }
            }

            OpCode::CmpEq => Node::binary_op(ip, 5, Some(id(0)?), val(1), "==", val(2)),
            OpCode::CmpLt => Node::binary_op(ip, 5, Some(id(0)?), val(1), "<", val(2)),
            OpCode::CmpLte => Node::binary_op(ip, 5, Some(id(0)?), val(1), "<=", val(2)),
            OpCode::CmpGt => Node::binary_op(ip, 5, Some(id(0)?), val(1), ">", val(2)),
            OpCode::CmpGte => Node::binary_op(ip, 5, Some(id(0)?), val(1), ">=", val(2)),

            // Calls: object.method(varargs...). Result in args[last].
            OpCode::CallMethod => {
                Node::call_method(ip, Some(id(2)?), val(1), id(0)?, self.varargs(ip, ins), false)
            }
            OpCode::CallParent => Node::call_method(
                ip,
                Some(id(1)?),
                Node::identifier_string(ip, "Parent"),
                id(0)?,
                self.varargs(ip, ins),
                false,
            ),
            OpCode::CallStatic => {
                Node::call_method(ip, Some(id(2)?), val(0), id(1)?, self.varargs(ip, ins), false)
            }

            OpCode::Return => {
                if self.return_none {
                    Node::ret(ip, None)
                } else {
                    Node::ret(ip, Some(val(0)))
                }
            }

            // Property get → `result = object.prop`; set → assign target.
            OpCode::PropGet => Node::property_access(ip, Some(id(2)?), val(1), id(0)?),
            OpCode::PropSet => {
                let target = Node::property_access(ip, None, val(1), id(0)?);
                Node::assign(ip, target, val(2))
            }

            OpCode::ArrayCreate => {
                let result = id(0)?;
                let ty = self.type_of(&result);
                Node::array_create(ip, Some(result), ty, val(1))
            }
            OpCode::ArrayLength => Node::array_length(ip, Some(id(0)?), val(1)),
            OpCode::ArrayGetElement => Node::array_access(ip, Some(id(0)?), val(1), val(2)),
            OpCode::ArraySetElement => {
                let target = Node::array_access(ip, None, val(0), val(1));
                Node::assign(ip, target, val(2))
            }
            // Array search → synthetic `array.find(value, start)` etc.
            OpCode::ArrayFindElement => {
                Node::call_method(ip, Some(id(1)?), val(0), "find", vec![val(2), val(3)], false)
            }
            OpCode::ArrayRFindElement => {
                Node::call_method(ip, Some(id(1)?), val(0), "rfind", vec![val(2), val(3)], false)
            }

            OpCode::Is => Node::binary_op(ip, 0, Some(id(0)?), val(1), "is", val(2)),

            OpCode::StructCreate => {
                let result = id(0)?;
                let ty = self.type_of(&result);
                Node::struct_create(ip, Some(result), ty)
            }
            OpCode::StructGet => Node::property_access(ip, Some(id(0)?), val(1), id(2)?),
            OpCode::StructSet => {
                let target = Node::property_access(ip, None, val(0), id(1)?);
                Node::assign(ip, target, val(2))
            }

            OpCode::ArrayFindStruct => Node::call_method(
                ip,
                Some(id(1)?),
                val(0),
                "findstruct",
                vec![val(2), val(3), val(4)],
                false,
            ),
            OpCode::ArrayRFindStruct => Node::call_method(
                ip,
                Some(id(1)?),
                val(0),
                "rfindstruct",
                vec![val(2), val(3), val(4)],
                false,
            ),
            OpCode::ArrayAdd => {
                Node::call_method(ip, None, val(0), "add", vec![val(1), val(2)], false)
            }
            OpCode::ArrayInsert => {
                Node::call_method(ip, None, val(0), "insert", vec![val(1), val(2)], false)
            }
            OpCode::ArrayRemoveLast => Node::call_method(ip, None, val(0), "removelast", vec![], false),
            OpCode::ArrayRemove => {
                Node::call_method(ip, None, val(0), "remove", vec![val(1), val(2)], false)
            }
            OpCode::ArrayClear => Node::call_method(ip, None, val(0), "clear", vec![], false),
            OpCode::ArrayGetAllMatchingStructs => Node::call_method(
                ip,
                Some(id(1)?),
                val(0),
                "GetMatchingStructs",
                vec![val(2), val(3), val(4), val(5)],
                // Experimental: syntax unverified pending a Starfield CK.
                true,
            ),

            // Starfield guards (lock/try-lock/unlock) — lifted to call-ish
            // markers here; the control-flow pass turns them into guard
            // statements. Modelled minimally for now via CallMethod so the
            // tree stays uniform; revisited when guard reconstruction lands.
            OpCode::LockGuards => Node::call_method(
                ip,
                None,
                Node::identifier_string(ip, "Guard"),
                "lock",
                self.varargs(ip, ins),
                false,
            ),
            OpCode::UnlockGuards => Node::call_method(
                ip,
                None,
                Node::identifier_string(ip, "Guard"),
                "unlock",
                self.varargs(ip, ins),
                false,
            ),
            OpCode::TryLockGuards => Node::call_method(
                ip,
                Some(id(0)?),
                Node::identifier_string(ip, "Guard"),
                "trylock",
                self.varargs(ip, ins),
                false,
            ),
        };
        Ok(Some(node))
    }

    /// Constant nodes for an instruction's var-arg operands (call args,
    /// guard lists).
    fn varargs(&self, ip: usize, ins: &Instruction) -> Vec<Node> {
        ins.var_args.iter().map(|v| from_value(ip, v)).collect()
    }
}

/// `checkAssign` — if a node computes into a *real* (non-temp) variable,
/// wrap it as `Assign(dest = that var, value = the node)`; temp-producing
/// nodes are left bare for [`rebuild_expression`] to inline.
fn check_assign(node: Node) -> Node {
    match &node.result {
        Some(r) if !is_temp_var(r) => {
            let begin = node.begin;
            let dest = Node::constant(begin, Value::Identifier(r.clone()));
            Node::assign(begin, dest, node)
        }
        _ => node,
    }
}

/// Copy-propagation within one block's statement list (Champollion
/// `rebuildExpression`). Each non-final (temp-producing) statement is
/// folded into the single following statement that consumes its result,
/// collapsing temps into nested expression trees. More than one consumer
/// of a temp at this stage is a structural error.
fn rebuild_expression(scope: &mut Vec<Node>, function: &Function) -> Result<(), DecompileError> {
    let mut i = 0;
    while i < scope.len() {
        if !scope[i].is_final() && i + 1 < scope.len() {
            // Non-final ⇒ result is a temp/nonevar identifier.
            let temp = scope[i].result.clone().expect("non-final node has a result");
            match count_constant_id(&scope[i + 1], &temp) {
                0 => i += 1,
                1 => {
                    let producer = scope.remove(i); // consumer shifts to index i
                    let mut slot = Some(producer);
                    replace_constant_id(&mut scope[i], &temp, &mut slot);
                    debug_assert!(slot.is_none(), "verified single match must be consumed");
                    i = 0; // restart, like the C++ `it = scope->begin()`
                }
                _ => {
                    return Err(DecompileError::ExpressionRebuildFailed {
                        function: function.name.clone(),
                        ip: scope[i + 1].begin,
                    })
                }
            }
        } else {
            i += 1;
        }
    }
    Ok(())
}

/// Count `Constant(Identifier(name))` occurrences in a node tree.
fn count_constant_id(node: &Node, name: &str) -> usize {
    let here = matches!(&node.kind, NodeKind::Constant(Value::Identifier(s)) if s == name) as usize;
    here + node.child_nodes().iter().map(|c| count_constant_id(c, name)).sum::<usize>()
}

/// Replace the first `Constant(Identifier(name))` in `node` with the
/// node held in `slot` (taking it). With the single-match precondition the
/// caller verifies, this performs exactly one substitution.
fn replace_constant_id(node: &mut Node, name: &str, slot: &mut Option<Node>) {
    if slot.is_none() {
        return;
    }
    if matches!(&node.kind, NodeKind::Constant(Value::Identifier(s)) if s == name) {
        *node = slot.take().expect("slot is Some");
        return;
    }
    for child in node.child_nodes_mut() {
        replace_constant_id(child, name, slot);
        if slot.is_none() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::cfg::build_cfg;
    use super::*;
    use crate::model::{Function, Instruction, Object, TypedName, Variable};

    fn ins(op: OpCode, args: Vec<Value>) -> Instruction {
        Instruction { op, args, var_args: Vec::new() }
    }
    fn ins_v(op: OpCode, args: Vec<Value>, var_args: Vec<Value>) -> Instruction {
        Instruction { op, args, var_args }
    }
    fn id(s: &str) -> Value {
        Value::Identifier(s.to_string())
    }
    fn local(name: &str, ty: &str) -> TypedName {
        TypedName { name: name.to_string(), type_name: ty.to_string() }
    }

    /// Lift a single-block function and return its scope.
    fn lift_single(function: Function) -> Vec<Node> {
        let object = Object::default();
        let cfg = build_cfg(&function).unwrap();
        let mut scopes = lift_function(&object, &function, &cfg).unwrap();
        scopes.remove(&0).unwrap()
    }

    #[test]
    fn temp_folds_into_its_single_consumer() {
        // ::temp0 = a + b ; x = ::temp0   →   x = (a + b)
        let f = Function {
            locals: vec![local("::temp0", "Int"), local("x", "Int")],
            instructions: vec![
                ins(OpCode::IAdd, vec![id("::temp0"), id("a"), id("b")]),
                ins(OpCode::Assign, vec![id("x"), id("::temp0")]),
            ],
            ..Function::default()
        };
        let scope = lift_single(f);
        assert_eq!(scope.len(), 1, "the temp statement was inlined");
        // x = Copy( (a + b) )
        let NodeKind::Assign { dest, value } = &scope[0].kind else {
            panic!("expected Assign, got {:?}", scope[0].kind);
        };
        assert!(matches!(&dest.kind, NodeKind::Constant(Value::Identifier(s)) if s == "x"));
        let NodeKind::Copy { value: copied } = &value.kind else {
            panic!("expected Copy, got {:?}", value.kind);
        };
        let NodeKind::BinaryOp { left, op, right } = &copied.kind else {
            panic!("expected BinaryOp, got {:?}", copied.kind);
        };
        assert_eq!(op, "+");
        assert!(matches!(&left.kind, NodeKind::Constant(Value::Identifier(s)) if s == "a"));
        assert!(matches!(&right.kind, NodeKind::Constant(Value::Identifier(s)) if s == "b"));
    }

    #[test]
    fn chained_temps_fold_into_one_expression() {
        // t0 = a + b ; t1 = t0 * c ; x = t1   →   x = ((a + b) * c)
        let f = Function {
            locals: vec![
                local("::temp0", "Int"),
                local("::temp1", "Int"),
                local("x", "Int"),
            ],
            instructions: vec![
                ins(OpCode::IAdd, vec![id("::temp0"), id("a"), id("b")]),
                ins(OpCode::IMul, vec![id("::temp1"), id("::temp0"), id("c")]),
                ins(OpCode::Assign, vec![id("x"), id("::temp1")]),
            ],
            ..Function::default()
        };
        let scope = lift_single(f);
        assert_eq!(scope.len(), 1);
        // x = Copy( (a + b) * c )
        let NodeKind::Assign { value, .. } = &scope[0].kind else { panic!() };
        let NodeKind::Copy { value: mul } = &value.kind else { panic!() };
        let NodeKind::BinaryOp { left, op, .. } = &mul.kind else { panic!() };
        assert_eq!(op, "*");
        // left of the * is the (a + b) subtree.
        assert!(matches!(&left.kind, NodeKind::BinaryOp { op, .. } if op == "+"));
    }

    #[test]
    fn call_with_inlined_argument() {
        // ::temp0 = a == b ; self.Foo(::temp0)   →   self.Foo(a == b)
        let f = Function {
            return_type_name: "None".into(),
            locals: vec![local("::temp0", "Bool")],
            instructions: vec![
                ins(OpCode::CmpEq, vec![id("::temp0"), id("a"), id("b")]),
                ins_v(
                    OpCode::CallMethod,
                    vec![id("Foo"), id("self"), id("::NoneVar")],
                    vec![id("::temp0")],
                ),
            ],
            ..Function::default()
        };
        let scope = lift_single(f);
        assert_eq!(scope.len(), 1);
        let NodeKind::CallMethod { method, params, .. } = &scope[0].kind else {
            panic!("expected CallMethod, got {:?}", scope[0].kind);
        };
        assert_eq!(method, "Foo");
        assert_eq!(params.len(), 1);
        assert!(matches!(&params[0].kind, NodeKind::BinaryOp { op, .. } if op == "=="));
    }

    #[test]
    fn property_set_lowers_to_assign_of_property_access() {
        // obj.Health = 100
        let f = Function {
            instructions: vec![ins(
                OpCode::PropSet,
                vec![id("Health"), id("obj"), Value::Integer(100)],
            )],
            ..Function::default()
        };
        let scope = lift_single(f);
        assert_eq!(scope.len(), 1);
        let NodeKind::Assign { dest, value } = &scope[0].kind else { panic!() };
        let NodeKind::PropertyAccess { object, property } = &dest.kind else {
            panic!("expected PropertyAccess dest, got {:?}", dest.kind);
        };
        assert_eq!(property, "Health");
        assert!(matches!(&object.kind, NodeKind::Constant(Value::Identifier(s)) if s == "obj"));
        assert!(matches!(&value.kind, NodeKind::Constant(Value::Integer(100))));
    }

    #[test]
    fn cast_between_different_types_is_a_cast_not_a_copy() {
        // ::temp0(ObjectReference) = cast actor   where actor: Actor
        let object = Object {
            variables: vec![Variable {
                name: "actor".into(),
                type_name: "Actor".into(),
                ..Variable::default()
            }],
            ..Object::default()
        };
        let f = Function {
            locals: vec![local("dest", "ObjectReference")],
            instructions: vec![
                ins(OpCode::Cast, vec![id("dest"), id("actor")]),
                // consume dest so it survives as a real assign target
                ins_v(OpCode::CallMethod, vec![id("Foo"), id("dest"), id("::NoneVar")], vec![]),
            ],
            return_type_name: "None".into(),
            ..Function::default()
        };
        let cfg = build_cfg(&f).unwrap();
        let scope = lift_function(&object, &f, &cfg).unwrap().remove(&0).unwrap();
        // First statement: dest = Cast(actor as ObjectReference)
        let NodeKind::Assign { value, .. } = &scope[0].kind else {
            panic!("expected Assign, got {:?}", scope[0].kind);
        };
        let NodeKind::Cast { target_type, .. } = &value.kind else {
            panic!("expected Cast, got {:?}", value.kind);
        };
        assert_eq!(target_type, "ObjectReference");
    }

    #[test]
    fn double_use_of_a_temp_is_an_error() {
        // ::temp0 = a + b ; x = ::temp0 + ::temp0   (temp consumed twice)
        let f = Function {
            locals: vec![local("::temp0", "Int"), local("x", "Int")],
            instructions: vec![
                ins(OpCode::IAdd, vec![id("::temp0"), id("a"), id("b")]),
                ins(OpCode::IAdd, vec![id("x"), id("::temp0"), id("::temp0")]),
            ],
            ..Function::default()
        };
        let object = Object::default();
        let cfg = build_cfg(&f).unwrap();
        assert!(matches!(
            lift_function(&object, &f, &cfg),
            Err(DecompileError::ExpressionRebuildFailed { .. })
        ));
    }
}
