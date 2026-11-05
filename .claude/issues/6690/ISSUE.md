# RF1-2026-11-05-01: `spawn_nif_mesh` and `spawn_mesh_instance` duplicate the material-to-component sequence

Labels: low,import-pipeline,nifal,tech-debt,bug

**Severity**: LOW (no runtime effect; maintainability)
**Dimension**: Refactor backlog, item 1
**Status**: NEW. Part of #6680

## Location
- `byroredux/src/scene/nif_loader.rs:1045` `spawn_nif_mesh` (581 lines, 333 of code)
- `byroredux/src/cell_loader/spawn/mesh_instance.rs:1041` `spawn_mesh_instance` (575 lines, 288 of code)

## Why
The two functions run the same material-to-component sequence, with 65 identical non-comment lines. Both directly insert these 13 component types: `Billboard`, `GlobalTransform`, `IsFxMesh`, `LocalBound`, `MaterialTextureDebugInfo`, `MeshHandle`, `Name`, `Parent`, `SceneFlags`, `SpeedTreeWind`, `TextureHandle`, `Transform`, `WorldBound`. Only the cell path adds `LightSource`. Every fix has to land twice, and text guards exist partly to keep the copies in step. This is the highest-priority item in #6680, because these copies can change behaviour when they drift.

## Change
Use one function to attach components. Callers supply only what really differs: transform composition, material overrides, parenting and the cell path's `LightSource` fallback. Move shared lines verbatim.

## Constraints
Run `grep -rn include_str` for both files before moving anything. Readers include `byroredux/src/material_translate.rs:1648-1649`, `byroredux/src/scene/nif_loader_light_tests.rs:27`, `byroredux/src/cell_loader/spawn/mesh_instance.rs:1758-1939`, `byroredux/src/anim_convert.rs:1482` and `byroredux/src/scene/nif_loader.rs:2052`. The doc comment at `byroredux/src/scene/nif_loader.rs:1037` records that an earlier extraction stayed in place for this reason.

## Acceptance Criteria
- Both paths attach components through one function
- `cargo test -p byroredux --bin byroredux` passes
- Non-timing tokens of the `bench:` line match a baseline build of the parent commit, for one loose-NIF run and one FNV cell run

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser-to-`Material` boundary, never in shaders or at render time
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A test or guard pins the change, and any guard that scans the old spelling was extended in the same commit

---
Source: refactor review of 2026-11-05 (at `16d9e18f3`), re-measured at `02f46f95d`. Counts come from line and regex scripts. Fix with: `/fix-issue <this issue>`.

_Immutable snapshot of the issue as filed. GitHub is authoritative for current state (`gh issue view 6690 --json state`)._
