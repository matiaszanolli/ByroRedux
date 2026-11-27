# #7433: DLOD: FO3 Wasteland at the default radius fills the geometry pool, holds back 60 LOD quads and fails the coverage verdict

- State at fetch: OPEN
- Labels: bug, medium, memory, game:fo3, terrain-exterior

Related: #6638 (M35 Phase 2), #7431. PRD `docs/prd/distant-lod-completion.md`, DLOD-BAND-007. Seen on 2026-11-27 on an RTX 4070 Ti, release build.

## Problem

At the default exterior radius, FO3 Wasteland fills the geometry pool, the distant-LOD rings stop, and the coverage verdict fails. The horizon is incomplete.

Run (default `--radius`, so 12):

`--esm Fallout3.esm --wrld Wasteland --grid 0,0 --bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa" --upscaler taa --bench-frames 300 --bench-hold`, then `lod.coverage` over `byro-dbg` about 5 s after `bench-hold:`.

```
lod-coverage: sampled=1 settled=0 overlaps=0 full_detail_overlaps=0 vwd_full_model_overlaps=0 terrain_churn=0 object_churn=0 tree_churn=0 terrain_resident=52 object_resident=139 tree_resident=0 placement_resident=0 object_moat_cells=159 tree_moat_cells=0 held_back=60 verdict=FAIL
```

From the engine log of the same run, which made no cell crossing:

1. An object-LOD upload (`wasteland.level4.x-24.y28.nif`) fails: `vertex pool hard cap exceeded: would grow from 5057983 to 5064596 (cap 5061175)`.
2. `Geometry pool admission reopened — retrying the held-back LOD quads`.
3. The same quad fails again (5065662 to 5072275, same cap).
4. `Exterior LOD rings stopped around (0,0) with the geometry pool at capacity: 60 distant-LOD quad(s) held back; the horizon is incomplete until a cell unload frees pool space, when the rings retry. Lower the exterior radius to leave room for the LOD rings.`

The same coverage line was read from a build of `bc5f837c1` sources and from the #7431 fix (`5e313e4e5`), so #7431 did not cause it. `tree_resident=0` is what DLOD-BAND-007 expects: the FO3 tree ring wants nothing at this radius.

The reported cap, 5 061 175, is below the 7 M vertex hard cap in `docs/engine/memory-budget.md`. It is probably the `Prop` tier's ceiling (the cap less the ground and actor reserves), but that was not confirmed. The upload error also suggests "a leaked cell unload", which does not fit a run with no crossing.

## History

#6638 lists this run as an acceptance criterion (`tree_resident=0`, `verdict=PASS`). Its exit comment reports A3 to A6 but not this run, and no result for it was found in the Phase 2 commit messages or the docs. **It is not known whether it ever passed.** PRD §3.2, as committed in `5e313e4e5`, says #6638 recorded a PASS on 2026-11-08. That sentence is wrong: it took the criterion for a result.

#6054 (closed) covered radius-12 pool saturation on FNV. The `m-exteriors.sh` boundary profiles run at radius 1, so they do not reach this.

## Acceptance Criteria

- [ ] The vertex use at this pose is broken down by upload source, and the ceiling the refused uploads hit is identified
- [ ] DLOD-BAND-007: the default-radius FO3 Wasteland run reads `tree_resident=0`, `settled=1`, `held_back=0`, `verdict=PASS`; or the owner's decision that this radius may hold LOD back is recorded in the PRD and the requirement's observable is restated

