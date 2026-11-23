# #7357: FO3-D2-001: the FO3 dialogue "declined INFOs" pin and its census text are stale (the FNV pin is stale too)

Labels: see GitHub (severity, domain, type and game labels)

Filed from `docs/audits/AUDIT_FO3_2026-11-23.md` (finding `FO3-D2-001`).

- **Severity**: LOW (test and doc staleness)
- **Dimension**: ESM data slice; consumer `/audit-gameplay`; owner `/audit-scripting`, FNV leg `/audit-fnv`
- **Location**: `crates/scripting/tests/condition_catalog_real_data.rs`, `fo3_dialogue_declined_by_unmodeled_conditions` (the 1202 assertion and its doc comment); the FNV leg `fnv_dialogue_declined_by_unmodeled_conditions` (613 and 69). The FO3 skill Dim 2 bullet repeats "1 202".
- **Status**: NEW
- **Description**: 1 202 was set by #6558 on 2026-11-03. `d3db6fd58` (#6545) added catalog rows for functions 110, 136, 278 and 391 and re-pinned only the FNV count (641 to 613). `b73899734` added rows for 49, 242 and 438 (CATALOG 103 to 106) and touched no test file. Those rows are not FNV-only: `condition_getter_game` maps `GameKind::Fallout3NV` to the shared Fallout family, so they fire on FO3 INFOs too.
- **Evidence** (measured, release, one named test per leg): FO3 `declined INFOs 1152 of 22327, quests 0`, assertion `left: 1152 right: 1202` fails. FNV `declined INFOs 609 of 23247, objective targets 68 of 772`, assertion `left: 609 right: 613` fails (the 69 objective-target pin is off by one as well). Oblivion's 2 639 / 450 is unchanged. Raw FO3 INFO CTDA counts for the new rows: fn 110 on 11 INFOs, 136 on 2, 391 on 15, 438 on 21, 49 on 1.
- **Impact**: a stale real-data gate hides the next real movement. About 50 FO3 INFOs (and 4 FNV) are now selectable that the pins still count as declined. No runtime defect.
- **Related**: #6460, #6558, #6545.
- **Suggested Fix**: re-pin FO3 to 1 152 and FNV to 609 / 68, and update the doc comment and the skill's "1 777 to 1 202" narrative. The pin sits behind `--ignored`, so no CI lane caught it; consider a floor with a documented tolerance instead of an exact number.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
