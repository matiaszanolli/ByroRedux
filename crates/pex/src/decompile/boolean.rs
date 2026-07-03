//! Short-circuit boolean reconstruction — collapse the block shapes a
//! compiler emits for `&&` / `||` into a single conditional block whose
//! condition is the combined boolean expression. Port of Champollion's
//! `PscDecompiler::rebuildBooleanOperators`. Runs **before**
//! [`super::control_flow`], so the control-flow pass sees one clean
//! conditional per source `If`/`While` instead of a short-circuit chain.
//!
//! The shape: a conditional block whose last statement computes the
//! condition variable, and whose true (`&&`) or false (`||`) edge is the
//! immediately-following block, is a short-circuit. The following block
//! (which recomputes the same condition variable) becomes the right
//! operand; the rejoin block past it is merged in and its edges adopted,
//! so the collapsed block chains into the eventual `If`/`While` test.
//!
//! ## Two deliberate departures from the C++
//!
//! 1. **No debug-line guard.** Champollion consults per-instruction source
//!    lines to reject merges that span lines. We rely on the structural
//!    pattern alone (the follow-block recomputing the condition variable),
//!    which is the load-bearing signal; the line check only suppresses
//!    rare false positives. Validated against the corpus decompile rate +
//!    the R5 fidelity gate.
//! 2. **Termination guard.** The C++ unconditionally re-processes the
//!    source block after a potential `||`; we re-process only when a
//!    collapse actually merged a non-exit block (which strictly shrinks
//!    the graph), so the loop always terminates.

use std::collections::BTreeMap;

use super::cfg::{Cfg, END};
use super::lift::rebuild_expression;
use super::node::{Node, NodeKind, SYNTH_IP};
use super::DecompileError;
use crate::model::Value;

/// Recursion cap for [`BoolPass::rebuild`] (SCR-D2-01 / #1815), mirroring
/// `control_flow::MAX_REBUILD_DEPTH` (SAFE-2026-06-23-02). This pass runs on
/// the same untrusted CFG one step before the control-flow pass, so it needs
/// the same bound: real Papyrus nests `&&`/`||` a handful deep, so 1024 is
/// far above any well-formed `.pex` while still stopping an adversarial one
/// from overflowing the stack.
const MAX_REBUILD_DEPTH: usize = 1024;

/// Collapse `&&`/`||` short-circuits across a function's CFG + per-block
/// scopes, in place. No-op for a bodyless function.
pub fn rebuild_boolean_operators(
    cfg: &mut Cfg,
    scopes: &mut BTreeMap<usize, Vec<Node>>,
    func_name: &str,
) -> Result<(), DecompileError> {
    if cfg.entry == END {
        return Ok(());
    }
    let (entry, exit) = (cfg.entry, cfg.exit);
    BoolPass { cfg, scopes, func_name }.rebuild(entry, exit, 0)
}

struct BoolPass<'a> {
    cfg: &'a mut Cfg,
    scopes: &'a mut BTreeMap<usize, Vec<Node>>,
    func_name: &'a str,
}

/// The identifier a scope's last statement computes into — the assign
/// destination if it's `Assign(dest = Constant(Identifier))`, else the
/// node's own result.
fn last_result(scope: &[Node]) -> Option<String> {
    let last = scope.last()?;
    if let NodeKind::Assign { dest, .. } = &last.kind {
        if let NodeKind::Constant(Value::Identifier(id)) = &dest.kind {
            return Some(id.clone());
        }
    }
    last.result.clone()
}

/// If the single-statement operand scope assigns the condition variable,
/// unwrap that assign to its bare value (the right operand of the boolean).
/// Returns the operand node, or `None` if the scope doesn't have the
/// expected single-statement-computing-`cond` shape.
fn take_operand(scope: &mut Vec<Node>, cond: &str) -> Option<Node> {
    if scope.len() != 1 {
        return None;
    }
    let result = match &scope[0].kind {
        NodeKind::Assign { dest, .. } => match &dest.kind {
            NodeKind::Constant(Value::Identifier(id)) => Some(id.clone()),
            _ => scope[0].result.clone(),
        },
        _ => scope[0].result.clone(),
    };
    if result.as_deref() != Some(cond) {
        return None;
    }
    // Unwrap `dest = value` → `value`.
    if let NodeKind::Assign { value, .. } = &mut scope[0].kind {
        let v = std::mem::replace(value.as_mut(), Node::constant(SYNTH_IP, Value::None));
        return Some(v);
    }
    Some(scope.remove(0))
}

