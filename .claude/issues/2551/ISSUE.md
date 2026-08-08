# FO3-D5-NEW-03: Degenerate bhkConvexVerticesShape collapses to a 1mm ball instead of the trimesh fallback

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2551
**Finding ID**: FO3-D5-NEW-03

**Severity**: LOW
**Dimension**: FO3 Collision Import (Havok → CollisionShape)
**Location**: `crates/nif/src/import/collision/shape.rs:177-190` (`BhkConvexVerticesShape` resolve arm); `crates/physics/src/convert.rs:153-160` (the degenerate-hull fallback)
**Status**: NEW

## Description
Degenerate `bhkConvexVerticesShape` collapses to a 1mm ball instead of the trimesh fallback. 17 occurrences found in real FO3 base+DLC content. `SharedShape::convex_hull(&pts)` rejects hulls with fewer than 4 non-coplanar points (a real Rapier constraint), and the flattener's fallback for that rejection is a tiny `SharedShape::ball(1e-3)` — functionally invisible/uncollidable at that scale, rather than falling back to the mesh's own trimesh geometry (which the sibling `TriMesh` variant does use as its own degenerate fallback path).

## Evidence
Confirmed directly: `convert.rs:153-160` — `let shape = SharedShape::convex_hull(&pts).unwrap_or_else(|| { log::warn!(...); SharedShape::ball(1e-3) });`.

## Impact
17 real FO3 collision shapes silently become a 1mm ball collider — effectively non-colliding for gameplay purposes (a player or object will pass straight through where the content author authored a solid convex shape). Low blast radius (17 occurrences across the full FO3+DLC corpus) but a real functional gap, not cosmetic.

## Suggested Fix
When `convex_hull` construction fails, fall back to the shape's own trimesh geometry (same treatment the `TriMesh` variant already gets) rather than an invisible 1mm ball, so a degenerate convex hull still produces a collidable — if imprecise — shape.

## Completeness Checks
- [ ] **TESTS**: A regression test constructs a degenerate (< 4 non-coplanar point) convex hull and confirms it falls back to a trimesh, not a 1mm ball
