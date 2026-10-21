# #6216: SCR-2026-10-21-02: The VM returns every native numeric read as `Float` and stores locals untyped, so `IDiv` over an Int-returning native runs as float division

**Labels**: medium,scripting,bug

**Source**: docs/audits/AUDIT_SCRIPTING_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Fragment Dispatch & Locks (Papyrus VM) · **Location**: `crates/scripting/src/fragment/vm/interp.rs:1134-1139` (`classify_native_value` → `Ok(Value::Float(number))`), `:454-460` (`Stmt::VarDecl` inserts the evaluated value, ignoring `variable.ty`), `:520-527` (`assign` to a local stores untyped); `crates/scripting/src/fragment/vm/value.rs:359-385` (integer `/` only when both runtime values are `Value::Int`), `:72` (`Float` text is `{:.6}`); `crates/pex/src/decompile/lift.rs:125` (`IDiv | FDiv` → `"/"`)

**Audit status**: NEW

**Untrusted-Input**: No

## Description

The `.pex` carries the integer/float distinction in its opcodes. The decompiler erases it, so the VM must recover it from runtime value types. Natives the CK declares `Int` all come back as `Value::Float`: `GetStage`, `GetItemCount`, `GetValueInt`, `GetLevel`, `GetRelationshipRank`, `GetFactionRank`. A typed local (`Int i = GetStage()`) does not coerce on declaration or assignment.

## Evidence

- `Int i = GetStage()` at stage 15, then `i / 10`: the VM computes 1.5, Papyrus 1. `(i / 10) * 10` gives 15 against Papyrus's 10.
- `GetItemCount(x) / 2` and `GetValueInt() / 2` are float divisions.
- `"Count: " + GetItemCount(x)` gives `"Count: 3.000000"`, against Papyrus `"Count: 3"`.
- Script variables are coerced by declared type on read (`interp.rs:849-858`). Locals are not.
- `docs/engine/scripting.md` lists no such gap.

## Impact

A VM run reaches a different integer result or message text than the original. The run completes and the state it writes is wrong, which is the failure mode the VM's "run whole or not at all" contract exists to exclude.

## Related

SCR-2026-10-21-01; closed #5974 and #5982.

## Suggested Fix

Give each native value read its declared return type (`Value::Int(number as i32)` for the Int-returning table rows). Coerce `VarDecl` and local `assign` to the declared type: Int truncates, Float widens.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every Int-returning row of the native value table; locals declared via function params as well as VarDecl)
- [ ] **TESTS**: A regression test pins this specific fix
