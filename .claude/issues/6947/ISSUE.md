# #6947: TD1-2026-11-15-01: `crates/plugin/src/esm/records/misc/world.rs` is 2052 production LOC (1910 on 10-31); eight record families, split by family

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6947
- **Labels**: low,esm-plugin,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Age**: 1910 at 670f81917 (2026-10-31), 2019 on 11-09, 2052 now. The growth is 12 commits. The main ones are 3312c84a4 and 3bfcbd58b (FO4 / FO76 fog tail, god-ray records, 11-08), cf3ccc958 (`covers_cell`, 11-07), 8774d1adb, cf901d557 and 6439145f9 (REGN RDMP / RDMD / lstring, 11-08 and 11-15), and 7af55a0df (XCT).
- **Effort**: medium (mechanical; the cost is the source-scan and citation hazards)
- **Labels**: `tech-debt`
- **Location**: `crates/plugin/src/esm/records/misc/world.rs:1-2050` (production); inline tests `:2051-3825`
- **Status**: NEW (no issue in the 1000-issue tech-debt set names `world.rs`)
- **Description**: The module doc says "navigation, regions, encounter zones, lighting templates, image-space adapters, activators, terminals". The file really holds eight independent record families that share only two row helpers (`rows`, `rows_exact`, `:981`/`:992`) and `common` imports. There is no shared state between them. The tests (1774 lines) are already grouped by family in three modules. The sibling files in `misc/` are each one family: `water.rs`, `quest.rs`, `scene.rs`, `music.rs`, `imagespace.rs` (that one is `IMAD`, not `IMGS`).
- **Evidence** (production line ranges at HEAD):

  | Family | Items | Lines | ~LOC |
  |---|---|---|---:|
  | `NAVI` / `NAVM` | `NaviRecord`, `NavmRecord`, `decode_nvnm`, `decode_nv*_row` | 18-660 | 640 |
  | `REGN` | `RegionData*`, `RegionArea`, `RegnRecord`, `select_active_region_sound`, `parse_regn`, `apply_region_payload` | 661-1288 | 630 |
  | `ECZN` / `CNDF` | | 1289-1391 | 100 |
  | `KYWD` / `AORU` | | 1392-1469 | 80 |
  | `LGTM` / `IMGS` | | 1470-1725 | 255 |
  | `ACTI` / `TERM` | | 1726-2050 | 325 |

  Inline tests: `tests` `:2052` (26 tests: LGTM / IMGS 11, ACTI / TERM 10, rest NAVI / ECZN / REGN), `regn_tests` `:2840` (439 lines), `navm_tests` `:3279` (548 lines).
- **Impact**: Every `REGN` or fog / god-ray change (the three busiest areas of the last three weeks) recompiles and re-reviews a file that also carries the navmesh and terminal decoders. 12 commits in 15 days touched it.
- **Suggested Fix**: Turn `world.rs` into a `world/` directory. `world/mod.rs` is a ~60-LOC hub holding the module doc, `pub use` of the same names `misc.rs` already re-exports (so `misc.rs:118` and every caller are unchanged), and `rows` / `rows_exact` as `pub(super)`.
  - Children, each taking its tests with it:
    - `navigation.rs`: NAVI / NAVM + `navm_tests`, ~640.
    - `region.rs`: REGN + ECZN + `regn_tests`, ~730. This is the largest child, with the `regn_tests` module beside it.
    - `lighting.rs`: LGTM + IMGS, ~255.
    - `activator_terminal.rs`: ACTI + TERM, ~325.
    - `keyword.rs`: KYWD + AORU + CNDF, ~180.
  - Hazards:
    - (a) `crates/plugin/src/esm/records/tests.rs:3303` runs `include_str!("misc/world.rs")` in `remap_fid_has_exactly_one_definition`. It must list the child files or the guard goes vacuous (*feedback_file_split_include_str*).
    - (b) Cite-by-line references need a sweep: `.claude/commands/audit-esm/SKILL.md:147,343,359`, `docs/prd/game-music.md:184`, `volumetric-lighting.md:155`, `procedural-exteriors.md:162,562`, `cross-cell-actor-travel.md:130`.
    - (c) The `//!` header already omits KYWD / AORU / CNDF and should be fixed in the same commit.
  - Do not split by name or by game. `world.rs` carries no per-game branches.
- **Related**: #4219 and #6145 (earlier barrel splits, precedent), `misc/pack.rs` (1578), `misc/water.rs` (1499) and `misc/magic.rs` (1390), which are under the line.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
