# FO4-D6-03: FO4 armor rating lives in FNAM and is never read

**Issue**: #2994
**Severity**: MEDIUM
**Dimension**: 6 — ESM item records
**Labels**: `medium,import-pipeline,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_FO4_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FO4_2026-08-16.md` (Dimension 6 — ESM item records).

**Location**: `crates/plugin/src/esm/records/items.rs`:361-376 (the ARMO `b"DNAM"` arm), and the **absence of any `b"FNAM"` arm** in `parse_armo`

## Description

The FO4 branch of the ARMO `b"DNAM"` arm decodes `dt`/`dr`, but **FO4 ARMO carries no `DNAM` at all** (0 of 688). Its armor rating is a `u16` at offset 0 of `FNAM` (present on 688/688), which `parse_armo` has no arm for.

## Evidence

Sub-record census as above; measured `FNAM` armor ratings: `Armor_Leather_TorsoE3` = 3, `Armor_Leather_ArmRightE3` = 1, `Clothes_InstituteLabCoat*` = 0.

xEdit FO4 ARMO:
```
wbStruct(FNAM, '', [
  wbInteger('Armor Rating', itU16),
  wbInteger('Base Addon Index', itU16),
  wbInteger('Stagger Rating', itU8, wbStaggerEnum), … ])
```

Re-verified 2026-08-17: the `b"DNAM"` arm has a `GameKind::Fallout4` branch setting `dt`/`dr`; there is no `b"FNAM"` arm anywhere in `parse_armo`.

## Impact

`armor_rating_x100`, `dt` and `dr` are all 0 for every FO4 armor piece, so `byroredux/src/inventory.rs`:90-104 falls through both protection branches and renders the bare string `"Armor"` for all 688. Any future damage mitigation has no input.

Note FO4's `FNAM` rating is a **raw `u16`, not the ×100 convention** `armor_rating_x100` implies — the field name will mislead whoever wires it.

## Suggested Fix

Add a `b"FNAM"` arm gated on `GameKind::Fallout4` reading `Armor Rating (u16)`, and either scale by 100 on the way in or rename the field to make the unit explicit.

## Related

- #2993 (FO4-D6-02 — same record, same arm cluster)
- #2992 (FO4-D6-01 — same "FO4 shares/misses the arm" root)

## Completeness Checks
- [ ] **SIBLING**: Every record in `parse_armo` checked for a sub-record present on disk with no arm
- [ ] **UNIT-CLARITY**: The ×100 vs raw ambiguity resolved by scaling or renaming — not left implicit
- [ ] **DEAD-ARM**: The unreachable FO4 `DNAM` branch removed or documented as intentionally dead
- [ ] **TESTS**: A regression test pins a known FO4 armor rating

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 2994 --json state` when live state is needed.*
