# RT-10: `light_count_directional` baseline is 1 on all five games; measured is 0 on four and 2 on the fifth — a gate that could never fail

**Issue**: #3556
**Labels**: bug, low, tech-debt, test-gap
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_RUNTIME_2026-08-30.md`

---

Source: `docs/audits/AUDIT_RUNTIME_2026-08-30.md` — RT-10.

## Description

The `light_count_directional` baseline row is **1** on all five games. With the real per-emitter dump now parsed (`kind=` rows), the measured value is **0** on four games and **2** on the fifth.

| Game | emitters | Directional | Point | baseline row |
|---|---|---|---|---|
| fnv | 30 | **0** | 30 | 1 |
| fo3 | 11 | **0** | 11 | 1 |
| oblivion | 10 | **2** | 8 | 1 |
| skyrim_se | 28 | **0** | 28 | 1 |
| fo4 | 685 | **0** | 685 | 1 |

## Impact

This confirms **#3424** live: the old row was derived from the mere presence of a `CellLightingRes` block, so it was **a gate that could never fail** — it has been asserting a constant against a constant on five games since it was introduced.

The emitter totals independently reproduce the `/audit-runtime` skill's 2026-08-27 observations (fnv 30, fo3 11, skyrim_se 28, fo4 685) **exactly**; only oblivion differs (8 -> 10), fully explained by the two synthetic directionals in RT-11.

## Suggested Fix

On the next `--regen`, replace the row with the **measured** `light_count_directional` plus a new `light_count_point` row. The point count is the one that actually varies and is the one that would have caught a lighting-collection regression.

## Completeness Checks
- [ ] **SIBLING**: Every other runtime baseline row audited for the same "derived from a presence check, cannot fail" shape
- [ ] **TESTS**: After regen, confirm the new rows can go red — deliberately drop an emitter and check the gate trips
