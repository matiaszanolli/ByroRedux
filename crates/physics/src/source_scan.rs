//! Helpers for this crate's source-scan tests — tests that `include_str!` a
//! source file and assert on its text because the property they pin (a phase
//! ordering, a fast-path gate, a doc claim) needs a running world to exercise.
//!
//! The helper itself lives in `byroredux-core::source_scan` (#5100); this
//! module re-exports it so the crate's scans keep one import path.

pub(crate) use byroredux_core::source_scan::production_text;
