//! Basic-block control-flow graph over a function's instruction stream.
//!
//! Port of Champollion's `PscCodeBlock` + `PscDecompiler::createFlowBlocks`
//! (and `findBlockForInstruction`). A *block* is a maximal run of
//! straight-line instructions that ends at a jump (or the function end)
//! and begins at a jump target (or instruction 0). Blocks are identified
//! by the index of their first instruction.
//!
//! Two synthetic anchors match the C++ exactly:
//! - the **exit block**, keyed at `instruction_count` with `end == END`,
//!   is the target of any jump/fall-through that leaves the last real
//!   instruction. Pre-creating it means a jump at the final instruction
//!   never needs a split (its `ip + 1` already maps to a block).
//! - the initial **full block** `[0, count-1]` falls through to the exit.
//!
//! Edges: an unconditional block has [`CodeBlock::next`]; a conditional
//! block (carrying a [`CodeBlock::condition`]) branches to
//! [`CodeBlock::on_true`] / [`CodeBlock::on_false`]. `next` and `on_true`
//! are the same field — a conditional block's true edge *is* its `next`,
//! matching Champollion.

use std::collections::BTreeMap;

use crate::model::{Function, Value};
use crate::opcode::OpCode;

use super::DecompileError;

/// Sentinel block key / "no successor" marker
/// (Champollion `PscCodeBlock::END`).
pub const END: usize = usize::MAX;

/// One basic block — a contiguous instruction range plus its successor
/// edges. Instruction indices are inclusive (`begin..=end`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeBlock {
    /// First instruction index (also this block's key in [`Cfg::blocks`]).
    pub begin: usize,
    /// Last instruction index, inclusive. [`END`] for the exit block.
    pub end: usize,
    /// Unconditional successor (block key), or [`END`]. Also the *true*
    /// edge when [`Self::condition`] is set ([`Self::on_true`]).
    pub next: usize,
    /// *False* edge (block key) for a conditional block, else [`END`].
    pub on_false: usize,
    /// The identifier whose value selects the branch. `Some` ⇒ this block
    /// ends in a conditional jump. For a bool-literal jump this is
    /// `"true"`/`"false"`.
    pub condition: Option<String>,
}

impl CodeBlock {
    fn new(begin: usize, end: usize) -> Self {
        Self {
            begin,
            end,
            next: END,
            on_false: END,
            condition: None,
        }
    }

    /// True iff this block ends in a conditional jump.
    pub fn is_conditional(&self) -> bool {
        self.condition.is_some()
    }

    /// The true-branch successor (an alias of [`Self::next`], matching
    /// Champollion's `onTrue()`).
    pub fn on_true(&self) -> usize {
        self.next
    }

    /// Split this block at instruction `at`: truncate it to `[begin, at-1]`
    /// falling through to `at`, and return the new tail block `[at, end]`
    /// which inherits the old successor edges. Mirrors
    /// `PscCodeBlock::split` — the caller inserts the returned tail.
    fn split(&mut self, at: usize) -> CodeBlock {
        let tail = CodeBlock {
            begin: at,
            end: self.end,
            next: self.next,
            on_false: self.on_false,
            condition: self.condition.take(),
        };
        self.end = at - 1;
        self.next = at;
        self.condition = None;
        self.on_false = END;
        tail
    }
}

/// The control-flow graph: blocks keyed by their `begin` index. A
/// [`BTreeMap`] mirrors Champollion's `std::map` (key-ordered iteration,
/// which the later control-flow-reconstruction pass depends on).
#[derive(Debug, Clone)]
pub struct Cfg {
    pub blocks: BTreeMap<usize, CodeBlock>,
    /// Entry block key (`0` for a non-empty function, [`END`] when the
    /// function has no instructions — bodyless native/abstract functions
    /// aren't decompiled).
    pub entry: usize,
    /// `instruction_count` — also the key of the exit block.
    pub exit: usize,
}

impl Cfg {
    /// Look up a block by key.
    pub fn block(&self, key: usize) -> Option<&CodeBlock> {
        self.blocks.get(&key)
    }
}

/// Key of the block containing instruction `ip`
/// (Champollion `findBlockForInstruction`). Blocks are contiguous and
/// non-overlapping, so the block with the greatest `begin <= ip` is the
/// one — provided it also covers `ip`.
fn find_block_for_instruction(blocks: &BTreeMap<usize, CodeBlock>, ip: usize) -> usize {
    match blocks.range(..=ip).next_back() {
        Some((&begin, block)) if ip <= block.end => begin,
        _ => END,
    }
}

/// Split the block keyed `block_key` at instruction `at` and insert the
/// resulting tail. No-op precondition mirrors the C++: the caller only
/// calls this when no block already begins at `at`.
fn split_block(blocks: &mut BTreeMap<usize, CodeBlock>, block_key: usize, at: usize) {
    let tail = blocks
        .get_mut(&block_key)
        .expect("split target block exists")
        .split(at);
    blocks.insert(tail.begin, tail);
}

