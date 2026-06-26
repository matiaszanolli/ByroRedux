//! Control-flow reconstruction — turn the basic-block graph + per-block
//! scopes into a structured statement tree (`If`/`ElseIf`/`Else`/`While`).
//! Port of Champollion's `PscDecompiler::rebuildControlFlow`.
//!
//! The shape of each structure is read off the block edges:
//! - a conditional block whose pre-false-exit block jumps **back** to it is
//!   a `While`;
//! - one whose pre-false-exit block falls through to the false target is a
//!   simple `If`;
//! - otherwise it's an `If`/`Else`, the else running from the false target
//!   to wherever the if-body's tail jumps.
//!
//! A leading condition inversion handles the `jmpt`-shaped case (the false
//! branch is the immediate fall-through) by negating the condition and
//! swapping the edges — exactly as the C++ does.
//!
//! After the tree is built, the same copy-propagation used per-block
//! ([`super::lift`]) runs once over the result so each block's trailing
//! comparison folds into the `If`/`While` condition placeholder.
//!
//! ## Known gap (intentional, for this commit)
//!
//! Short-circuit boolean collapse (`rebuildBooleanOperators`) is **not yet
//! ported**. `&&`-style conditions still reconstruct correctly — as nested
//! `If`s — but `||`-style short-circuits (where the block before the false
//! exit is *itself* conditional) are not folded and that block is skipped,
//! matching the C++ control-flow pass run without its boolean pre-pass.
//! The boolean pass lands in a following commit, validated against the
//! decompiler fidelity gate.

use std::collections::BTreeMap;

/// Recursion cap for [`Reconstructor::rebuild`] (SAFE-2026-06-23-02). Each
/// nested `If`/`Else`/`While` body recurses once; real Papyrus nests a
/// handful deep, so 1024 is far above any well-formed `.pex` while still
/// bounding an adversarial / malformed one before it can overflow the stack.
const MAX_REBUILD_DEPTH: usize = 1024;

use super::cfg::{Cfg, END};
use super::lift::rebuild_expression;
use super::node::Node;
use super::DecompileError;
use crate::model::Value;

/// Reconstruct a function's structured statement tree from its CFG and the
/// lifted per-block scopes. Consumes both (scopes are drained into the
/// tree; the CFG's condition edges are mutated by inversion).
pub fn reconstruct(
    cfg: Cfg,
    scopes: BTreeMap<usize, Vec<Node>>,
    func_name: &str,
) -> Result<Vec<Node>, DecompileError> {
    if cfg.entry == END {
        return Ok(Vec::new()); // bodyless function
    }
    let entry = cfg.entry;
    let exit = cfg.exit;
    let mut r = Reconstructor { cfg, scopes, func_name: func_name.to_string() };
    r.rebuild(entry, exit, 0)
}

struct Reconstructor {
    cfg: Cfg,
    scopes: BTreeMap<usize, Vec<Node>>,
    func_name: String,
}

impl Reconstructor {
    fn fail(&self) -> DecompileError {
        DecompileError::ControlFlowFailed { function: self.func_name.clone() }
    }

    /// Drain a block's lifted scope (Champollion `mergeChildren` clears the
    /// source).
    fn take_scope(&mut self, key: usize) -> Vec<Node> {
        self.scopes.remove(&key).unwrap_or_default()
    }

    /// The block key just before instruction-anchor `exit` (i.e. the block
    /// containing `exit - 1`), or `END`/`Err` for the degenerate `exit == 0`.
    fn before_exit(&self, exit: usize) -> usize {
        if exit == 0 {
            END
        } else {
            self.cfg.find_block(exit - 1)
        }
    }

