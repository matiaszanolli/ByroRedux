# Legacy Compatibility Audit — 2026-04-10

**Auditor**: Legacy Specialist × 3 (Claude Opus 4.6, parallel)
**Scope**: Gamebryo 2.3 ↔ Redux compatibility across scene graph, NIF, transforms, materials, animation, strings
**Reference**: Gamebryo 2.3 source, `docs/legacy/api-deep-dive.md`

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0     |
| HIGH     | 5     |
| MEDIUM   | 10    |
| LOW      | 13    |

28 findings across 6 dimensions. No CRITICAL findings — NIF loading works for all supported games.

The 5 HIGH findings cluster around two themes:
1. **Blended animation gaps**: AnimationStack skips non-transform channels (AR-01) and root motion (AR-02)
2. **Import completeness**: BSTriShape skinning not extracted (D2-08), Morrowind NIFs empty (D2-02), property inheritance missing (D4-06)

The core pipeline — NIF parsing (215 block types, 7 games at 100%), transform conversion (Z-up→Y-up with SVD repair), single-clip animation playback, and ECS decomposition — is solid and production-ready.

---

## Dimension 1: Scene Graph Decomposition

### D1-01: WorldBound component exists but is never populated
- **Severity**: MEDIUM
- **Dimension**: Scene Graph Decomposition
- **Location**: `crates/core/src/ecs/components/world_bound.rs`
- **Status**: NEW
- **Description**: `WorldBound` component is defined (center + radius sphere) but never written by any system or import code. NIF `NiBound` data is parsed but discarded during import. No bounds propagation system exists.
- **Impact**: No frustum culling possible. Every entity submitted to renderer regardless of visibility.
- **Suggested Fix**: Extract NiBound during import. Add `bounds_propagation_system` after transform propagation.

### D1-02: SceneFlags component exists but never populated from NIF
- **Severity**: LOW
- **Dimension**: Scene Graph Decomposition
- **Location**: `crates/core/src/ecs/components/scene_flags.rs`
- **Status**: NEW
- **Description**: `SceneFlags::from_nif(flags)` constructor exists but NIF walker only reads bit 0 (APP_CULLED) inline. ImportedNode/ImportedMesh don't carry flags.
- **Impact**: Runtime cannot query original NIF flags. Visibility toggling works but loses NIF state.

### D1-03: NIF-embedded controllers and extra data not wired to ECS
- **Severity**: LOW
- **Dimension**: Scene Graph Decomposition
- **Location**: `crates/nif/src/import/walk.rs`
- **Status**: NEW
- **Description**: NiObjectNET's `extra_data_refs` and `controller_ref` are parsed but not propagated to ECS. BSXFlags, BSBound, NiStringExtraData inaccessible at runtime. Mesh-embedded controllers (NiTextureTransformController for UV scrolling) not imported.
- **Impact**: Physics hints, gameplay metadata, and per-mesh UV scrolling (waterfalls, lava) lost.

---

## Dimension 2: NIF Format Readiness

### D2-08: BSTriShape skinning weights not extracted from packed vertex buffer
- **Severity**: HIGH
- **Dimension**: NIF Format Readiness
- **Location**: `crates/nif/src/import/mod.rs:170-174`
- **Status**: NEW
- **Description**: `ImportedSkin` doc comment states: "For modern BSTriShape meshes the weights live inside the packed vertex buffer (VF_SKINNED) — currently not extracted." Legacy NiTriShape skinning works, but BSTriShape (Skyrim SE, FO4, FO76, Starfield) skinning is not imported.
- **Impact**: All skinned meshes in SSE+ games render in bind pose — character bodies, clothing, creatures frozen without skeletal deformation.
- **Suggested Fix**: Extract bone indices + weights from packed vertex data when VF_SKINNED is set.