/// Combine `left` (the source's last statement) with `right` (the operand)
/// under `op`, preserving an enclosing assign if present. Returns the node
/// to push back onto the source scope.
fn combine(left: Node, op: &str, right: Node, cond: &str) -> Node {
    let prec = if op == "&&" { 7 } else { 8 };
    match left.kind {
        NodeKind::Assign { dest, value } => {
            let combined = Node::binary_op(SYNTH_IP, prec, Some(cond.to_string()), *value, op, right);
            Node::assign(SYNTH_IP, *dest, combined)
        }
        _ => Node::binary_op(SYNTH_IP, prec, Some(cond.to_string()), left, op, right),
    }
}

impl BoolPass<'_> {
    /// `depth` bounds nested short-circuit recursion against a malformed /
    /// adversarial `.pex` (SCR-D2-01 / #1815) — see [`MAX_REBUILD_DEPTH`].
    fn rebuild(&mut self, start: usize, end: usize, depth: usize) -> Result<(), DecompileError> {
        if depth > MAX_REBUILD_DEPTH {
            return Err(DecompileError::RecursionLimit {
                function: self.func_name.to_string(),
                limit: MAX_REBUILD_DEPTH,
            });
        }
        let mut it = start;
        while it != end {
            let current = it;
            let Some(block) = self.cfg.block(current).cloned() else {
                it = self.cfg.next_key(current).unwrap_or(end);
                continue;
            };
            let scope = self.scopes.get(&current).cloned().unwrap_or_default();

            let mut reprocess = false;
            if block.is_conditional() && !scope.is_empty() {
                if let Some(cond) = block.condition.clone() {
                    if last_result(&scope).as_deref() == Some(&cond) {
                        let end_plus_1 = block.end + 1;
                        if block.on_true() == end_plus_1 {
                            // Potential `&&`: true edge falls through.
                            self.rebuild(block.on_true(), block.on_false, depth + 1)?;
                            reprocess = self.collapse(current, &cond, BoolOp::And)?;
                        } else if block.on_false == end_plus_1 {
                            // Potential `||`: false edge falls through.
                            self.rebuild(block.on_false, block.on_true(), depth + 1)?;
                            reprocess = self.collapse(current, &cond, BoolOp::Or)?;
                        }
                    }
                }
            }

            it = if reprocess {
                current
            } else {
                self.cfg.next_key(current).unwrap_or(end)
            };
        }
        Ok(())
    }

    /// Try to collapse `current` with its fall-through operand block under
    /// `op`. Returns `true` when a collapse merged a non-exit rejoin block
    /// (so `current` should be re-processed for a further chain).
    fn collapse(&mut self, current: usize, cond: &str, op: BoolOp) -> Result<bool, DecompileError> {
        let src = self.cfg.block(current).cloned().expect("source block exists");
        // For `&&` the operand is the true block and the rejoin is the
        // false block; for `||` they swap.
        let (operand_key, rejoin_key) = match op {
            BoolOp::And => (src.on_true(), src.on_false),
            BoolOp::Or => (src.on_false, src.on_true()),
        };

        let mut operand_scope = match self.scopes.get(&operand_key) {
            Some(s) => s.clone(),
            None => return Ok(false),
        };
        let Some(right) = take_operand(&mut operand_scope, cond) else {
            return Ok(false);
        };

        // Build the combined expression onto the source scope.
        let mut src_scope = self.scopes.remove(&current).unwrap_or_default();
        let left = src_scope.pop().expect("conditional source has a last statement");
        src_scope.push(combine(left, op.as_str(), right, cond));

        // The operand block is now folded into the expression — drop it.
        self.cfg.blocks.remove(&operand_key);
        self.scopes.remove(&operand_key);

        // Merge the rejoin block's statements in, and adopt its edges so
        // `current` chains into the eventual If/While test.
        let rejoin = self.cfg.block(rejoin_key).cloned();
        let rejoin_scope = self.scopes.remove(&rejoin_key).unwrap_or_default();
        src_scope.extend(rejoin_scope);
        rebuild_expression(&mut src_scope, self.func_name)?;
        self.scopes.insert(current, src_scope);

        let reprocess = match rejoin {
            Some(r) if r.end != END => {
                {
                    let b = self.cfg.blocks.get_mut(&current).expect("source exists");
                    b.end = r.end;
                    b.condition = r.condition.clone();
                    b.next = r.next;
                    b.on_false = r.on_false;
                }
                self.cfg.blocks.remove(&rejoin_key);
                true
            }
            Some(r) => {
                // Rejoin is the exit anchor: the block is no longer
                // conditional (it now ends the function's straight-line flow).
                let end = r.begin;
                let b = self.cfg.blocks.get_mut(&current).expect("source exists");
                b.end = end;
                b.condition = r.condition.clone();
                b.next = end;
                b.on_false = end;
                false
            }
            None => false,
        };
        Ok(reprocess)
    }
}

