# SCR-D6-2026-09-22-01: obscript_vm.rs's If-chain and decode_args recursion have no depth cap — mod-supplied SCDA bytecode can abort the process

**Issue**: #4750
**Filed**: 2026-09-22 (audit-publish, AUDIT_SCRIPTING_2026-09-22.md)

**Severity**: HIGH
**Dimension**: Legacy ObScript Execution
**Location**: `crates/scripting/src/obscript_vm.rs::Vm::exec_if_chain` (`:288-317`) ↔ `Vm::run_arm_body` (`:322-344`); `Vm::decode_args` (`:557-643`, the `b'X'` arm at `:621-633` recursing into itself at `:629`, reachable from any ordinary statement call via `run_or_skip_statement`, no `If` required). Contrast: `crates/scripting/src/obscript_runtime.rs:41` `MAX_LEGACY_OBSCRIPT_NESTING: usize = 32`, enforced at `:290`/`:523` with a passing regression test (`source_compiler_rejects_excessive_nesting`).

## Description
The new M47.3 ObScript bytecode interpreter (`crates/scripting/src/obscript_vm.rs`, landed `6229e7d6e`) has no recursion-depth cap on either of its two recursive routines: nested `If`/`ElseIf` blocks via mutually-recursive `exec_if_chain`/`run_arm_body`, and nested command-call arguments via `decode_args`'s self-recursive `'X'` handling. Its sibling interpreter, `obscript_runtime.rs`, already carries `MAX_LEGACY_OBSCRIPT_NESTING = 32`, added proactively when that older interpreter gained `If` support — that precedent was not carried into the new VM. Both nested shapes are legal and arbitrarily deep in the on-disk compiled-script format; a Rust stack overflow from unbounded recursion is an uncatchable process abort.

## Evidence
Verified against HEAD `c3f298a24`: `MAX_LEGACY_OBSCRIPT_NESTING` exists only in `obscript_runtime.rs`, zero matches in `obscript_vm.rs`; `exec_if_chain`/`run_arm_body` mutually recurse with no depth parameter; `decode_args`'s `'X'` arm recurses into itself untracked. No nesting-depth test exists for either routine. Minimum per-level on-disk cost is under 20 bytes for either vector, so a few KB of crafted `SCDA` reaches hundreds of nesting levels.

## Impact
A single malicious or corrupted quest script's plugin crashes the engine process with no fallback, on the routine 5-second quest tick that drives every running Oblivion/FO3/FNV quest's `SCDA` bytecode.

## Related
`obscript_runtime.rs`'s own `MAX_LEGACY_OBSCRIPT_NESTING` (same crate, same class, already fixed there); #1712, #1815, #3783 (prior uncatchable-abort findings, `.pex`/`.psc` frontends); SCR-D1-2026-09-14-01 (same failure shape, different mechanism, already fixed)

## Suggested Fix
Add `MAX_OBSCRIPT_VM_NESTING` (32, matching the sibling, is a reasonable start) and thread a depth counter through both `exec_if_chain`↔`run_arm_body` and `decode_args`'s `'X'` self-call, returning a decline/`None` past the cap. Add two regression tests mirroring `obscript_runtime::tests::source_compiler_rejects_excessive_nesting`.
