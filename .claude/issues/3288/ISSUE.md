# 3288: RT-2026-08-24-01: fnv/oblivion/skyrim_se runtime baselines are 18 days and fo3's is 71 days stale

**Severity**: LOW · **Report**: `docs/audits/AUDIT_RUNTIME_2026-08-24.md` (RT-2026-08-24-01)

## Description

Coverage-freshness observation, not a demonstrated regression — no live measurement was taken this run (an engine/`byro-dbg` instance was already live, so per the no-parallel-engine-launch rule, Phase 2-4 of `/audit-runtime` were skipped). The fo3 baseline predates the exterior-streaming tranche and the `#973`/`#2221`/`#3231` render/material commits — roughly two and a half months of history. Combined with still-open #3005 (fnv/fo3 draw-batch regression, not re-baselined since 2026-08-16), fo3 is the worst-state arm of the five.

## Location

`.claude/audit-baselines/runtime/*.tsv`

## Evidence

| Baseline | Last regenerated | Age |
|---|---|---|
| `fnv-FreesideAtomicWrangler.tsv` | 2026-08-06 | 18 days |
| `fo3-MegatonPlayerHouse.tsv` | 2026-06-14 | 71 days |
| `oblivion-ICMarketDistrictTheGildedCarafe.tsv` | 2026-08-06 | 18 days |
| `skyrim_se-WhiterunDragonsreach.tsv` | 2026-08-06 | 18 days |
| `fo4-InstituteBioScience.tsv` | 2026-08-22 | 2 days (fresh) |

## Impact

The next audit able to launch the engine should prioritize fo3, and expect drift on `tex_missing`/`draws=` given the identified material/spawn commits.

## Related

Existing #3005 (OPEN — fnv/fo3 draw-batch regression), Existing #2521 (OPEN — fo3 entities_total drift).

## Suggested Fix

No code fix — schedule a live `/audit-runtime --game all` when no engine/`byro-dbg` instance is running, prioritizing fo3 first.

## Completeness Checks
- [ ] **TESTS**: N/A — infrastructure-freshness item, resolved by running the audit