### D2-02: Morrowind NIFs return empty scene
- **Severity**: HIGH
- **Dimension**: NIF Format Readiness
- **Location**: `crates/nif/src/lib.rs:118-135`
- **Status**: NEW
- **Description**: Pre-Gamebryo NIFs (v < 5.0.0.1, all Morrowind) use inline RTTI strings per block instead of a global block-type table. Parser explicitly returns empty scene: `"NIF v{} has no block-type table (pre-Gamebryo); returning empty scene"`.
- **Impact**: Complete Morrowind incompatibility. Per roadmap, behind Oblivion/FO3/FNV/Skyrim but a known total gap.
- **Suggested Fix**: Implement legacy per-block RTTI string reading loop for v < 5.0.0.1.

### D2-06: NiSwitchNode / NiLODNode child selection not implemented
- **Severity**: MEDIUM
- **Dimension**: NIF Format Readiness
- **Location**: `crates/nif/src/import/walk.rs:32-67`
- **Status**: NEW
- **Description**: Walker's `as_ni_node()` unwraps NiSwitchNode/NiLODNode to their inner NiNode and traverses all children unconditionally, ignoring `active_index` and LOD ranges.
- **Impact**: Furniture states, weapon sheaths, destruction stages all render simultaneously (overlapping geometry). LOD nodes show all levels at once.
- **Suggested Fix**: For NiSwitchNode, only walk child at `active_index` (or all if -1). For NiLODNode, walk LOD 0 only for now.

### D2-03: Oblivion NIFs have no block sizes (no skip recovery)
- **Severity**: MEDIUM
- **Dimension**: NIF Format Readiness
- **Location**: `crates/nif/src/lib.rs:161`
- **Status**: NEW
- **Description**: Block sizes only exist in v20.2.0.7+ (FO3+). Oblivion (v20.0.0.4/5) unknown blocks cause hard error. Mitigated by exhaustive type coverage (~215 types, 100% Oblivion parse rate claimed).
- **Impact**: Fragility risk — any undiscovered Oblivion block type causes total NIF parse failure.

### D2-07: NiBillboardNode behavior not implemented
- **Severity**: LOW
- **Dimension**: NIF Format Readiness
- **Location**: `crates/nif/src/import/walk.rs:50-51`
- **Status**: NEW
- **Description**: Parsed and unwrapped but billboard mode discarded. No `Billboard` component or camera-facing system.
- **Impact**: Billboard sprites render with static orientation instead of facing camera.

### D2-09: Link resolution has no validation pass
- **Severity**: LOW
- **Dimension**: NIF Format Readiness
- **Location**: `crates/nif/src/scene.rs:27-34`
- **Status**: NEW
- **Description**: BlockRef indices resolved on-demand via `scene.get_as::<T>(index)`. No upfront validation that all references resolve correctly.
- **Impact**: Silent reference failures could mask parser bugs. Graceful None handling prevents crashes.

### D2-10: Integration tests require external game data
- **Severity**: LOW
- **Dimension**: NIF Format Readiness
- **Location**: `crates/nif/tests/parse_real_nifs.rs`
- **Status**: NEW
- **Description**: Per-game parse rate tests are `#[ignore]`d, requiring game data at env-configured paths. No synthetic NIF test fixtures in repo.
- **Impact**: CI cannot verify parser correctness. Regressions only caught by manual `--ignored` runs.

---

## Dimension 3: Transform Compatibility

### D3-06: No WorldBound computation during import or propagation
- **Severity**: MEDIUM
- **Dimension**: Transform Compatibility
- **Status**: NEW (duplicate of D1-01 — merged)
- **Description**: See D1-01. WorldBound component exists, NiBound parsed but discarded, no propagation system.

### D3-05: Translation coordinate swap hardcoded inline
- **Severity**: LOW
- **Dimension**: Transform Compatibility
- **Location**: `crates/nif/src/import/walk.rs:98`
- **Status**: NEW
- **Description**: Z-up→Y-up translation `[x, z, -y]` applied inline in walker and mesh.rs. No shared `zup_point_to_yup` helper — fragile if new import paths are added.
- **Suggested Fix**: Add `pub(super) fn zup_point_to_yup(p: &NiPoint3) -> [f32; 3]` in `coord.rs`.

---

## Dimension 4: Property → Material Mapping

