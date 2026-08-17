# SAFE-2026-08-16-04: no hostile-input test for SandboxRuntime::compile

**Issue**: #3051
**Severity**: LOW
**Labels**: `low,safety,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_SAFETY_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SAFETY_2026-08-16.md`.

**Location**: `crates/mod-runtime/src/runtime.rs`:115-126; test module `crates/mod-runtime/src/tests.rs`

## Description

**No test feeds `SandboxRuntime::compile` hostile non-wasm bytes**, and there is no bound on compilation cost for an in-limit adversarial component.

## Impact

`compile` is the first thing untrusted bytes touch. Two gaps:

1. **No negative-input coverage** — nothing asserts that garbage, truncated, or malformed-but-plausible input produces a clean `Err` rather than a panic.
2. **No compilation-cost bound** — a component that satisfies every size limit can still be structured to make compilation expensive, and nothing caps that.

The crate is a trust boundary with no engine consumer yet, so this is contract coverage rather than a live exploit — but it is the entry point, and it is the least-tested part of it.

## Suggested Fix

Add negative-input tests (garbage bytes, truncated headers, valid-wasm-invalid-component) asserting `Err` and no panic. Separately, apply wasmtime's compilation fuel/limits so an in-size adversarial component cannot consume unbounded compile time.

## Related

- #3049 (SAFE-02), #3050 (SAFE-03) — same crate's other trust-boundary gaps
- #3014 (SCR-D8-04) — the same "parser of untrusted input with no negative-input coverage" shape in `crates/hkx`

## Completeness Checks
- [ ] **NEGATIVE-INPUT**: Tests cover garbage, truncated and malformed-plausible bytes
- [ ] **NO-PANIC**: Every rejection is an `Err`, never a panic across the trust boundary
- [ ] **COMPILE-BOUND**: Compilation cost is bounded for an in-limit component
- [ ] **TESTS**: The new tests fail if `compile` is made to panic on bad input

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3051 --json state` when live state is needed.*
