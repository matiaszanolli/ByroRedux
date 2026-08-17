# REG-2026-08-16-D5-01: Oblivion truncation gate is one-directional; parsed= never read back

**Issue**: #3082
**Severity**: MEDIUM
**Labels**: `medium,nif-parser,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_REGRESSION_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_REGRESSION_2026-08-16.md` (Dimension 5 — Green-by-construction guard sweep).

**Location**: `crates/nif/tests/block_coverage_baselines.rs`:125 and the Oblivion baseline files

## Description

The Oblivion truncation gate is **one-directional**, so five of its six baseline files are permanently un-guarded — and the `parsed=` count it writes is **never read back**.

## Evidence

```rust
// crates/nif/tests/block_coverage_baselines.rs:125 (re-verified 2026-08-17)
"# Oblivion sizeless-truncation baseline\ttruncating={}\tparsed={}\n",
```

The gate compares in one direction only, and `parsed=` is written to the baseline but never asserted against on a subsequent run — so a drop in parsed count cannot fail the test.

## Impact

A regression that reduces how many Oblivion blocks parse — while keeping the truncation count at or below baseline — passes silently. That is precisely the shape the v10.x stride-drift work exists to prevent.

Five of six baseline files being un-guarded means the gate covers a sixth of what it appears to.

## Suggested Fix

Make the comparison two-directional (fail on truncation increase **and** on parsed decrease), and assert `parsed=` against the recorded baseline rather than only writing it.

## Related

- #3041 (FNV-D5-01 — the parse-rate gate that covers one archive of nine; same "narrower than it looks" class)
- The v10.x stride-drift family this gate guards

## Completeness Checks
- [ ] **TWO-DIRECTIONAL**: A parsed-count decrease fails the gate
- [ ] **ALL-BASELINES**: All six baseline files are actually compared, not just one
- [ ] **READ-BACK**: `parsed=` is asserted, not merely written
- [ ] **TESTS**: Deliberately dropping a parse causes the gate to fail

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3082 --json state` when live state is needed.*
