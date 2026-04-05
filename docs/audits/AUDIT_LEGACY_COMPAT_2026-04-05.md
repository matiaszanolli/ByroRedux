# Legacy Compatibility Audit — 2026-04-05

Compares Gamebryo 2.3 architecture against ByroRedux implementation.
Identifies gaps blocking NIF loading, animation playback, or content rendering.

## Executive Summary

**22 findings** across 6 dimensions: 4 HIGH, 9 MEDIUM, 6 LOW, 3 INFO.

The engine handles FNV and Skyrim SE content well. Major gaps cluster around:
1. **Missing Material component** — rich material data parsed but discarded at import
2. **Missing NIF block types** — skinning, morphing, and several property types
3. **NIF strings not interned** — performance issue at scale
4. **Transform propagation in wrong crate** — should be in core, not binary

## Findings

---

### LC-01: No Material component — rich material data discarded at import
- **Severity**: HIGH
- **Dimension**: Property → Material Mapping
- **Location**: `crates/core/src/ecs/components/` (absent), `crates/nif/src/import.rs:516`
- **Status**: NEW
- **Description**: NiMaterialProperty (7 fields: ambient, diffuse, specular, emissive, shininess, alpha, emissive_mult) and BSLightingShaderProperty (specular, glossiness, emissive, UV offset/scale, shader flags) are fully parsed but all data beyond diffuse texture path, 3 booleans (alpha/two-sided/decal), and vertex colors is discarded at import.
- **Impact**: No specular highlights, no emissive glow, no normal mapping, no glossiness variation. All surfaces render with identical material properties.
- **Suggested Fix**: Create a `Material` component capturing: diffuse/normal/specular texture paths, specular color+strength, emissive color+multiplier, glossiness, alpha threshold, UV offset/scale, shader flags.

### LC-02: Missing skinning block types (NiSkinInstance, NiSkinData, NiSkinPartition)
- **Severity**: HIGH
- **Dimension**: NIF Format Readiness
- **Location**: `crates/nif/src/blocks/mod.rs` (absent)
- **Status**: NEW
- **Description**: Character meshes in Oblivion/FO3/FNV/Skyrim use NiSkinInstance + NiSkinData + NiSkinPartition for skeletal deformation. None are registered in parse_block(). Skinned meshes parse as NiUnknown if block sizes are available, or hard-error on pre-20.2.0.7 NIFs.
- **Impact**: No character rendering. All actor meshes fail or render in bind pose.
- **Suggested Fix**: Add parsers for the three skinning types. NiSkinInstance references bones by block ref; NiSkinData stores per-bone transforms + per-vertex weights; NiSkinPartition holds GPU-ready partitioned vertex groups.

### LC-03: Missing NiStencilProperty and NiZBufferProperty parsers
- **Severity**: HIGH
- **Dimension**: NIF Format Readiness
- **Location**: `crates/nif/src/blocks/mod.rs` (absent)
- **Status**: NEW
- **Description**: NiStencilProperty (type 7) controls two-sided rendering and stencil ops. NiZBufferProperty (type 11) controls depth test/write. Both are attached to geometry in Oblivion/FNV NIFs. NiStencilProperty has a fragile NiUnknown heuristic (last 4 bytes = draw_mode) instead of a proper parser.
- **Impact**: Incorrect two-sided detection on some meshes; depth state not controllable per-mesh.
- **Suggested Fix**: Add dedicated parsers. NiStencilProperty: 8 fields (enable, test_func, ref, mask, fail/zfail/pass actions, draw_mode). NiZBufferProperty: 2 fields (flags, function).

### LC-04: Missing NiBlendTransformInterpolator
- **Severity**: HIGH
- **Dimension**: Animation Readiness
- **Location**: `crates/nif/src/blocks/interpolator.rs` (absent)
- **Status**: NEW
- **Description**: NiBlendTransformInterpolator is used by NiControllerManager for NIF-level animation blending (cross-fade between idle→walk→run at the interpolator level). Without it, embedded multi-sequence animations can't blend.
- **Impact**: NiControllerManager sequences play but can't cross-fade; transitions are hard cuts.
- **Suggested Fix**: Parse NiBlendTransformInterpolator (inherits NiBlendInterpolator: managed flag, weight threshold, interp count, single index, high/low priority, array of weighted interpolator refs).

### LC-05: WorldBound component missing
- **Severity**: MEDIUM
- **Dimension**: Scene Graph Decomposition
- **Location**: `crates/core/src/ecs/components/` (absent)
- **Status**: NEW
- **Description**: NiAVObject's m_kWorldBound (bounding sphere) has no Redux equivalent. Blocks frustum culling and spatial queries.
- **Impact**: No view frustum culling — all entities submitted for rendering regardless of visibility.
- **Suggested Fix**: Add `WorldBound { center: Vec3, radius: f32 }` component with PackedStorage. Compute from mesh AABB + world transform.

