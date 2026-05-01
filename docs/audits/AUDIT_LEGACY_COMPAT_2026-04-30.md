# Legacy Compatibility Audit — 2026-04-30

**Scope**: Gamebryo 2.3 vs Redux — scene graph, NIF format, transforms, materials, animation, strings
**Baseline**: Prior audit 2026-04-24 (11 open: 0 CRITICAL / 0 HIGH / 2 MEDIUM / 9 LOW)
**Sessions reviewed**: 6 days. Commits since baseline: 41 (M40 closeout, NIF audit 04-26 / 04-28 / 04-30 fix waves, M41.0 Phase 0–4 NPC-spawn pipeline, NiSkinData field-order root-cause fix).

---

## Executive Summary

| Severity | Prior Open | Fixed | Still Open | New | **Total Open** |
|----------|-----------:|------:|-----------:|----:|---------------:|
| CRITICAL | 0 | — | 0 | 0 | **0** |
| HIGH     | 0 | — | 0 | 0 | **0** |
| MEDIUM   | 2 | 2 | 0 | 1 | **1** |
| LOW      | 9 | 4 | 5 | 1 | **6** |
| **Total** | **11** | **6** | **5** | **2** | **7** |

**Both prior MEDIUMs closed end-to-end** — D5-NEW-01 (`NiLookAtInterpolator`) and D5-NEW-02 (`NiPathInterpolator`) reach `extract_transform_channel` and route through dedicated handlers (`anim.rs:1015`, `anim.rs:1032`; closed via #604 / #605).

The codebase remains structurally complete for static + skeletal content rendering across Oblivion → Starfield. The single new MEDIUM (`D3-NEW-01`) is a legacy-formula gap exposed during the M41.0 Phase 1b.x skinning bug-bash: the team correctly captured `NiSkinData::skinTransform` (the global skin-space → skeleton-space offset) onto `SkinnedMesh.global_skin_transform`, but `compute_palette_into` does not multiply it into the per-bone palette. The gap is **self-acknowledged** in code (`scene.rs:1729-1734`) — a prior right-multiply attempt looked visually worse and was reverted pending invariant checks. Listed here so the milestone tracker reflects it.

The single new LOW (`D5-NEW-03`) is the Phase 2 deferral note: the KF idle clip loads and registers but no `AnimationPlayer` is attached to spawned NPCs (also self-acknowledged at `npc_spawn.rs:622-654`). Strictly a milestone gap, not a defect.

---

## Prior Finding Resolution

| Prior ID | Title | Prior Sev | Status | Notes |
|----------|-------|-----------|--------|-------|
| D5-NEW-01 | `NiLookAtInterpolator` not decoded | MEDIUM | **FIXED** | #604; `anim.rs:1015` dispatches |
| D5-NEW-02 | `NiPathInterpolator` not decoded | MEDIUM | **FIXED** | #605; `anim.rs:1032` dispatches |
| D4-NEW-02 | UV clamp mode discarded | LOW | **FIXED** | #761; `MaterialInfo.texture_clamp_mode` + per-clamp samplers (`texture_registry.rs:397-454`) |
| D6-NEW-01 | `MaterialInfo` `String` allocs | LOW | **FIXED** | `MaterialInfo` now `Option<FixedString>` via `intern_texture_path` (`material/mod.rs:31, 278-323`) |
| D4-NEW-01 (3 of 4) | Wireframe / Dither / Shade properties dropped | LOW | **FIXED** | #703; `walker.rs:588-603` consumes them |
| D4-NEW-01 (NiFogProperty) | NiFogProperty dropped | LOW | **STILL OPEN** | one-off: 1 in vanilla Oblivion |
| D1-NEW-01 | `NiNode.culling_mode` not honoured outside `BsMultiBoundNode` | LOW | **STILL OPEN** | `walk.rs:213-216, 451-453` still `BsMultiBoundNode`-only |
| N2-NEW-01 | `VF_INSTANCE = 0x200` no decoder | LOW | **STILL OPEN** | declared `#[allow(dead_code)]` at `tri_shape.rs:380`; only test references |
| #337 | `NiStencilProperty` not in Vulkan pipeline | LOW | **STILL OPEN** | only `is_two_sided()` consumed (`walker.rs:569-576`) |
| #221 (carry from older audits) | `NiMaterialProperty` ambient/diffuse | LOW | **FIXED** | closed earlier in window |
| #231 (carry from older audits) | NIF string-table double-intern | LOW | **FIXED** | closed earlier in window |

---

## New Findings

### MEDIUM

#### D3-NEW-01: `SkinnedMesh::compute_palette_into` drops `global_skin_transform`
- **Severity**: MEDIUM (legacy-correctness gap; self-acknowledged in code)
- **Dimension**: Transform Compatibility
- **Game Affected**: All games that ship Gamebryo `NiSkinData` chains where the top-level `skinTransform` is non-identity. In practice this includes Bethesda body NIFs across Oblivion / FO3 / FNV — the M41.0 Phase 1b.x debug session showed Doc Mitchell's `NiSkinData::skinTransform` is the cyclic-permutation rotation `[[0,1,0],[0,0,1],[1,0,0]]`, not identity.
- **Location**: [crates/core/src/ecs/components/skinned_mesh.rs:130-141](crates/core/src/ecs/components/skinned_mesh.rs#L130-L141), [byroredux/src/scene.rs:1729-1743](byroredux/src/scene.rs#L1729-L1743)
- **Status**: NEW
- **Description**: Gamebryo's `NiSkinningMeshModifier` composes `boneMatrix = boneToWorld × inverse(NiSkinData::skinTransform) × inverse(NiSkinData::bones[i].skinTransform)`. OpenMW's `RigGeometry::cull` uses the equivalent `vec × invBindMatrix × boneSkelSpace × skinToSkel × dataTransform`. Both engines apply the **two** inverse terms — the global and the per-bone. Redux currently composes only the per-bone term:

  ```rust
  // skinned_mesh.rs:137
  Some(world) => world * *bind_inv,   // missing × global_skin_inverse
  ```

  The `global_skin_transform` field IS captured on import (`scene.rs:1735`) and stored on `SkinnedMesh` (line 55), but no consumer multiplies it. The author comment at `scene.rs:1729-1734` documents the open question: a prior attempt to right-multiply `global_skin_transform` produced a visually worse result, suggesting either an OSG-row-vec ↔ glam-column-major translation error or that one of `global_skin_transform` / `bind_inverse` already encodes the skin-space offset and the second multiplication double-applies it.

- **Impact**: Skinned meshes whose `NiSkinData::skinTransform` is identity (FO4+ `BSSkin`, many particle systems) render correctly. Skinned meshes with non-identity global term — including Bethesda body NIFs — render with the global rotation factored out of the palette. The visual symptom on Doc Mitchell was the head/body misalignment that the M41.0 Phase 1b.x bug-bash chased before discovering the deeper field-order parser bug (`#767`); the field-order fix landed correctly but only restored half the legacy formula.

- **Suggested Fix** (next investigation step, not a one-line change): write a numeric invariant test — pick a real skinned NIF (Doc Mitchell, Sunny Smiles head NIF, vanilla skeleton.nif), compute palette via the OpenMW formula in a unit test, compare against current Redux output with the global term commented in vs. out. Establish ground truth before re-attempting the multiplication. Field-order on disk is now correct (`#767`); the remaining question is purely runtime composition.

- **Related**: M41.0 Phase 1b.x commits 8ec6a69, 4177e06, 41aed79; OpenMW credit in commit c34cb6a; live-debugger plumbing in commits 22e4bb0, 41aed79.

### LOW

#### D5-NEW-03: NPC `AnimationPlayer` never attached to spawned skeleton
- **Severity**: LOW (intentional Phase 2.x deferral; not a defect)
- **Dimension**: Animation Readiness
- **Game Affected**: FNV / FO3 / Oblivion (kf-era games — pre-baked-FaceGen path is bind-pose-only by design per the M41.0 plan, so Skyrim+ is out of scope until Phase 6).
- **Location**: [byroredux/src/npc_spawn.rs:622-655](byroredux/src/npc_spawn.rs#L622-L655)
- **Status**: NEW (companion to D3-NEW-01; both surfaced during M41.0 Phase 1b.x debug)
- **Description**: M41.0 Phase 2 added the KF loader machinery — `humanoid_default_idle_kf_path` resolves, `byroredux_nif::anim::import_kf` parses, `AnimationClipRegistry::add` registers — but the spawn function deliberately discards the resulting handle:

  ```rust
  // npc_spawn.rs:654
  let _ = idle_clip_handle;
  // (skel_root similarly dropped at line 655)
  ```

  The 30-line comment block at lines 622-653 documents the deferral: bind-pose mismatch between idle KF and the spawned skeleton entity caused NPCs to "vanish" (vertex weighting against unresolved channel roots), so Phase 2 ships only the clip-registration plumbing without the `AnimationPlayer` attach. Compare `cell_loader.rs:2193-2194` which DOES attach `AnimationPlayer.root_entity = Some(...)` for non-NPC placements.

- **Impact**: NPCs spawn at correct world positions and render in bind pose, but do not idle-breathe. The user-visible effect at Goodsprings / Doc Mitchell scenes is "NPCs are static T-pose-ish even though the animation runtime is otherwise live". The KF parsing + clip registration costs are paid (~few KB heap + clip table churn); only the per-entity `AnimationPlayer` insert is missing.

- **Suggested Fix**: Pair with D3-NEW-01 — once the palette composition matches the legacy formula, the bind-pose mismatch root cause may dissolve, at which point the clip-handle assignment becomes a one-line addition. If the gap turns out to be real (KF channel roots reference a node that's not in the merged `node_by_name` map), expand the map to cover both the skeleton hierarchy and the KF target nodes, then attach.

---

## Verified Working — Confirmed No Gaps

Re-verified during this audit (don't re-investigate):

- **NiSkinData on-disk parse** — Both `skin_transform` (top-level global) and `bones[i].skin_transform` (per-bone bind-inverse) now use `read_ni_transform_struct` (Rotation→Translation→Scale per nif.xml STRUCT order), distinct from the NiAVObject inline `read_ni_transform` (Translation→Rotation→Scale). [skin.rs:100, 108](crates/nif/src/blocks/skin.rs#L100-L108) — fixed by #767 / commit 8ec6a69.
- **`BsPackedGeomDataCombined.transform`** — Same field-order fix applied; Starfield combined-geom blocks now parse correctly. [extra_data.rs:662](crates/nif/src/blocks/extra_data.rs#L662) — #767 / commit 4177e06.
- **FaceGen morph evaluator coord frame** — `apply_morphs` (`crates/facegen/src/eval.rs:62-101`) operates on raw NIF-local Z-up vertices; the Z-up→Y-up swap happens at scene-import / placement-root level via `cell_loader.rs:864-877`. No double-axis-swap.
- **NPC spawn parenting** — `npc_spawn.rs:319-323, 366-367, 446-447, 595-596` parents body/head under the placement root through the standard `Parent` / `Children` ECS components, not a parallel propagation system. Single canonical transform path.
- **Component coverage** — 21 component modules in `crates/core/src/ecs/components/`. The set covers every `NiAVObject`-derived field needed for static + skeletal rendering, lighting, collision, and animation — `Transform` / `GlobalTransform` / `Parent` / `Children` / `MeshHandle` / `Material` / `Name` / `LocalBound` / `WorldBound` / `BSXFlags` / `SceneFlags` / `LightSource` / `CellRoot` / `Camera` / `Billboard` / `Particle` / `SkinnedMesh` / `Texture` / `FormIdComponent` / `Collision` / `Animated*` set.
- **Animation runtime** — Format-neutral. KF importer feeds `AnimationClip` (channels, players, stack blending, root motion, text events all present in `crates/core/src/animation/`). M41.0 Phase 6 Havok stub will attach to the same runtime — no per-format runtime split is required.
- **Property pipeline (consumed)** — `NiAlphaProperty` / `NiTexturingProperty` / `NiMaterialProperty` / `NiSpecularProperty` / `NiVertexColorProperty` / `NiZBufferProperty` / `NiStencilProperty` (two-sided only) / `NiWireframeProperty` / `NiDitherProperty` / `NiShadeProperty`. Eight of nine vanilla `NiProperty` subtypes have material consumers; only `NiFogProperty` remains.
- **Shader properties (consumed)** — `BSLightingShaderProperty` (8 variants), `BSEffectShaderProperty` (full inc. env_map / env_mask via #719), `BSShaderNoLightingProperty`, `BSShaderPPLightingProperty` (inc. emissive at bsver>34 via #716), `SkyShaderProperty`, `TileShaderProperty`.
- **Animation interpolator dispatch** — `NiTransformInterpolator`, `NiBSplineCompTransformInterpolator`, `NiLookAtInterpolator`, `NiPathInterpolator`, the `NiBlendInterpolator` family, `NiBoolTimelineInterpolator`. Five of five vanilla transform-channel interpolators reach a downcast in `extract_transform_channel`.
- **String interning** — NIF header strings flow as `Arc<str>` and are accepted by `MaterialInfo` / `AnimationClip` channels via `FixedString` without the prior double-intern (#231).
- **Boundary semantics** — All 17 `until=` exclusive-boundary sites flipped from `<=` to `<` (#765, #769; commits 171d840 + 2befd8c). Final grep `version() <= NifVersion(0x...)` returns zero hits.
- **Allocation budget** — All file-driven Vec allocations route through `NifStream::allocate_vec`, including the inner `weights_per_vert` row in BSGeometry (#768). Final hostile-bytes regression test pins behaviour.

---

## Priority Fix Order

1. **D3-NEW-01** (MEDIUM) — Skin palette `global_skin_transform` composition. Investigation step (write the numeric invariant test against OpenMW formula) must precede any code change. Pairs with D5-NEW-03.
2. **D5-NEW-03** (LOW) — NPC `AnimationPlayer` attach. Likely dissolves once D3-NEW-01 is closed; carry as a milestone reminder.
3. **D1-NEW-01** (LOW) — `NiNode.culling_mode` generalisation. ~5 LOC; small win.
4. **#337** (LOW) — Stencil pipeline. Defer; visual impact extremely rare.
5. **D4-NEW-01 NiFogProperty** (LOW) — Defer; ~1 instance in shipped content.
6. **N2-NEW-01** (LOW) — `VF_INSTANCE`. Defer until renderer needs per-vertex instance data.

---

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-04-30.md
```
