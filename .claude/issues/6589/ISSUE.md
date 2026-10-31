# REG-2026-10-31-04: Regression of #6461: nine new `docs/smoke-tests/m43-*.sh` scripts are in no contract loop, and the playable-smoke contract lane is red

Labels: medium,tech-debt,test-gap,bug
Source: docs/audits/ (2026-10-31 report)

**Severity**: MEDIUM
**Dimension**: CI gates / closed-fix regression
**Source**: `docs/audits/AUDIT_REGRESSION_2026-10-31.md` REG-2026-10-31-04 (the suite summary lists it as known and not re-filed; no open issue existed at publish time)
**Status**: regression of #6461 (closed 2026-10-30, "the smoke contract checker covers m42-package-procedures, vats-targeting and w2-dynamic-water, and cannot lose the next gate"). That issue's own comment says the class recurred three times already (#5427, #6057, #6461). The lane first went red on `59ae0b846` (2026-10-31 02:43 UTC).

## Location
`scripts/check-playable-smoke-contracts.sh:123` (the `m43-adia-conversations m43-item-drop m43-crafting` loop) and `:328-332` (`UNCHECKED_SMOKE_GATES`). Uncovered scripts (each has zero mentions in the checker, re-verified at HEAD `670f81917`): `m43-armor`, `m43-attraction`, `m43-attraction-reaction`, `m43-fo4-actions`, `m43-hello`, `m43-mines`, `m43-node-window`, `m43-skyrim-actions`, `m43-tempering` (all under `docs/smoke-tests/`).

## Description
The checker fails when a script in `docs/smoke-tests/` is neither named earlier in the checker nor listed in `UNCHECKED_SMOKE_GATES`. The M43 live-gate work (`4dd6197dc`, `7c9414845`, `93428cba1`, `834e83822`, `31aca901d`, `542f3de7b`, ...) added nine scripts and touched neither list. `fail()` calls `exit 1`, so CI shows only the first (`m43-armor.sh`).

## Evidence
CI run 41215450288, job "Playable smoke gate contracts": `FAIL -- docs/smoke-tests/m43-armor.sh is in no contract loop and not in UNCHECKED_SMOKE_GATES`. The miss list was reproduced by running the checker's name test over `docs/smoke-tests/*.sh` without executing any script (the gate scripts launch the engine and need game data).

## Impact
The lane is red, and a gate that exits 0 on missing game data would be counted as a pass (the `SKIP != PASS` contract the checker protects). Two of the nine, `m43-hello` and `m43-attraction-reaction`, are staged-mover live gates whose missing-data behaviour nobody has verified.

## Suggested Fix
Add the nine scripts to the `m43-adia-conversations m43-item-drop m43-crafting` loop after confirming each exits 77 on missing data, or list them in `UNCHECKED_SMOKE_GATES` with a reason. Make `fail` collect all misses so one CI run shows the full list.

## Related
#6461, #6057, #5427, #6331

## Completeness Checks
- [ ] **SIBLING**: Every script in `docs/smoke-tests/` (not only `m43-*`) is covered or listed
- [ ] **TESTS**: Each newly covered script verified to exit 77 (not 0) when game data is absent
