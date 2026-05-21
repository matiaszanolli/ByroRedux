# NIF Audit — 2026-05-19 (Dim 4 focused)

**Scope**: Dimension 4 — Import Pipeline Correctness (`crates/nif/src/import/`)
**Mode**: `--focus 4 --depth deep`
**Method**: static review against niftools `nif.xml`, legacy Gamebryo 2.3 source, nifly + OpenMW references, post-Session-35 layout. No archive sweeps this pass — every "needs measurement" claim is labelled.
**Dedup baseline**: 31 open issues at `/tmp/audit/nif/issues.json`; `docs/audits/AUDIT_NIF_2026-05-12.md` cross-referenced; `git log --since="2026-05-12" -- crates/nif/src/import/` walked commit-by-commit (15 commits, all open-issue follow-ups or refactors).

## Executive Summary

**0 CRITICAL · 0 HIGH · 5 MEDIUM · 4 LOW · 2 INFO = 11 findings**

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 5 |
| LOW | 4 |
| INFO | 2 |

### Headline (MEDIUM tier)

1. **NIF-DIM4-01 + 02** — #982's alpha-cascade fix only half-landed. The `MaterialInfo.alpha_property_consumed` field was added + set, but the consumer at [walker.rs:494](crates/nif/src/import/material/walker.rs#L494) still tests the old `!alpha_blend && !alpha_test`. BSEffectShader's implicit `alpha_blend = true` is also not unwound by an explicit opaque `NiAlphaProperty`. The issue was closed administratively; the bug remains. **Code-vs-issue drift** is itself the regression risk.
2. **NIF-DIM4-03** — `BSGeometry::skin_instance_ref` is parsed but never resolved. Every Starfield NPC body imports with `skin: None` and renders in bind pose. No `extract_skin_bs_geometry` sibling exists (NiTriShape + BsTriShape paths both have one).
3. **NIF-DIM4-04** — Skyrim SE BsTriShape tangent-synthesis fallback gates on `shape.normals`/`shape.uvs` which are empty after the [sse_recon.rs](crates/nif/src/import/mesh/sse_recon.rs) reconstruction path. SSE meshes lacking `VF_TANGENTS` fall through to empty tangents instead of synthesised ones — visible as lower-quality / inverted normals on UV-mirrored regions.
4. **NIF-DIM4-05** — FO76 `BSEffectShaderProperty` ships 5 fields (`reflectance_texture`, `lighting_texture`, `emit_gradient_texture`, `emittance_color`, `luminance`) that the parser reads but `capture_effect_shader_data` drops. Same defect class as the closed #719 / open #762 — FO76 surface area is incomplete.

### Dedup status

**Carryovers from prior NIF audits still open**:

- **NIF-D4-NEW-05** → promoted to MEDIUM here (NIF-DIM4-01). #982 closed administratively but the cascade-gate code change was never made.
- **NIF-D4-NEW-06** → promoted to MEDIUM (NIF-DIM4-02). #982 attached a doc-comment but no code change.
- **NIF-D4-NEW-07** → re-confirmed LOW (NIF-DIM4-09). #982 explicitly deferred.
- **NIF-D4-NEW-08** → re-confirmed LOW (NIF-DIM4-08). No code change has landed.

**Closed since 2026-05-12** (Dim-4 footprint):

| Issue | Topic | Commit |
|-------|-------|--------|
| #976 | NIF-D4-NEW-02 — BSLightingShaderProperty `.mat` capture | 5c0f290f |
| #977 | NIF-D4-NEW-03 — BSSkyShader / BSWaterShader consumers | (`material/walker.rs:443-476` + tests) |
| #1086 | REN-D16-001 / BSGeometry UDEC3 tangents | 9c91cc1a |
| #1183 | Starfield Root Material sidecar (lighting shader) | 325db725 |
| #988 | NiLodTriShape walker arm (flat + hierarchical) | bd80acb0 |
| #1077 (P1) | BGSM pbr / translucency / model_space_normals forwarded | ff7e8aa3 |
| #1076 | BGSM v>2 standalone slots (specular/lighting/flow/wrinkle) | e55a8a47 |
| #1188 | FO4 PreCombined Mesh fallback | eeddc81b |

### Infrastructure gap