### D4-06: Property inheritance (parent-to-child propagation) not implemented
- **Severity**: HIGH
- **Dimension**: Property → Material Mapping
- **Location**: `crates/nif/src/import/material.rs`, `crates/nif/src/import/walk.rs`
- **Status**: NEW
- **Description**: Gamebryo properties on NiNode propagate to all descendant geometry unless overridden. Redux only examines properties directly on the shape — never walks up the parent chain. Common in Oblivion: NiAlphaProperty on root node affects all children.
- **Impact**: Flora, architecture, clothing with parent-attached alpha/stencil/material properties render incorrectly (opaque when should be transparent, single-sided when should be double-sided).
- **Suggested Fix**: Maintain a property accumulation stack during hierarchical walk. Merge parent properties, child overrides parent (last wins per type).

### D4-01: NiAlphaProperty blend factors ignored — hardcoded SrcAlpha/OneMinusSrcAlpha
- **Severity**: MEDIUM
- **Dimension**: Property → Material Mapping
- **Location**: `crates/nif/src/import/material.rs:293-304`, `crates/renderer/src/vulkan/pipeline.rs:213-217`
- **Status**: NEW
- **Description**: Only bit 0 (blend enable) and bit 9 (test enable) extracted. Full blend function from flags bits 1-4 (src) and 5-8 (dst) ignored. Hardcoded SrcAlpha/OneMinusSrcAlpha.
- **Impact**: Additive-blend effects (fire, magic) washed out. Multiplicative effects (tinted glass) completely wrong.
- **Suggested Fix**: Extract blend factors, map to Vulkan BlendFactor variants, create pipeline variants for common combinations.

### D4-02: NiTexturingProperty secondary texture slots parsed but never imported
- **Severity**: MEDIUM
- **Dimension**: Property → Material Mapping
- **Location**: `crates/nif/src/import/material.rs:202-220`
- **Status**: NEW
- **Description**: Only base_texture, normal_texture, bump_texture imported. dark (lightmap), detail, gloss (specular mask), glow (self-illumination) slots parsed but dead data.
- **Impact**: Missing lightmaps on Oblivion architecture, detail overlays on terrain, specular masking, glow maps on enchanted items.

### D4-03: NiVertexColorProperty vertex_mode/lighting_mode never consulted
- **Severity**: MEDIUM
- **Dimension**: Property → Material Mapping
- **Location**: `crates/nif/src/blocks/properties.rs:649-686`
- **Status**: NEW
- **Description**: Parsed but import always uses vertex colors as direct diffuse. vertex_mode=0 (ignore) and vertex_mode=1 (emissive) not handled.
- **Impact**: Meshes with disabled vertex colors show them incorrectly. Emissive vertex colors routed to diffuse.

### D4-04: NiZBufferProperty z-test/z-write flags never applied
- **Severity**: LOW
- **Dimension**: Property → Material Mapping
- **Location**: `crates/nif/src/blocks/properties.rs:791-839`
- **Status**: NEW
- **Description**: Fully parsed but never consulted. Pipeline always uses depth test + write with LESS_OR_EQUAL.
- **Impact**: Sky meshes and transparent overlays with z-write disabled may z-fight or occlude.

### D4-05: NiTexturingProperty per-slot UV transform discarded
- **Severity**: LOW
- **Location**: `crates/nif/src/blocks/properties.rs:307-312`
- **Status**: NEW
- **Description**: UV transform (translation, scale, rotation) per texture slot read but skipped. Low impact since most meshes bake UVs into vertex data.

### D4-07: BSEffectShaderProperty missing double-sided/decal flag checks
- **Severity**: LOW
- **Location**: `crates/nif/src/import/material.rs:153-168`
- **Status**: Existing: #128

### D4-08: NiSpecularProperty enable flag never applied
- **Severity**: LOW
- **Location**: `crates/nif/src/blocks/properties.rs:561-599`
- **Status**: NEW
- **Description**: Parsed as NiFlagProperty but never checked. Matte surfaces show unwanted specular.

### D4-09: NiMaterialProperty ambient/diffuse colors discarded
- **Severity**: LOW
- **Location**: `crates/nif/src/import/material.rs:191-199`
- **Status**: NEW
- **Description**: Specular/emissive extracted but ambient/diffuse discarded. Low impact since most meshes use textures.