/// Resolve a `jmpf`/`jmpt` condition operand to the identifier the later
/// expression pass will look for. Identifiers pass through; bool literals
/// become `"true"`/`"false"`; an integer literal is stringified (a rare
/// constant-condition jump — Champollion routes these through its temp
/// table, which we don't model).
fn condition_name(arg: &Value, ip: usize) -> Result<String, DecompileError> {
    match arg {
        Value::Identifier(name) => Ok(name.clone()),
        Value::Bool(true) => Ok("true".to_string()),
        Value::Bool(false) => Ok("false".to_string()),
        Value::Integer(n) => Ok(n.to_string()),
        _ => Err(DecompileError::BadJumpCondition { ip }),
    }
}

/// Build the basic-block CFG for a function body
/// (Champollion `PscDecompiler::createFlowBlocks`, sans the
/// node-creation tail — that is the next pass).
///
/// Returns an empty CFG (`entry == END`) for a bodyless function.
pub fn build_cfg(function: &Function) -> Result<Cfg, DecompileError> {
    let instructions = &function.instructions;
    let count = instructions.len();

    if count == 0 {
        return Ok(Cfg {
            blocks: BTreeMap::new(),
            entry: END,
            exit: 0,
        });
    }

    let mut blocks: BTreeMap<usize, CodeBlock> = BTreeMap::new();
    // The whole body as one block, falling through to the exit anchor.
    let mut full = CodeBlock::new(0, count - 1);
    full.next = count;
    blocks.insert(0, full);
    // Exit anchor: begin == count, end == END.
    blocks.insert(count, CodeBlock::new(count, END));

    for (ip, ins) in instructions.iter().enumerate() {
        let block_key = find_block_for_instruction(&blocks, ip);
        match ins.op {
            OpCode::Jmp => {
                let offset = match ins.args.first() {
                    Some(Value::Integer(n)) => *n as i64,
                    _ => return Err(DecompileError::BadJumpOffset { ip }),
                };
                let target = checked_target(ip, offset, count)?;

                // End this block at the jump; the fall-through becomes a
                // fresh block (unless one already starts there).
                if !blocks.contains_key(&(ip + 1)) {
                    split_block(&mut blocks, block_key, ip + 1);
                }
                // Redirect this block to the jump target.
                blocks.get_mut(&block_key).expect("block exists").next = target;
                // Ensure the target begins a block.
                if !blocks.contains_key(&target) {
                    let containing = find_block_for_instruction(&blocks, target);
                    split_block(&mut blocks, containing, target);
                }
            }
            OpCode::JmpF | OpCode::JmpT => {
                let offset = match ins.args.get(1) {
                    Some(Value::Integer(n)) => *n as i64,
                    _ => return Err(DecompileError::BadJumpOffset { ip }),
                };
                let target = checked_target(ip, offset, count)?;
                let condition = condition_name(
                    ins.args.first().ok_or(DecompileError::BadJumpCondition { ip })?,
                    ip,
                )?;

                if !blocks.contains_key(&(ip + 1)) {
                    split_block(&mut blocks, block_key, ip + 1);
                }
                if !blocks.contains_key(&target) {
                    let containing = find_block_for_instruction(&blocks, target);
                    split_block(&mut blocks, containing, target);
                }

                let block = blocks.get_mut(&block_key).expect("block exists");
                // jmpf jumps to `target` when the condition is FALSE, so
                // the fall-through (ip+1) is the true edge; jmpt is mirror.
                let (on_true, on_false) = if ins.op == OpCode::JmpF {
                    (ip + 1, target)
                } else {
                    (target, ip + 1)
                };
                block.condition = Some(condition);
                block.next = on_true;
                block.on_false = on_false;
            }
            _ => {}
        }
    }

    Ok(Cfg {
        blocks,
        entry: 0,
        exit: count,
    })
}

