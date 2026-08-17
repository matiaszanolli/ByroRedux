# RT-2026-08-16-04: no CI job runs any smoke gate, and all three exit 0 when Skyrim data is absent

**Issue**: #3003
**Severity**: MEDIUM
**Dimension**: Gate execution
**Labels**: `medium,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_RUNTIME_2026-08-16.md`.

**Location**: `.github/workflows/ci.yml` · `docs/smoke-tests/p0-door-interaction.sh`:41-43 · `p1-character-traversal.sh`:46-48 · `p2-melee-core.sh`:45-48

## Description

**No CI job runs any smoke gate**, and all three scripts `exit 0` when the required game data is absent — so an absent-data run is indistinguishable from a pass.

## Evidence

```bash
# p2-melee-core.sh:45-48
if [[ ! -f "$required" ]]; then
    echo "smoke[p2-melee-core]: SKIP -- missing $required"
    exit 0
fi
```

Re-verified 2026-08-17: `.github/workflows/ci.yml` exists and contains **zero** references to `smoke`; `grep -rl "smoke" .github/workflows/` returns nothing.

## Impact

The playable-vertical-slice gates — the only runtime verification the P0–P2 work has — are never executed automatically, and when a human runs one without game data it reports success.

This is the mechanism that let #3001 (both P0 and P1 deterministically RED) go unnoticed for three commits.

## Suggested Fix

`exit 0` on missing data is defensible for a local developer run, but it must be distinguishable from a pass — use a distinct exit code (e.g. 77 / `SKIP`) or require an explicit `--allow-skip` flag, so an automated caller can tell "skipped" from "passed".

Separately, wire at least one gate into CI on a runner that has data, or gate it behind a manually-triggered workflow so it is at least runnable on demand.

## Related

- #3001 (RT-2026-08-16-02 — the RED gates this masked)
- #3000 (RT-2026-08-16-01)

## Completeness Checks
- [ ] **SKIP≠PASS**: A data-less run is distinguishable from a passing run by exit code
- [ ] **SIBLING**: All three scripts get the same treatment
- [ ] **RUNNABLE**: At least one path exists to actually execute a gate automatically
- [ ] **TESTS**: A CI-side check verifies the gates are invoked, not merely present

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3003 --json state` when live state is needed.*