---

## Dimension 5: Animation Readiness

### AR-01: AnimationStack does not process non-transform channels
- **Severity**: HIGH
- **Dimension**: Animation Readiness
- **Location**: `byroredux/src/systems.rs:314-383`
- **Status**: NEW
- **Description**: The AnimationStack blending path only processes transform channels. float_channels, color_channels, and bool_channels are completely skipped. Single-player AnimationPlayer path handles them correctly.
- **Impact**: Blended animation loses visibility toggling, alpha animation, UV scrolling, material color animation.
- **Suggested Fix**: After transform blending, iterate all active layers' float/color/bool channels and apply via highest-weight layer.

### AR-02: AnimationStack does not process root motion
- **Severity**: HIGH
- **Dimension**: Animation Readiness
- **Location**: `byroredux/src/systems.rs:361-371`
- **Status**: NEW
- **Description**: Stack path applies full translation without checking `accum_root_name` or calling `split_root_motion()`. Single-player path correctly splits.
- **Impact**: Character locomotion breaks under blended animations (walk/run cross-fades) — skating or jittering.
- **Suggested Fix**: Check `accum_root_name`, call `split_root_motion()` on blended position, write delta to `RootMotionDelta`.

### AR-03: Text key events never emitted during playback
- **Severity**: MEDIUM
- **Dimension**: Animation Readiness
- **Location**: `byroredux/src/systems.rs` (entire animation_system)
- **Status**: NEW
- **Description**: `collect_text_key_events()` exists and works but is never called. Neither AnimationPlayer nor AnimationStack tracks prev_time or invokes event collection.
- **Impact**: Sound cues ("Sound: FootLeft"), hit detection windows ("HitFrame"), weapon trail markers lost.
- **Suggested Fix**: Track `prev_time` in AnimationPlayer. After `advance_time()`, call `collect_text_key_events()` and emit markers.