/// `ip + offset`, validated to land in `0..=count` (the inclusive bound is
/// the exit anchor — a jump may target one past the last instruction).
fn checked_target(ip: usize, offset: i64, count: usize) -> Result<usize, DecompileError> {
    let target = ip as i64 + offset;
    if target < 0 || target > count as i64 {
        return Err(DecompileError::JumpOutOfRange {
            ip,
            target,
            max: count,
        });
    }
    Ok(target as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Instruction;

    /// Build a function from a list of `(op, args)` — enough to shape a CFG.
    fn func(instrs: Vec<(OpCode, Vec<Value>)>) -> Function {
        Function {
            instructions: instrs
                .into_iter()
                .map(|(op, args)| Instruction { op, args, var_args: Vec::new() })
                .collect(),
            ..Function::default()
        }
    }

    fn id(s: &str) -> Value {
        Value::Identifier(s.to_string())
    }

    #[test]
    fn bodyless_function_yields_empty_cfg() {
        let cfg = build_cfg(&Function::default()).unwrap();
        assert_eq!(cfg.entry, END);
        assert!(cfg.blocks.is_empty());
    }

    #[test]
    fn straight_line_is_one_block_plus_exit() {
        // assign a b ; assign c d ; return a   (no jumps)
        let f = func(vec![
            (OpCode::Assign, vec![id("a"), id("b")]),
            (OpCode::Assign, vec![id("c"), id("d")]),
            (OpCode::Return, vec![id("a")]),
        ]);
        let cfg = build_cfg(&f).unwrap();
        assert_eq!(cfg.entry, 0);
        assert_eq!(cfg.exit, 3);
        // One real block [0,2] → exit(3), plus the exit anchor.
        assert_eq!(cfg.blocks.len(), 2);
        let entry = cfg.block(0).unwrap();
        assert_eq!((entry.begin, entry.end), (0, 2));
        assert_eq!(entry.next, 3);
        assert!(!entry.is_conditional());
        let exit = cfg.block(3).unwrap();
        assert_eq!(exit.end, END);
    }

    #[test]
    fn forward_jmpf_builds_an_if_diamond() {
        // 0: cmp_eq t, a, b         (sets temp `t`)
        // 1: jmpf t, +3   -> target 4   (skip the body when false)
        // 2: <body stmt>
        // 3: <body stmt>
        // 4: return
        let f = func(vec![
            (OpCode::CmpEq, vec![id("t"), id("a"), id("b")]),
            (OpCode::JmpF, vec![id("t"), Value::Integer(3)]),
            (OpCode::Assign, vec![id("x"), id("y")]),
            (OpCode::Assign, vec![id("x"), id("z")]),
            (OpCode::Return, vec![id("x")]),
        ]);
        let cfg = build_cfg(&f).unwrap();

        // Block 0 = [0,1], conditional on `t`: true→2 (body), false→4 (exit-of-if).
        let b0 = cfg.block(0).unwrap();
        assert_eq!((b0.begin, b0.end), (0, 1));
        assert!(b0.is_conditional());
        assert_eq!(b0.condition.as_deref(), Some("t"));
        assert_eq!(b0.on_true(), 2);
        assert_eq!(b0.on_false, 4);

        // Body block [2,3] falls through to 4.
        let body = cfg.block(2).unwrap();
        assert_eq!((body.begin, body.end), (2, 3));
        assert_eq!(body.next, 4);
        assert!(!body.is_conditional());

        // Join block [4,4] → exit(5).
        let join = cfg.block(4).unwrap();
        assert_eq!((join.begin, join.end), (4, 4));
        assert_eq!(join.next, 5);
    }

    #[test]
    fn backward_jmpt_builds_a_loop_edge() {
        // 0: <loop head / cond compute>
        // 1: jmpf t, +3   -> 4 (exit loop when false)
        // 2: <body>
        // 3: jmp -3       -> 0 (back to head)
        // 4: return
        let f = func(vec![
            (OpCode::CmpEq, vec![id("t"), id("i"), id("n")]),
            (OpCode::JmpF, vec![id("t"), Value::Integer(3)]),
            (OpCode::Assign, vec![id("x"), id("y")]),
            (OpCode::Jmp, vec![Value::Integer(-3)]),
            (OpCode::Return, vec![id("x")]),
        ]);
        let cfg = build_cfg(&f).unwrap();

        // Conditional head block [0,1]: true→2 (body), false→4 (after loop).
        let head = cfg.block(0).unwrap();
        assert_eq!((head.begin, head.end), (0, 1));
        assert_eq!(head.condition.as_deref(), Some("t"));
        assert_eq!(head.on_true(), 2);
        assert_eq!(head.on_false, 4);

        // Body block [2,3] jumps back to the head (key 0) — the loop edge.
        let body = cfg.block(2).unwrap();
        assert_eq!((body.begin, body.end), (2, 3));
        assert_eq!(body.next, 0);
        assert!(!body.is_conditional());

        // After-loop block [4,4] → exit(5).
        assert_eq!(cfg.block(4).unwrap().next, 5);
    }

    #[test]
    fn jump_out_of_range_is_an_error() {
        let f = func(vec![(OpCode::Jmp, vec![Value::Integer(99)])]);
        assert!(matches!(
            build_cfg(&f),
            Err(DecompileError::JumpOutOfRange { .. })
        ));
    }

    #[test]
    fn non_integer_jump_offset_is_an_error() {
        let f = func(vec![(OpCode::Jmp, vec![id("notanint")])]);
        assert!(matches!(
            build_cfg(&f),
            Err(DecompileError::BadJumpOffset { .. })
        ));
    }
}
