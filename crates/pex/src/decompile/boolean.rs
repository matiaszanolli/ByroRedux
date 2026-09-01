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
//!    pattern alone, which means the *edge shape* has to carry what the
//!    line check would otherwise catch: [`BoolPass::collapse`] therefore
//!    requires the operand block to fall through to the rejoin. Without
//!    that requirement a `While` whose body recomputes the loop condition
//!    is silently collapsed and the loop disappears (#2655).
//!
//!    Note the corpus decompile rate does **not** validate this departure:
//!    the smoke harness discards the resulting `Script`, so it measures
//!    robustness (no panic, no `Err`), not fidelity — a wrong AST scores as
//!    a success. The R5 fidelity gate does check shape, but it is a single
//!    `#[ignore]`d script that only runs with Skyrim SE data on disk.
//! 2. **Termination guard.** The C++ unconditionally re-processes the
//!    source block after a potential `||`; we re-process only when a
//!    collapse actually merged a non-exit block (which strictly shrinks
//!    the graph), so the `while` loop in [`BoolPass::rebuild`] terminates.
//!
//!    #2667 — that argument covers the *iterative* loop only. The same
//!    edge adoption can drive `rebuild`'s **recursion** back into the block
//!    range it is already walking (the adopted `next` points at the range
//!    start for `&&`, `on_false` for `||`), and nothing about a shrinking
//!    graph bounds that. [`MAX_REBUILD_DEPTH`] is what bounds it, and the
//!    resulting `RecursionLimit` is a clean decline.

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
    BoolPass {
        cfg,
        scopes,
        func_name,
    }
    .rebuild(entry, exit, 0)
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
            let combined =
                Node::binary_op(SYNTH_IP, prec, Some(cond.to_string()), *value, op, right);
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
                // #2667 — not "control-flow": this pass runs one step before
                // that one, and a chain that overflows here used to be
                // reported under the other pass's name.
                pass: "short-circuit boolean",
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
            let mut reprocess = false;
            if block.is_conditional() {
                if let Some(cond) = block.condition.clone() {
                    // #3783 — test the block shape and read the scope by
                    // REFERENCE. This used to be a `.cloned()` lookup on
                    // `self.scopes`, hoisted above the
                    // `is_conditional()` test, so every
                    // block on every visit — including straight-line
                    // functions with no conditional block at all — paid a
                    // full deep copy of its expression trees, once more per
                    // `reprocess` re-visit. `Node`'s derived `Clone` recurses
                    // once per tree level with no cap, so on a deep tree that
                    // clone was a stack-overflow `SIGABRT` (not a panic:
                    // `translate_pex`'s `catch_unwind` guard cannot intercept
                    // it). The clone bought nothing — the scope is read here
                    // and nowhere else in this iteration.
                    let condition_is_last_result = self.scopes.get(&current).is_some_and(|scope| {
                        !scope.is_empty() && last_result(scope).as_deref() == Some(&cond)
                    });
                    if condition_is_last_result {
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
        // #2667 (SCR-D3-NEW11-02) — this was `.expect("source block exists")`,
        // an invariant *inherited* rather than local. `rebuild` verifies the
        // block exists, but then recurses into the operand range before
        // calling here, and a nested `collapse` whose rejoin is an enclosing
        // (on-stack) block removes that ancestor from the CFG. The panic
        // survived only because the adopted edge necessarily points back into
        // the range being walked, so `rebuild` re-enters and `MAX_REBUILD_DEPTH`
        // fires first — a crafted `.pex` reaches `RecursionLimit`, not this
        // line. That is a real guarantee today and no guarantee at all if the
        // cap is ever raised or bypassed, so decline locally instead.
        let Some(src) = self.cfg.block(current).cloned() else {
            return Ok(false);
        };
        // For `&&` the operand is the true block and the rejoin is the
        // false block; for `||` they swap.
        let (operand_key, rejoin_key) = match op {
            BoolOp::And => (src.on_true(), src.on_false),
            BoolOp::Or => (src.on_false, src.on_true()),
        };
        if operand_key == current || rejoin_key == current {
            // #2667 — a self-referential edge. Sibling of the degenerate shape
            // below: folding `current` into itself would have the rejoin merge
            // remove the very block being rewritten (`blocks.remove(&rejoin_key)`
            // after `blocks.get_mut(&current)`), leaving every predecessor
            // pointing at a hole. Declining here is also what keeps the two
            // remaining `get_mut(&current).expect(…)` uses below *locally*
            // sound: after this guard, nothing between the lookup above and
            // those calls can remove `current`.
            return Ok(false);
        }
        if operand_key == rejoin_key {
            // Degenerate/adversarial shape (SCR-D3-NEW-01 / #2028): a
            // conditional block whose true and false edges are the same
            // target. Removing the operand block below would also remove
            // `rejoin_key`, leaving `current`'s edges pointing at a block
            // that no longer exists. Decline rather than corrupt the CFG —
            // never reachable from real compiler output.
            return Ok(false);
        }

        // The operand block must *fall through* to the rejoin (SCR-D3-NEW11-01
        // / #2655). Without this, a `While` whose single-statement body happens
        // to recompute the loop-condition variable matches all three structural
        // signals — conditional source, last statement computes the condition,
        // follow block recomputes the same variable — while its `next` edge
        // points *backwards* to the loop head. Collapsing it deletes the back
        // edge, so the loop vanishes from the AST and is replaced by an `&&`
        // that was never in the source. Champollion suppresses this with a
        // per-instruction debug-line check; we deliberately don't consult debug
        // lines (departure 1 above), so the edge shape is what has to carry it.
        let falls_through_to_rejoin = self
            .cfg
            .block(operand_key)
            .is_some_and(|b| b.next == rejoin_key && !b.is_conditional());
        if !falls_through_to_rejoin {
            return Ok(false);
        }

        let mut operand_scope = match self.scopes.get(&operand_key) {
            Some(s) => s.clone(),
            None => return Ok(false),
        };
        let Some(right) = take_operand(&mut operand_scope, cond) else {
            return Ok(false);
        };

        // Build the combined expression onto the source scope.
        let mut src_scope = self.scopes.remove(&current).unwrap_or_default();
        // #2667 — likewise inherited, not local: `rebuild` checked
        // `!scope.is_empty()` on a *clone* taken before the recursive
        // `rebuild` call, which can have merged this scope away in the
        // meantime. Put it back and decline rather than panic.
        let Some(left) = src_scope.pop() else {
            self.scopes.insert(current, src_scope);
            return Ok(false);
        };
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
    use super::super::cfg::{build_cfg, CodeBlock};
    use super::super::control_flow::reconstruct;
    use super::super::lift::lift_function;
    use super::super::node::NodeKind;
    use super::*;
    use crate::model::{Function, Instruction, Object, TypedName};
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
    fn local(n: &str, t: &str) -> TypedName {
        TypedName {
            name: n.to_string(),
            type_name: t.to_string(),
        }
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
        nodes
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::IfElse { .. }))
            .count()
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

    /// SCR-D3-NEW11-01 (#2655) — a `While` whose single-statement body
    /// recomputes the loop-condition variable matches all three of the
    /// boolean pass's structural signals, but its operand block jumps
    /// *backwards* to the loop head instead of falling through to the
    /// rejoin. Collapsing it would delete the back edge, erasing the loop
    /// and fabricating an `&&` that was never in the source.
    #[test]
    fn loop_body_recomputing_the_condition_is_not_collapsed_into_an_and() {
        // 0: ::temp0 = (a == b)   ; loop condition
        // 1: jmpf ::temp0, +3 -> 4 ; loop exit test
        // 2: ::temp0 = (c == d)   ; body — single stmt, writes ::temp0
        // 3: jmp -3 -> 0          ; back edge
        // 4: return
        let f = Function {
            return_type_name: "None".into(),
            locals: vec![local("::temp0", "Bool")],
            instructions: vec![
                ins(OpCode::CmpEq, vec![id("::temp0"), id("a"), id("b")]),
                ins(OpCode::JmpF, vec![id("::temp0"), Value::Integer(3)]),
                ins(OpCode::CmpEq, vec![id("::temp0"), id("c"), id("d")]),
                ins(OpCode::Jmp, vec![Value::Integer(-3)]),
                ins(OpCode::Return, vec![id("::NoneVar")]),
            ],
            ..Function::default()
        };
        let tree = decompile(f);
        assert!(
            tree.iter()
                .any(|n| matches!(n.kind, NodeKind::While { .. })),
            "the loop must survive the boolean pass, got: {tree:#?}"
        );
        assert!(
            !has_binop(&tree, "&&"),
            "no `&&` may be fabricated from the back edge, got: {tree:#?}"
        );
    }

    #[test]
    fn straight_line_with_a_call_is_unchanged() {
        let f = Function {
            return_type_name: "None".into(),
            instructions: vec![
                ins_v(
                    OpCode::CallMethod,
                    vec![id("Foo"), id("self"), id("::NoneVar")],
                    vec![],
                ),
                ins(OpCode::Return, vec![id("::NoneVar")]),
            ],
            ..Function::default()
        };
        let tree = decompile(f);
        assert_eq!(child_ifs(&tree), 0);
    }

    /// SCR-D3-NEW-01 (#2028) — a conditional block whose true and false
    /// edges are the same target (`operand_key == rejoin_key`, never
    /// emitted by real compiler output) must decline the collapse rather
    /// than remove the shared block and leave `current`'s edges dangling.
    #[test]
    fn collapse_declines_when_operand_and_rejoin_keys_are_equal() {
        let mut cfg = Cfg {
            blocks: BTreeMap::new(),
            entry: 0,
            exit: 2,
        };
        cfg.blocks.insert(
            0,
            CodeBlock {
                begin: 0,
                end: 0,
                next: 1,
                on_false: 1,
                condition: Some("t".to_string()),
            },
        );
        cfg.blocks.insert(
            1,
            CodeBlock {
                begin: 1,
                end: 1,
                next: 2,
                on_false: END,
                condition: None,
            },
        );
        let mut scopes: BTreeMap<usize, Vec<Node>> = BTreeMap::new();
        scopes.insert(
            0,
            vec![Node::assign(
                SYNTH_IP,
                Node::constant(SYNTH_IP, id("t")),
                Node::constant(SYNTH_IP, id("a")),
            )],
        );
        scopes.insert(
            1,
            vec![Node::assign(
                SYNTH_IP,
                Node::constant(SYNTH_IP, id("t")),
                Node::constant(SYNTH_IP, id("b")),
            )],
        );
        let mut pass = BoolPass {
            cfg: &mut cfg,
            scopes: &mut scopes,
            func_name: "Degenerate",
        };
        let collapsed = pass
            .collapse(0, "t", BoolOp::And)
            .expect("degenerate shape must decline, not error");
        assert!(
            !collapsed,
            "must not report a merge for operand_key == rejoin_key"
        );
        assert!(
            pass.cfg.blocks.contains_key(&1),
            "the shared operand/rejoin block must remain intact when declined"
        );
        assert!(
            pass.scopes.contains_key(&1),
            "the shared operand/rejoin scope must remain intact when declined"
        );
    }

    /// #2667 (SCR-D3-NEW11-02) — `collapse` must not assume the block it was
    /// asked about still exists.
    ///
    /// `rebuild` verifies it before recursing into the operand range, but a
    /// nested collapse whose rejoin is an *enclosing* (on-stack) block removes
    /// that ancestor from the CFG; the ancestor's own `collapse` then ran
    /// `self.cfg.block(current).expect("source block exists")` on a hole. The
    /// panic was unreachable only because the adopted edge forces `rebuild`
    /// back into the range it is walking, so `MAX_REBUILD_DEPTH` fires first —
    /// a property of a *cap*, not a local invariant. Calling `collapse`
    /// directly is what separates the two: no recursion is involved here, so
    /// nothing but the guard itself is under test.
    #[test]
    fn collapse_declines_when_its_source_block_is_gone() {
        let mut cfg = Cfg {
            blocks: BTreeMap::new(),
            entry: 0,
            exit: 2,
        };
        // Deliberately empty: `current` was removed by an inner collapse.
        let mut scopes: BTreeMap<usize, Vec<Node>> = BTreeMap::new();
        let mut pass = BoolPass {
            cfg: &mut cfg,
            scopes: &mut scopes,
            func_name: "VanishedSource",
        };

        let collapsed = pass
            .collapse(0, "t", BoolOp::And)
            .expect("a missing source block must decline, not error");
        assert!(!collapsed, "nothing was merged, so nothing to re-process");
    }

    /// #2667 — the same locality point for the source *scope*. `rebuild`
    /// checks `!scope.is_empty()` on a clone taken before its recursive call,
    /// so by the time `collapse` pops the last statement the scope may have
    /// been merged away. Declining must also leave the scope where it was:
    /// `collapse` removes it from the map before popping.
    #[test]
    fn collapse_declines_and_restores_when_the_source_scope_is_empty() {
        let mut cfg = Cfg {
            blocks: BTreeMap::new(),
            entry: 0,
            exit: 3,
        };
        cfg.blocks.insert(
            0,
            CodeBlock {
                begin: 0,
                end: 0,
                next: 1,
                on_false: 2,
                condition: Some("t".to_string()),
            },
        );
        // Operand falls through to the rejoin, so the collapse gets as far as
        // popping the source scope's last statement.
        cfg.blocks.insert(
            1,
            CodeBlock {
                begin: 1,
                end: 1,
                next: 2,
                on_false: END,
                condition: None,
            },
        );
        cfg.blocks.insert(
            2,
            CodeBlock {
                begin: 2,
                end: 2,
                next: 3,
                on_false: END,
                condition: None,
            },
        );
        let mut scopes: BTreeMap<usize, Vec<Node>> = BTreeMap::new();
        scopes.insert(0, Vec::new());
        scopes.insert(
            1,
            vec![Node::assign(
                SYNTH_IP,
                Node::constant(SYNTH_IP, id("t")),
                Node::constant(SYNTH_IP, id("b")),
            )],
        );
        let mut pass = BoolPass {
            cfg: &mut cfg,
            scopes: &mut scopes,
            func_name: "EmptySource",
        };

        let collapsed = pass
            .collapse(0, "t", BoolOp::And)
            .expect("an empty source scope must decline, not error");
        assert!(!collapsed);
        assert!(
            pass.scopes.contains_key(&0),
            "the declined source scope must be put back, not left removed"
        );
        assert!(
            pass.cfg.blocks.contains_key(&1) && pass.scopes.contains_key(&1),
            "declining must not consume the operand block"
        );
    }

    /// #2667 — a self-referential rejoin edge. Merging it would run
    /// `blocks.remove(&rejoin_key)` on the very block just rewritten through
    /// `blocks.get_mut(&current)`, leaving every predecessor pointing at a
    /// hole. Sibling of `collapse_declines_when_operand_and_rejoin_keys_are_equal`.
    #[test]
    fn collapse_declines_when_the_rejoin_is_the_source_itself() {
        let mut cfg = Cfg {
            blocks: BTreeMap::new(),
            entry: 0,
            exit: 2,
        };
        cfg.blocks.insert(
            0,
            CodeBlock {
                begin: 0,
                end: 0,
                next: 1,
                // `&&` takes the false edge as the rejoin — point it at self.
                on_false: 0,
                condition: Some("t".to_string()),
            },
        );
        cfg.blocks.insert(
            1,
            CodeBlock {
                begin: 1,
                end: 1,
                next: 0,
                on_false: END,
                condition: None,
            },
        );
        let mut scopes: BTreeMap<usize, Vec<Node>> = BTreeMap::new();
        for key in [0usize, 1] {
            scopes.insert(
                key,
                vec![Node::assign(
                    SYNTH_IP,
                    Node::constant(SYNTH_IP, id("t")),
                    Node::constant(SYNTH_IP, id("a")),
                )],
            );
        }
        let mut pass = BoolPass {
            cfg: &mut cfg,
            scopes: &mut scopes,
            func_name: "SelfRejoin",
        };

        let collapsed = pass
            .collapse(0, "t", BoolOp::And)
            .expect("a self-referential rejoin must decline, not error");
        assert!(!collapsed);
        assert!(
            pass.cfg.blocks.contains_key(&0),
            "the source block must survive its own declined collapse"
        );
    }

    /// SCR-D2-01 (#1815) — an adversarial / malformed `.pex` that would nest
    /// short-circuit operands deeper than the cap is rejected with a
    /// `RecursionLimit` error rather than overflowing the stack. Mirrors
    /// `control_flow::rebuild_rejects_excessive_recursion_depth`: the depth
    /// guard fires before any CFG access, so a trivial pass exercises it.
    #[test]
    fn rebuild_rejects_excessive_recursion_depth() {
        let mut cfg = Cfg {
            blocks: BTreeMap::new(),
            entry: 0,
            exit: 0,
        };
        let mut scopes = BTreeMap::new();
        let mut pass = BoolPass {
            cfg: &mut cfg,
            scopes: &mut scopes,
            func_name: "Deep",
        };
        let err = pass
            .rebuild(0, 0, MAX_REBUILD_DEPTH + 1)
            .expect_err("over-deep recursion must error, not overflow");
        assert!(
            matches!(err, DecompileError::RecursionLimit { limit, .. } if limit == MAX_REBUILD_DEPTH),
            "got {err:?}"
        );
        // #2667 — and it must name *this* pass. The message used to hardcode
        // "control-flow reconstruction", so a short-circuit-chain overflow
        // sent triage to the other file.
        let text = err.to_string();
        assert!(
            text.contains("short-circuit boolean"),
            "a boolean-pass overflow must say so: {text}"
        );
        assert!(
            !text.contains("control-flow"),
            "and must not attribute itself to the control-flow pass: {text}"
        );
    }

    /// #3783 — the pass must not deep-clone a block's node scope before it
    /// knows the block is even conditional.
    ///
    /// `self.scopes.get(&current).cloned().unwrap_or_default()` used to sit
    /// above the `is_conditional()` test, so a straight-line function with
    /// no conditional block at all still paid a full recursive copy of every
    /// block's expression trees — once more per `reprocess` re-visit. On a
    /// deep tree `Node`'s derived `Clone` overflows the stack, and that is a
    /// `SIGABRT` `translate_pex`'s `catch_unwind` guard cannot intercept.
    ///
    /// Pinned by source inspection because the failure mode aborts the
    /// process rather than panicking — a behavioural test could not observe
    /// it and survive. Whitespace-insensitive so a reformat can't break it.
    #[test]
    fn the_scope_is_read_by_reference_not_deep_cloned() {
        let source = include_str!("boolean.rs");
        // Scan the production half only — this test's own assertion strings
        // quote the very pattern it forbids.
        let production = &source[..source
            .find("#[cfg(test)]")
            .expect("boolean.rs must retain its test module")];
        let normalized: String = production.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(
            !normalized.contains("self.scopes.get(&current).cloned()"),
            "rebuild must read the block scope by reference; a `.cloned()` here \
             is an unbounded recursive copy of every block's expression trees"
        );
        let guard = normalized
            .find("ifblock.is_conditional()")
            .expect("rebuild must gate on is_conditional()");
        let read = normalized
            .find("self.scopes.get(&current)")
            .expect("rebuild must still read the block scope");
        assert!(
            guard < read,
            "the block-shape test must run BEFORE the scope is touched"
        );
    }
}
