# Issue #974

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/974
**Title**: NIF-D5-NEW-01: Orphan-parse sweep — ≥36 dispatched types parsed but never downcast by importer (expands #869)
**Labels**: enhancement, nif-parser, import-pipeline, high
**Audit source**: docs/audits/AUDIT_NIF_2026-05-12.md

---

**Source**: `docs/audits/AUDIT_NIF_2026-05-12.md` (Dim 5)
**Severity**: HIGH
**Dimension**: Coverage Gaps
**Game Affected**: FO3, FNV, Skyrim SE, FO4, FO76 (FO4/FO76 heaviest hit)
**Tracking meta-issue** — expands the open #869 (NiWireframeProperty / NiShadeProperty parsed but never consumed) to the full orphan-parse surface.

## Description

Every dispatched block type carries a parser; not every dispatched type has an importer consumer. Confirmed via static enumeration of all `downcast_ref::<T>` / `get_as::<T>` call sites in `crates/nif/src/import/walk.rs` and `crates/nif/src/anim.rs` (24 distinct targets) vs the 211 dispatch arms in `crates/nif/src/blocks/mod.rs`. At least 36 dispatched types fall in the orphan set — they parse cleanly into `NifScene` but no downstream system reads them.

## Orphan set (non-exhaustive)

| Type | Game(s) | Visible drop |
|---|---|---|
| `BsTreadTransfInterpolator` | FO3/FNV/SSE | Vertibird / Liberty Prime tread animation static (#941 parser landed, no consumer) |
| `BsEyeCenterExtraData` | FO4/FO76 | FaceGen dialogue camera frames NIF origin instead of eye centroid (#720 parser landed, no consumer) |
| `BsConnectPointParents` | FO4/FO76 | Weapon-mod attachment points dropped — weapon mod system can't thread them |
| `BsConnectPointChildren` | FO4/FO76 | Same |
| `NiPSysGravityFieldModifier` | FO3+ | Particle gravity ignored |
| `NiPSysVortexFieldModifier` | FO3+ | Particle vortex forces ignored |
| `NiPSysDragFieldModifier` | FO3+ | Particle drag ignored |
| `NiPSysTurbulenceFieldModifier` | FO3+ | Particle turbulence ignored |
| `NiPSysAirFieldModifier` | FO3+ | Particle air-wind ignored |
| `NiPSysRadialFieldModifier` | FO3+ | Particle radial forces ignored |
| `NiLightColorController` | FO3+ | Every lantern emits constant color |
| `NiLightIntensityController` | FO3+ | Every campfire emits constant intensity |
| `NiLightRadiusController` | FO3+ | Plasma weapon light radius static |
| `NiLightDimmerController` | FO3+ | No flicker / pulse |
| `NiFloatExtraDataController` | FO3+ | Time-varying float extra data static |
| `BsClothExtraData` | FO4 | Capes / dynamic cloth ignored |
| `BsDecalPlacementVectorExtraData` | FO4 | Placed decal layer dropped |
| `BsBehaviorGraphExtraData` | SSE/FO4/FO76 | Skyrim animation graph filename never resolved |
| `BsInvMarker` | SSE/FO4/FO76 | Inventory thumbnail camera dropped |
| `BsBound` | All | Pre-computed sphere ignored — engine recomputes bounds |
| `BsWArray` | FO3+ | Old Havok ragdoll skin-weight array dropped |
| `BsCollisionQueryProxyExtraData` | FO76 | #728 parser landed, no consumer |
| `BsDistantObjectLargeRefExtraData` | SSE | #942 parser landed, no consumer |
| `NiBsBoneLodController` | Oblivion | Creature LOD ignored |
| `BsRangeNode` (kind discriminator) | SSE | Blast/DamageStage/Debris kind stamped at parse but importer walks all four as plain NiNode |

## Why HIGH

The parser pays the cost, the importer surfaces a populated struct, the renderer/animation/equip systems silently drop the type-specific payload. The defect class is identical to open #869 (NiWireframeProperty + NiShadeProperty) — that's two types; this is ≥36. Aggregate visible-impact across shipping content is large.

## Suggested resolution

This is too broad for a single PR. Recommended path:

1. **Enumerate every dispatched type** (211 arms) and assert at least one `downcast_ref::<T>` consumer exists — codify as a build-time check or a doc-test in `import/mod.rs`
2. **Triage by visible-impact band**:
   - Band A (immediate): light controllers (4), particle field modifiers (6), `BsConnectPointParents/Children` (2) — ~12 types where the visible drop ships in vanilla cells
   - Band B (deferred): FaceGen camera, vertibird treads, behavior graph — visible but contextual
   - Band C (won't-fix soon): cloth, decal placement, large-ref tagging — deferred to specific milestones, document the rationale in `blocks/mod.rs` next to the dispatch arm
3. **Per-family follow-up issues** linked here for Band A first. Each family is a small PR (one `scene.get_as::<T>` site + a handful of ECS components to wire it through).

## Completeness Checks (for the per-family follow-ups)

- [ ] **SIBLING**: When wiring consumer for type X, check that sibling type Y (e.g. `NiLightIntensityController` sibling to `NiLightColorController`) is also wired in the same PR
- [ ] **TESTS**: Each consumer wiring includes at least one downcast assertion in an integration test
- [ ] **ECS**: Verify the target ECS component (LightSource, AnimationPlayer, etc.) actually consumes the new field — don't just populate a Component nobody reads (would be a meta-orphan)
- [ ] **DOC**: Per-family PRs document the wire convention in `import/types.rs` comment block

## Audit reference

`docs/audits/AUDIT_NIF_2026-05-12.md` § Findings → HIGH → NIF-D5-NEW-01.

Related: #869 (the original two-type instance of this pattern; close as a prerequisite or absorb into this tracking issue once Band A lands).

