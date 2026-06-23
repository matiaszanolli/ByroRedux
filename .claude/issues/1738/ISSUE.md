# SCR-D3-02: Stale Known-gap doc-rot — control_flow.rs claims the boolean pass is unported

Filed as: matiaszanolli/ByroRedux#1738
Source audit: `docs/audits/AUDIT_SCRIPTING_2026-06-23.md`

- **Severity**: LOW
- **Dimension**: Decompiler Control-Flow / Boolean / Lower
- **Location**: `crates/pex/src/decompile/control_flow.rs:21-29`, `mod.rs:6-14`
- **Labels**: low, legacy-compat, documentation

## Description
control_flow.rs §"Known gap" states short-circuit boolean collapse "is not yet ported"; it HAS shipped (`rebuild_boolean_operators` wired in `decompile_body`, lower.rs:216). `mod.rs:7-14` similarly tags passes 2-4 "(next)".

## Suggested Fix
Rewrite both docstrings; reference the residual SCR-D3-01 behaviour instead of "not yet ported."
