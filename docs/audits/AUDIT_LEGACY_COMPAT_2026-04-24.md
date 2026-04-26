# Legacy Compatibility Audit — 2026-04-24

**Scope**: Gamebryo 2.3 vs Redux — scene graph, NIF format, transforms, materials, animation, strings
**Baseline**: Prior audit 2026-04-15 (16 open: 0 CRITICAL / 0 HIGH / 2 MEDIUM / 14 LOW)
**Sessions reviewed**: 9 days of bug-bash (commits 5698276 → a2a3fcd, sessions 13–18)

---

## Executive Summary

| Severity | Prior Open | Fixed | Still Open | New | **Total Open** |
|----------|-----------:|------:|-----------:|----:|---------------:|
| CRITICAL | 0 | — | 0 | 0 | **0** |
| HIGH     | 0 | — | 0 | 0 | **0** |
| MEDIUM   | 2 | 2 | 0 | 2 | **2** |
| LOW      | 14 | 11 | 3 | 6 | **9** |
| **Total** | **16** | **13** | **3** | **8** | **11** |

**13 of 16 prior findings resolved** during the bug-bash. Both prior MEDIUMs (LC-01 NIF-embedded controllers via #261; AR-08 NiBlendInterpolator via #334) are closed end-to-end. Surviving prior items are the 3 documented LOW polishers — #221 (ambient/diffuse), #231 (string double-intern), #337 (NiStencilProperty pipeline).

The codebase is structurally complete for static + skeletal content rendering across Oblivion, FO3, FNV, Skyrim, FO4. Remaining gaps are all fidelity improvements; none block content loading.

The 2 new MEDIUMs (D5-NEW-01, D5-NEW-02) are companion gaps: `extract_transform_channel` in [anim.rs:874-915](crates/nif/src/anim.rs#L874-L915) only dispatches to `NiTransformInterpolator` and `NiBSplineCompTransformInterpolator`; the recently-added `NiLookAtInterpolator` (commit 7548e64) and existing `NiPathInterpolator` parsers reach the function but silently return `None`.

---

## Prior Finding Resolution

| Prior ID | Title | Prior Sev | Status | Notes |
|----------|-------|-----------|--------|-------|
| LC-01 / #261 | NIF-embedded controllers | MEDIUM | **FIXED** | 37112c8 — walks `controller_ref` chain in anim.rs:368 |
| AR-08 / #334 | NiBlendInterpolator unconsumed | MEDIUM | **FIXED** | resolve_blend_interpolator_target follows dominant sub-interp |
| AR-09 / #338 | KFM state machine | LOW | **FIXED** | 07dc6b1 — AnimationController component |
| AR-10 / #339 | text key Vec\<String\> alloc | LOW | **FIXED** | visitor pattern + scratch buffer |
| SI-05 / #340 | per-frame channel name lowercase | LOW | **FIXED** | FixedString channels at clip load |
| N2-01 / #336 | VF_UVS_2 / VF_LAND_DATA constants | LOW | **FIXED** | abd91b5 — declared + tests |
| D1-04 / #335 | affected_nodes ignored | LOW | **FIXED** | resolved in walker |
| LC-06..LC-08 / #266 | doc / UV / no-sorter triple | LOW | **FIXED** | bundled close |
| D1-02 / #222 | SceneFlags not populated | LOW | **FIXED** | flow plumbed |
| D3-05 / #232 | inline coord swap helper | LOW | **FIXED** | helper extracted |
| #229 | keyframe alloc | LOW | **FIXED** | non-allocating closures |
| D4-09 / #221 | ambient/diffuse colors discarded | LOW | **STILL OPEN** | Material lacks the fields |
| SI-02+SI-04 / #231 | string double-intern + clip name heap | LOW | **STILL OPEN** | NIF Arc\<str\> still re-interned |
| D4-NEW-01 / #337 | NiStencilProperty pipeline | LOW | **STILL OPEN** | only is_two_sided() consumed |

---

## New Findings

### MEDIUM

#### D5-NEW-01: NiLookAtInterpolator reaches `extract_transform_channel` but is never decoded
- **Severity**: MEDIUM
- **Dimension**: Animation Readiness
- **Game Affected**: Oblivion (embedded look-at chains), FNV (~18 occurrences in R3 histogram), Skyrim (~5)
- **Location**: [crates/nif/src/anim.rs:874-915](crates/nif/src/anim.rs#L874-L915), [crates/nif/src/blocks/interpolator.rs:407-450](crates/nif/src/blocks/interpolator.rs#L407-L450)
- **Status**: NEW
- **Description**: Commit 7548e64 (Session 18) added the `NiLookAtInterpolator` block parser as the modern replacement for the deprecated `NiLookAtController`. The block parses correctly and `parse_block` dispatches it (mod.rs:597). However, when a `ControlledBlock.interpolator_ref` resolves to `NiLookAtInterpolator`, `extract_transform_channel` checks only `NiTransformInterpolator` (line 888) and `NiBSplineCompTransformInterpolator` (line 910), then returns `None`. The blend-target shim (#334) does not change this — it resolves a `NiBlendTransformInterpolator` to a sub-interpolator that may itself be a `NiLookAtInterpolator`. Result: NPC head/eye look-at sequences embedded in the NIF (rather than driven by KFM at runtime) are silently dropped.
- **Evidence**:
  ```rust
  // crates/nif/src/anim.rs:888-915
  if let Some(interp) = scene.get_as::<NiTransformInterpolator>(interp_idx) { ... }
  // Fall back to the Skyrim / FO4 NiBSplineCompTransformInterpolator path.
  if let Some(interp) = scene.get_as::<NiBSplineCompTransformInterpolator>(interp_idx) {
      return extract_transform_channel_bspline(scene, interp);
  }
  None  // <-- NiLookAtInterpolator + NiPathInterpolator fall through here
  ```
- **Impact**: Embedded look-at sequences silently degrade to static transforms. Single-clip KF playback works; NIFs with controller-manager chains containing a look-at sub-interpolator lose head tracking on creatures and dragons.
- **Suggested Fix**: Add a third branch that downcasts to `NiLookAtInterpolator`. Sample the look-at vector against the static transform from `NiAVObject`, emit either a constant TransformChannel (when target is the world origin) or a `LookAt` ECS component carrying the target ref so the runtime can compute the rotation each frame.

#### D5-NEW-02: NiPathInterpolator falls through `extract_transform_channel` the same way
- **Severity**: MEDIUM
- **Dimension**: Animation Readiness
- **Game Affected**: Oblivion (door swings), FO3/FNV (moving platforms), Skyrim (minecart rails, dragons)
- **Location**: [crates/nif/src/anim.rs:874-915](crates/nif/src/anim.rs#L874-L915), [crates/nif/src/blocks/interpolator.rs:556-620](crates/nif/src/blocks/interpolator.rs#L556-L620)
- **Status**: NEW
- **Description**: `NiPathInterpolator` (driven by `NiPosData` + `NiFloatData` + a percent-along-path interpolator) is fully parsed (regression test at mod.rs:1349-1373 from #394) and dispatches via `parse_block`. Like D5-NEW-01, it never reaches a downcast in `extract_transform_channel`, so spline-path-driven embedded animations are silently dropped. The legacy `NiPathController` is parsed but explicitly stubbed (mod.rs:567 comment "legacy NiTimeController"); content using the pre-Bethesda path setup loses translation entirely.
- **Impact**: Embedded path animations (door hinge sweeps, dragon flight curves authored as splines, minecart spline rails) static-pose. Scripted door open in Oblivion that ships a path interpolator on the door NIF — non-functional from the NIF side.
- **Suggested Fix**: Sample `NiPathInterpolator` at the same `BSPLINE_SAMPLE_HZ` cadence used for `NiBSplineCompTransformInterpolator`, emit linear-interpolated translation keys. Rotation may be derivable from path tangent (Frenet frame) but Gamebryo just held the static rotation — match that.

### LOW

#### D1-NEW-01: NiNode.culling_mode (BSVER ≥ 83) not consumed outside `BsMultiBoundNode`
- **Severity**: LOW
- **Dimension**: Scene Graph Decomposition
- **Game Affected**: Skyrim, FO4 (NiNode subclasses other than BsMultiBoundNode)
- **Location**: [crates/nif/src/blocks/node.rs:230-262](crates/nif/src/blocks/node.rs#L230-L262), [crates/nif/src/import/walk.rs:182-186](crates/nif/src/import/walk.rs#L182-L186)
- **Status**: NEW
- **Description**: Skyrim+ added a `culling_mode: u32` field to `NiNode` itself (BSVER ≥ 83), not just `BsMultiBoundNode`. Mode 2 (always-hidden) and 3 (force-culled) on a generic `NiNode` reach the parser correctly but only the `BsMultiBoundNode` downcast path at walk.rs:182-186 honors them. A plain `NiNode` with `culling_mode == 2` is recursed and rendered.
- **Impact**: Author-flagged invisible subtrees on plain NiNode parents render as visible. Most production NIFs use BsMultiBoundNode for occluders, so the in-the-wild count is small, but mod content using bare NiNode loses the hint.
- **Suggested Fix**: Generalize the check at walk.rs:182 — call `as_ni_node(block)` and check `node.culling_mode` regardless of subclass. Drop the BsMultiBoundNode-specific branch.

#### D4-NEW-01: NiFogProperty / NiWireframeProperty / NiDitherProperty / NiShadeProperty silently dropped
- **Severity**: LOW
- **Dimension**: Property → Material Mapping
- **Game Affected**: Oblivion (rare), FO3/FNV (rare)
- **Location**: [crates/nif/src/blocks/mod.rs:336-345](crates/nif/src/blocks/mod.rs#L336-L345) (parsed), [crates/nif/src/import/material.rs](crates/nif/src/import/material.rs) (no consumer for any of these four)
- **Status**: NEW
- **Description**: Four legacy `NiProperty` types reach `parse_block` and are stored on `NifScene` (`NiFogProperty` as a full struct, the other three reduced to `NiFlagProperty` blocks). None is checked in `extract_material_info`. Searching `material.rs` for these names returns zero hits. The fog property has fully decoded depth/color/flags fields (properties.rs:1203-1239) that go unread; the three flag properties lose their enable bits.
- **Impact**: Rare in shipped content (`NiFogProperty` ≈ 1 in vanilla Oblivion; the other three are mostly editor/debug). Per-mesh fog override falls back to global fog. Wireframe and flat-shading visual styles for debug content render as solid-smooth.
- **Suggested Fix**: Defer until a target asset surfaces the gap. If addressed: extend `MaterialInfo` with `fog_overrides: Option<(f32, NiColor, u16)>`, `wireframe: bool`, `flat_shaded: bool`, plus pipeline variants for the rasterization-mode toggles.

#### N2-NEW-01: VF_INSTANCE vertex desc flag declared but no decoder
- **Severity**: LOW
- **Dimension**: NIF Format Readiness
- **Game Affected**: FO4, FO76, Starfield (instanced statics)
- **Location**: [crates/nif/src/blocks/tri_shape.rs:361-364](crates/nif/src/blocks/tri_shape.rs#L361-L364)
- **Status**: NEW (companion to closed #336)
- **Description**: #336 added `VF_UVS_2` and `VF_LAND_DATA` constants and round-trip tests. The same commit declared `VF_INSTANCE = 0x200` but did not add a decode branch. Searching the file shows the constant is referenced only inside its own unit tests (lines 2362, 2382). `BsTriShape::parse` walks the desc word and skips unknown bits; instance-stream payload is silent-dropped.
- **Impact**: FO4+ instanced terrain pieces (cliffs, rocks reused thousands of times in worldspaces) carry per-instance data the renderer never receives. Currently the renderer doesn't consume per-vertex instance data anyway, so end-to-end impact is zero — flagging only because the gap will surface when GPU-driven instancing is wired.
- **Suggested Fix**: When VF_INSTANCE is needed for the renderer, decode the attribute slice and forward it as `BsInstanceStream` data on the imported mesh. Until then, add a debug counter to know if vanilla content actually exercises the bit.

#### D6-NEW-01: NIF header strings reach ECS via `Arc<str>` then Vec\<String\> path duplicates allocations
- **Severity**: LOW
- **Dimension**: String Interning
- **Game Affected**: All
- **Location**: [crates/nif/src/import/material.rs:126-154](crates/nif/src/import/material.rs#L126-L154), [byroredux/src/scene.rs](byroredux/src/scene.rs)
- **Status**: NEW (companion to #231 — same root cause, separate fix path)
- **Description**: #231 covers the NIF header `Arc<str>` table being re-interned into ECS `StringPool` at clip load. A second site has the same shape: `MaterialInfo` carries `Option<String>` and `Vec<String>` for texture paths (filename strings already interned in the NIF header). Each material clone re-allocates these heap Strings; resolution paths in render.rs deref the String each frame. The texture registry already caches by path — so the duplication is on the import-staging side, not the GPU side.
- **Impact**: Per-cell allocator pressure on cell load. Typical interior cell with 200 meshes × 4 texture slots × 60-byte path = ~50 KB redundant heap. Not a frame-time issue.
- **Suggested Fix**: Bundle with #231 — accept a `&StringPool` parameter into `MaterialInfo::extract` and store `FixedString` instead of `String`. Update `TextureRegistry::resolve` to accept `FixedString` lookups.

#### D4-NEW-02: NiTexturingProperty UV mode (clamp/wrap) parsed but discarded
- **Severity**: LOW
- **Dimension**: Property → Material Mapping
- **Game Affected**: Oblivion, FO3, FNV
- **Location**: [crates/nif/src/blocks/properties.rs](crates/nif/src/blocks/properties.rs), [crates/nif/src/import/material.rs:543](crates/nif/src/import/material.rs#L543)
- **Status**: NEW
- **Description**: `TexDesc.clamp_mode` (an enum: WRAP_S_WRAP_T, WRAP_S_CLAMP_T, CLAMP_S_WRAP_T, CLAMP_S_CLAMP_T) is parsed for every texture descriptor but the importer keeps only `texture_index` and `uv_set`. The renderer creates a single sampler per texture format with hardcoded `VK_SAMPLER_ADDRESS_MODE_REPEAT`. Meshes that author clamp-on-edge for decals or skybox seams render with repeating bleed.
- **Impact**: Edge bleeding on decals and seam-clamped textures. Visible on scope crosshairs, some Oblivion architecture trim, and pre-shader skybox quads.
- **Suggested Fix**: Promote `TexDesc.clamp_mode` to `MaterialInfo`, then to `Material`, then create samplers per `(format, clamp_mode)` pair in the renderer.

---

## Verified Working — Confirmed No Gaps

Re-verified during this audit (don't re-investigate):

- **Transform composition** — Matrix3 → Quat (Shepperd + SVD), Z-up → Y-up, local × parent = world propagation. All import paths (mesh, particle, light, collision, animation) use `zup_matrix_to_yup_quat` consistently.
- **Negative-determinant rotation matrices** — SVD repair at coord.rs:147 ensures `det = +1` for proper rotations.
- **Non-uniform scale** — Detected at draw.rs:505-520 for inverse-transpose normal matrix.
- **NiAVObject controller chain** — `walk_controller_chain` (anim.rs:368) traverses `controller_ref` → `next_controller_ref` end-to-end. The dead `controller_ref: BlockRef::NULL` writes in `import/mod.rs` populate a separate ImportedNode field that no consumer reads — controllers are imported via the parallel chain walker, not the ECS struct field.
- **bhkRigidBodyT** — Shares binary layout with bhkRigidBody (collision.rs:171, regression #546). Downcast at import/collision.rs:31 handles both.
- **Property pipeline (consumed)** — NiAlphaProperty (full), NiTexturingProperty (slots 0-3 + decal slots), NiMaterialProperty (specular, emissive, shininess, alpha), NiSpecularProperty (enable bit), NiVertexColorProperty (source mode + lighting mode), NiZBufferProperty (depth test/write/function — fixed in #398), NiStencilProperty (is_two_sided helper). Decal flag handling unified via shared helper (#454).
- **Shader properties** — BSLightingShaderProperty (8 variants, #562), BSEffectShaderProperty (full), BSShaderNoLightingProperty (with falloff cone + decal — #451/#454), SkyShaderProperty (#550), TileShaderProperty (#455).
- **NiBlendInterpolator family** — `resolve_blend_interpolator_target` (anim.rs:1335) follows the dominant sub-interpolator. Float, Bool, Point3 blend interpolators handled (#548 NiBoolTimelineInterpolator).
- **TBC rotation** — Hermite log-space SLERP with per-neighbor rebasing (#230).
- **Text key wrap behavior** — Loop wrap handled in [prev_time, duration] ∪ [0, curr_time] (#339).
- **Channel name pre-interning** — `AnimationClip.channels` uses `HashMap<FixedString, TransformChannel>` (#340).
- **NIF version coverage** — All seven game generations parsing at 100% per R3 baseline (177,286 NIFs).
- **BSBound import** — [import/mod.rs:497-522](crates/nif/src/import/mod.rs#L497-L522) extracts BSBound onto `ImportedScene.bs_bound` alongside BSXFlags.

---

## Priority Fix Order

1. **D5-NEW-01** (MEDIUM) — NiLookAtInterpolator dispatch: closes the parser-vs-importer gap from the just-merged 7548e64.
2. **D5-NEW-02** (MEDIUM) — NiPathInterpolator dispatch: same shape, finishes the family.
3. **D1-NEW-01** (LOW) — NiNode.culling_mode: small, ~5-line change, removes the BsMultiBoundNode-only restriction.
4. **D4-NEW-02** (LOW) — UV clamp mode: visible on decals, larger plumbing scope (sampler dedup).
5. **N2-NEW-01** (LOW) — VF_INSTANCE: defer until renderer uses per-vertex instance data.
6. **D6-NEW-01** (LOW) — Bundle with #231 if/when that gets prioritised.
7. **D4-NEW-01** (LOW) — Defer; rare content.
8. **#221**, **#231**, **#337** — Carry-over LOW polish.

---

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-04-24.md
```
