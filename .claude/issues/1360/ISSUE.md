# #1360 — D6-01: BhkConvexSweepShape parsed but has no collision resolve arm

_Snapshot from AUDIT_NIFAL_2026-05-30. GitHub is authoritative for live state._

**Severity**: HIGH · **Source**: AUDIT_NIFAL_2026-05-30 (D6-01) · cross-ref `/audit-nifal`

**Dimension**: Collision · **Tier Violated**: no-leak (canonical-resolution completeness) · **Game Affected**: Oblivion (NIF v10.0.1.0); any title authoring a convex-sweep collider

**Location**: `crates/nif/src/import/collision.rs::resolve_shape_inner` (no arm; falls through to the unsupported-shape `None` ~L447-452). Block parsed at `crates/nif/src/blocks/collision/shape_compound.rs:204`, dispatched at `crates/nif/src/blocks/mod.rs:1104`.

**Description**: `BhkConvexSweepShape { shape_ref, material, radius }` is a wrapper shape carrying a child `shape_ref`. There is no `downcast_ref::<BhkConvexSweepShape>()` arm in `resolve_shape_inner`, so the block reaches the unsupported-shape fallback and returns `None` — the wrapped child is never resolved. #1329 (CLOSED) added the *parser* arm (block now lands in the scene) but not the *resolve* arm, so the collider that was previously lost to file truncation is now lost to the resolve fallback — **the leak migrated from the parser tier to the canonical tier.**

**Impact**: Oblivion convex-sweep colliders silently vanish — the mesh renders but has no collision.

**Suggested Fix**: mirror the `BhkMoppBvTreeShape` recurse-into-wrapped-shape arm:
```rust
if let Some(s) = block.as_any().downcast_ref::<BhkConvexSweepShape>() {
    return resolve_shape(scene, s.shape_ref, visited);
}
```
(The `radius` convex-inflation is a refinement; resolving the inner shape recovers the bulk of the authored collision.)

## Completeness Checks
- [ ] **SIBLING**: D6-02 (`BhkMeshShape`) is the same #1329 parse-then-drop class — fix both together; add a test that EVERY dispatched `Bhk*Shape` has a resolve arm (the structural guard against the next migration).
- [ ] **TESTS**: Regression test — a synthetic `BhkConvexSweepShape` wrapping a box resolves to the box's `CollisionShape`, not `None`.
