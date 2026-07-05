**Severity**: LOW (documentation) · **Dimension**: ESM Record Coverage · **Game**: FO3
**Source**: `docs/audits/AUDIT_FO3_2026-07-05.md` (FO3-D3-002)

## Description
`crates/plugin/tests/parse_real_esm.rs::parse_rate_fo3_esm` asserts `index.total() >= FO3_TOTAL_FLOOR` with `FO3_TOTAL_FLOOR = 30_000`, but the explanatory comment above the assertion says "18,007 records observed on the GOTY master; `FO3_TOTAL_FLOOR` sits **slightly below**." 30,000 is 12k *above* 18,007 — mutually inconsistent. The "18,007" figure is a stale leftover (from `AUDIT_FO3_2026-04-19.md`, before the index grew to ~95 typed categories). `index.total()` sums the typed category maps (a subset of structured records) via `index.rs::total()` — which is also not the file's raw 37,459-structured / 44,657-total baseline. Three unreconciled numbers in one place.

## Evidence
- `crates/plugin/tests/parse_real_esm.rs` — `const FO3_TOTAL_FLOOR: usize = 30_000;` and the "18,007 records observed … sits slightly below" comment above `assert!(index.total() >= FO3_TOTAL_FLOOR)`.
- `crates/plugin/src/esm/.../index.rs::total()` — sums `Self::categories()` (typed-map records only), not the file's raw record count.

## Impact
No runtime effect — the assertion passes on live data. Documentation rot: a future auditor reading this test is told the FO3 parse yields 18,007 records while the floor already assumes ≥ 30,000, and neither is reconciled with the re-verified 44,657-total / 37,459-structured baseline.

## Related
The stale-baseline hazard class flagged in `/audit-fo3` ("do NOT use any older 13,684 structured figure").

## Suggested Fix
Update the comment to state what `index.total()` actually measures (sum of typed category maps, ≈ the value the 30,000 floor guards) and cite the re-verified 2026-05-26 figures (44,657 total = 37,459 structured + 7,198 NAVM) for the *file*, distinguishing them from `total()`. Comment/const-doc only.

## Completeness Checks
- [ ] **SIBLING**: Check the FNV/Skyrim `*_TOTAL_FLOOR` comments in the same test file for the same stale-baseline rot
- [ ] **TESTS**: N/A — the assertion already exists; this reconciles its prose
