# 3287: REG-2026-08-24-02: translate_pex panic-catch (#1816) has no guard test

**Severity**: LOW · **Report**: `docs/audits/AUDIT_REGRESSION_2026-08-24.md` (REG-2026-08-24-02)

## Description

The `#1816` fix wraps `decompile_script` in `std::panic::catch_unwind`, confirmed present and correctly reasoned about. However, no test exercises the panic path itself — no fixture `.pex` trips an internal `.expect()` to assert `translate_pex` returns `None` instead of propagating a panic. A future refactor accidentally removing the wrapper would not be caught by CI.

## Location

`crates/scripting/src/translate/mod.rs:111-121`

## Evidence

`grep -rn "translate_pex.*panic\|panic.*translate_pex" crates/ byroredux/ --include='*.rs'` finds no test.

## Impact

Low — the fix is correct today; this is a hardening gap, not a live bug.

## Related

#1816 (the fix this guards, still correctly in place).

## Suggested Fix

Add a fixture `.pex` known to trip one of the cited `.expect()`s (`cfg.rs::split_block`, `control_flow.rs`, `lift.rs`, boolean-pass expects) and assert `translate_pex` returns `None` rather than unwinding.

## Completeness Checks
- [ ] **TESTS**: A fixture `.pex` that trips an internal `.expect()`, asserting `translate_pex` returns `None`
