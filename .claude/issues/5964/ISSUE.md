# SKY-D2-2026-10-17-01: Stale shader-type comments - 'ride unused' in shader_data.rs and wrong enum names in the triangle.frag ladder header

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/5964 (snapshot as filed 2026-10-17; GitHub is authoritative for current state)

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-17.md` (report ID SKY-2-01) · **Severity**: LOW (doc rot) · **Dimension**: 2 · **Game**: Skyrim SE

## Description
Comments about Skyrim shader-type dispatch no longer match the code:
1. `crates/nif/src/import/material/shader_data.rs:92-100` (`apply_shader_type_data` doc) says renderer dispatch on `material_kind` "is tracked separately (SK-D3-02). Until that lands these values ride unused on the `Material` component". Kinds 5/6/14 are consumed at `crates/renderer/shaders/triangle.frag:1547-1574`, kind 11 at `:1430` / `:2991` and `include/ray_hit.glsl:537`, parallax passes/scale at `triangle.frag:305-306` and `ray_hit.glsl:322-347`.
2. `triangle.frag:1530-1539` (ladder header) names "8 MultiIndexSnow ... 19 MultiTexture". nif.xml `BSLightingShaderType` (`nif.xml:1402-1422`) has 8 = Multitexture Landscape, 10 = Snow, 19 = Multitexture Landscape LOD Blend.
3. The type-1 bullet ("`inst.envMapIndex != 0u` fed by POM/PBR path") misstates the gate, which is `mat.envMapIndex != 0u || mat.materialKind == 1u` at `:2960-2961`, with strength via `multiLayerEnvmapStrength` (`static_meshes.rs:970-994`).
4. The kind-11 stub note at `:1596-1601` says the cubemap path is still needed; kind 11 already feeds `environmentStrength` at `:2990-2992`.

## Impact
Misleads the next reader into thinking variants are unconsumed or mapping 8/10/19 wrongly; the previous audit's "Consumed" claims were only verifiable by reading past these comments.

## Suggested fix
Rewrite the `shader_data.rs` doc to name the live consumers; correct the ladder enum names to nif.xml; fix the type-1 bullet; trim the kind-11 note to refraction / Fresnel coupling. If `triangle.frag` is edited the `.spv` must be rebuilt with plain `-V` (the reflection test needs OpName).

Related: #344, #562, #619 (closed).

## Completeness Checks
- [ ] **SIBLING**: `ray_hit.glsl` and the other shader copies checked for the same stale ladder text
- [ ] **TESTS**: `triangle.frag.spv` recompiled and the shader reflection test still passes
