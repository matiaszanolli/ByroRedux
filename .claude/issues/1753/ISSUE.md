# TD2-005: inline Z-up->Y-up axis-swap leaks outside the canonical coord module

_Filed 2026-06-26 as #1753 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1753` for live state)._

**Severity**: LOW · **Dimension**: 2 — Logic Duplication
**Location**: `crates/nif/src/import/mesh/sse_recon.rs:268,291,380` · `crates/nif/src/import/mesh/tangent.rs:88-92,254` · `byroredux/src/cell_loader/terrain.rs:351`
**Status**: NEW · **Audit**: TD2-005

## Description
The exact Z-up→Y-up `(x,y,z) → (x,z,-y)` axis-swap (identical to the canonical `byroredux_core::math::coord::zup_to_yup_pos`) is re-typed inline on direction vectors. The #1044 consolidation sweep (which lists the five places the flip "used to" live) missed these array-form sites because they operate on tuple-destructured `(x,y,z)` rather than `NiPoint3`. 40 other call sites route correctly through the helper.

## Evidence
```
// sse_recon.rs:268 / :291 / :380
positions.push([x, z, -y]); normals.push([nx, nz, -ny]); tangents.push([bx, bz, -by, sign]);
// tangent.rs:88-92
let t_yup = [bethesda_bx, bethesda_bz, -bethesda_by]; let n_yup = [n_zup.x, n_zup.z, -n_zup.y];
// terrain.rs:351
let position = [bx, bz, -by];
```
The comments at every site even acknowledge it ("same `(x, y, z) → (x, z, -y)` swap … throughout import").

## Impact
LOW — the math is trivially correct and no fix has diverged. But it is a policy leak: any future change to the axis convention at the import boundary would silently miss these sites, reintroducing the exact bug class #1044 was created to eliminate.

## Suggested Fix
Route each through `byroredux_core::math::coord::zup_to_yup_pos([x,y,z])` (positions/normals) and the `[..., sign]` tangent variant flipping only the xyz triplet. tangent.rs/sse_recon.rs already import the core math crate; terrain.rs needs the `use`.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: the flip lives only in `import/coord.rs` / `anim/coord.rs` / `byroredux_core::math::coord` after the fix
- [ ] **TESTS**: SSE-recon + tangent + terrain import tests still produce identical vertex data
