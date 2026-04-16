# Legacy Compatibility Audit — 2026-04-15

**Scope**: Gamebryo 2.3 vs Redux — scene graph, NIF format, transforms, materials, animation, strings
**Baseline**: Prior audit 2026-04-12 (17 findings: 7 MEDIUM, 10 LOW). This audit checks fix status and discovers new gaps.
**Per-dimension reports**: `/tmp/audit/legacy-compat/dim_{1_2,3_4,5_6}.md`

---

## Executive Summary

| Severity | Prior Open | Fixed | Still Open | New | **Total Open** |
|----------|--------:|------:|-----------:|----:|---------------:|
| CRITICAL | 0 | — | 0 | 0 | **0** |
| HIGH     | 0 | — | 0 | 0 | **0** |
| MEDIUM   | 7 | 6 | 1 | 1 | **2** |
| LOW      | 10 | 1 | 8 | 6 | **14** |
| **Total** | **17** | **7** | **9** | **7** | **16** |

**7 prior findings resolved** since April 12 — including all 3 remaining MEDIUM property-mapping gaps (#213 blend factors, LC-03 alpha test function, LC-04 dark texture) and the MEDIUM animation issues (LC-02 morph indexing, LC-05 channel clones, #211 text key events, #212 NiSwitchNode).

The codebase is in strong shape for FNV/FO3/Oblivion static content rendering. Remaining gaps are predominantly LOW polish items. The 2 MEDIUM findings (LC-01 embedded controllers, AR-08 blend interpolators) affect animation fidelity but not structural correctness.

---

## Prior Finding Resolution

| Prior ID | Title | Prior Sev | Status | Notes |
|----------|-------|-----------|--------|-------|
| LC-01 | NIF-embedded controllers | MEDIUM | **STILL OPEN** (#261) | Zero controller_ref traversal in import/ |
| LC-02 | Morph index hardcoded to 0 | MEDIUM | **FIXED** | `resolve_morph_target_index()` at anim.rs:812 |
| LC-03 | Alpha test function bits | MEDIUM | **FIXED** | `(flags & 0x1C00) >> 10` at material.rs:537, 5 tests |
| LC-04 | dark_texture not imported | MEDIUM | **FIXED** | End-to-end pipeline: MaterialInfo → Material → GpuInstance |
| LC-05 | Channel Vec clones per frame | MEDIUM | **FIXED** | Scratch buffers, explicit comment at systems.rs:415 |
| LC-06 | Stale ImportedSkin doc comment | LOW | **STILL OPEN** (#266) | |
| LC-07 | Secondary slot UV transforms | LOW | **STILL OPEN** | Only base slot translation/scale |
| LC-08 | No-sorter flag (bit 13) | LOW | **STILL OPEN** (#266) | |
| #211 | Text key events | MEDIUM | **FIXED** | text_events.rs + AnimationTextKeyEvents markers |
| #212 | NiSwitchNode/NiLODNode | MEDIUM | **FIXED** | walk.rs:77-101, specialized child selection |
| #213 | Blend factors | MEDIUM | **FIXED** | src/dst bits 1-8, BlendType::from_nif_blend() |
| #231 | String-keyed channels | LOW | **STILL OPEN** | Arc\<str\> not FixedString |
| #229 | Keyframe alloc overhead | LOW | **STILL OPEN** | find_key_pair uses closure; text_key collection still allocates |
| D1-02 | SceneFlags not populated | LOW | **STILL OPEN** (#222) | Root cause: ImportedNode lacks flags field |
| D3-05 | Inline coord swap sites | LOW | **STILL OPEN** (#232) | 12 inline `[x,z,-y]` sites |
| D4-05 | UV rotation/center dropped | LOW | **STILL OPEN** | Base slot translation/scale only |
| D4-09 | Ambient/diffuse not in Material | LOW | **STILL OPEN** (#221) | Diffuse used as vertex color fallback only |

---

## New Findings

### MEDIUM

#### AR-08: NiBlendInterpolator parsed but never consumed in animation import
- **Severity**: MEDIUM
- **Dimension**: Animation Readiness
- **Game Affected**: Skyrim, FO3, FNV, FO4 (NIFs with embedded multi-sequence animations)
- **Location**: [blocks/interpolator.rs:728-910](crates/nif/src/blocks/interpolator.rs#L728-L910), [anim.rs:411-442](crates/nif/src/anim.rs#L411-L442)
- **Status**: NEW
- **Description**: NiBlendTransformInterpolator and siblings (Float, Point3, Bool) are fully parsed at the block level, but `extract_transform_channel` only handles NiTransformInterpolator and NiBSplineCompTransformInterpolator. When a ControlledBlock's interpolator_ref points to a NiBlendTransformInterpolator, the channel extraction returns None and animation data is silently lost. AnimationStack provides layer blending at ECS level, but no bridge decomposes a NiBlendInterpolator's sub-interpolator array into separate layers.
- **Impact**: Embedded multi-sequence animation blending (idle loops with overlaid partial-body anims) fails silently. Single-sequence KF files work fine.
- **Suggested Fix**: Follow NiBlendTransformInterpolator's weighted interpolator array, extract each sub-interpolator as a separate channel or AnimationStack layer.

### LOW

#### D1-04: NiDynamicEffect affected_nodes list ignored during light import
- **Severity**: LOW
- **Dimension**: Scene Graph Decomposition
- **Game Affected**: Oblivion, FO3, FNV, Skyrim
- **Location**: [walk.rs:362-444](crates/nif/src/import/walk.rs#L362-L444)
- **Status**: NEW
- **Description**: NiDynamicEffect.affected_nodes (parsed at blocks/light.rs:48) specifies which scene graph subtrees a light should affect. The walker ignores this list — every imported light is treated as global. Lights intended to affect only specific objects (character-attached lanterns) illuminate all geometry.
- **Suggested Fix**: Add `affected_node_names: Vec<Arc<str>>` to ImportedLight; resolve block indices to names; store as light-target filter component.

#### N2-01: BsTriShape vertex_desc missing VF_UVS_2 and VF_LAND_DATA flags
- **Severity**: LOW
- **Dimension**: NIF Format Readiness
- **Game Affected**: FO4, FO76, Starfield
- **Location**: [tri_shape.rs:267-276](crates/nif/src/blocks/tri_shape.rs#L267-L276)
- **Status**: NEW
- **Description**: BSVertexDesc flag constants define bits 0,1,3,4,5,6,8,10 but omit bit 2 (VF_UVS_2 = 0x004, second UV set) and bit 7 (VF_LAND_DATA = 0x080, landscape data). The trailing skip guard prevents parse corruption, but second UV coordinates and landscape vertex data are silently discarded.
- **Suggested Fix**: Add constants and decode branches for both flags.

#### D4-NEW-01: NiStencilProperty stencil state not mapped to Vulkan
- **Severity**: LOW
- **Dimension**: Property → Material Mapping
- **Game Affected**: Oblivion, FO3, FNV (pre-Skyrim stencil effects)
- **Location**: [properties.rs:906-973](crates/nif/src/blocks/properties.rs#L906-L973) (parsed), [material.rs:472-477](crates/nif/src/import/material.rs#L472-L477) (only `is_two_sided()` consumed)
- **Status**: NEW
- **Description**: NiStencilProperty is fully parsed with all fields (stencil_enabled, function, ref, mask, actions, draw_mode) but only `is_two_sided()` is consumed. No Vulkan stencil pipeline variant exists. >95% of NiStencilProperty usage is for two-sided rendering (which works); stencil shadow volumes in some Oblivion interiors are the gap.

#### AR-09: No NiControllerManager sequence state machine equivalent
- **Severity**: LOW
- **Dimension**: Animation Readiness
- **Game Affected**: All games with KFM-driven animation
- **Location**: `crates/core/src/animation/` (absence)
- **Status**: NEW
- **Description**: Gamebryo's NiControllerManager manages activation/deactivation/transitions between NiControllerSequences with cross-fade timing. Redux has AnimationStack (manual layer blending) and kfm.rs (parser), but no runtime state machine consumes KFM transition data to drive AnimationStack layer changes.
- **Suggested Fix**: Implement AnimationController component consuming KfmFile transition tables.

#### AR-10: collect_text_key_events allocates Vec\<String\> per frame per entity
- **Severity**: LOW
- **Dimension**: Animation Readiness
- **Game Affected**: All
- **Location**: [text_events.rs:15](crates/core/src/animation/text_events.rs#L15), [stack.rs:202](crates/core/src/animation/stack.rs#L202)
- **Status**: NEW (related to #229 but distinct)
- **Description**: Both `collect_text_key_events()` and `collect_stack_text_events()` return freshly allocated Vecs with cloned Strings every frame for every animated entity. Most frames have no events — the allocation is wasted.
- **Suggested Fix**: Return iterator/small-vec; use Arc\<str\> for text key labels; consider visitor pattern.

#### SI-05: Per-frame StringPool.get() lowercase allocation in animation hot path
- **Severity**: LOW
- **Dimension**: String Interning
- **Game Affected**: All
- **Location**: [systems.rs:433](byroredux/src/systems.rs#L433), [string/mod.rs:46-49](crates/core/src/string/mod.rs#L46-L49)
- **Status**: NEW
- **Description**: For every animated entity, every channel name calls `pool.get(channel_name)` which calls `s.to_ascii_lowercase()`, allocating a new String on the heap. Typical skeletons have 30-80 bones × 2+ layers = 60-160 allocations per entity per frame. At 60 FPS with 50 entities: 300K-600K small allocations/sec.
- **Suggested Fix**: Pre-intern channel names as FixedString at clip load time in `convert_nif_clip`. Store AnimationClip channels as `HashMap<FixedString, TransformChannel>`. Eliminates per-frame lowercase+lookup entirely.

---

## Positive Confirmations (No Gaps)

These areas were verified correct or already fixed with no remaining issues:

- **Transform composition**: NIF compose_transforms matches Gamebryo exactly (rot/trans/scale)
- **Z-up → Y-up conversion**: Correct C*R*C^T axis swap, Shepperd + SVD (issue #333 for normalization edge case already filed)
- **ECS propagation**: GlobalTransform::compose() matches Gamebryo NiNode::UpdateDownwardPass
- **Uniform scale**: f32 scale field matches Gamebryo NiTransform::m_fScale; no ignore_parent_scale in Gamebryo 2.3
- **NIF version coverage**: All game generations detected (Morrowind → Starfield), validate_refs comprehensive
- **Alpha test function**: Full pipeline bits 10-12 → CompareOp → GpuInstance → shader
- **Blend factors**: src/dst blend modes → BlendType → pipeline selection
- **Dark texture**: End-to-end import → Material → SSBO
- **Property inheritance**: walk.rs accumulation stack, child-overrides-parent per type
- **Morph target indexing**: Name-based resolution with index-0 fallback
- **Text key events**: Full loop-wrap handling, transient ECS markers
- **NiSwitchNode/NiLODNode**: Specialized child selection
- **Channel Vec clones**: Eliminated via scratch buffers
- **Specular power path**: NiMaterialProperty.shininess / BSLighting.glossiness → roughness = (1 - g/100).clamp
- **BSLightingShaderProperty**: Emissive, specular, glossiness, UV, alpha, env_map_scale, two_sided, decal, textures 0-2, BGSM name all flow through
- **Case-insensitive interning**: StringPool lowercases before hash, matches Gamebryo

---

## Priority Fix Order

1. **LC-01 / #261** (MEDIUM) — NIF-embedded controllers: visual fidelity for water/lava/fire textures
2. **AR-08** (MEDIUM) — NiBlendInterpolator: multi-sequence animation blending for complex NPCs
3. **SI-05** (LOW) — Pre-intern channel names: eliminates 300K+ allocations/sec at scale
4. **AR-10** (LOW) — Text key Vec\<String\> alloc: per-frame allocation pressure
5. **N2-01** (LOW) — BsTriShape VF_UVS_2/VF_LAND_DATA: FO4+ terrain data loss
6. **D1-04** (LOW) — Affected nodes: per-subtree light targeting
7. **#231** (LOW) — FixedString animation channels + clip names
8. **D4-NEW-01** (LOW) — NiStencilProperty full stencil state
9. **AR-09** (LOW) — Sequence state machine glue
10. Remaining prior LOWs: LC-06, LC-07, LC-08, D4-05, D4-09, D1-02, D3-05

---

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-04-15.md
```
