# #1339 — D3-03: Worldspace transition re-acquires sky textures without releasing prior set

_Snapshot as filed from AUDIT_FNV_2026-05-30 (d3-03). GitHub is authoritative for live state — query `gh issue view 1339 --json state`._

**Severity**: HIGH · **Dimension**: Cell Loading · **Source**: AUDIT_FNV_2026-05-30 (D3-03)

**Location**: `byroredux/src/scene/world_setup.rs:191-312` (re-acquire) ; `byroredux/src/main.rs:1218` (transition handler) ; `byroredux/src/cell_loader/unload.rs:125-149` (the #1199 deferral comment)

**Description**: `apply_worldspace_weather` resolves 4 cloud layers + 1 CLMT sun sprite (refcount bumps), which are worldspace-scoped and intentionally survive per-cell unload (#1199). On a door transition to an Exterior destination (main.rs:1218), the handler tears down the interior and drains streaming — neither releases the old worldspace's sky textures — then calls `apply_worldspace_weather` again, re-acquiring a fresh set while the old set stays pinned. The `SkyParamsRes::texture_indices()` accessor built for this release is dead on the release side.

**Evidence**: `unload_cell` explicitly skips SkyParamsRes textures (unload.rs:125-149, the #1199 comment). main.rs around the transition calls `apply_worldspace_weather` (main.rs:1218) with no preceding `drop_texture`/`texture_indices` call (grep of main.rs for `texture_indices`/`drop_texture` near the transition returns nothing). #1199 is CLOSED with the deferral baked in; the boundary release was never added.

**Impact**: Up to 5 GPU textures (4 cloud + 1 sun sprite; bindless slots + VkImages) leaked per interior→exterior / exterior→exterior worldspace crossing, for the process lifetime. Self-caps per unique worldspace but accumulates for a player door-walking between Mojave/DLC regions.

**Suggested Fix**: In the Exterior transition handler, before the new `apply_worldspace_weather`, read `SkyParamsRes::texture_indices()` from the current resource and `ctx.texture_registry.drop_texture` each non-zero / non-fallback handle — the boundary release #1199 deferred.

## Completeness Checks
- [ ] **SIBLING**: Apply the same boundary release on the interior→interior transition path if it ever seeds sky resources.
- [ ] **DROP**: Verify no double-drop when the new worldspace re-resolves a texture path identical to the old one (shared registry slot).
- [ ] **TESTS**: Regression test — cross two worldspaces and assert the first worldspace's sky-texture refcounts are released.
