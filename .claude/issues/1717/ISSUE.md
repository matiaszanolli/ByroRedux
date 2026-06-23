# SF-D7-01: ROADMAP / compat-matrix Starfield parse rates understate current state

**Severity**: LOW (documentation; the code is *better* than the doc claims)
**Dimension**: 7 — Real-Data Validation
**Location**: `ROADMAP.md` — Starfield compat-matrix row (line ~206) + per-game NIF clean-parse-rate row (line ~736)
**Status**: NEW (CONFIRMED against live ROADMAP.md @ 2d4c350d)

## Description
The ROADMAP compat matrix records "Starfield 98.6% aggregate, Meshes01 97.21%, MeshesPatch 98.11%, sweep date 2026-04-27". The live `parse_rate_starfield_all_meshes` sweep this round (git `2d4c350d`) reports **Meshes01 100.00%, Meshes02 100.00%, MeshesPatch 98.91%, LODMeshes 100.00%, FaceMeshes 100.00%**, recoverable 100% on all five. The aggregate clean rate is now ≈ 99.6%, not 98.6%. The intervening parser work (≥ #1510 BSShaderType155 tail, #1606 starfield_tail, #754 BSWeakReferenceNode, #722 cloth) lifted the rate but the matrix was never refreshed. Confirmed: ROADMAP.md line 206 still reads `97.21%` / `98.11%` / `98.6%` / `2026-04-27`, and the per-game clean-parse-rate row (line 736) still reads `Starfield 98.6% aggregate ... Sweep date 2026-04-27`.

## Evidence
```
[Starfield/Meshes01.ba2]    clean 100.00% (31058 clean / 0 trunc / 0 failed)
[Starfield/Meshes02.ba2]    clean 100.00% (7552 clean / 0 trunc / 0 failed)
[Starfield/MeshesPatch.ba2] clean  98.91% (29524 clean / 325 trunc / 0 failed)
[Starfield/LODMeshes.ba2]   clean 100.00% (19535 clean / 0 trunc / 0 failed)
[Starfield/FaceMeshes.ba2]  clean 100.00% (1282 clean / 0 trunc / 0 failed)
```

## Impact
None at runtime. Stale published figures only — risks a future audit "discovering" an improvement that already happened, or under-selling SF support in status reporting. (Distinct from the feature-matrix doc-rot tracked elsewhere — this is the ROADMAP compat-matrix + per-game parse-rate rows.)

## Suggested Fix
Refresh the Starfield compat-matrix row + the per-game clean-parse-rate row in `ROADMAP.md` with the 2026-06-23 figures; note the MeshesPatch truncation tail at 325/29 849 (1.09%, has not grown). Related: #746/#747 (residual MeshesPatch truncation tail).

## Completeness Checks
- [ ] **SIBLING**: The aggregate figure on line 736 and the per-archive figures on line 206 are updated together (and the `HISTORY.md` sweep narrative if it cites the old numbers)
- [ ] **TESTS**: The `parse_rate_starfield_all_meshes` figures quoted are reproduced from the live opt-in sweep before committing the doc edit
