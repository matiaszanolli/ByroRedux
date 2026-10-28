# RT-3: entities_total rose 3.3 % on skyrim_se WhiterunDragonsreach (+327 entities from a649b55b4 unattributed)

Labels: medium, tech-debt, bug, game:skyrim
Source: docs/audits/AUDIT_RUNTIME_2026-10-27.md

**Report**: `docs/audits/AUDIT_RUNTIME_2026-10-27.md` · **Finding**: RT-3 · **Severity**: MEDIUM · **Dimension**: Telemetry diff
**Location**: `.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv` rows `entities_total`, `bench_draws_cmds` (verified still 9499 / 2494 at HEAD)
**Game / Cell**: skyrim_se / WhiterunDragonsreach

## Description
`entities_total` is 9809 against a 9499 baseline (+3.3 %, outside the +-2 % gate); cmds 2494 -> 2495; skin_live 122 -> 123. The main step is `a649b55b4` ("M47.2: defaultSetStageTrigScript recognizer; decompiled scripts keep their variable defaults"): 9508 -> 9835 between bisect idx 620 and 621. Smaller later steps: 9835 -> 9847 (idx 705-795), -> 9863 by 2026-10-20, -> 9809 after (not bisected). The cmds +1 / skin_live +1 step landed before idx 307.

The audit did not establish which entities `a649b55b4` adds (+327). HEAD's log shows `M47.2 scripts: 12 REFRs recognized, 2 trigger volumes spawned` and three `rejected provider handler in .pex 'defaultsetStageTrigSCRIPT' ... UnsupportedStatement` warnings. The recognizer is live in this cell, but a two-volume trigger does not obviously account for 327 entities.

## Evidence
Bisect rows 620/621/624/705/795/850/1229 in the report; HEAD capture 9809 / 2495 / 123.

## Impact
The gate reads red. Until someone attributes the +327 to specific entities, it is unproven that they are intended (could be a spawn-leak or duplicate-spawn regression).

## Related
RT-1 / the all-games `--regen` issue; M47.2 (`a649b55b4`).

## Suggested Fix
Before the regen, diff `entities` by name or kind at `dbc80585f` vs `a649b55b4` (`byro-dbg entities`) to name the 327. Then regen the skyrim_se TSV with that attribution in the `# regenerated:` block.

## Completeness Checks
- [ ] **SIBLING**: Check whether other M47.2-recognized cells (FNV/FO4) show similar unexplained entity growth
- [ ] **TESTS**: TSV regenerated in the same commit as any engine change