#[derive(Clone, Copy)]
enum BoolOp {
    And,
    Or,
}

impl BoolOp {
    fn as_str(self) -> &'static str {
        match self {
            BoolOp::And => "&&",
            BoolOp::Or => "||",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::cfg::build_cfg;
    use super::super::control_flow::reconstruct;
    use super::super::lift::lift_function;
    use super::super::node::NodeKind;
    use super::*;
    use crate::model::{Function, Instruction, Object, TypedName};
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
    fn local(n: &str, t: &str) -> TypedName {
        TypedName { name: n.to_string(), type_name: t.to_string() }
    }

    /// Full pipeline including the boolean pass.
    fn decompile(function: Function) -> Vec<Node> {
        let object = Object::default();
        let cfg = build_cfg(&function).unwrap();
        let mut scopes = lift_function(&object, &function, &cfg).unwrap();
        let mut cfg = cfg;
        rebuild_boolean_operators(&mut cfg, &mut scopes, &function.name).unwrap();
        reconstruct(cfg, scopes, &function.name).unwrap()
    }

    /// Recursively look for a BinaryOp with the given operator.
    fn has_binop(nodes: &[Node], op: &str) -> bool {
        nodes.iter().any(|n| node_has_binop(n, op))
    }
    fn node_has_binop(node: &Node, op: &str) -> bool {
        let here = matches!(&node.kind, NodeKind::BinaryOp { op: o, .. } if o == op);
        here || node.child_nodes().iter().any(|c| node_has_binop(c, op))
    }
    fn child_ifs(nodes: &[Node]) -> usize {
        nodes.iter().filter(|n| matches!(n.kind, NodeKind::IfElse { .. })).count()
    }

    #[test]
    fn and_collapses_to_a_single_if_with_an_and_condition() {
        // if (a && b)
        //     x = 1
        // t = a ; jmpf t,+? ; t = b ; jmpf t,exit ; body ; return
        // 0: assign t, a
        // 1: jmpf t, 4   (short-circuit to the rejoin test... lands on 2's test)
        // 2: assign t, b
        // 3: jmpf t, 2   (the if-test: jmpf to after-body)  -> exit-of-if at 6? keep simple
        // Build: 0 t=a; 1 jmpf t -> 2 (skip to 2)... craft a real && shape:
        // 0: t = a
        // 1: jmpf t, +3  -> 4   (if !a short-circuit past t=b AND the body guard? )
        // Simpler canonical && shape the compiler emits:
        // 0: t=a ; 1: jmpf t,2(->3) ; 2: t=b ; 3: jmpf t,2(->5) ; 4: x=1 ; 5: return
        let f = Function {
            return_type_name: "None".into(),
            locals: vec![local("::temp0", "Bool"), local("x", "Int")],
            instructions: vec![
                ins(OpCode::Assign, vec![id("::temp0"), id("a")]),
                ins(OpCode::JmpF, vec![id("::temp0"), Value::Integer(2)]), // -> 3 (short-circuit to the if-test)
                ins(OpCode::Assign, vec![id("::temp0"), id("b")]),
                ins(OpCode::JmpF, vec![id("::temp0"), Value::Integer(2)]), // -> 5 (after body)
                ins(OpCode::Assign, vec![id("x"), Value::Integer(1)]),
                ins(OpCode::Return, vec![id("::NoneVar")]),
            ],
            ..Function::default()
        };
        let tree = decompile(f);
        // Exactly one If, and its condition uses `&&`.
        assert_eq!(child_ifs(&tree), 1, "collapsed to a single if (not nested)");
        assert!(has_binop(&tree, "&&"), "condition is an && expression");
    }

    #[test]
    fn or_collapses_to_a_single_if_with_an_or_condition() {
        // The compiler shape for `if (a || b) ; x = 1`:
        //   0: t = a
        //   1: jmpt t, +2  -> 3   (short-circuit: if a, jump to the if-test
        //                          with t still true)
        //   2: t = b
        //   3: jmpf t, +2  -> 5   (the if-test)
        //   4: x = 1
        //   5: return
        // jmpt: onTrue = target, onFalse = fall-through → the `||` short-circuit.
        let f = Function {
            return_type_name: "None".into(),
            locals: vec![local("::temp0", "Bool"), local("x", "Int")],
            instructions: vec![
                ins(OpCode::Assign, vec![id("::temp0"), id("a")]),
                ins(OpCode::JmpT, vec![id("::temp0"), Value::Integer(2)]), // a true → if-test at 3
                ins(OpCode::Assign, vec![id("::temp0"), id("b")]),
                ins(OpCode::JmpF, vec![id("::temp0"), Value::Integer(2)]), // !b → after at 5
                ins(OpCode::Assign, vec![id("x"), Value::Integer(1)]),
                ins(OpCode::Return, vec![id("::NoneVar")]),
            ],
            ..Function::default()
        };
        let tree = decompile(f);
        assert_eq!(child_ifs(&tree), 1, "collapsed to a single if");
        assert!(has_binop(&tree, "||"), "condition is an || expression");
    }

    #[test]
    fn plain_if_is_untouched_by_the_boolean_pass() {
        // if (a == b) ; x = 1  — no short-circuit, stays a single simple if.
        let f = Function {
            return_type_name: "None".into(),
            locals: vec![local("::temp0", "Bool"), local("x", "Int")],
            instructions: vec![
                ins(OpCode::CmpEq, vec![id("::temp0"), id("a"), id("b")]),
                ins(OpCode::JmpF, vec![id("::temp0"), Value::Integer(2)]),
                ins(OpCode::Assign, vec![id("x"), Value::Integer(1)]),
                ins(OpCode::Return, vec![id("::NoneVar")]),
            ],
            ..Function::default()
        };
        let tree = decompile(f);
        assert_eq!(child_ifs(&tree), 1);
        assert!(!has_binop(&tree, "&&") && !has_binop(&tree, "||"));
        assert!(has_binop(&tree, "=="));
    }

    #[test]
    fn straight_line_with_a_call_is_unchanged() {
        let f = Function {
            return_type_name: "None".into(),
            instructions: vec![
                ins_v(OpCode::CallMethod, vec![id("Foo"), id("self"), id("::NoneVar")], vec![]),
                ins(OpCode::Return, vec![id("::NoneVar")]),
            ],
            ..Function::default()
        };
        let tree = decompile(f);
        assert_eq!(child_ifs(&tree), 0);
    }

    /// SCR-D2-01 (#1815) — an adversarial / malformed `.pex` that would nest
    /// short-circuit operands deeper than the cap is rejected with a
    /// `RecursionLimit` error rather than overflowing the stack. Mirrors
    /// `control_flow::rebuild_rejects_excessive_recursion_depth`: the depth
    /// guard fires before any CFG access, so a trivial pass exercises it.
    #[test]
    fn rebuild_rejects_excessive_recursion_depth() {
        let mut cfg = Cfg { blocks: BTreeMap::new(), entry: 0, exit: 0 };
        let mut scopes = BTreeMap::new();
        let mut pass = BoolPass { cfg: &mut cfg, scopes: &mut scopes, func_name: "Deep" };
        let err = pass
            .rebuild(0, 0, MAX_REBUILD_DEPTH + 1)
            .expect_err("over-deep recursion must error, not overflow");
        assert!(
            matches!(err, DecompileError::RecursionLimit { limit, .. } if limit == MAX_REBUILD_DEPTH),
            "got {err:?}"
        );
    }
}
