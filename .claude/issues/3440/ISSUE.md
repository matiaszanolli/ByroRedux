# #3440 — TD4-2026-08-27-03: AUDIT_TECH_DEBT_2026-08-24.md #[ignore] baseline is 171 where the real figure was 121

Labels: `low,tech-debt,doc-rot,documentation`
Filed: 2026-08-28 · Source report: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md`

---

**Severity**: LOW · **Dimension**: 4 — Audit-Finding Rot · **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md` (TD4-2026-08-27-03)

**Location**: `docs/audits/AUDIT_TECH_DEBT_2026-08-24.md` — the Baseline Snapshot block (`#[ignore] tests (*.rs only): 171`), repeated in that report's Executive Summary Dim-9 row and its "Premises investigated" section

## Description
The 08-24 report's Dim-9 narrative rests on the figure 171 ("was 154; the bare `.` recipe over the whole tree reads 503 — 313 are docs/markdown false hits"). The 503 and 313 figures are reproducible; **171 is not**. Measured at that report's own HEAD (`07a029ea`, the last commit of 2026-08-24) with the recipe the SKILL prescribes and the report says it used, the count is **121**. No variant of the recipe yields 171.

The section is explicitly labelled "for the next audit's diff", so the wrong number is sitting in the one place the next sweep is guaranteed to read it — and read against today's 126 it looks like a 45-test deletion.

## Evidence
```
$ git grep -h -E '^[[:space:]]*#\[ignore\]' 07a029ea -- '*.rs' | wc -l
121                                   # the SKILL's recipe, .rs only
$ git grep -h -E '^[[:space:]]*#\[ignore'  07a029ea -- '*.rs' | wc -l
149                                   # + the `= "reason"` form
$ git grep -h -E '#\[ignore'          07a029ea -- '*.rs' | wc -l
200                                   # unanchored
$ git grep -h -E '#\[ignore'          07a029ea             | wc -l
550                                   # whole repo (the report's own 503-class figure)

# Same commit, the other three baselines, for contrast — these reproduce:
markers: 20 (report: 20)   allow(dead_code): 68 (report: 69)
```

Live figure at HEAD `969d81c8`: 126 by the same recipe. The true movement is 121 → 126, ordinary growth.

## Impact
The 2026-08-27 sweep had to spend a measurement cycle disproving an apparent 45-test deletion before it could report a baseline. Left uncorrected the next sweep repeats that, or worse files a phantom "test coverage regression" finding. This is the failure mode the Baseline Snapshot section exists to prevent, occurring in the Baseline Snapshot section.

## Related
#2262 (CLOSED — the *other* `#[ignore]`-count recipe defect, a false 2.4× regression from whole-repo textual scanning; that one is real and the 08-24 report correctly identified it, which is what makes the un-reproducible number sitting beside it easy to miss). The `#[ignore = "reason"]` recipe blind spot filed alongside this report is a third, separate defect in the same figure.

## Suggested Fix
Amend the 08-24 report's Baseline Snapshot to `121` with a one-line note that the previously-published `171`/`154` pair is unreproducible, so the number is corrected at the place the next audit reads it rather than only in the 08-27 report.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other three baselines in that block reproduce; re-verify the prior `AUDIT_TECH_DEBT_*` snapshots for the same class)
