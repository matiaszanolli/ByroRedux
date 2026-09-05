# #3830: REN-2026-09-05-DOC-01: shader-pipeline.md's volumetrics_inject.comp binding table documents 12 bindings; the live shader declares 24

Filed from `docs/audits/AUDIT_RENDERER_2026-09-05.md` (REN-2026-09-05-DOC-01) via `/audit-publish`, 2026-09-05.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3830 --json state`.

---

**Severity**: LOW — authoritative-doc divergence, no runtime effect
**Dimension**: Volumetrics / SSBO-Indexing (doc-rot)
**Source**: `docs/audits/AUDIT_RENDERER_2026-09-05.md` (`REN-2026-09-05-DOC-01`)

*Reported independently by both Dimension 2 and Dimension 16 of the same sweep — merged into one finding.*

## Location

`docs/engine/shader-pipeline.md`, the table headed ``` `volumetrics_inject.comp` (12 bindings, widened by #2228/#2231's fog-volume work…) ```. Ground truth: `crates/renderer/shaders/volumetrics_inject.comp`, bindings 0–23 inclusive.

## Description

The table stops at binding 11 (`detailDensityNoise`). The live shader declares twelve more, entirely undocumented: 12 (`emissionHistory`), 13 (`previousEmissionHistory`), 14–17 (`combustionState` / `previousCombustionState` / `combustionDynamics` / `previousCombustionDynamics`), 18 (`CombustionLightMomentBuffer`), 19–21 (`BoundaryInstanceBuffer` / `BoundaryVertexBuffer` / `BoundaryIndexBuffer`), 22–23 (`combustionOptical` / `previousCombustionOptical`) — exactly doubling the documented count.

The doc's own hedge ("verify against the source before relying on this table for a new binding") proved warranted, but the drift has gone well past what that caveat implies.

## Evidence

`grep -oE "binding = [0-9]+" crates/renderer/shaders/volumetrics_inject.comp | grep -oE "[0-9]+" | sort -nu` → `0`…`23` (24 values). The doc table's last row is binding 11. Re-verified at publish time.

## Impact

Audit-methodology only, but higher-stakes than a typical size mislabel — this is the doc every audit is told to prefer over re-deriving facts from source. An auditor trusting its binding *count* could reasonably conclude the boundary-geometry read path does not exist rather than checking it. **That is close to what actually happened**: bindings 19–21 are where this sweep's CRITICAL finding (#3829) lives, and they are absent from the table.

## Related

#2228 / #2231 (the fog-volume work the table says it was last updated for), `REN-2026-08-30-D2-03`, #2314 / `TD3-206` (the reason this table exists at all — it drifted once before and got a table for it; the table has now drifted again).

## Suggested Fix

Regenerate the table's rows from the live `layout(...)` declarations. Longer-term, a table that has now drifted twice needs a generation script or a drift-pinning test — the pattern `froxel_grid_cost_matches_the_memory_budget_doc` already uses (`include_str!` the doc, assert substrings) applied to the shader's actual binding count would have caught this before it reached double the documented size.

## Completeness Checks

- [ ] **SIBLING**: Other per-shader binding tables in the same doc checked for the same drift
- [ ] **TESTS**: A drift-pinning test ties the doc's binding count to the live shader
