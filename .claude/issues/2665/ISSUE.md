# #2665: SCR-D1-NEW11-01: FunctionInfo's field docstrings describe Champollion, not this codebase -- one asserts a decompiler safety guard that was deliberately removed

**Severity**: LOW
**Dimension**: PEX Reader & Opcode Decode (Dimension 1)
**Untrusted-Input**: No
**Location**: `crates/pex/src/model.rs:251` and `crates/pex/src/model.rs:253-255`
**Status**: NEW

## Description

Two adjacent doc comments on `FunctionInfo` state things that are false of ByroRedux (both are accurate descriptions of the upstream C++, which is presumably where they came from).

1. `line_numbers` is documented as being consumed by the boolean pass to avoid cross-line merges. **It is not.** `boolean.rs`'s own module doc calls out the *absence* of that guard as deliberate Champollion departure #1, and `line_numbers` has **zero readers anywhere in the workspace** -- it is parsed purely to keep the stream aligned.
2. `function_type` is documented as falling back to `Method` on an unknown byte. The reader maps unknown bytes to `None` (the field is `Option<FunctionType>` precisely so it can).

## Evidence

`crates/pex/src/model.rs:253-255`:

```rust
/// One source line per instruction -- the decompiler's boolean-operator
/// reconstruction uses these to avoid merging across source lines.
pub line_numbers: Vec<u16>,
```

vs `crates/pex/src/decompile/boolean.rs:17-22`:

```
//! 1. **No debug-line guard.** Champollion consults per-instruction source
//!    lines to reject merges that span lines. We rely on the structural
//!    pattern alone (the follow-block recomputing the condition variable),
//!    which is the load-bearing signal; the line check only suppresses
//!    rare false positives.
```

`grep -rn "line_numbers" --include="*.rs" crates/ byroredux/` returns only the five sites inside `reader.rs` that populate it, plus the `model.rs` declaration. No consumer.

`crates/pex/src/model.rs:251` says "`Method` when the byte is unknown"; `crates/pex/src/reader.rs:206-211` is `0 => Some(Method), 1 => Some(Getter), 2 => Some(Setter), _ => None`.

## Impact

Documentation only -- no runtime behaviour is affected and the decode itself is correct. The blast radius is future audit and maintenance accuracy on the single highest-scrutiny pass in this crate.

This is materially more than a typo now: the docstring advertises a false-positive-merge safety guard that does not exist, on exactly the pass where SCR-D3-NEW11-01 shows the absent guard has a real consequence (a `While` loop silently erased). A reader trusting `model.rs` instead of `boolean.rs` would conclude the structural signal is backed by a line check, and could dismiss a real cross-line merge bug as impossible -- or "restore" a guard believing the data is already wired up.

## Related

#2290 (same doc-rot class in `translate/source.rs`); SCR-D3-NEW11-01 (the live consequence of the guard the docstring claims exists)

## Suggested Fix

Two one-line edits. Rewrite `model.rs:253-255` to say what is true -- one source line per instruction, parsed for stream alignment and diagnostics, **currently unread** (the boolean pass deliberately relies on the structural signal alone; see `boolean.rs`'s departure #1). Change `model.rs:251` to "`None` when the byte is unknown".

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
