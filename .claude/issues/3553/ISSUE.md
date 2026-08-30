# RT-7: `skin_pool_live` grew against its `<= baseline` direction on three of five games

**Issue**: #3553
**Labels**: bug, medium, tech-debt
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-30.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-30.md` — RT-7.

## Description

`skin_pool_live` grew against its `<= baseline` gate direction on three of five games.

| Game | baseline | current | delta |
|---|---|---|---|
| fnv | 206 | **217** | +11 |
| skyrim_se | 83 | **133** | +50 |
| fo4 | 248 | **299** | +51 |
| fo3 | 7 | 7 | exact |
| oblivion | 4 | 4 | exact |

## Mitigating evidence (measured)

`skin_pool_overflow_attempts` is **0** on all five games and `skin_pool_max` is **1364** on all five, so nothing is rendering in bind pose for lack of a slot and the #1284 cap has ample headroom (299/1364 = 22%). This continues the documented benign-creep line from the skin-version-gate work rather than being a new spill.

## Impact

Three baseline rows are stale in the failing direction, so the gate reports a regression every sweep. As with RT-4, a row that always fails stops being read — and it is the row sitting next to the one that actually matters.

## Suggested Fix

Regenerate the three rows. Keep the hard gate on `skin_pool_overflow_attempts == 0` and `skin_pool_max`, which is the pair that carries real signal; treat `skin_pool_live` as advisory or give it a band rather than an exact-match gate.

Note that skyrim_se's row should **not** be regenerated in isolation — that cell's `entities_total` and draw split are separately unexplained (RT-2 / RT-8), and its `skin_pool_live` +50 may be coupled to them.

## Completeness Checks
- [ ] **SIBLING**: The gate's semantics for the other pool rows (`skin_pool_max`, `skin_pool_overflow_attempts`) stated explicitly so the strict/advisory split is visible in the baseline file
- [ ] **TESTS**: Regen commit records *why* each row moved, per the existing baseline-header convention
