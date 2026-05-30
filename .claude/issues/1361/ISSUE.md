# #1361 — D6-02: BhkMeshShape parsed but has no collision resolve arm

_Snapshot from AUDIT_NIFAL_2026-05-30. GitHub is authoritative for live state._

**Severity**: HIGH · **Source**: AUDIT_NIFAL_2026-05-30 (D6-02) · cross-ref `/audit-nifal`

**Dimension**: Collision · **Tier Violated**: no-leak (canonical-resolution completeness) · **Game Affected**: Oblivion (TES4 — the block exists only at v10.0.1.0)

**Location**: `crates/nif/src/import/collision.rs::resolve_shape_inner` (no arm). Block parsed at `crates/nif/src/blocks/collision/shape_mesh.rs:216`, dispatched at `crates/nif/src/blocks/mod.rs:1105`.

**Description**: `BhkMeshShape { radius, scale: [f32;4], data_refs: Vec<BlockRef> }` where `data_refs` reference `NiTriStripsData` — directly resolvable as a TriMesh exactly like `BhkNiTriStripsShape` (which `resolve_tri_strips_collision` ~L456 already handles). No resolve arm exists, so the block drops at the unsupported fallback despite the geometry being fully readable with existing code. Same #1329 parser-fixed-but-resolve-missing migration as D6-01.

**Impact**: Oblivion mesh-collision (the analogue of the handled `BhkNiTriStripsShape`) silently dropped.

**Suggested Fix**: add an arm building a strips merge over `s.data_refs` — refactor `resolve_tri_strips_collision` to take `&[BlockRef]` + scale, or inline the same vertex/strip-merge loop. Fold `BhkMeshShape.scale` (`[f32;4]`, per-axis) in alongside `havok_scale`.

## Completeness Checks
- [ ] **SIBLING**: paired with D6-01 — add the "every dispatched `Bhk*Shape` has a resolve arm" structural test once for both.
- [ ] **TESTS**: Regression test — a synthetic `BhkMeshShape` with one `NiTriStripsData` ref resolves to a `TriMesh` `CollisionShape`.
