# PERF-D0-01: bench-of-record is past its 30-commit gate with no successor tracker

**Issue**: #3063
**Severity**: LOW
**Labels**: `low,performance,documentation`
**Source report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-16.md` (Dimension 0 — bench provenance).

**Location**: `ROADMAP.md`:137-175 (LIVE block) and `ROADMAP.md`:1090

## Description

The bench-of-record is **past its own 30-commit staleness gate with no successor tracker filed**. The report measured 34 commits past at the time of writing; re-checked 2026-08-17 the distance from the LIVE block's cited refresh commit to HEAD is substantially larger still.

## Evidence

`ROADMAP.md`:364 states the position explicitly: *"stale. Next tracker not yet filed; the 30-commit threshold and the …"*.

The precedent is recorded in the same file — `ROADMAP.md`:1090 documents **R6a-stale-9**, where the threshold tripped and a tracker *was* filed, listing the post-bench commits that touched the hot path. That process was not repeated this time.

## Impact

Every performance comparison in this sweep — including #3005 and #3006, the two draw-batch/entity regressions — is measured against a baseline nobody has re-established. A regression attributed to a recent change could equally be drift accumulated since the last refresh.

The staleness itself is expected and handled by design; the gap is that the **tracker was not filed**, so the condition is invisible rather than scheduled.

## Suggested Fix

File the successor staleness tracker (R6a-stale-N) following the R6a-stale-9 template at `ROADMAP.md`:1090 — list the post-bench commits that touched the hot path — and schedule the re-run. Byte-stability of `scripts/fsr-bench-matrix.sh` and `scripts/fsr_bench_report.py` is what makes the comparison valid, so flag any edit to either.

## Related

- #3005, #3006 (RT-06/07 — the two telemetry regressions measured against this baseline)
- `ROADMAP.md`:1090 (R6a-stale-9 — the template)

## Completeness Checks
- [ ] **TRACKER-FILED**: A successor R6a-stale-N entry exists with its post-bench commit list
- [ ] **HARNESS-STABLE**: `fsr-bench-matrix.sh` / `fsr_bench_report.py` unchanged, or the change is itself benched
- [ ] **DOWNSTREAM**: #3005/#3006 re-evaluated against the refreshed baseline
- [ ] **PATH-GATE**: `_audit-validate.sh` still passes after the ROADMAP edit

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3063 --json state` when live state is needed.*
