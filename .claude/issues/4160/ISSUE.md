# NIF-D2-2026-09-11-04: bsver::OBLIVION has no production consumer

URL: https://github.com/matiaszanolli/ByroRedux/issues/4160
Labels: bug, nif-parser, low, tech-debt, nif

---

**Severity**: LOW
**Dimension**: 2 — Version Gating
**Location**: `crates/nif/src/version.rs:401-404`
**Status**: NEW

**Description**: `bsver::OBLIVION` has no production (non-test) consumer — the same dead-helper class removed twice already (#1511/#1840/#1897), narrower this time since it's at least test-fixture-exercised.

**Evidence**: `grep -rn "bsver::OBLIVION" crates/nif/src/` finds exactly one hit, `header.rs:92`, inside `impl NifHeader` gated `#[cfg(test)]` (`NifHeader::test_oblivion()`).

**Impact**: None functionally — dead in production code, but risks the same repeated "is this dead?" investigation cost the prior two removals (#1511/#1840/#1897) already paid.

**Suggested Fix**: Either wire a real production consumer, or explicitly document `bsver::OBLIVION` as test-fixture-only so a future dead-code sweep doesn't re-litigate it.

## Completeness Checks
- [ ] **SIBLING**: Cross-check other `bsver::*` constants for the same test-only-consumer shape