    /// Rebuild the structured statements for the block range
    /// `[start, end)` (end is the stop-anchor block key, exclusive).
    ///
    /// `depth` bounds nested-body recursion against a malformed / adversarial
    /// `.pex` (SAFE-2026-06-23-02) — see [`MAX_REBUILD_DEPTH`].
    fn rebuild(&mut self, start: usize, end: usize, depth: usize) -> Result<Vec<Node>, DecompileError> {
        if depth > MAX_REBUILD_DEPTH {
            return Err(DecompileError::RecursionLimit {
                function: self.func_name.clone(),
                limit: MAX_REBUILD_DEPTH,
            });
        }
        if end < start {
            return Err(self.fail());
        }
        let mut result: Vec<Node> = Vec::new();
        let mut it = start;
        while it != end {
            let current = it;
            let block = self.cfg.block(current).cloned().ok_or_else(|| self.fail())?;
            let mut jump_to: Option<usize> = None;

            if block.is_conditional() {
                let cond_name = block.condition.clone().expect("conditional block has a condition");
                let mut on_true = block.on_true();
                let mut on_false = block.on_false;
                let mut exit = on_false;
                let mut before = self.before_exit(exit);
                if before == END {
                    return Err(self.fail());
                }

                // The condition placeholder; the trailing comparison folds
                // into it during the final rebuild_expression.
                let mut condition = Node::constant(
                    super::node::SYNTH_IP,
                    Value::Identifier(cond_name.clone()),
                );

                // jmpt shape: the block before the false exit is *this*
                // block ⇒ the false branch is the immediate fall-through.
                // Negate the condition and swap the edges.
                if before == current {
                    condition = Node::unary_op(
                        super::node::SYNTH_IP,
                        10,
                        Some(cond_name.clone()),
                        "!",
                        condition,
                    );
                    {
                        let b = self.cfg.blocks.get_mut(&current).expect("block exists");
                        // setCondition(cond, onFalse, onTrue): swap edges.
                        b.next = on_false;
                        b.on_false = on_true;
                    }
                    let b = self.cfg.block(current).expect("block exists");
                    on_true = b.on_true();
                    on_false = b.on_false;
                    exit = on_false;
                    before = self.before_exit(exit);
                    if before == END {
                        return Err(self.fail());
                    }
                }

                let last = self.cfg.block(before).cloned().ok_or_else(|| self.fail())?;

                if !last.is_conditional() && last.next == current {
                    // While: the body's tail jumps back to the condition.
                    let (body_start, body_end) = (on_true, on_false);
                    result.extend(self.take_scope(current));
                    let body = self.rebuild(body_start, body_end, depth + 1)?;
                    result.push(Node::while_node(condition, body));
                    jump_to = Some(body_end);
                } else if !last.is_conditional() {
                    if last.next == exit {
                        // Simple If.
                        let (body_start, body_end) = (on_true, on_false);
                        result.extend(self.take_scope(current));
                        let body = self.rebuild(body_start, body_end, depth + 1)?;
                        result.push(Node::if_else(condition, body, Vec::new()));
                        jump_to = Some(body_end);
                    } else {
                        // If / Else.
                        let if_start = on_true;
                        let else_start = on_false;
                        let end_else = last.next;
                        result.extend(self.take_scope(current));
                        let if_body = self.rebuild(if_start, else_start, depth + 1)?;
                        let else_body = self.rebuild(else_start, end_else, depth + 1)?;
                        result.push(Node::if_else(condition, if_body, else_body));
                        jump_to = Some(end_else);
                    }
                }
                // else: `last` is conditional — the short-circuit (||) case
                // the boolean pass handles. Left unmerged, advance by one,
                // matching the C++ control-flow pass without that pre-pass.
            } else {
                // Unconditional block: splice its statements into the result.
                result.extend(self.take_scope(current));
            }

            it = match jump_to {
                Some(k) => k,
                None => self.cfg.next_key(current).unwrap_or(end),
            };
        }

        rebuild_expression(&mut result, &self.func_name)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::super::cfg::build_cfg;
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

    /// SAFE-2026-06-23-02 — an adversarial / malformed `.pex` that would nest
    /// deeper than the cap is rejected with a `RecursionLimit` error rather
    /// than overflowing the stack. The depth guard fires before any CFG
    /// access, so a trivial reconstructor exercises it.
    #[test]
    fn rebuild_rejects_excessive_recursion_depth() {
        let mut r = Reconstructor {
            cfg: Cfg {
                blocks: BTreeMap::new(),
                entry: 0,
                exit: 0,
            },
            scopes: BTreeMap::new(),
            func_name: "Deep".to_string(),
        };
        let err = r
            .rebuild(0, 0, MAX_REBUILD_DEPTH + 1)
            .expect_err("over-deep recursion must error, not overflow");
        assert!(
            matches!(err, DecompileError::RecursionLimit { limit, .. } if limit == MAX_REBUILD_DEPTH),
            "got {err:?}"
        );
    }

    /// Full pipeline for a single function: cfg → lift → reconstruct.
    fn decompile(function: Function) -> Vec<Node> {
        let object = Object::default();
        let cfg = build_cfg(&function).unwrap();
        let scopes = lift_function(&object, &function, &cfg).unwrap();
        reconstruct(cfg, scopes, &function.name).unwrap()
    }

    #[test]
    fn simple_if_reconstructs() {
        // if (a == b)
        //     x = 1
        // <after>
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
        // [ IfElse(a == b, [ x = 1 ]), Return ]
        let if_node = tree.iter().find(|n| matches!(n.kind, NodeKind::IfElse { .. })).unwrap();
        let NodeKind::IfElse { condition, body, else_body, .. } = &if_node.kind else { panic!() };
        assert!(matches!(&condition.kind, NodeKind::BinaryOp { op, .. } if op == "=="));
        assert!(else_body.is_empty(), "simple if has no else");
        assert_eq!(body.len(), 1);
        assert!(matches!(&body[0].kind, NodeKind::Assign { .. }));
    }

    #[test]
    fn if_else_reconstructs_both_branches() {
        // if (a == b)        ip0: cmp ; ip1: jmpf +3 (to else at 4)
        //     x = 1          ip2: assign x,1
        //                    ip3: jmp +3 (to end at 6)
        // else               ip4: assign x,2
        //     x = 2          ip5: jmp +1 (to end at 6)   [tail of else]
        // <end>              ip6: return
        let f = Function {
            return_type_name: "None".into(),
            locals: vec![local("::temp0", "Bool"), local("x", "Int")],
            instructions: vec![
                ins(OpCode::CmpEq, vec![id("::temp0"), id("a"), id("b")]),
                ins(OpCode::JmpF, vec![id("::temp0"), Value::Integer(3)]),
                ins(OpCode::Assign, vec![id("x"), Value::Integer(1)]),
                ins(OpCode::Jmp, vec![Value::Integer(3)]),
                ins(OpCode::Assign, vec![id("x"), Value::Integer(2)]),
                ins(OpCode::Jmp, vec![Value::Integer(1)]),
                ins(OpCode::Return, vec![id("::NoneVar")]),
            ],
            ..Function::default()
        };
        let tree = decompile(f);
        let if_node = tree.iter().find(|n| matches!(n.kind, NodeKind::IfElse { .. })).unwrap();
        let NodeKind::IfElse { body, else_body, .. } = &if_node.kind else { panic!() };
        assert_eq!(body.len(), 1, "if body");
        assert_eq!(else_body.len(), 1, "else body");
    }

    #[test]
    fn while_loop_reconstructs() {
        // while (a == b)     ip0: cmp ; ip1: jmpf +3 (exit to 4)
        //     foo()          ip2: callmethod foo
        //                    ip3: jmp -3 (back to 0)
        // <after>            ip4: return
        let f = Function {
            return_type_name: "None".into(),
            locals: vec![local("::temp0", "Bool")],
            instructions: vec![
                ins(OpCode::CmpEq, vec![id("::temp0"), id("a"), id("b")]),
                ins(OpCode::JmpF, vec![id("::temp0"), Value::Integer(3)]),
                ins_v(OpCode::CallMethod, vec![id("foo"), id("self"), id("::NoneVar")], vec![]),
                ins(OpCode::Jmp, vec![Value::Integer(-3)]),
                ins(OpCode::Return, vec![id("::NoneVar")]),
            ],
            ..Function::default()
        };
        let tree = decompile(f);
        let while_node = tree.iter().find(|n| matches!(n.kind, NodeKind::While { .. })).unwrap();
        let NodeKind::While { condition, body } = &while_node.kind else { panic!() };
        assert!(matches!(&condition.kind, NodeKind::BinaryOp { op, .. } if op == "=="));
        assert_eq!(body.len(), 1);
        assert!(matches!(&body[0].kind, NodeKind::CallMethod { .. }));
    }

    #[test]
    fn nested_and_becomes_nested_ifs() {
        // if (a) ; if (b)   →  nested Ifs (the && pass is a later commit)
        //     x = 1
        // ip0: jmpf a,+4 (to 5)
        // ip1: jmpf b,+3 (to 5)
        // ip2: assign x,1
        // ... return at 5 (jmpf offsets land on the return)
        let f = Function {
            return_type_name: "None".into(),
            locals: vec![local("x", "Int")],
            instructions: vec![
                ins(OpCode::JmpF, vec![id("a"), Value::Integer(4)]),
                ins(OpCode::JmpF, vec![id("b"), Value::Integer(3)]),
                ins(OpCode::Assign, vec![id("x"), Value::Integer(1)]),
                ins(OpCode::Return, vec![id("::NoneVar")]),
            ],
            ..Function::default()
        };
        let tree = decompile(f);
        let outer = tree.iter().find(|n| matches!(n.kind, NodeKind::IfElse { .. })).unwrap();
        let NodeKind::IfElse { condition, body, .. } = &outer.kind else { panic!() };
        // outer condition is the bare identifier `a`
        assert!(matches!(&condition.kind, NodeKind::Constant(Value::Identifier(s)) if s == "a"));
        // body contains the inner if on `b`
        let inner = body.iter().find(|n| matches!(n.kind, NodeKind::IfElse { .. })).unwrap();
        let NodeKind::IfElse { condition: inner_cond, .. } = &inner.kind else { panic!() };
        assert!(matches!(&inner_cond.kind, NodeKind::Constant(Value::Identifier(s)) if s == "b"));
    }

    #[test]
    fn straight_line_has_no_control_flow_nodes() {
        let f = Function {
            return_type_name: "None".into(),
            locals: vec![local("x", "Int")],
            instructions: vec![
                ins(OpCode::Assign, vec![id("x"), Value::Integer(1)]),
                ins(OpCode::Return, vec![id("::NoneVar")]),
            ],
            ..Function::default()
        };
        let tree = decompile(f);
        assert!(tree.iter().all(|n| !matches!(
            n.kind,
            NodeKind::IfElse { .. } | NodeKind::While { .. }
        )));
    }
}
