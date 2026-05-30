# #1340 — D3-04: Door-walk + debug-console interior load discards CellLoadResult.lighting

_Snapshot as filed from AUDIT_FNV_2026-05-30 (d3-04). GitHub is authoritative for live state — query `gh issue view 1340 --json state`._

**Severity**: HIGH · **Dimension**: Cell Loading · **Source**: AUDIT_FNV_2026-05-30 (D3-04)

**Location**: `byroredux/src/cell_loader/transition.rs` (`load_interior_cell`, discards the result) and `byroredux/src/debug_load.rs` (`cell.load`, discards it); contrast `byroredux/src/scene.rs:185-214` (startup path applies it)

**Description**: The startup interior loader (scene.rs:185-214) reads `result.lighting` and installs `CellLightingRes::from_cell_lighting(lit, dir, is_interior=true)`. The door-walk transition path `load_interior_cell` and the `cell.load` debug command both call the same loader but discard the `CellLoadResult` (the lighting is parsed, then thrown away). So a runtime-loaded interior renders with the **previous** cell's `CellLightingRes`.

**Evidence**: scene.rs:185 `if let Some(ref lit) = result.lighting { ... world.insert_resource(CellLightingRes::from_cell_lighting(lit, dir, true)) }`. Neither `transition.rs` nor `debug_load.rs` references `CellLightingRes`/`from_cell_lighting`/`result.lighting` (grep returns nothing). The `is_interior` flag gates the directional sun (#1282 fix) and the interior clear color downstream.

**Impact**: Any interior reached at runtime (the M40 gameplay door-walk path, and the `cell.load` debug command) gets wrong ambient/fog, exterior clear color, and the directional sun leaking into a sealed interior — the exact sun-shaft failure #1282 closed for the gate-on-`is_interior` case. Hits FNV's reference interiors (saloon, vault, Doc Mitchell's) once door-walking is exercised. Only the `--cell` startup path is correct.

**Suggested Fix**: Move the `result.lighting → CellLightingRes::from_cell_lighting(..., true)` block into a shared helper called by the startup, transition, AND debug-console paths (or return `CellLoadResult` for each caller to install). Sharing the helper avoids re-duplicating the directional-rotation `euler_zup_to_quat_yup` logic.

## Completeness Checks
- [ ] **SIBLING**: Verify ALL three interior-load entry points (startup `--cell`, door-walk transition, `cell.load` debug) route through the shared lighting-apply helper.
- [ ] **TESTS**: Regression test — load an exterior, door-walk into an interior, assert `CellLightingRes.is_interior == true` and the directional sun is suppressed.