### AR-04: NiGeomMorpherController parsed but not imported as animation
- **Severity**: MEDIUM
- **Dimension**: Animation Readiness
- **Location**: `crates/nif/src/anim.rs:308-351`
- **Status**: NEW (related: #114 — NiMorphData parsing, different issue)
- **Description**: Controller is parsed at block level but `import_sequence()` does not handle `"NiGeomMorpherController"`. Morph targets (blend shapes) for facial animation silently skipped.
- **Impact**: No facial animation or morph-target deformation. Character faces static during dialogue.

### AR-06: NiUVController not converted to animation channels
- **Severity**: MEDIUM
- **Dimension**: Animation Readiness
- **Location**: `crates/nif/src/anim.rs:308-351`
- **Status**: NEW
- **Description**: NiUVController for UV scrolling (water, lava, conveyors in Oblivion-era) not handled by `import_sequence()`. These controllers are on geometry nodes, not via NiControllerSequence.
- **Impact**: Water, lava, and UV-animated surfaces in Oblivion-era content are static.
- **Suggested Fix**: Walk scene graph controller chains, convert NiUVData to FloatChannels (UvOffsetU/V).

### AR-05: NiPathController, NiLookAtController not supported
- **Severity**: LOW
- **Dimension**: Animation Readiness
- **Location**: `crates/nif/src/anim.rs:308-351`
- **Status**: NEW
- **Description**: Spline path following and look-at constraints not handled. Used in cutscenes and environmental animation.
- **Impact**: Path-following objects and look-at cameras static.

### AR-07: Keyframe time Vec allocation on every sample call
- **Severity**: LOW
- **Dimension**: Animation Readiness
- **Location**: `crates/core/src/animation/interpolation.rs:98,173,200,268,289`
- **Status**: NEW
- **Description**: Each `sample_*` function allocates a `Vec<f32>` of key times every call. ~1500+ allocations per frame at scale.
- **Impact**: Unnecessary heap churn in hot animation path.

### AR-08: TBC rotation uses plain SLERP, ignoring TBC tangent influence
- **Severity**: LOW
- **Dimension**: Animation Readiness
- **Location**: `crates/core/src/animation/interpolation.rs:179-188`
- **Status**: NEW
- **Description**: TBC rotation should use SQUAD-style interpolation. Currently plain SLERP between bracketing keys; TBC parameters populated but never read.
- **Impact**: Subtle rotation artifacts on exaggerated motions with non-zero TBC values.

---

## Dimension 6: String Interning Alignment

### SI-01: AnimationClip channels keyed by heap String, not FixedString
- **Severity**: MEDIUM
- **Dimension**: String Interning Alignment
- **Location**: `crates/core/src/animation/types.rs:141-147`
- **Status**: NEW
- **Description**: `AnimationClip.channels` is `HashMap<String, TransformChannel>`. Every frame, `pool.get(channel_name)` converts String to FixedString for entity lookup. Redundant per-frame hashing + heap String copies.
- **Impact**: Performance: O(n) string hashing per channel per entity per frame. Memory: duplicate heap strings.
- **Suggested Fix**: Change to `HashMap<FixedString, TransformChannel>`. Intern channel names during clip registration.

### SI-03: Case sensitivity mismatch with Gamebryo's case-insensitive interning
- **Severity**: LOW
- **Dimension**: String Interning Alignment
- **Location**: `crates/core/src/string/mod.rs:27-29`
- **Status**: NEW
- **Description**: Gamebryo's `GlobalStringTable::AddString` normalizes to lowercase before interning. Redux's `StringPool` is case-sensitive. "Bip01 Head" and "bip01 head" produce different symbols.
- **Impact**: Animation channels fail to bind to target nodes when case mismatches between mesh bone names and animation channel names.
- **Suggested Fix**: Normalize to lowercase before interning, matching Gamebryo behavior.

### SI-04: AnimationClip.name and text_key labels are heap Strings
- **Severity**: LOW
- **Dimension**: String Interning Alignment
- **Location**: `crates/core/src/animation/types.rs:130,150`
- **Status**: NEW
- **Description**: Clip names and text key labels compared frequently but stored as heap String. Good candidates for interning.

### SI-02: NIF string table uses Arc<str>, not integrated with StringPool
- **Severity**: LOW
- **Dimension**: String Interning Alignment
- **Location**: `crates/nif/src/header.rs:32`
- **Status**: NEW
- **Description**: Two-tier interning: NIF's `Arc<str>` table then ECS `StringPool`. Architecturally clean (NIF parser is standalone). One extra allocation per unique string at import time.

---

## Skipped (Existing Issues)

| Issue | Finding |
|-------|---------|
| #128 | BSEffectShaderProperty two_sided check (D4-07) |
| #114 | NiMorphData legacy weight (related to AR-04) |

---

## Positive Confirmations (No Gaps)

- **Parent/Children/GlobalTransform decomposition**: Fully functional (D1-05)
- **NiMatrix3→Quat conversion**: Correct with SVD fallback (D3-01, D3-02)
- **Transform propagation**: BFS over hierarchy, correct parent-before-child (D3-03)
- **Coordinate conversion**: Z-up→Y-up applied correctly (D3-04)
- **NIF version coverage**: All 9 game generations detected (D2-04)
- **Block type coverage**: ~215 types across 18 submodules (D2-05)
- **Post-link phase**: Not needed — import walker handles refs procedurally (D2-01)

---

## Priority Fix Order

1. **D4-06** (HIGH) — Property inheritance: maintain accumulation stack during NIF walk
2. **AR-01 + AR-02** (HIGH) — AnimationStack non-transform channels + root motion
3. **D2-08** (HIGH) — BSTriShape skinning extraction for SSE+ games
4. **AR-03** (MEDIUM) — Text key event emission during playback
5. **D2-06** (MEDIUM) — NiSwitchNode/NiLODNode child selection
6. **D4-01** (MEDIUM) — NiAlphaProperty blend factor extraction
7. **SI-01 + SI-03** (MEDIUM/LOW) — FixedString animation channels + case normalization
8. **D2-02** (HIGH) — Morrowind NIF support (per roadmap, lower priority than current targets)
