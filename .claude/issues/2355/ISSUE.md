# SF-D8-04: NIFAL collision slice never fires on Starfield — all colliders route to undecoded BhkSystemBinary, non-Architecture content spawns with no collider at all

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2355
**Labels**: bug,nif-parser,medium,legacy-compat

---

**Severity**: MEDIUM
**Dimension**: 8 — NIFAL Canonical Material Translation (Starfield audit, 2026-08-03)
**Location**: `byroredux/src/cell_loader/spawn.rs:1477-1478` (synthesized-trimesh fallback, `RenderLayer::Architecture` gate)
**Status**: NEW, CONFIRMED against current code

## Description

The NIFAL collision slice never fires on Starfield content at all — 100% of Starfield colliders route to the undecoded `BhkSystemBinary` blob (33,867 `bhkNPCollisionObject` + 22,895 `bhkPhysicsSystem` + 316 `bhkRagdollSystem` in Meshes01, zero `bhk*Shape` blocks of any kind). `BhkMultiSphereShape`/`BhkConvexListShape` translation, while correctly implemented for Oblivion→FO4, is dead code with respect to Starfield — sharper and broader than the ROADMAP's existing "ragdolls blocked on `BhkSystemBinary`" note, since it's *all* Starfield collision, not just ragdolls.

The synthesized-trimesh fallback is also narrower than the shape arms it stands in for: confirmed it only fires for `RenderLayer::Architecture` (`spawn.rs:1478`: `&& base_layer == byroredux_core::ecs::components::RenderLayer::Architecture`) — so Starfield Clutter/Actor/container content currently spawns with **no collider at all**, not even an approximate one.

## Evidence

- `spawn.rs:1477-1478`: the synthesized-trimesh-collider-ghost fallback is gated on `RenderLayer::Architecture` — confirmed no broader gate exists.
- Corpus histogram (this audit, Dimension 8): zero `bhk*Shape` blocks anywhere in Starfield content; all collision references are `BhkSystemBinary`-backed.

## Impact

All Starfield Clutter/Actor/container content is currently non-collidable (no physics interaction at all), not merely "using an approximate collider." This is a real, measurable rendering/gameplay gap, broader in scope than the existing ROADMAP "ragdolls blocked" note.

## Suggested Fix (short term)

Widen the synthesized-trimesh fallback beyond `RenderLayer::Architecture` to cover Clutter/Actor/container layers too (approximate collision is better than none), and log a once-per-cell count of dropped `BhkSystemBinary` colliders so the gap is measurable going forward.

## Completeness Checks
- [ ] **SIBLING**: Confirm the same `BhkSystemBinary` gap applies to FO76 (same collision family)
- [ ] **CANONICAL-BOUNDARY**: Fix is in `byroredux/src/cell_loader/spawn.rs`, the spawn-time fallback, not the NIFAL translation boundary itself — the real fix (decoding `BhkSystemBinary`) is future work tracked separately in PHYSAL notes. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins the widened fallback (non-Architecture Starfield content gets a synthesized collider)
