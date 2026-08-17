# RT-2026-08-16-07: fo4 InstituteBioScience grew +18.0% entities and +14.9% DrawCommands

**Issue**: #3006
**Severity**: MEDIUM
**Dimension**: Telemetry baseline
**Labels**: `medium,performance,bug`
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_RUNTIME_2026-08-16.md` (runtime telemetry baseline diff).

**Location**: `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv`

## Description

`fo4 InstituteBioScience` moved `entities_total` **12448 → 14688 (+18.0%)**, far outside the ±2% tolerance band — and unlike the documented benign pattern for that metric (#1705), the rendering side moved **with** it rather than staying flat:

- `bench_draws_cmds` 3440 → 3954 (**+14.9%**), past its ×1.1 gate
- `skin_pool_live` 124 → 248 (**doubled**)

So the entity rise is **rendering, not bookkeeping**.

## Evidence

Measured by `/audit-runtime` against the checked-in baseline on 2026-08-16. Baseline file re-confirmed present 2026-08-17.

The distinguishing signal is that both halves of the split moved together — the benign pattern (#1705) is an entity-count rise with flat draw commands, which is not what happened here.

## Impact

An 18% entity rise with a 15% draw-command rise on a single FO4 interior is a real workload increase, not an accounting change. Doubling `skin_pool_live` additionally suggests more skinned entities are being resident than before, which has VRAM implications against the project's ~4 GB budget.

## Suggested Fix

Identify what began spawning or rendering additional entities in this cell. The FO4 precombine path (`cell_loader/precombined.rs`) and the M49 CSG slice are the likeliest candidates given the scene, but that is a starting point, **not a diagnosis** — measure before changing anything.

## Related

- #1705 (the benign entity-count pattern this case is explicitly *not*)
- #3005 (RT-2026-08-16-06 — the FNV/FO3 regression with a different shape)

## Completeness Checks
- [ ] **DIAGNOSED**: The entity source is identified before any re-baseline
- [ ] **VRAM**: The doubled `skin_pool_live` checked against the memory budget
- [ ] **SIBLING**: Other FO4 scenes checked for the same rise
- [ ] **TESTS**: The telemetry gate fails on this metric until resolved

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3006 --json state` when live state is needed.*
