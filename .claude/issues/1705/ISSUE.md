# RT-3: entities_total exact-match metric drifted up on fnv (+102), skyrim (+5), fo4 (+10) without a regen

**Severity**: LOW
**Dimension**: tech-debt (baseline hygiene) — surfaced via runtime telemetry
**Location**: `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv`, `.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv`, `.claude/audit-baselines/runtime/fo4-InstituteBioScience.tsv` — `entities_total` rows.
**Status**: NEW (continuation of the pattern in AUDIT_RUNTIME_2026-06-14 RT-2 for FNV and RT-4 for FO4; CONFIRMED — baselines on disk read 9250 / 6044 / 11279)

## Description
Three baselines carry an `entities_total` that the engine no longer reproduces exactly: fnv 9250→9352, skyrim 6044→6049, fo4 11279→11289. These are non-rendering entities (collision-only bodies, ragdoll/character rig, markers) added by ongoing work; the load-bearing render count (`bench_draws_cmds`) is unchanged on fnv and fo4 (3800=3800), and the symptom metrics (tex.missing / mesh_fail / skin) all pass.

## Evidence
Baselines on disk: `fnv entities_total 9250`, `skyrim_se entities_total 6044`, `fo4 entities_total 11279` (the fo4 tsv already carries an `entities_total 9167 -> 11279 (+2112): intentional NON-rendering drift` comment). Current captures: 9352 / 6049 / 11289.

## Impact
Cosmetic — but `entities_total` being a hard "exact match" metric means every benign non-render entity addition trips a false diff and can mask a real entity-count regression in the noise. This is the third audit to log the same drift.

## Suggested Fix
Either (a) regen the three baselines with `--regen` after eyeballing (the entity deltas are accounted for by collision/ragdoll/material work), or (b) split the contract into `render_entities` (= `draws_cmds`, keep exact) and `entities_total` (move to a ±2% tolerance), per the 06-14 RT-4 suggestion. Until then these three are knowingly-stale, not regressions.

## Completeness Checks
- [ ] **SIBLING**: If the contract is split, all five game baselines adopt the same `render_entities` / `entities_total` split
- [ ] **TESTS**: The runtime baseline gate enforces the new tolerance/exact split