### LC-06: SceneFlags component missing
- **Severity**: MEDIUM
- **Dimension**: Scene Graph Decomposition
- **Location**: `crates/core/src/ecs/components/` (absent)
- **Status**: NEW
- **Description**: NiAVObject's m_uFlags (APP_CULLED, SELECTIVE_UPDATE, IS_NODE etc.) has no Redux equivalent. APP_CULLED is read every frame to skip rendering.
- **Impact**: No per-entity visibility toggle.
- **Suggested Fix**: Add `SceneFlags(u16)` component with PackedStorage and named constants for each flag.

### LC-07: Missing collision block types (bhk* family)
- **Severity**: MEDIUM
- **Dimension**: NIF Format Readiness
- **Location**: `crates/nif/src/blocks/mod.rs` (absent)
- **Status**: NEW
- **Description**: bhkCollisionObject, bhkRigidBody, bhkMoppBvTreeShape etc. are present in virtually every world-geometry NIF. While not rendered, they consume block indices. On pre-20.2.0.7 NIFs (Oblivion), block sizes may be unavailable, causing hard errors.
- **Impact**: Oblivion NIF loading failures on meshes with collision data but no block size table.
- **Suggested Fix**: Add skip-only parsers that consume the correct byte count without storing data. Alternatively, ensure all Oblivion NIFs have block size fallbacks.

### LC-08: NiVertexColorProperty not parsed
- **Severity**: MEDIUM
- **Dimension**: Property → Material Mapping
- **Location**: `crates/nif/src/blocks/mod.rs` (absent)
- **Status**: NEW
- **Description**: Controls vertex color lighting/blending mode (source mode + lighting mode). Oblivion NIFs use this to specify how vertex colors interact with material.
- **Impact**: Vertex colors always applied as direct modulation; incorrect blending on meshes that specify emissive or ambient-only vertex color modes.
- **Suggested Fix**: Simple parser: 2 u16 fields (flags, lighting_mode) or 2 u32 fields depending on version.

### LC-09: NiStencilProperty uses fragile NiUnknown heuristic
- **Severity**: MEDIUM
- **Dimension**: Property → Material Mapping
- **Location**: `crates/nif/src/import.rs:617-633`
- **Status**: NEW
- **Description**: Instead of a proper parser, NiStencilProperty is handled as NiUnknown with a heuristic that reads the last 4 bytes as draw_mode. This assumes fixed block layout and will break on version variants.
- **Impact**: Incorrect two-sided detection on some Oblivion meshes.
- **Suggested Fix**: Add a dedicated NiStencilProperty parser (8 fields).

### LC-10: NIF strings not interned into StringPool
- **Severity**: MEDIUM
- **Dimension**: String Interning Alignment
- **Location**: `crates/nif/src/stream.rs` (read_string returns String)
- **Status**: Existing: #55
- **Description**: NIF parser stores all strings as owned `String` allocations. The engine's StringPool is never used during import. Common strings (bone names, node names) are duplicated across NIFs.
- **Impact**: Performance at scale — excessive heap fragmentation, O(n) string comparison in animation channel lookups instead of O(1) symbol comparison.
- **Related**: #55 (NIF string table clones on every read)

### LC-11: No KFM file support
- **Severity**: MEDIUM
- **Dimension**: Animation Readiness
- **Location**: `crates/nif/` (absent)
- **Status**: NEW
- **Description**: KFM files define animation state machines (sequence transitions, blend times, sync groups, chain info). No parser exists. AnimationStack provides programmatic blending as a partial substitute.
- **Impact**: No data-driven animation transitions; all blending must be coded manually.
- **Suggested Fix**: Parse KFM format (sequence list, transition table with blend times, chain definitions).

### LC-12: Case-insensitive path normalization absent
- **Severity**: MEDIUM
- **Dimension**: String Interning Alignment
- **Location**: `crates/nif/src/import.rs`, `crates/renderer/src/texture_registry.rs`
- **Status**: NEW
- **Description**: Bethesda content has mixed-case texture paths. Without normalization before interning, the same texture can be loaded multiple times with different case variants.
- **Impact**: Duplicate textures in GPU memory; wasted VRAM.
- **Suggested Fix**: Normalize all asset paths to lowercase before interning or texture registry lookup.

### LC-13: Transform propagation system in binary crate
- **Severity**: MEDIUM
- **Dimension**: Transform Compatibility
- **Location**: `byroredux/src/main.rs:446-525`
- **Status**: NEW
- **Description**: The transform propagation system (equivalent to NiNode::UpdateDownwardPass) is defined in the binary crate, not in core. Any other consumer (tests, tools, headless) must duplicate it.
- **Impact**: Code reuse blocked; architectural coupling to binary crate.
- **Suggested Fix**: Move to `crates/core/src/ecs/systems/transform.rs`.

