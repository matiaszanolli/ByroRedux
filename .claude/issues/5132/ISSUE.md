# #5132: RT-2026-09-29-06: fo4 InstituteBioScience `light_count_point` 685 → 680 — 5 emitters reclassified Point → Spot by `b9e961eeb`; the runtime schema has no spot-light row

**Labels**: medium, bug, tech-debt, test-gap, game:fo4

**Source report**: `docs/audits/AUDIT_RUNTIME_2026-09-29.md` (report ID `RT-6`)
**Severity**: MEDIUM (exact metric moved; the schema cannot express the change)
**Dimension**: Telemetry diff (exact metric) / harness schema

## Location
- `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv` `light_count_point 685`
- `byroredux/src/bench.rs` `REQUIRED_METRICS`
- `/audit-runtime` skill Phase 3 metric table

## Description
Current: 680 Point + 5 Spot (`LightSource emitters: 685`, `lights=685` unchanged). `cb44d99f6` dumps 685 `kind=Point`. `b9e961eeb` dumps 680 `kind=Point` plus 5 `kind=Spot`; that commit adds `crates/plugin/examples/spot_light_census.rs` and changes `crates/core/src/ecs/components/light.rs` and `lighting.rs`. No light was lost. The metric table counts only Point and Directional, so a correct reclassification reads as a regression, and a wrong one would go unnoticed on the Spot side.

## Evidence
`/tmp/audit/runtime/bisect-fo4-InstituteBioScience-{cb44d99f6,b9e961eeb}/*.telem.txt` (kind tallies).

## Impact
A false exact-metric failure on FO4, and a blind spot for spot-light regressions on every game.

## Related
RT-3, RT-5.

## Suggested Fix
Add `light_count_spot` to `REQUIRED_METRICS`, the TSVs and the skill's Phase 3 table (exact direction). Regenerate FO4 with 680/5 once the 5 spot classifications have been spot-checked against their LIGH records.

Validated at HEAD 9fcfdc3fc: `REQUIRED_METRICS` in `bench.rs` lists only `light_count_point` / `light_count_directional` (no `spot` string in the file); FO4 TSV still has `light_count_point 685`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (all five TSVs gain the row)
- [ ] **TESTS**: A regression test pins this specific fix
