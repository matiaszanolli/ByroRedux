# Issue #483

FNV-2-L3: Tautological d.len() >= 20 inside outer >= 28 gate in XCLL fog path

---

## Severity: Low (dead code)

**Location**: `crates/plugin/src/esm/cell.rs:587`

## Problem

```rust
if sub.data.len() >= 28 {
    // ...
    if d.len() >= 20 { /* read fog fields */ }
    else { /* unreachable */ }
}
```

The inner `d.len() >= 20` check is always true inside the outer `>= 28` block. Dead conditional; the `else` branch is unreachable. Currently no functional bug — FNV's 40-byte XCLL correctly fills fog_color/fog_near/fog_far.

## Impact

Code hygiene only. Misleads anyone reading the XCLL parse logic.

## Fix

Drop the conditional; always read fog_color / fog_near / fog_far inline. Pulls three lines out of the 28-byte block and clarifies the branching.

## Completeness Checks

- [ ] **TESTS**: Existing XCLL parser tests pass unchanged
- [ ] **SIBLING**: Audit XCLL for other dead-condition guards

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-2-L3)
