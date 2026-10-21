# #6268: SCR-2026-10-21-11: Doc rot in two touched functions

**Labels**: low,scripting,documentation,doc-rot

**Source**: docs/audits/AUDIT_SCRIPTING_2026-10-21.md · **Severity**: LOW · **Dimension**: Fragment Dispatch / Legacy ObScript · **Location**: `crates/scripting/src/fragment/reference.rs:629-641`; `crates/scripting/src/obscript_vm.rs:1286-1296`

**Audit status**: NEW

## Description

- `11685934d` inserted `fn raised` between `collect_deliveries`' doc block and the function. The "deliveries this frame's markers make…" doc now heads `raised`, and `collect_deliveries` has none. This is the same pattern as #6136.
- `resolve_ref_var`'s first paragraph still says "the VM models no ref-valued command, so a ref local is never assigned a real reference". The `ref_locals` paragraph under it contradicts that.

## Suggested Fix

Move `raised` above the doc block, and delete the stale paragraph.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other doc blocks displaced by helper insertions (#6136 pattern))
