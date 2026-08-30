# RT-8: `entities_total` left the +/-2% band on three games; skyrim_se's +15.2% is coupled to the RT-2 draw split, not benign creep

**Issue**: #3554
**Labels**: bug, medium, tech-debt, game:skyrim
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-30.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-30.md` — RT-8.

## Description

`entities_total` left the +/-2% band on three of five games.

| Game | baseline | current | delta |
|---|---|---|---|
| skyrim_se | 8126 | **9363** | **+15.2%** |
| fo4 | 18256 | **19399** | **+6.26%** |
| fnv | 7174 | **7342** | **+2.34%** |
| oblivion | 705 | 718 | +1.84% (in band) |
| fo3 | 3493 | 3493 | **exact** |

## Corroboration that the skyrim rise is not purely benign

The standard defence for this creep (per #1705 / #2216, and written into `.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv`'s own header) is that `bench_draws_cmds` **falls** while entities rise — more non-rendering bodies, not more rendering.

That holds for fo4 here (cmds +1.3% against entities +6.3%). It does **not** hold for skyrim_se, where `cmds` rose **+5.0%** alongside the +15.2%, and the draw split broke contract on both batch axes (see RT-2). Treat skyrim_se's entity rise as **coupled to RT-2**, not as independent benign drift.

## Impact

The +15.2% on the reference Skyrim interior is the largest single-sweep entity jump this baseline has recorded, and the mechanism that has justified every previous jump does not apply to it.

## Suggested Fix

Bisect skyrim_se between 2026-08-06 and HEAD (same window as RT-2 — likely the same commit). Regenerate the fnv/fo4 rows only once RT-2 is understood; hold skyrim_se stale so the evidence survives.

## Completeness Checks
- [ ] **SIBLING**: The cmds-fell-while-entities-rose test applied per game rather than assumed workspace-wide
- [ ] **TESTS**: Regen commit records the mechanism for each row, per the baseline-header convention
