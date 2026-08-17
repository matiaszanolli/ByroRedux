# SAFE-2026-08-16-02: SandboxConfig::validate enforces only lower bounds

**Issue**: #3049
**Severity**: MEDIUM
**Labels**: `medium,safety,bug`
**Source report**: `docs/audits/AUDIT_SAFETY_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SAFETY_2026-08-16.md`.

**Location**: `crates/mod-runtime/src/limits.rs`:38-78 (`validate`); consumed at `crates/mod-runtime/src/runtime.rs`:94-96
**Status note**: NEW — this dimension has **never been audited before** (added to the skill 2026-08-13, one day after the last safety audit).

## Description

`SandboxConfig::validate()` enforces only **lower** bounds. An oversized `max_wasm_stack_bytes` turns guest recursion into a host **process abort** — which wasmtime documents explicitly.

## Evidence

```rust
// crates/mod-runtime/src/limits.rs:38-78 (re-verified 2026-08-17)
if self.max_memory_bytes < 64 * 1024 { return Err(…) }
…
if self.max_wasm_stack_bytes == 0 { return Err(…) }     // only a zero check
```

Every check is a floor. There is no ceiling on `max_wasm_stack_bytes`, and wasmtime's documented behaviour when the configured wasm stack exceeds what the host thread can provide is to abort the process rather than trap the guest.

## Impact

`crates/mod-runtime` is the **trust boundary** between untrusted community WASM and the host — its entire purpose is that a hostile guest cannot take down the engine. A config that permits an unbounded wasm stack defeats that for the recursion case: the guest gets a host abort instead of a trap.

The crate has **no engine consumer yet**, so this is a contract defect rather than a live exploit — which is the right way to audit it per `_audit-common.md`.

## Suggested Fix

Add an upper bound on `max_wasm_stack_bytes` in `validate()`, below the host thread stack size the runtime actually allocates, so an over-large value is rejected at config time rather than aborting at call time.

Review the other limit fields for missing ceilings in the same pass — the floors-only pattern suggests it was systematic.

## Related

- #3050 (SAFE-03), #3051 (SAFE-04) — the same crate's other trust-boundary gaps
- `_audit-common.md`'s un-owned-subsystem table (Mod Runtime)

## Completeness Checks
- [ ] **CEILINGS**: Every limit field reviewed for a missing upper bound, not just the stack
- [ ] **TRAP-NOT-ABORT**: A hostile guest gets a trap or a rejected config, never a host abort
- [ ] **HOST-CONSISTENT**: The ceiling is derived from the host thread stack the runtime actually allocates
- [ ] **TESTS**: A test asserts an over-large `max_wasm_stack_bytes` is rejected by `validate()`

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3049 --json state` when live state is needed.*
