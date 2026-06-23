# SCR-D3-01: control_flow ||-skip silently drops a conditional block's statements when the boolean pre-pass declines to collapse

Filed as: matiaszanolli/ByroRedux#1732
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: MEDIUM
- **Dimension**: Decompiler Control-Flow / Boolean / Lower · Untrusted-Input: Yes
- **Location**: `crates/pex/src/decompile/control_flow.rs:169-181`
- **Labels**: medium, legacy-compat, bug

## Description
When `reconstruct` reaches a conditional block whose `before` is itself conditional, no arm fires and `take_scope(current)` is never called, so the block's lifted statements (incl. the condition) are silently discarded. `boolean::take_operand` declines when the operand needs >1 statement, reaching this branch with a real guard. Invisible to corpus smoke (panic/Err only).

## Suggested Fix
In the conditional-with-conditional-predecessor branch, return `ControlFlowFailed` (force a clean upstream decline) rather than silently dropping the block.