dhat / alloc-counter regression coverage remains NOT wired (2026-05-04 baseline). Dim-4 findings are visual-correctness / coverage, not allocation hot paths — this gap is not load-bearing for any finding here.

---

## Findings

### MEDIUM

#### NIF-DIM4-01: NiAlphaProperty cascade gate still tests `!alpha_blend && !alpha_test` — #982's fix landed only the data plumbing

- **Game Affected**: Oblivion / FO3 / FNV (NiTriShape legacy-property path). Skyrim+ binds via `alpha_property_ref` so this path isn't hit.
- **File**: [crates/nif/src/import/material/walker.rs:493-498](crates/nif/src/import/material/walker.rs#L493-L498)
- **Symptom**: A shape that authors `NiAlphaProperty { flags: 0 }` (explicit "no blending, no test") leaves both `info.alpha_blend == false` and `info.alpha_test == false`. The cascade gate then admits the parent NiNode's `NiAlphaProperty` and silently overwrites the shape's choice. On Oblivion interior cells where the parent NiNode authors alpha-blend to cover a translucent sibling, the explicit-opaque sibling ends up rendered as alpha-blended (z-sort artifacts + visible draw-order glitch).
- **Cause**: #982 added `alpha_property_consumed` to `MaterialInfo` (line 493), defaulted it to `false` (line 836), set it from `apply_alpha_flags` (line 936). The cascade gate at line 494 was never converted to use it. The field doc-comment at `material/mod.rs:484-493` describes the intended semantic; the actual gate doesn't apply it.
- **Fix**: Replace the gate:
  ```rust
  if !info.alpha_property_consumed {
      if let Some(alpha) = scene.get_as::<NiAlphaProperty>(idx) {
          apply_alpha_flags(&mut info, alpha);
      }
  }
  ```
  Add round-trip test in `material/alpha_flag_tests.rs`.
- **Estimated Impact**: visual correctness on Oblivion / FO3 / FNV interior meshes. **Needs measurement** on `Oblivion - Meshes.bsa` to quantify (shapes with both inherited and direct NiAlphaProperty).
- **Regression Risk**: tiny — the new gate is strictly stricter.

#### NIF-DIM4-02: BSEffectShader's implicit `alpha_blend = true` is not unwound by an explicit opaque NiAlphaProperty on the same shape

- **Game Affected**: Skyrim+ / FO4+ (effect-shader-backed shapes bound to a `NiAlphaProperty { flags: 0 }`).
- **File**: [crates/nif/src/import/material/walker.rs:426-435](crates/nif/src/import/material/walker.rs#L426-L435) (implicit blend) + [:480-484](crates/nif/src/import/material/walker.rs#L480-L484) (`apply_alpha_flags`)
- **Symptom**: A BSEffectShader-backed shape with `alpha_property_ref → NiAlphaProperty { flags: 0 }` ends up `info.alpha_blend == true` (set implicitly) and never cleared. Renderer routes to transparent pipeline, costing a sort + alpha-test miss.
- **Cause**: ordering. BSEffectShader branch runs first; implicit `info.alpha_blend = true` fires at line 427. Then `alpha_property_ref` branch runs at line 480-484. `apply_alpha_flags` only clears `alpha_blend` when `alpha_test` is set (line 925); with `flags=0`, nothing clears the implicit blend.
- **Fix**: Defer the implicit-blend write until after `alpha_property_ref` consumed. In the BSEffectShader branch, record `set_implicit_blend = !info.alpha_property_consumed`; after `alpha_property_ref`, if `set_implicit_blend && !info.alpha_property_consumed && !info.alpha_blend && !info.alpha_test`, then write `info.alpha_blend = true`.
- **Estimated Impact**: visual correctness on Skyrim+ / FO4+ effect-shader meshes with explicit-opaque NiAlphaProperty. **Needs measurement** on `Skyrim - Meshes.bsa`.
- **Regression Risk**: low — only affects the (currently broken) explicit-opaque pairing.

#### NIF-DIM4-03: `BSGeometry::skin_instance_ref` parsed but never consumed — Starfield NPC bodies import with `skin: None` and render in bind pose

- **Game Affected**: Starfield (sole `BSGeometry` user)
- **File**: [crates/nif/src/import/mesh/bs_geometry.rs:228-230](crates/nif/src/import/mesh/bs_geometry.rs#L228-L230) — the `Some(ImportedMesh { … skin: None, … })` literal
- **Symptom**: Every Starfield BSGeometry with a `skin_instance_ref` populates `ImportedMesh.skin = None`. Cell loader's "no skin → rigid placement" branch fires; NPC bodies, creatures, dismemberment-capable geometry all sit in bind pose regardless of skeleton + animation.
- **Cause**: parsed at [blocks/bs_geometry.rs:60, 98, 118](crates/nif/src/blocks/bs_geometry.rs#L60). NiTriShape and BsTriShape have `extract_skin_*` siblings; no `extract_skin_bs_geometry` exists. The literal `skin: None,` is the single drop site.
- **Fix**: implement `extract_skin_bs_geometry(scene, shape)` mirroring the BsTriShape FO4+ path ([mesh/skin.rs:174-211](crates/nif/src/import/mesh/skin.rs#L174-L211)). Starfield uses `BSSkin::Instance` + `BSSkin::BoneData` per nif.xml. Per-vertex bone indices/weights deferred until BSGeometry parser surfaces them (separate scope).
- **Estimated Impact**: structural. Every Starfield NPC import is stillborn today. **Needs measurement**: count of BSGeometry blocks in vanilla `Starfield - *.ba2` with `skin_instance_ref != BlockRef::NULL`; gut ≥10K (all character meshes).
- **Regression Risk**: low — defaulting to `None` today is a silent no-op; wiring up only adds correctness.

#### NIF-DIM4-04: BSTriShape tangent-synthesis fallback bails when inline `shape.normals` / `shape.uvs` are empty — SSE-reconstructed shapes without VF_TANGENTS get empty tangents

- **Game Affected**: Skyrim SE (BSTriShape with `data_size == 0` going through `try_reconstruct_sse_geometry`)
- **File**: [crates/nif/src/import/mesh/bs_tri_shape.rs:133-170](crates/nif/src/import/mesh/bs_tri_shape.rs#L133-L170)
- **Symptom**: SSE BsTriShape with geometry in `SseSkinGlobalBuffer` AND vertex descriptor lacking `VF_TANGENTS` ends up `tangents = Vec::new()`. The synthesis check tests `shape.normals` / `shape.uvs`, which are empty in the reconstruction case (the data lives in local `positions / sse_normals / sse_uvs`). Renderer falls back to screen-space derivative TBN → lower-quality normal maps + inverted normals on UV-mirrored regions.
- **Cause**: precedence chain at lines 133-170 tests `shape.normals` / `shape.uvs` — empty after reconstruction. Synthesis branch gate fails; empty fallback fires.
- **Fix**: extract a `synthesize_tangents_yup` sibling in `mesh/tangent.rs` (the `bs_geometry.rs` deferred comment at lines 113-118 already calls this out). Branch to it from the SSE-reconstructed path using `positions` + `sse_normals` + `sse_uvs` + the recomputed `triangles_for_synth`.
- **Estimated Impact**: visual quality on Skyrim SE NPCs / creatures lacking `VF_TANGENTS`. **Needs measurement**: walk `Skyrim - Meshes0.bsa` counting SSE-reconstructed BSTriShape blocks where `vertex_attrs & VF_TANGENTS == 0`. Initial guess: low single-digit percent of skinned SSE meshes.
- **Regression Risk**: low — replacing "empty → Path-2 fallback" with "synthesised → Path-1" is monotonic on quality.

#### NIF-DIM4-05: BSEffectShaderProperty's FO76-specific fields silently dropped at import

- **Game Affected**: FO76 (BSVER == 155)
- **File**: [crates/nif/src/import/material/shader_data.rs:11-63](crates/nif/src/import/material/shader_data.rs#L11-L63) (`capture_effect_shader_data`)
- **Symptom**: `BSEffectShaderProperty` ships 5 FO76-specific fields: `reflectance_texture`, `lighting_texture`, `emit_gradient_texture`, `emittance_color`, `luminance` (`LuminanceParams`). Parser at [blocks/shader.rs:1346-1355](crates/nif/src/blocks/shader.rs#L1346-L1355) reads them; `capture_effect_shader_data` omits all five. Same defect class as the open #762 `.mat` path and the closed #719 env-map / env-mask forwarding.
- **Cause**: `BsEffectShaderData` ([material/mod.rs:752-812](crates/nif/src/import/material/mod.rs#L752-L812)) has no fields for the FO76 quintet. Capture can't emit what doesn't exist on the destination.
- **Fix**: extend `BsEffectShaderData`:
  ```rust
  pub reflectance_texture: Option<String>,
  pub lighting_texture: Option<String>,
  pub emit_gradient_texture: Option<String>,
  pub emittance_color: Option<[f32; 3]>,
  pub luminance: Option<LuminanceParams>,
  ```
  Populate in `capture_effect_shader_data` (gate on non-default per existing pattern). Renderer consumption = separate scope.
- **Estimated Impact**: data plumbing only — renderer doesn't consume yet. Pattern mirrors #345 (capture-ready, render-side follow-up). FO76 emissive / luminance materials are common on UI overlays, glow planes, reactive effects.
- **Regression Risk**: zero today (no consumer).

---

### LOW

#### NIF-DIM4-06: `BsTriShapeKind::SubIndex` segmentation payload silently dropped at import

- **Game Affected**: Skyrim SE DLC / FO4 / FO76 (BSSubIndexTriShape)
- **File**: [crates/nif/src/import/mesh/bs_tri_shape.rs:172+](crates/nif/src/import/mesh/bs_tri_shape.rs#L172) — no SubIndex propagation in the `ImportedMesh` literal
- **Symptom**: parser successfully decodes `BsSubIndexTriShapeData` (segments table + optional shared SSF metadata) but `extract_bs_tri_shape` drops the `kind` discriminator. Dismemberment / body-part segmentation is invisible to consumers.
- **Cause**: `ImportedMesh` doesn't model the wire-type discriminator for `BsTriShape` (only for `NiNode` subclasses via `range_kind` / `bs_value_node` / `bs_ordered_node`).
- **Fix**: add `bs_sub_index: Option<BsSubIndexData>` to `ImportedMesh`. Extract when `shape.kind == BsTriShapeKind::SubIndex(data)`.
- **Estimated Impact**: blocks dismemberment system implementation (currently deferred). Same orphan-parse pattern as NIF-D5-NEW-01.
- **Regression Risk**: zero (additive).

#### NIF-DIM4-07: `BsTriShapeKind::LOD { lod0, lod1, lod2 }` triangle-count cutoffs dropped — FO4 distant-LOD selection loses authored thresholds

- **Game Affected**: FO4 (BSLODTriShape, distinct from NiLodTriShape which #988 wired)
- **File**: same drop site as DIM4-06
- **Symptom**: parser captures the 3 LOD cutoffs; importer discards them. Distant-terrain / building LOD selection at render time has no authored input.
- **Cause**: same as DIM4-06 — no `ImportedMesh` field.
- **Fix**: add `bs_lod_cutoffs: Option<[u32; 3]>` to `ImportedMesh`. Populate when `shape.kind == BsTriShapeKind::LOD { … }`.
- **Estimated Impact**: future M35 LOD selector blocked from honouring authored thresholds.
- **Regression Risk**: zero (additive).

#### NIF-DIM4-08 (carryover, re-confirmed): `NiVertexColorProperty` from inherited-property loop silently overrides Skyrim+ shader-flag vertex-color intent

- **Game Affected**: Skyrim+ modded content (vanilla doesn't trip it)
- **File**: [crates/nif/src/import/material/walker.rs:888-891](crates/nif/src/import/material/walker.rs#L888-L891)
- **Symptom**: A Skyrim+ mesh authoring both `BSLightingShaderProperty` (with vertex-color flags) AND a legacy `NiVertexColorProperty` (inherited) lets the latter unconditionally overwrite `info.vertex_color_mode`. Modded content fielding both lands on the wrong mode.
- **Cause**: no `has_material_data` / `material_kind != 0` gate. BSLighting branch sets `has_material_data = true` at line 296; the NiVertexColorProperty consumer here doesn't read it.
- **Fix**: gate assignment on `!info.has_material_data`, mirroring the `texture_path.is_none()` pattern.
- **Estimated Impact**: niche modded corner. **Needs measurement**: vanilla overlap rate is zero; modded Skyrim rate unknown.

#### NIF-DIM4-09 (carryover, re-confirmed): `BSGeometry` external-LOD slot 0 short-circuits to None when LOD 0 is `External` despite later LODs being `Internal`

- **Game Affected**: Starfield (theoretical — vanilla doesn't mix LOD slots within one BSGeometry)
- **File**: [crates/nif/src/import/mesh/bs_geometry.rs:28-62](crates/nif/src/import/mesh/bs_geometry.rs#L28-L62)
- **Symptom**: when `shape.has_internal_geom_data()` is set, `extract_bs_geometry` pulls `shape.meshes.first()` and bails with `None` if that first slot is `External`. No fallback to scan remaining LOD slots. The mirror "Stage B" code-path correctly iterates every slot.
- **Cause**: structural asymmetry. Stage-A `first()` assumes LOD 0 is always Internal when the flag is set.
- **Fix**: replace with `shape.meshes.iter().find_map(|m| match &m.kind { Internal {…} => Some(…), External {…} => None })`.
- **Estimated Impact**: dormant on vanilla; insurance against future content / mods.

---

### INFO

#### NIF-DIM4-10: `BSLightingShaderType` doc-text in nif.xml uses "TS6" for EnvMap Mask but the actual `BSShaderTextureSet` slot is 5 — current Rust code is correct

- **File**: [crates/nif/src/import/material/walker.rs:233-245](crates/nif/src/import/material/walker.rs#L233-L245)
- **Status**: NOT a bug. The `BSLightingShaderType` enum descriptions say "Enables EnvMap Mask(TS6)" for EnvironmentMap (1), MultiLayerParallax (11), EyeEnvmap (16). But `BSShaderTextureSet`'s Textures field documentation (nif.xml line 6310-6319) says slot 5 = Environment Mask, slot 6 = Subsurface (MultiLayerParallax), slot 7 = Back Lighting. OpenMW's `TextureType` enum confirms slot 5 = `EnvironmentMask`. Our code routes slot 5 → `env_mask` (correct).
- **Fix**: none in code. Recommend an inline comment near the slot-routing match arm citing this to inoculate future audits.

#### NIF-DIM4-11: `ImportedMesh.specular_map / lighting_map / flow_map / wrinkle_map / is_pbr / has_translucency / model_space_normals` are NIF-import-side stubs filled at the BGSM merge stage — by-design but worth documenting

- **File**: [crates/nif/src/import/types.rs:376-426](crates/nif/src/import/types.rs#L376-L426)
- **Status**: NOT a bug. All four mesh extractors set these to `None` / `false` with matching comments pointing at `merge_bgsm_into_mesh`. Spread "who owns this field" across NIF crate + BGSM provider. A future refactor could hoist the merge into a single post-process function and tag the relevant fields `pub(crate)` (currently `pub`, lets consumers accidentally set them at NIF import and get silently clobbered).
- **Fix**: doc-comment addition only.

---

## Cross-Checklist Coverage

Mapping back to the 12-item Dim-4 checklist:

| # | Checklist item | Status |
|---|---|---|
| 1 | NiAVObject `.av.*` routing | ✅ CLEAN — every consumer routes through `.av.*` |
| 2 | Shader property lookup per game | ⚠️ DIM4-05 (FO76 effect-shader fields dropped post-parse); rest clean |
| 3 | Texture slot routing | ✅ CLEAN — slot 5 = env_mask matches OpenMW (DIM4-10 = doc-text drift in nif.xml, not engine drift) |
| 4 | Z-up → Y-up consistency | ✅ CLEAN — positions, normals, tangent.xyz (authored + synthesised), bone bind poses, light vectors, BSBound all routed |
| 5 | Decal flag detection | ✅ CLEAN — legacy vs modern split intact (#454 lockstep); cross-era CRC32 + compile-time asserts pin bit equivalence |
| 6 | Two-sided detection | ✅ CLEAN — FO3/FNV skip SLSF2 per #441; Skyrim+ FO76+ CRC32 fallback per #712 |
| 7 | Alpha property resolution | ❌ DIM4-01 (cascade gate stale) + DIM4-02 (BSEffect implicit blend) |
| 8 | Tangent extraction | ⚠️ DIM4-04 (SSE-reconstruction synthesis gate); 3 of 4 paths clean (#786 swap, FO4 inline #795/#796, Mikkelsen fallback) |
| 9 | Material path capture | ✅ CLEAN — `.bgsm` / `.bgem` / `.mat` case-insensitive, trailing whitespace trim per #749, BSLighting + BSEffect both delegate (#976) |
| 10 | ImportedScene/Mesh/Node field contracts | ⚠️ DIM4-03 / DIM4-06 / DIM4-07 (missing-field gaps); walk flat + hierarchical don't diverge |
| 11 | SSE skinned-geometry reconstruction (#559) | ✅ CLEAN — gate fires only when `vertices.is_empty() && triangles.is_empty()`; #725 drop-on-out-of-range vertex_map policy in place |
| 12 | Collision import | ✅ CLEAN — Havok ×7.0 scale verified against legacy 2.3 SRC; mass/friction/restitution/damping preserved; motion type mapping correct (5 = FIXED folded into `_ => Static` catch-all, semantically correct) |

---

## Prioritized Fix Order

### Correctness (MEDIUM — visual / structural)

1. **NIF-DIM4-01** — flip the cascade gate to `!info.alpha_property_consumed`. One-line fix + synthetic test. Closes the half-landed #982. **LOW risk.**
2. **NIF-DIM4-02** — defer the BSEffectShader implicit blend until after `alpha_property_ref` is consumed. Synthetic test. **LOW risk.**
3. **NIF-DIM4-03** — implement `extract_skin_bs_geometry`. Unblocks Starfield NPC skinning. **LOW risk** (currently always returns None).
4. **NIF-DIM4-04** — `synthesize_tangents_yup` sibling, branched from the SSE-reconstructed path. **LOW risk** (monotonic on quality).

### Data plumbing (MEDIUM — capture-only, renderer follow-up later)

5. **NIF-DIM4-05** — extend `BsEffectShaderData` with the 5 FO76 fields + capture wiring. **ZERO risk** (no consumer yet).

### Coverage gaps (LOW — additive)

6. **NIF-DIM4-06 + 07** — add `bs_sub_index` and `bs_lod_cutoffs` to `ImportedMesh`. Unblocks dismemberment system + M35 LOD selector. **ZERO risk** (additive).

### Carryovers (LOW — defer until #982-era patterns are re-examined)

7. **NIF-DIM4-08** — `NiVertexColorProperty` gate on `!has_material_data`. Niche modded corner.
8. **NIF-DIM4-09** — `BSGeometry` external-LOD slot 0 iteration. Dormant on vanilla.

### Documentation hygiene (INFO)

9. **NIF-DIM4-10** — inline comment near slot-5 routing citing OpenMW + nif.xml line 6310-6319.
10. **NIF-DIM4-11** — doc-comment on the BGSM-merge fields in `types.rs`; consider `pub(crate)` visibility narrowing in a future pass.

---

## Notes

- **Premise drift**: each finding's code excerpt was verified against the post-Session-35 file layout. `walk.rs` → `walk/mod.rs` rename + `mesh.rs` / `material.rs` directory splits translated through.
- **Carryover handling**: #982 was inspected commit-by-commit (`git show 01957517`). Three of its four Group-D claims landed data plumbing but not consumer changes (DIM4-01/02) or explicitly deferred (DIM4-09). These surface here as MEDIUM rather than LOW because the bug-vs-fix-claim drift is itself a regression-risk vector — a future auditor reading #982's body would conclude the cascade gate was fixed.
- **`needs measurement` labels**: DIM4-01 (real-content prevalence), DIM4-02 (FO4 Effect+NiAlpha overlap), DIM4-03 (Starfield NPC skin count), DIM4-04 (SSE no-VF_TANGENTS rate), DIM4-05 (FO76 emissive-material count) all benefit from a one-shot archive sweep; none structurally hard.
- **Scope boundary**: GPU-side / Vulkan validation out of scope. DIM4-03 / DIM4-05 / DIM4-06 / DIM4-07 touch `GpuInstance` via `ImportedMesh` but the cause is import-side; renderer-side dispatch is separate work.
- No speculative Vulkan barrier changes; no allocation-hot-path findings.