### LC-14: NiGeomMorpherController missing
- **Severity**: MEDIUM
- **Dimension**: Animation Readiness
- **Location**: `crates/nif/src/blocks/mod.rs` (absent)
- **Status**: NEW
- **Description**: Morph target controller for facial animation and mesh deformation. Used in character head/face meshes across all Bethesda games.
- **Impact**: No facial animation; character faces render in default expression.
- **Suggested Fix**: Parse NiGeomMorpherController (base + morph count + data_ref + always_update + target_count + targets). Also needs NiMorphData (morph count + per-morph key arrays + vertex deltas).

### LC-15: Oblivion variant detection fragile
- **Severity**: MEDIUM
- **Dimension**: NIF Format Readiness
- **Location**: `crates/nif/src/version.rs:84-86`
- **Status**: NEW
- **Description**: Detection uses `(_, 0) if version == V20_0_0_5 => Oblivion` but many Oblivion NIFs use version 20.2.0.7 with user_version=0. The fallback `uv < 11 => Oblivion` catches most cases but is untested with edge-case Oblivion exports.
- **Impact**: Possible incorrect feature flag selection on some Oblivion NIFs.
- **Suggested Fix**: Add test cases for known Oblivion NIF headers. Document the detection heuristic.

### LC-16: CollisionObject component missing
- **Severity**: LOW
- **Dimension**: Scene Graph Decomposition
- **Location**: `crates/core/src/ecs/components/` (absent)
- **Status**: NEW
- **Description**: NiAVObject's m_spCollisionObject has no equivalent. Acceptable since physics is not on the current roadmap.
- **Impact**: None currently; needed when collision/physics is implemented.

### LC-17: No Material/Property components in core ECS
- **Severity**: LOW
- **Dimension**: Scene Graph Decomposition
- **Location**: `crates/core/src/ecs/components/` (absent)
- **Status**: NEW (related to LC-01)
- **Description**: Material data handled inside renderer as ad-hoc draw command flags rather than portable ECS components.
- **Impact**: Renderer-coupled material state; other systems can't query material properties.

### LC-18: NiFogProperty not parsed
- **Severity**: LOW
- **Dimension**: Property → Material Mapping
- **Location**: `crates/nif/src/blocks/mod.rs` (absent)
- **Status**: NEW
- **Description**: Controls per-object fog parameters. Rarely used in Bethesda content (fog is typically global).
- **Impact**: Minimal visual impact.

### LC-19: SVD degenerate matrix threshold arbitrary
- **Severity**: LOW
- **Dimension**: Transform Compatibility
- **Location**: `crates/nif/src/import.rs:808`
- **Status**: NEW
- **Description**: Fast-path determinant check uses tolerance 0.1. A tighter threshold (0.01) would catch subtly scaled matrices from modded content.
- **Impact**: Very rare edge case — modded NIFs with non-uniform scale could have subtle rotation errors.

### LC-20: Naming inconsistency between docs and code
- **Severity**: INFO
- **Dimension**: Scene Graph Decomposition
- **Location**: `docs/legacy/api-deep-dive.md:158-159`
- **Status**: NEW
- **Description**: Docs say "LocalTransform" / "WorldTransform" but code uses "Transform" / "GlobalTransform".

### LC-21: No post-link phase in NIF loading
- **Severity**: INFO
- **Dimension**: NIF Format Readiness
- **Location**: `crates/nif/src/scene.rs`
- **Status**: NEW
- **Description**: Gamebryo's NiStream has an explicit PostLinkObject() callback. Redux uses lazy link resolution via get_as<T>(). Works for current block types but will need post-link when skinning blocks are added (NiSkinInstance resolves bone arrays).

### LC-22: Quaternion component order fragile
- **Severity**: INFO
- **Dimension**: Transform Compatibility
- **Location**: `crates/nif/src/import.rs:828`
- **Status**: NEW
- **Description**: zup_matrix_to_yup_quat returns [i,j,k,w] matching glam's from_xyzw but differs from Gamebryo's serialization order [w,x,y,z]. Currently correct but undocumented.

## Coverage Matrix

| Dimension | Findings | Critical Gaps |
|-----------|----------|---------------|
| 1. Scene Graph | 5 | WorldBound, SceneFlags |
| 2. NIF Format | 5 | Skinning blocks, stencil/zbuffer |
| 3. Transform | 4 | Propagation in wrong crate |
| 4. Properties | 4 | Material component, stencil parser |
| 5. Animation | 3 | Blend interpolator, morph controller |
| 6. Strings | 2 | Not interned, no case normalization |

## Prioritized Fix Order

1. **LC-01** (Material component) + **LC-03** (stencil/zbuffer parsers) — highest visual impact
2. **LC-02** (skinning blocks) — needed for character rendering
3. **LC-04** (blend interpolator) — needed for smooth animation transitions
4. **LC-05** + **LC-06** (WorldBound + SceneFlags) — needed for culling
5. **LC-10** + **LC-12** (string interning + case normalization) — scale performance
6. **LC-07** (collision skipping) — Oblivion compatibility
7. Everything else — incremental improvements
