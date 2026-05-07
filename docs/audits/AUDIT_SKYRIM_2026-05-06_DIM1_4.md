# Skyrim SE Compatibility Audit — Dimensions 1 & 4

**Date**: 2026-05-06
**Scope**: `--focus 1,4` (BSTriShape vertex format · BSEffectShaderProperty + specialty nodes)
**Baseline**: Meshes0 sweep — 100.00% clean / 0 truncated / 0 recovered / 0 realignment WARN (post #836–#838).

## Executive Summary

Both dimensions land clean. **No P0/P1 findings.** Four LOW + two INFO across both dimensions, all in `crates/nif/`. The packed-vertex parser, SSE skin-buffer reconstruction, and effect-shader trailer are field-aligned with `nif.xml`; the three regression guards (#836 data_size gate, #837 BsLag/ProceduralLightning parsers, #838 NiLodTriShape distinct dispatch) are still in place with citations.

The most consequential finding is **SK-D4-04**: BSEffect's SLSF1 SOFT_EFFECT / GREYSCALE_TO_PALETTE_* and SLSF2 EFFECT_LIGHTING bits are captured during parse but never consumed by the import or render path. This explains a known visual gap on Skyrim spell FX (greyscale-to-palette renders as raw luminance, soft particles hard-cut against geometry). The fix is import-side capture only — shader work for soft-particle depth read + palette LUT sampler is non-trivial and can stage separately.

The two SK-D1 findings are defense-in-depth: (1) the `Bitangent X` / `Unused W` slot is read into a tangent-named local even on non-tangented meshes (output is still correct, just semantically noisy), and (2) the SSE packed-buffer decoder hard-codes full-precision positions in a way that becomes a foot-gun if the decoder is later extended to FO4. The third (SK-D1-NN-03) is the one with a visible-quality angle: inline BSTriShape bone weights aren't renormalized after half-float decode, so a 4-influence vertex can drift up to ~0.4% off unit sum — visible only on extreme close-ups, but it produces an asymmetry between the inline path and `densify_sparse_weights` (NiSkinData) that DOES renormalize.

## Findings

### Dimension 1: BSTriShape Vertex Format

#### [LOW] SK-D1-NN-01 — `Bitangent X` / `Unused W` slot read into tangent variable when `VF_TANGENTS` clear

**File**: `crates/nif/src/blocks/tri_shape.rs:647-666` and `crates/nif/src/import/mesh.rs:1374-1380`

nif.xml `BSVertexData` (`nif.xml:2107-2126`) splits the slot two ways: `Bitangent X` if `(ARG & 0x411) == 0x411` (Vertex+Tangent+FullPrec), `Unused W` if `(ARG & 0x411) == 0x401` (Vertex+FullPrec, no Tangent). The current parser unconditionally names it `bitangent_x`. Byte-count is identical (4 bytes), tangent assembly is gated on `bitangent_z` being `Some` (only set under `VF_TANGENTS && VF_NORMALS`), so the bogus value never reaches output. Semantic noise only.

**Suggested fix**: Either gate `bitangent_x = Some(_)` on `VF_TANGENTS` and `skip(4)` otherwise, or rename to `bitangent_x_or_unused` with a one-line comment.

**Dedup**: new

#### [LOW] SK-D1-NN-02 — SSE packed buffer assumes always-full-precision positions

**File**: `crates/nif/src/import/mesh.rs:1370-1380`

Decoder hard-codes 12-byte f32 positions with the comment "SSE always uses full-precision." Correct today (`bsver in [100, 130)` gate at `try_reconstruct_sse_geometry`, and `BSVertexDataSSE` at `nif.xml:2128-2141` is unconditionally f32). Latent: if the decoder is extended to FO4 (bsver=130), `BSVertexData` at `nif.xml:2107` is conditional on `(ARG & 0x401) == 0x401` for full precision. The decoder will mis-decode every FO4 mesh that ships without `VF_FULL_PRECISION`.

**Suggested fix**: Mirror the inline parser's `bsver() < 130 || vertex_attrs & VF_FULL_PRECISION != 0` rule, OR add a `debug_assert!(bsver < 130)` so the foot-gun fires in tests rather than in shipped FO4 content.

**Dedup**: new

#### [LOW] SK-D1-NN-03 — Inline BSTriShape bone weights not re-normalized after half-float decode (asymmetric vs NiSkinData path)

**File**: `crates/nif/src/blocks/tri_shape.rs:736` and `crates/nif/src/import/mesh.rs:1437-1449`

Inline + SSE-buffer paths decode 4 × IEEE-754 half-precision weights and pass them through unchanged. `densify_sparse_weights` (line 2043, NiSkinData path) DOES renormalize. `triangle.vert:120-146` computes `xform = w.x * bones[base + bIdx.x] + ...` without dividing by `wsum` (which is computed only for the rigid-fallback `< 0.001` check). Half-float quantization error (~1-part-in-1024 per component) means a 4-influence vertex can drift up to ~0.4% off unit sum — a fraction of bone scale per frame. Visible only on extreme close-ups, but the asymmetry between two skin paths producing different quality on the same mesh is the real concern.

**Suggested fix**: Renormalize once at decode time (in `read_vertex_skin_data` or a helper called by both inline and SSE-buffer paths). Skip when `wsum` already within `1e-4` of `1.0` to avoid touching well-formed content.

**Dedup**: new

### Dimension 4: BSEffectShaderProperty + Specialty Nodes

#### [LOW] SK-D4-01 — BSVER==131 silently produces zero shader flags AND zero CRC arrays (doc-only)

**File**: `crates/nif/src/blocks/shader.rs:1497-1523`

Parser is consistent with `nif.xml:6641-6648` — typed flag pair gated `#NI_BS_LT_FO4#` (BSVER < 130) or `#BS_FO4#` (== 130); CRC arrays gated `#BS_GTE_132#`. So BSVER==131 is genuinely a schema gap — pre-release FO4 dev stream (`BS_FO4_2 = (BSVER >= 130 AND BSVER <= 139)`, only ever used for `bhkRigidBodyCInfo2010`). The BLSP sibling site has a `#409` rationale comment; the BSEffect site doesn't.

**Suggested fix**: One-line comment at `shader.rs:1497` mirroring the BLSP rationale ("bsver == 131 is an intentional gap — pre-release FO4 dev stream, no flag pair, no CRC arrays per nif.xml verexpr discipline").

**Dedup**: #409 covers BLSP; new (doc-only) for the BSEffect sibling.

#### [LOW] SK-D4-04 — BSEffect SOFT_EFFECT / GREYSCALE_TO_PALETTE_* / EFFECT_LIGHTING flag bits captured but not consumed

**File**: `crates/nif/src/import/material/walker.rs:350-367`, `crates/nif/src/shader_flags.rs`

BSEffect path inspects `is_two_sided_from_modern_shader_flags` and `is_decal_from_modern_shader_flags` only. No explicit consumer for SLSF1 bits 0x40 (SOFT_EFFECT), 0x80 (GREYSCALE_TO_PALETTE_COLOR), 0x100 (GREYSCALE_TO_PALETTE_ALPHA), or SLSF2 bit 0x100 (EFFECT_LIGHTING) — even though `shader_flags.rs:201` documents the modern-shader CRC table covers them. The flags ARE captured into `shader_flags_1/2 + sf1_crcs/sf2_crcs`; only the import-side and shader-side consumption is missing.

**Impact**: BSEffect surfaces flagged for greyscale-to-palette (e.g. some `meshes/effects/magic*.nif`) render their `greyscale_texture` as raw luminance instead of palette-mapped color; Soft Effect surfaces (smoke, dust) lack near-camera depth feathering and hard-cut against geometry. Explains a known visual gap on Skyrim spell FX.

**Suggested fix**: Add `effect_soft / effect_palette_color / effect_palette_alpha / effect_lit` booleans to `MaterialInfo.effect_shader` (capture site already exists at `walker.rs:372` — `capture_effect_shader_data`). Plumb through to `material_kind = 101` fragment branch in `triangle.frag:1075`. Defer the soft-particle depth read + palette LUT sampler bindings (those are non-trivial). **Action item is the import-side capture only.**

**Dedup**: new (no existing issue covers Skyrim BSEffect flag-bit consumption).

#### [INFO] SK-D4-02 — BSEffect `base_color → emissive_color` remap is documented TODO, not a bug

**File**: `crates/nif/src/import/material/walker.rs:295-309`

`info.emissive_color = shader.base_color[0..3]` with a 12-line comment explaining the additive routing. `triangle.frag:1074-1078` (`MATERIAL_KIND_EFFECT_SHADER == 101`) renders pure additive `texColor * emissiveColor * emissiveMult`, so flames/glow rings/force fields render correctly. Becomes load-bearing only when a separate "lit BSEffect" path lands.

**Dedup**: #166 (semantic rename), #706 (FX-1 wiring). Not a finding.

#### [INFO] SK-D4-03 — `as_ni_node` does not list BSFadeNode/BSBlastNode/BSDamageStage (already covered by parse-time aliasing)

**File**: `crates/nif/src/import/walk.rs:62-97`, cross-ref `crates/nif/src/blocks/mod.rs:165-202`

`BSFadeNode`, `BSLeafAnimNode`, `BSFaceGenNiNode`, `RootCollisionNode`, `AvoidNode`, `NiBSAnimationNode`, `NiBSParticleNode` all alias to `NiNode::parse`, so they downcast as plain `NiNode` at line 64-66. `BSBlastNode` / `BSDamageStage` / `BSDebrisNode` parse as `BsRangeNode` (with `with_kind(...)`), and `BsRangeNode` IS in the unwrap list at line 90. All four wrapper families covered. The audit suggestion to add them is already satisfied by parse-time aliasing.

**Dedup**: verified-healthy.

## Verified-Healthy

### Dimension 1
- All 11 vertex-flag bits match `nif.xml:2077-2090` (`VF_VERTEX=0x001` through `VF_FULL_PRECISION=0x400`).
- Half-float decode (`half_to_f32`, `tri_shape.rs:1186-1212`) handles all 4 IEEE-754 binary16 cases (zero, subnormal w/ manual normalization loop, inf/NaN, normal). Bias 127-15=112 correct.
- Packed normal decode (`byte_to_normal`): canonical `(b/127.5) - 1.0` UNORM-to-SNORM.
- Triangle index width: u16 array, matches `nif.xml:2070-2074`. `num_triangles` u16/u32 selection by `bsver >= 130` matches `nif.xml:8236-8237`.
- `data_size`-derived stride recovery (#621) correctly delimited by `data_size != 0 && num_vertices != 0` (#836).
- `extract_bs_tri_shape` flag-combo coverage: full-precision on/off, skinned on/off, inline tangents, eye data, no-UV, missing geometry.
- FO76 `Bound Min Max` skip (`tri_shape.rs:480-482`) — strict `bsver() == 155` per nif.xml `#BS_F76#`.
- Particle-data trailing skip — `bsver < 130` gate, placed OUTSIDE the `data_size > 0` block per #341.
- BSDynamicTriShape `vertex_desc` post-overwrite update sets `VF_FULL_PRECISION` so downstream consumers see post-overwrite reality (#621 / SK-D1-04).
- Bone-index partition remap: single-partition fast path identity-widens; multi-partition builds inverse `vertex_map`. Falls back to identity widen on missing partition table (synthetic content).
- SSE skin-payload fallback (`import/mesh.rs:1694-1710`, #638) — both inline + global buffer paths converge on same `(weights, indices)` shape.
- Tangent reassembly invariant: gates on `bitangent_z`, matching `(ARG & 0x18) == 0x18`. Sign derivation via `sign(dot(B, cross(N, T)))` is rotation-invariant — Y-up swap on xyz safe.
- `consumed > vertex_size_bytes` guard errors out on malformed descriptors instead of wrapping.
- `vertex_data_size` consistency at SSE-buffer decoder: rejects `vertex_size==0`, requires `raw_bytes.len() % vertex_size == 0`, refuses `VF_VERTEX==0`.

### Dimension 4
- BSEffectShaderProperty field order vs `nif.xml:6639-6675` matches across BSVER 83/100/130 (Skyrim), 130-139 (FO4), 155 (FO76). Includes packed `clamp/lighting_influence/env_map_min_lod/unused` quad at `shader.rs:1533-1536`, `refraction_power` BSVER>=155 gate (post-#746), and the FO76 `reflectance/lighting/emittance/emit_gradient/luminance` trailer.
- `LuminanceParams` matches `BSSPLuminanceParams` at `nif.xml:6566-6571`.
- BSEffect `material_reference` stopcond at `shader.rs:1485-1491` consistent with `#BSVER# #GTE# 155 #AND# Name` plus suffix-aware test.
- `material_kind = 101` round-trips `walker.rs:404` → `triangle.frag:1074-1078`. Test `effect_shader_sets_material_kind_to_101` pins the value.
- `triangle.frag:1079+` falloff cone honors `falloff_start_angle / falloff_stop_angle / falloff_start_opacity / falloff_stop_opacity` per BSEffect spec (#620).
- Specialty geometry dispatch at `blocks/mod.rs`: BSDynamicTriShape (315), BSLODTriShape (285, distinct `NiLodTriShape`), BSMeshLODTriShape (286), BSSubIndexTriShape (299), BSTreeNode (180), BSPackedCombined[Shared]GeomDataExtra (519). All present.

## Regression-Guard Status

| Guard | Status | Citation |
|------|--------|----------|
| **#836** — BSTriShape `data_size` warning gated on `num_vertices != 0` | IN PLACE | `tri_shape.rs:544` — `if data_size != 0 && num_vertices != 0`; SSE-zero-vert case documented at lines 533-543. |
| **#837** — Dedicated `BsLagBoneController` + `BsProceduralLightningController` parsers | IN PLACE | `controller/mod.rs:108`; dispatched at `blocks/mod.rs:642-644`; tests at `controller/tests.rs:562-657`. |
| **#838** — `NiLodTriShape` dispatched separately from BSTriShape (NiTriBasedGeom inherit chain) | IN PLACE | `blocks/mod.rs:285` dispatches `BSLODTriShape` to `tri_shape::NiLodTriShape::parse`; rationale comment at lines 269-284. |

## Forward Notes (Out of Scope for This Audit)

- **Effect-shader visual completeness** (SK-D4-04 follow-on): import-side capture is the cheap fix; shader-side soft-particle depth feathering + palette LUT sampler binding need their own milestone. This is the highest-impact follow-up item from this audit.
- **SSE-buffer decoder FO4 extension** (SK-D1-NN-02): only relevant if `try_reconstruct_sse_geometry` gets extended to bsver=130. Add the gate or `debug_assert` whichever lands first.
- Other dimensions (2 BSA v105 LZ4, 3 BSLightingShaderProperty 8 variants, 5 real-data validation, 6 ESM readiness) were not run in this pass — see `--focus` arg `1,4`.
