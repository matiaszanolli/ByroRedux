# #1863: RT-1: Oblivion runtime baseline bench_draws_gpu_calls is stale (3 vs live 4)

- **Severity**: LOW
- **Labels**: `low`, `performance`, `bug`
- **Source**: `docs/audits/AUDIT_RUNTIME_2026-07-03.md` (RT-1)
- **Dimension**: runtime / baseline staleness

## Location
`.claude/audit-baselines/runtime/oblivion-ICMarketDistrictTheGildedCarafe.tsv:9`

## Description
Baseline records `bench_draws_gpu_calls 3`; four independent sweeps since 06-14 (06-23, 06-26, 07-02, 07-03) all read `4`. Stable drift, not jitter — same pattern as #1833 (FNV `skin_pool_max`).

## Impact
Cosmetic-only; no render defect. Recurs as audit noise until the baseline is regenerated.

## Related
#1833 (identical pattern, different metric/game)

## Suggested Fix
`--regen` the Oblivion baseline alongside a fix for #1833 in the same housekeeping pass.
