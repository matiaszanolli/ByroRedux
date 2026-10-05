# #5248: FNV-2026-10-05-D2-01: #5223's dismember-trigger recognizer misses exterior triggers whose XLKR target is in the persistent CELL — 29 FO3 live-base corpses still spawn alive

Labels: medium,gameplay,esm-plugin,bug,game:fnv,game:fo3,legacy-compat
Filed from: docs/audits/AUDIT_FNV_2026-10-05.md

**Source**: `docs/audits/AUDIT_FNV_2026-10-05.md` (FNV-2026-10-05-D2-01) + FO3 addendum in `docs/audits/AUDIT_FO3_2026-10-05.md` · **Severity**: MEDIUM · **Dimension**: ESM Data Slice (authored corpses) · **Status**: NEW — incomplete fix of closed #5223 (not a regression)

## Description

`load_references_budgeted` computes `script_killed_corpse_forms(refs, record_index)` **once per call, over that call's `refs` only**. Pass 1 marks the `XLKR` targets of dismember-family triggers found in `refs`; the consumer stamps `apply_starts_dead` only on placements in the same `refs`.

Exterior placements arrive in two separate calls:
- the worldspace **persistent CELL** (`PersistentCellApplyJob::advance`, `local_refs`), which holds the persistent actors;
- one call per **temporary exterior cell** (`cell.references`), which holds the non-persistent trigger activators.

A trigger and a persistent target therefore never share a call: when the trigger's cell loads, its target was already spawned by the persistent apply, and the set the persistent apply computed held no triggers. Every exterior trigger → persistent-target link is missed.

The census guard (`fo3_fnv_script_killed_corpses_match_the_measured_census`) unions `script_killed_corpse_forms` over every cell and counts link targets globally, so it passes regardless of locality — it reports the fix as complete.

## Location
- `byroredux/src/cell_loader/reference_state.rs` — `script_killed_corpse_forms` (Pass 1 builds `killed` from triggers in `refs` only)
- `byroredux/src/cell_loader/references/mod.rs` — per-call invocation in `load_references_budgeted` (~:335) and the `apply_starts_dead` consumer (~:761-765)
- `byroredux/src/cell_loader/exterior.rs` — persistent CELL applied with `local_refs` only (~:293-294, ~:1160-1166); each grid cell applied with its own `cell.references` (~:2162-2163)
- `byroredux/src/cell_loader/reference_state.rs` — census test (~:662-730, `union.extend(script_killed_corpse_forms(&cell.references, &index))`)

## Evidence

Scratch probe over the shipped masters (global FormIDs), resolving each dismember-trigger target to the cell list that places it:

```
FNV (10 targets; 6 same-cell interior, 4 cross-cell):
TRIGGER 000CEDC9 in [EXT wastelandnv (7, -9)] -> 000CEDCA in PERSIST wastelandnv  base DEADRaider1MeleeCF hp=0 sd=true
TRIGGER 000CD726 in [EXT wastelandnv (3, -8)] -> 000CD346 in PERSIST wastelandnv  base CrFeralGhoulGlowingOneDEAD sd=true
TRIGGER 000CEE23 in [EXT wastelandnv (8, -9)] -> 000CEE22 in PERSIST wastelandnv  base DEADWastelanderAAMNV hp=0 sd=true
TRIGGER 0014E835 in [EXT freesidenorthworld (1,-2)] -> 0014E836 in PERSIST freesidenorthworld base VMS49RangefinderCourer hp=0 sd=true
(live-base FNV targets are all same-cell interiors — recognised)
```

FO3 addendum (independent probe over `Fallout3.esm` + 5 DLC; matches the FNV audit's count of 29 exactly):

```
dismember-trigger links corpus-wide: 113 (Fallout3.esm 85, ThePitt 8, BrokenSteel 7, Zeta 9, PointLookout 3, Anchorage 1)
  same load bucket : 52  -- every one an INTERIOR cell (trigger and target in the same CELL)
  cross bucket     : 61  -- trigger in a temporary EXTERIOR grid cell, target ONLY in the worldspace PERSISTENT CELL
Fallout3.esm: 54 of 85 cross-bucket; 0 exterior links are same-cell -> no exterior link is recognised
  29 cross targets sit on positive-health bases (not covered by #5005), so they spawn alive:
    10 LvlSupermutantGunDISMEMBERNOAMMO   6 LvlSupermutantMeleeDISMEMBERNOAMMO   5 LvlWastelanderDISMEMBER
     4 MS06SuperMutantGunDEAD (000645E0, 000645E1, 0005ECFA, 0005ECFB)   2 LvlSupermutantGunDISMEMBER
     1 MS06SuperMutantMeleeDEAD (0005ECFC)   1 DeadSuperMutant2Melee
  trigger worldspaces of the 29: statesmanroofworld 16, dcworld09 7 (the MS06 "Head of State" site), dcworld08 4, dcworld10 1, dcworld12 1
DLC: 7 cross-bucket (BrokenSteel 4, PointLookout 2, ThePitt 1), all already dead via #5005 base health -> no DLC impact
Self-kill (pass 2): 3 NPC_ placements; 0 CREA bases carry a kill script in FO3 + DLC
```

- All 5 MS06 refs are among the live residuals — including `000645E0`, the ref #5223's own commit message and the #5005 test doc name as the fixed case.
- The repo's `corpse_trigger_probe` example prints a target only when it is in the same cell (4 `TARGET:` lines on FNV, not the census's 10).

## Impact
- **FO3**: 29 of the 49 positive-health corpses #5223 was filed for still spawn as live, armed, hostile actors (#4414) and cannot be looted. Every one sits in a DC sub-worldspace (Statesman Hotel roof, MS06 "Head of State", dcworld08/10/12) — none in `wasteland` — so the `m-exteriors.sh fo3` gate (`MegatonWorld`) cannot catch it.
- **FNV**: no current impact — the 4 cross-cell targets are base-health corpses already covered by #5005. Any DLC/mod linking a dismember trigger to a persistent live-base actor would hit the same gap.
- The census guard reports the fix as complete.

## Related
#5223 (closed), #5005, #4414, SCR-D6-2026-10-05-01 (EDID-vs-bytecode matching — a different defect), GAME-D4-2026-10-05-01, AUDIT_SAVE_2026-10-05 note on #5223 shipping at save v32.

## Suggested Fix
- Compute the script-killed set **once per load order**, over every cell in the index (FormIDs are global), and store it as a resource; the streaming and persistent-CELL paths consult the same set.
- This also removes the per-resume recomputation (re-scans and clones editor-id strings across FNV's 4,495 `XLKR` placements on every budgeted resume).
- Make the census test locality-aware: feed each cell's refs separately and require the union of **per-call** results to equal 85 + 3 (FO3) and 10 + 2 (FNV). For FO3, assert `000645E0` is in the set the **persistent-CELL** apply consults.

## Completeness Checks
- [ ] **SIBLING**: Every other per-call `refs`-scoped recognizer in `cell_loader/` checked for the same persistent-CELL / temp-cell locality split
- [ ] **LOCK_ORDER**: If the set becomes a World resource read during cell load, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins locality-independent recognition (per-call union == census; `000645E0` reached via the persistent-CELL apply)
