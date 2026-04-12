# Legacy Compatibility Audit — 2026-04-12

**Scope**: Gamebryo 2.3 vs Redux — scene graph, NIF, transforms, materials, animation, strings
**Baseline**: Prior audit 2026-04-10 (28 findings). This audit checks fix status and discovers new gaps.

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0     |
| HIGH     | 0     |
| MEDIUM   | 7     |
| LOW      | 10    |
| **Total** | **17** |

**Major progress since April 10.** Of the 28 prior findings (5 HIGH, 10 MEDIUM, 13 LOW):
- **15 FIXED** — property inheritance, frustum culling, BSTriShape skinning, Morrowind NIFs, AnimationStack non-transform channels + root motion, secondary texture slots, vertex color modes, NiZBufferProperty, NiSpecularProperty, TBC rotation, NiUV/NiTextureTransform controllers, NifScene::validate_refs(), NiBillboardNode, case-insensitive string interning
- **5 OPEN (already tracked)** — NiSwitchNode (#212), blend factors (#213), text key events (#211), String-keyed channels (#231), NiPathController animation conversion
- **8 PARTIALLY OPEN or NEW** — see findings below

All 5 prior HIGH findings are now resolved. No new CRITICAL or HIGH gaps. The remaining gaps are MEDIUM (visual fidelity) and LOW (cleanup/polish).

---

## Prior Finding Resolution

| Prior ID | Title | Prior Sev | Status | Notes |
|----------|-------|-----------|--------|-------|
| D4-06 | Property inheritance | HIGH | **FIXED** | #208/#209, property accumulation stack in walk.rs |
| AR-01 | AnimationStack non-transform channels | HIGH | **FIXED** | #207, float/color/bool channels applied |
| AR-02 | AnimationStack root motion | HIGH | **FIXED** | #207, split_root_motion on blended position |
| D2-08 | BSTriShape skinning | HIGH | **FIXED** | VF_SKINNED vertex decode in mesh.rs |
| D2-02 | Morrowind NIFs empty | HIGH | **FIXED** | Inline RTTI type name reading |
| D1-01/D3-06 | WorldBound never populated | MEDIUM | **FIXED** | FrustumPlanes, LocalBound/WorldBound propagation |
| D4-02 | Secondary texture slots | MEDIUM | **FIXED** | #214, dark/detail/gloss/glow imported |
| D4-03 | NiVertexColorProperty mode | MEDIUM | **FIXED** | #214, vertex_mode 0/1/2 handled |
| D2-06 | NiSwitchNode/NiLODNode | MEDIUM | **OPEN** | Existing: #212 |
| D4-01 | NiAlphaProperty blend factors | MEDIUM | **OPEN** | Existing: #213 |
| AR-03 | Text key events | MEDIUM | **OPEN** | Existing: #211 |
| AR-04 | NiGeomMorpherController | MEDIUM | **PARTIAL** | Parsed, but morph index hardcoded to 0 |
| AR-06 | NiUVController | MEDIUM | **FIXED** | NiTextureTransformController all 5 ops |
| SI-01 | Channels keyed by String | MEDIUM | **OPEN** | Existing: #231 |
| SI-03 | Case sensitivity | LOW | **FIXED** | StringPool lowercases before intern |
| D2-03 | Oblivion no block sizes | MEDIUM | **MITIGATED** (LOW) | 100% parse rate, 177K NIFs |
| D2-09 | No link validation | LOW | **FIXED** | #226, NifScene::validate_refs() |
| D2-10 | Tests need game data | LOW | **MITIGATED** | Synthetic fixtures added |
| D2-07 | NiBillboardNode | LOW | **FIXED** | billboard_system + Billboard component |
| AR-05 | NiPath/LookAtController | LOW | **PARTIAL** | Block parsing added, animation conversion not |
| AR-07 | Keyframe time Vec allocs | LOW | **OPEN** | Per-sample Vec<f32> allocation |
| AR-08 | TBC rotation plain SLERP | LOW | **FIXED** | Log-space cubic Hermite |
| SI-02 | NIF Arc<str> not StringPool | LOW | **BY DESIGN** | nif crate intentionally standalone |
| SI-04 | Clip name/text keys heap | LOW | **OPEN** | Existing: #231 |
| D1-02 | SceneFlags not populated | LOW | **OPEN** | Component exists, never attached |
| D1-03 | Controllers/extra not wired | LOW | **OPEN** | Only BSXFlags/BSBound extracted |
| D3-05 | Inline coord swap | LOW | **OPEN** | 10+ inline `[x,z,-y]` sites |
| D4-04 | NiZBufferProperty | LOW | **FIXED** | z-test/z-write extracted and applied |
| D4-05 | UV transform per slot | LOW | **PARTIAL** | Base slot only, rotation/center dropped |
| D4-08 | NiSpecularProperty | LOW | **FIXED** | Enable flag checked |
| D4-09 | NiMaterialProperty amb/diff | LOW | **PARTIAL** | Diffuse used as fallback, ambient dropped |

---

## New Findings

### LC-01: NIF-embedded controllers not traversed during mesh import
- **Severity**: MEDIUM
- **Dimension**: Scene Graph Decomposition
- **Location**: `crates/nif/src/import/walk.rs`
- **Status**: NEW
- **Description**: NiObjectNET's `controller_ref` chains (NiVisController, NiFlipController, NiTextureTransformController on geometry nodes) are never traversed during import. The animation system only handles KF-file controllers. Water surfaces are static, fire textures don't animate, visibility effects don't toggle in interior cells.
- **Impact**: All mesh-embedded animations lost. Affects waterfalls, lava, torches, and animated signage.
- **Suggested Fix**: Walk controller chains during import, convert to float/bool channels on the target entity.

### LC-02: NiGeomMorpherController morph index hardcoded to 0
- **Severity**: MEDIUM
- **Dimension**: Animation Readiness
- **Location**: `crates/nif/src/anim.rs:351`
- **Status**: NEW (partial fix of prior AR-04)
- **Description**: Controller is now handled in import_sequence but morph target index is hardcoded to `MorphWeight(0)`. Multi-target facial animation collapses all weights to a single morph.
- **Impact**: Only first morph target animates. Facial expressions limited to single shape.
- **Suggested Fix**: Iterate morph target interpolators, emit separate `MorphWeight(i)` channels.

### LC-03: Alpha test function bits not extracted
- **Severity**: MEDIUM
- **Dimension**: Property → Material Mapping
- **Location**: `crates/nif/src/import/material.rs`
- **Status**: NEW
- **Description**: NiAlphaProperty flags bits 10-12 encode the alpha test function (LESS, GREATER, EQUAL, etc.). Only the enable bit and threshold are extracted. Default GREATER_EQUAL assumed but content can override.
- **Impact**: Meshes with inverted alpha test (LESS) render incorrectly — cutout geometry shows wrong pixels.
- **Suggested Fix**: Extract test function bits and map to Vulkan CompareOp.

### LC-04: dark_texture slot (multiplicative lightmap) parsed but never imported
- **Severity**: MEDIUM
- **Dimension**: Property → Material Mapping
- **Location**: `crates/nif/src/import/material.rs`
- **Status**: NEW
- **Description**: NiTexturingProperty slot 1 (`dark_texture`) is used for baked shadow/lightmap data on Oblivion interior architecture. The slot is parsed at the block level but never extracted in the material import path.
- **Impact**: Missing baked shadows on Oblivion interiors. Architecture appears flat-lit.
- **Suggested Fix**: Extract dark_texture path alongside other secondary slots.

### LC-05: AnimationStack clones channel Vecs per frame
- **Severity**: MEDIUM
- **Dimension**: Animation Readiness
- **Location**: `byroredux/src/systems.rs:454-456`
- **Status**: NEW
- **Description**: The stack path clones `float_channels`, `color_channels`, `bool_channels` from the dominant clip every frame to work around lock ordering. Heap allocation on every frame per animated entity.
- **Impact**: Performance: unnecessary per-frame allocations in the animation hot path.
- **Suggested Fix**: Cache clip handle + time as locals, drop the stack lock, re-access the registry without cloning.

### LC-06: Stale ImportedSkin doc comment
- **Severity**: LOW
- **Dimension**: NIF Format Readiness
- **Location**: `crates/nif/src/import/mod.rs:207-210`
- **Status**: NEW
- **Description**: Doc comment says BSTriShape weights are "currently not extracted" but `mesh.rs:504-505` does extract them. Misleading.
- **Suggested Fix**: Update the doc comment.

### LC-07: Per-secondary-slot UV transforms discarded
- **Severity**: LOW
- **Dimension**: Property → Material Mapping
- **Location**: `crates/nif/src/blocks/properties.rs:307-312`
- **Status**: NEW (extends prior D4-05)
- **Description**: UV transforms for secondary texture slots (detail, glow, gloss) are parsed but discarded. Only the base slot's translation/scale are consumed. Rotation, center, and transform_method also dropped.
- **Impact**: Detail/glow textures with independent tiling render at wrong scale. Low real-world frequency.

### LC-08: NiAlphaProperty no-sorter flag (bit 13) not extracted
- **Severity**: LOW
- **Dimension**: Property → Material Mapping
- **Location**: `crates/nif/src/import/material.rs`
- **Status**: NEW
- **Description**: Bit 13 of NiAlphaProperty flags disables depth sorting for this mesh. Not extracted. With the new depth-sorted alpha blending (#241), this flag becomes relevant.
- **Suggested Fix**: Extract bit 13, skip depth sorting for flagged meshes.

---

## Positive Confirmations (No Gaps)

These areas were verified correct with no issues:

- **NiMatrix3 → Quat conversion**: SVD fallback for degenerate rotations works correctly
- **Transform propagation**: BFS over Parent/Children hierarchy, parent-before-child ordering
- **Coordinate conversion**: Z-up → Y-up applied correctly at quaternion level
- **NIF version coverage**: All game generations detected (Morrowind through Starfield)
- **Block type coverage**: ~215 types across 18 submodules, 100% parse rate across 177K NIFs
- **Property inheritance**: Accumulation stack in walk.rs, child overrides parent per type
- **Frustum culling**: LocalBound → WorldBound propagation → FrustumPlanes intersection test
- **Case-insensitive interning**: StringPool lowercases before hashing, matches Gamebryo behavior
- **AnimationStack blending**: Transform + non-transform channels + root motion all functional

---

## Priority Fix Order

1. **LC-01** (MEDIUM) — NIF-embedded controllers: visual fidelity for water/lava/fire
2. **#212** (MEDIUM) — NiSwitchNode child selection: double-rendered geometry
3. **#213** (MEDIUM) — NiAlphaProperty blend factors: additive/multiplicative FX
4. **LC-03** (MEDIUM) — Alpha test function: cutout rendering correctness
5. **LC-04** (MEDIUM) — dark_texture lightmap: Oblivion interior lighting
6. **#211** (MEDIUM) — Text key events: gameplay event synchronization
7. **LC-02** (MEDIUM) — Morph target indexing: facial animation
8. **LC-05** (MEDIUM) — Stack channel clone: performance
9. **#231** (LOW) — FixedString animation channels + clip names
10. **LC-06..LC-08** (LOW) — Doc comment, UV transforms, no-sorter flag
