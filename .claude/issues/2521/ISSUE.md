# RT-1: entities_total drift on fo3 MegatonPlayerHouse marginally exceeds the ±2% tolerance band

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2521
**Finding ID**: RT-1 (source: `docs/audits/AUDIT_RUNTIME_2026-08-07.md`)

**Severity**: LOW
**Report**: `/audit-runtime` (Runtime Telemetry Audit)
**Game**: fo3
**Cell**: MegatonPlayerHouse
**Status**: NEW

## Description
Per the skill's RT-3/#1705 note, `entities_total` counts *all* ECS entities including non-rendering bodies (collision colliders, ragdoll/character rig, markers), and is expected to drift benignly as collision/physics work lands without changing what actually renders. The fo3 baseline is the only one of the five with committed runtime baselines that has never been regenerated since its initial 2026-06-14 creation (the other four were all refreshed 2026-08-06 alongside prior sort-mechanism and skin-gate fixes), so it is the most likely of the five to show accumulated drift simply from being the oldest snapshot.

## Evidence
Confirmed directly: `.claude/audit-baselines/runtime/fo3-MegatonPlayerHouse.tsv` still carries `# regenerated: 2026-06-14 (BASELINE CREATED...)` and `entities_total 3311`. Current run: 3380 (+69, **+2.08%** — 0.08 points past the skill's ±2% tolerance band for this metric). `bench_draws_cmds` — the metric the skill designates as the exact render-load contract — **fell** 1839→1565 (−14.9%) over the same window, the opposite direction a real "more stuff got spawned and is rendering" regression would move. `tex_missing_unique_paths`, `mesh_cache_failed_count`, and all three `skin_pool_*` fields matched the baseline exactly.

## Impact
None observed — this is a tolerance-metric drift note, not a functional regression. Flagged per the skill's explicit LOW-severity rule ("count drift within ±5% on a tolerance metric") since 2.08% is technically past the ±2% line, even though every corroborating signal points the same direction as the other four games' already-accepted benign creep.

## Related
Same mechanism as the skyrim_se baseline's documented `entities_total` creep (RT-2/#2216) and the general RT-3/#1705 tolerance design.

## Suggested Fix
Regenerate the fo3 baseline with `--regen` next time fo3-touching work lands (it is the one baseline of the five that is now ~2 months stale relative to the others), the same way skyrim_se/fnv/oblivion/fo4 were refreshed 2026-08-06. Not urgent on its own — bundle with the next fo3-relevant session rather than a standalone regen.

## Completeness Checks
- [ ] **TESTS**: N/A (baseline regen, not a code fix) — confirm the regenerated baseline's other four gating metrics still match after regen
