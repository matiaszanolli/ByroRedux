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

mod cfg;

pub use cfg::{build_cfg, CodeBlock, Cfg, END};

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
}
