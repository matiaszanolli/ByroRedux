//! Phase-2 decompiler — lift a function's stack-based Papyrus bytecode
//! ([`Function::instructions`](crate::Function)) back to structured form.
//! A port of Champollion's `Decompiler/` (`PscCodeBlock` +
//! `PscDecompiler`), retargeted: where Champollion emits `.psc` text,
//! ByroRedux lowers to `byroredux_papyrus::ast::Script` (the final commit).
//!
//! Pipeline, built up across commits:
//!
//! 1. **`cfg`** — basic-block control-flow graph (this commit). Splits the
//!    flat instruction stream into blocks at jump boundaries and records
//!    each block's successor edges.
//! 2. *opcode → node-tree lifting + copy-propagation* (next).
//! 3. *control-flow + boolean-operator reconstruction* (next).
//! 4. *lower the node tree → `byroredux_papyrus::ast::Script`* (next).

mod boolean;
mod cfg;
mod control_flow;
mod event_names;
mod lift;
mod lower;
mod node;

pub use boolean::rebuild_boolean_operators;
pub use cfg::{build_cfg, CodeBlock, Cfg, END};
pub use control_flow::reconstruct;
pub use lift::lift_function;
pub use lower::decompile_script;
pub use node::{Node, NodeKind};

use thiserror::Error;

/// Why decompilation of a function body failed. Structural defects in the
/// bytecode that a well-formed `.pex` never exhibits.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DecompileError {
    /// A jump's relative target lands outside `0..=instruction_count`
    /// (the inclusive upper bound is the synthetic function-exit block).
    #[error("jump at instruction {ip} targets {target}, outside 0..={max}")]
    JumpOutOfRange { ip: usize, target: i64, max: usize },

    /// A `jmp`/`jmpf`/`jmpt` offset operand wasn't an integer.
    #[error("jump at instruction {ip} has a non-integer offset operand")]
    BadJumpOffset { ip: usize },

    /// A `jmpf`/`jmpt` condition operand wasn't an identifier, bool, or
    /// integer (the three forms Champollion accepts).
    #[error("conditional jump at instruction {ip} has an unsupported condition operand")]
    BadJumpCondition { ip: usize },

    /// An operand that must be an identifier (a destination, method name,
    /// or property name) was a literal instead.
    #[error("instruction {ip} expected an identifier operand")]
    ExpectedIdentifier { ip: usize },

    /// Copy-propagation found a temp consumed by more than one expression
    /// — the bytecode doesn't fit the single-use temp model.
    #[error("failed to rebuild expression in '{function}' at instruction {ip}")]
    ExpressionRebuildFailed { function: String, ip: usize },

    /// Control-flow reconstruction couldn't match the block graph to a
    /// structured shape (a malformed or unexpected jump topology).
    #[error("failed to rebuild control flow in '{function}'")]
    ControlFlowFailed { function: String },

    /// Control-flow reconstruction nested deeper than the recursion cap —
    /// a malformed / adversarial `.pex` (SAFE-2026-06-23-02). Bounded so an
    /// untrusted plugin can't blow the stack; the cap sits far above any
    /// real Papyrus nesting depth.
    #[error("control-flow reconstruction in '{function}' exceeded the recursion limit ({limit})")]
    RecursionLimit { function: String, limit: usize },

    /// The `.pex` carried no object to decompile into a script.
    #[error(".pex has no object to decompile")]
    EmptyPex,
}
