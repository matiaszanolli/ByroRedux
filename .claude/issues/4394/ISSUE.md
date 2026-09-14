# #4394 — NIFAL-D2-2026-09-14-01: Starfield BSGeometry local bound is in unscaled Havok units while its vertices are Havok-scaled — every Starfield mesh's `LocalBound` is ~70× too small

**Labels**: high,nifal,import-pipeline,renderer,bug,game:starfield
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: HIGH (rendering correctness: frustum culling and render-layer depth bias on every Starfield mesh)
- **Dimension**: Geometry/Transform
- **Tier Violated**: single-boundary (the Havok-unit conversion for the category is applied to `positions` but not to the sibling authored sphere; `local_bound_*` leaves the boundary in a different unit system)
- **Game Affected**: Starfield (all BSGeometry)
- **Location**: `crates/nif/src/import/mesh/bs_geometry.rs:374-413` (sphere used verbatim, `([cx, cy, cz], r)`), `crates/nif/src/blocks/bs_geometry.rs:421-431` (vertices multiplied by `HAVOK_SCALE`)
- **Status**: NEW (the same hypothesis as closed #2098, which was closed 2026-08-06 with only a `log::debug!` diagnostic; the real-data check it asked for was never run, and behaviour never changed, so this is not a regression)
- **Description**: `BSGeometryMeshData::parse` decodes positions as `unpack_norm_i16(v, scale, HAVOK_SCALE)`. The NIF-level `BSGeometry.bounding_sphere` is read raw and never scaled. `extract_bs_geometry` uses it verbatim whenever `r > 0`, under a comment that argues only its *basis* ("already in Y-up"), never its *units*. The #2098 guard `bs_geometry_bounding_sphere_mismatch` detects exactly this condition but only logs at debug level.
- **Evidence**: The orchestrator re-read both sites and confirmed that no scale is applied to the sphere. The agent's corpus census over 40,000 vanilla Starfield shapes found:
  - The verbatim sphere encloses the vertices on **0 / 40,000** shapes.
  - Sphere × 69.969 (centre and radius) encloses them on **39,939 / 40,000**.
  - Samples: `stsoccintsegsmwallmidbot_scktd01.nif` has r = 4.3397 and vertex extent 303.643 = r × K; `shpgenintpersmwallforemid02.nif` has r = 2.4225 and extent 169.499.
- **Impact**:
  - **Frustum culling**: `byroredux/src/render/static_meshes.rs:394` culls Starfield architecture while it is still largely on screen, so walls pop at frustum edges. They still shadow through the TLAS while invisible.
  - **Depth bias**: `escalate_small_static_to_clutter` (50-unit threshold) demotes nearly all Starfield architecture to Clutter depth bias. This is very likely the real mechanism behind the "#1294 trap" note.
  - **Cell bounds**: the cell foreground AABB (`byroredux/src/cell_loader/spawn.rs:205-222`) under-covers Starfield placements.
  - **Log noise**: the #2098 debug log fires on effectively every Starfield mesh.
- **Related**: #2098 (closed, log-only), #1294, `crates/nif/src/import/mesh/bs_geometry_bounding_sphere_tests.rs` (pins only the log helper).
- **Suggested Fix**: Scale the authored sphere's centre and radius by the Havok factor at the single extraction site; expose the constant or add a game-units accessor next to the parser. Fix the comment to state the unit contract. Promote the mismatch helper into a unit test. Add a data-gated corpus test mirroring `switchboard_precombine_transforms_match_authored_bounds`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
