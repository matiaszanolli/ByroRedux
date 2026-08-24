# 3284: FNV-2026-08-24-D1-01: FNV WaterKind vocabulary partially repaired - creek added, spill/potomac/fountain still missing

**Severity**: LOW · **Report**: `docs/audits/AUDIT_FNV_2026-08-24.md` (FNV-2026-08-24-D1-01)

## Description

The prior finding (never filed) censused all 78 vanilla FNV `WATR` EditorIDs and found zero matches, meaning `WaterFlow` was unreachable even for named moving-water records (`CreekWater01`, etc). `canal` and `creek` were since added via the `LC-D5-02` shared-token hoist — every `Creek*` FNV record now classifies `River`. `spill`/`potomac`/`fountain` were not added — `Potomac` (WastelandNV's worldspace-default water, backing ten worldspaces) still classifies `Calm`.

## Location

`byroredux/src/material_translate.rs:189-204` (`water_kind_from_name`)

## Evidence

Current token set: `rapid`, `waterfall`/`falls`, `river`/`stream`/`canal`/`creek` — no `spill`, `potomac`, `fountain`.

## Impact

Reduced from HIGH-adjacent (Creek family now fixed) to a narrow residual gap. `Potomac` as the worldspace-default water is the item worth another look.

## Related

`LC-D5-02` (the token-list unification this fix already completed).

## Suggested Fix

Add `potomac` (and optionally `spill`) if design confirms FNV's Potomac River water should carry current; otherwise document `Calm`-by-default as intentional.

## Completeness Checks
- [ ] **TESTS**: A regression test asserting `Potomac`'s classification, whichever way the design decision lands
