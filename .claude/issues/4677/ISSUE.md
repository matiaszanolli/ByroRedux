# CHAR-2026-09-21-D4-02: FO4 stored path emits duplicate Health/AP keys on ~2,500 vanilla NPCs — DNAM-wins precedence holds only by push order

**Severity**: LOW
**Dimension**: Population Boundary
**Game**: FO4 (and FO76/Starfield via the same `Stored` model)

## Description

`derive_stored_actor_values` (`crates/plugin/src/esm/records/actor_value_derive.rs`) copies PRPS verbatim and then pushes the baked DNAM Health/AP. The same AVIF keys therefore appear twice, and `ActorValues::from_pairs` (`set_base` in order, `crates/core/src/ecs/components/actor_values.rs`) keeps the last one. The result matches the capture only because the DNAM pushes come second:
- Nothing states this precedence. The function doc says "PRPS verbatim plus baked DNAM", and the capture never mentions PRPS authoring these keys.
- No test covers the collision: `fo4_stored_returns_prps_verbatim_plus_baked_derived`'s PRPS fixture has no Health/AP pair.

## Evidence

Verified at HEAD `ee6d3fb39`: `out.extend_from_slice(props);` precedes the `for (avif_editor_id, baked) in [("Health", …), ("ActionPoints", …)]` push loop.

Census of `Fallout4.esm` (read-only, per the audit report) covers 3,015 `NPC_` records:
- PRPS authors Health on 2,848. Of those, 2,525 also carry DNAM calc_health > 0, and 2,490 disagree.
- PRPS authors ActionPoints on 2,800. Of those, 2,415 carry DNAM AP > 0, and 799 disagree.
- Colliding PRPS Health values: 100.0 x611, 0.0 x350, **-10.0 x185**, 50.0 x170, 40.0 x131.

## Impact

- Correct today.
- A reorder, sort-by-key or dedup-first refactor would silently give ~1,000 vanilla FO4 actors 0 or -10 base Health, which means dead on spawn or undamageable.

## Related

CHAR-2026-09-21-D4-01 (the player sees the same collision, where even the DNAM answer is wrong); #3481 / #4086 (earlier FO4 stored-path precedence fixes).

## Suggested Fix

- Drop PRPS pairs keyed on the Health/AP AVIFs when a baked value is present, so the precedence is explicit.
- Add a fixture with PRPS Health -10 and DNAM 150, asserting 150.
- Record the census in the FO4 capture's NPC-storage section.

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D4-02)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix
