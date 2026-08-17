# SAFE-2026-08-16-03: mod-runtime log budget is a lifetime cap with no drain

**Issue**: #3050
**Severity**: LOW
**Labels**: `low,safety,bug`
**Source report**: `docs/audits/AUDIT_SAFETY_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SAFETY_2026-08-16.md`.

**Location**: `crates/mod-runtime/src/runtime.rs`:263-301 (the `log` host fn), :187-189 (`logs()`), :229-249 (`enter`/`quarantine`)

## Description

mod-runtime's log budget is a **lifetime-total cap with no drain**, so exceeding it permanently quarantines an otherwise-healthy guest.

## Impact

A long-running, well-behaved mod that logs at any steady rate will eventually hit the lifetime cap and be quarantined — not for misbehaving, but for running long enough. The quarantine is the same mechanism used for actual faults, so the two become indistinguishable.

LOW because the crate has no engine consumer yet, but the semantics are wrong in a way that will only show up after long sessions, which is the hardest time to diagnose it.

## Suggested Fix

Make the budget a rate (drain on read via `logs()`, or a sliding window) rather than a lifetime total. A guest that has had its logs consumed should regain budget.

Keep a separate, genuinely fatal condition for runaway logging if one is wanted — but distinguish it from "has logged a lot over a long life".

## Related

- #3049 (SAFE-02), #3051 (SAFE-04) — same crate
- #2964 (UI-D2-01) — the mirror-image problem (an unbounded diagnostic channel); here the bound is real but never resets

## Completeness Checks
- [ ] **DRAINABLE**: `logs()` returning entries frees budget
- [ ] **DISTINGUISHABLE**: Log-budget quarantine is distinguishable from a fault quarantine
- [ ] **SIBLING**: Other lifetime-total budgets in the crate reviewed for the same shape
- [ ] **TESTS**: A test logs past the cap, drains, and asserts the guest stays healthy

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3050 --json state` when live state is needed.*
