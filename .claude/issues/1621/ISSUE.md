# RT-4: FO4 entities_total drifted +2164 against committed exact-match runtime baseline

- **GitHub**: #1621
- **Severity**: medium
- **Labels**: medium, tech-debt, bug
- **Source**: docs/audits/AUDIT_RUNTIME_2026-06-14.md (RT-4)

## Description
FO4 `entities_total` 9167→11331 (+2164, +23.6%) vs `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv` (`# regenerated: 2026-06-01`). Draw counts, skin pool, tex.missing, mesh_cache_failed all unchanged → the extra entities don't render. Attributable to intentional commits (`1c26bc25` collision-only spawn, `2a14b2b7` ragdoll M41.x, `83d6a155` material/PBR). Stale-baseline drift, not a regression.

## Evidence
`/tmp/audit/runtime/fo4-InstituteBioScience.{telem.txt,engine.log}` — `Scene ready: 11331 entities`; CSG clean; reproduced 11331 in both parallel and solo runs.

## Suggested Fix
`/audit-runtime --game fo4 --regen` after verifying the +2164 is entirely collision-only / ragdoll / marker entities (so the refresh isn't laundering a real bug).

## Note
No `audit-infra` label in repo — mapped to `tech-debt`.
