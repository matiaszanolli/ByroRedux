# #3756: FO3-2026-08-30-D3-03: the parse_rate_fo3_esm baseline comment misstates index.total() as 31,101 (measures 44,666) and calls 44,657 the file record count (the file holds 718,952)

**Labels**: documentation, low, legacy-compat, game:fo3, esm-plugin, doc-rot, test-gap
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_FO3_2026-08-30.md` · **Severity**: LOW (doc-rot in a regression-gate rationale) · **Dimension**: 3 (ESM Record Coverage)
**Game affected**: Fallout 3

## Location
- `crates/plugin/tests/parse_real_esm.rs` — the `index.total()` baseline comment (currently `:1474-1482`) and the `FO3_TOTAL_FLOOR` const doc (`:52`)

## Description
The comment reads:

> `index.total()` … Observed 2026-04 on the GOTY master: 31,101; `FO3_TOTAL_FLOOR` (30,000) sits just below it … Distinct from the *file* baseline re-verified 2026-05-26: 44,657 total = 37,459 structured + 7,198 NAVM.

**Both halves are wrong:**

1. `index.total()` measures **44,666** today, not 31,101 — 43 % above the figure the 30,000 floor was reasoned from. The floor no longer "sits just below" anything: **the gate would still pass after losing a third of the index.**
2. The "*file* baseline … 44,657" is not a file quantity. The file holds **718,952 records** (validated against HEDR `numrec` 808,699 — records + GRUPs = 808,700, delta 1 = the TES4 header itself). 44,657 is the *same* index-sum metric measured on 2026-05-26 (44,657 then, 44,666 now), which is exactly what the sentence claims it is "distinct from".

Re-verified 2026-08-30: both the const doc ("Observed 2026-04: 31,101") and the inline comment are unchanged.

## Impact
The ROADMAP FO3 compat row and the `/audit-fo3` skill both carry "44,657 = 37,459 structured + 7,198 NAVMs" as *the FO3 ESM record count*. Read literally it understates the file by **16×**.

More usefully: the gate cannot detect a regression anywhere in the **663,824-record cell tree** (REFR / CELL / LAND / ACHR / ACRE / PGRE), because none of that tier except NAVM enters `total()` — which is precisely the tier the FO3 `PGRE`/`PROJ` defects live in (#3542 and the PROJ-MODL issue filed alongside this one).

`index.total()` also double-counts by design (`cells.statics` overlaps `items` / `activators` / …), which `EsmIndex::categories()`'s own doc states and this comment does not.

**Sibling nit in the same area**: `GameKind`'s enum doc at `crates/plugin/src/esm/reader.rs` says "Fallout 3 (HEDR 0.85)"; the installed GOTY master measures **0.94**, which the table 25 lines below states correctly.

## Related
#3542 and the FO3 PROJ-MODL issue (the cell tier this gate cannot see); #446/#447 (the category additions the stale figures predate).

## Suggested Fix
Re-cut the comment to "index-sum, observed 44,666 on 2026-08-30; double-counts the `cells.statics` overlap by design", raise `FO3_TOTAL_FLOOR` to ~44,000, and add a **separate cell-tier assertion** (placed refs ≥ 573,000, exterior cells ≥ 41,900) so a regression there is visible at all. Fix the `GameKind` HEDR 0.85 → 0.94 nit in the same pass.

## Completeness Checks
- [ ] **SIBLING**: the other per-game `*_TOTAL_FLOOR` consts were reasoned from the same era — check whether FNV/FO4/Skyrim floors have drifted the same way
- [ ] **TESTS**: the new cell-tier assertion is the fix; it must fail if the PGRE/PROJ tier regresses
