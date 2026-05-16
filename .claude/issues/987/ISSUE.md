# #987 — NIF-D5-ORPHAN-C: Band C orphan-parse bucket — cloth / decal placement / collision-query proxy / inv markers / W-array (long-term deferred)

Labels: enhancement, nif-parser, import-pipeline, low
State: OPEN

**Source**: #974 Band C — orphan-parse follow-up (long-term deferred bucket)
**Severity**: LOW (no shipping-cell-render impact; each needs a substantial subsystem that isn't on any current milestone)
**Domain**: NIF import + future subsystem stubs

## Description

Five orphan-parse types where the parser dispatches cleanly but no near-term consumer is planned. Documented here so future audits don't re-discover them.

| Type | Authored on | What it would drive | Why deferred |
|---|---|---|---|
| `BsClothExtraData` | FO4 capes / dynamic cloth | Real-time cloth simulation (PBD / position-based dynamics) on cape mesh subsets | Cloth sim is not on any current milestone; M28 Rapier3D handles rigid bodies, not soft bodies. Would need Rapier3D Joints + custom PBD pass or a dedicated cloth crate |
| `BsDecalPlacementVectorExtraData` | FO4 placed decals | Per-mesh decal layer (blood splatters, scorch marks pre-baked into mesh) | Renderer-side decal pass not yet implemented; current decal handling is via NIF flag bits + render-state, not a dedicated decal pipeline |
| `BsCollisionQueryProxyExtraData` | FO76 collision-query optimization | Skip BLAS for query-only collision shapes (footstep raycasts, hover-target tests) | Optimization, not correctness; the default BLAS path produces the same query results |
| `BsInvMarker` | Skyrim/FO4 inventory thumbnails | UI inventory-grid thumbnail camera params (orbit center, zoom, rotation) | Needs Pip-Boy / SkyrimUI inventory grid renderer (Scaleform). M44 audio is closer to landing than this |
| `BsWArray` | FO3+ old Havok ragdoll skin weights | Pre-Skyrim ragdoll constraint system (Havok 2010-era) | Skyrim+ uses bhkRagdollSystem / bhkPhysicsSystem; pre-Skyrim ragdolls fall back to the engine's default skeletal-pose-null (acceptable for FO3/FNV) |

## Suggested approach

**Close this issue as "documented out-of-scope"** — no implementation work planned in the next several milestones. The parser keeps dispatching them so the data is available if/when a future subsystem needs it; the orphan-status is intentional.

Re-open with a specific implementation plan when:
- M-CLOTH (no current milestone) — `BsClothExtraData`
- Renderer-decal-pass (no current milestone) — `BsDecalPlacementVectorExtraData`
- M-INVENTORY-UI (Scaleform inventory grid) — `BsInvMarker`
- Pre-Skyrim ragdoll port (not on roadmap) — `BsWArray`
- BLAS-skip optimization pass (likely never — query-only proxy is a niche optimization) — `BsCollisionQueryProxyExtraData`

## Completeness Check (closure documentation)

- [ ] Add a comment block at each of the five parser sites in `crates/nif/src/blocks/` citing this issue + the "intentionally orphan-parse" rationale, so a future audit sees the explicit decision rather than re-discovering them
- [ ] Update `docs/audits/AUDIT_NIF_<date>.md` orphan checklist to mark these five as "documented as won't-fix-soon (NIF-D5-ORPHAN-C)" rather than as ongoing gaps

## Source quote (audit report)

> Band C (won't-fix soon): cloth, decal placement, large-ref tagging — deferred to specific milestones, document the rationale in `blocks/mod.rs` next to the dispatch arm

`docs/audits/AUDIT_NIF_2026-05-12.md` § HIGH → NIF-D5-NEW-01 (orphan-parse meta).

Related: #974 (meta), #869 (original orphan-parse instance).
