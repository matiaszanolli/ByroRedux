# Issue #487

FNV-5-F1: parse_real_nifs.rs MIN_SUCCESS_RATE=0.95 allows 5% silent regression vs 100% roadmap claim

---

## Severity: Low (CI gate drift)

**Location**: `crates/nif/tests/parse_real_nifs.rs:21`; same pattern at `crates/nif/examples/nif_stats.rs:22`

## Problem

```rust
const MIN_SUCCESS_RATE: f64 = 0.95;
```

Test asserts 95% success. ROADMAP.md:365,481 claims 100.00% parse rate for FNV. The gap means a regression dropping to 95.01% would not fail CI — up to ~744 NIFs could silently start failing without the test catching it.

The 100% figure is empirically true today (14881/14881 verified), but the gate is too loose to protect it.

## Fix

Add a vanilla-archive-specific constant:

```rust
const MIN_SUCCESS_RATE_VANILLA: f64 = 1.0;
const MIN_SUCCESS_RATE_MOD: f64 = 0.95;  // future: mod compatibility budget

// Detection: if archive path matches known vanilla BSA, use the tighter gate
```

Alternatively, raise `MIN_SUCCESS_RATE` to `1.0` and add explicit per-archive expected-failure lists if any drift is actually tolerable.

Same treatment for the `nif_stats.rs:22` exit-code gate.

## Completeness Checks

- [ ] **TESTS**: Run against vanilla FNV Meshes.bsa with gate at 1.0 — should still pass
- [ ] **SIBLING**: Apply to per-game tests (FO3/Oblivion/Skyrim/FO4/Starfield)
- [ ] **DOCS**: ROADMAP.md should match the CI gate value

Audit: `docs/audits/AUDIT_FNV_2026-04-20.md` (FNV-5-F1)
