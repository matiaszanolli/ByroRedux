# #4428: FO4-D2-2026-09-16-02: The BGSM merge binds `envmap_texture` regardless of the chain's `environment_mapping` flag; a bound cubemap switches the shader into "explicit environment" with strength `env_map_scale` (0), wasting RT reflection rays and zer…

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4428
- **Labels**: medium,nifal,renderer,performance,game:fo4,legacy-compat,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_FO4_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: MEDIUM (performance plus a wrong reflection term, visual only; most affected surfaces are dielectric)
- **Dimension**: 2 (BGSM/BGEM consumption)
- **Location**:
  - `byroredux/src/asset_provider/material/merge.rs:692-698`: unconditional `fill(&mut material.textures.environment, &bgsm.envmap_texture, …)`
  - Its gated siblings: `byroredux/src/asset_provider/material/mod.rs:166-177` (`forward_bgsm_env_map_scale`, gated on `bgsm.base.environment_mapping`) and `merge.rs:1059-1072` (the BGEM env fill, gated on `env_mapping_enabled()` since #2643)
  - Shader: `crates/renderer/shaders/triangle.frag:2865-2870` (`hasAuthoredCubemap = envMapIndex != 0` → `hasExplicitEnvironment` → `needsEnvironmentReflection`), `:2896-2898` (`environmentStrength = max(multiLayerEnvmapStrength, 0)`), `:2976-2977`
  - `byroredux/src/render/static_meshes.rs:858-876` (`multi_layer_envmap_strength = env_map_scale` for non-MLP)
- **Status**: NEW. This is the BGSM sibling of #2643, which gated only the BGEM arm.
- **Description**: The BGSM arm honours the authored `environment_mapping` bit for the mask *scale* but ignores it for the cubemap *texture*. That is the same "one authored bit, honoured for one field and ignored for its neighbour" split that #2643 fixed on BGEM. In `triangle.frag`, a non-zero `envMapIndex` counts as explicit environment authoring:
  - `needsEnvironmentReflection` becomes true even for dielectrics.
  - An RT reflection ray is traced whenever `roughness < 0.6`.
  - The result is scaled by `environmentStrength = env_map_scale`.

  With env mapping disabled, `env_map_scale` keeps the NIF value, which is 0 for every non-Envmap shader type. The traced reflection is therefore multiplied by zero. Conductors (`metalness > 0.3`) that would otherwise take the implicit strength-1.0 path lose their reflection entirely.
- **Evidence**:
  - BGSM corpus: 3,446 BGSMs author `envmap_texture`; **444** of them have `environment_mapping = false` (e.g. `diamondsigns01alphaflatglow.bgsm`, `rrdiabanner01blank.bgsm`).
  - `Fallout4 - Meshes.ba2`, BGSM-backed shapes whose whole template chain has env mapping off but a cubemap is bound anyway: **8,979**. Of these, **6,130** are bound only by this merge fill. The other 2,849 are bound by the NIF's unconditional FO4 slot-4 route (160 NIF-only, 2,689 both).
  - Of the 8,979: 8,806 have `env_map_scale = 0.0`, so the reflection term is multiplied by zero. 7,170 have leaf roughness < 0.6 and pay for a reflection ray. Up to 181 have saturation metalness > 0.3 and lose reflections they get without the cubemap. 173 are `shader_type = 1` with a non-zero scale and gain an env reflection the BGSM disabled.
- **Impact**: Wasted per-fragment RT reflection rays on thousands of vanilla surfaces. Missing reflections on a small set of conductors. Reflections the material turned off appear on 173 shapes.
- **Related**: #2643, #2608, #2472, NIFAL-D8-2026-09-16-03 (same "BGSM enable bit parsed but not consulted" shape for `glowmap`)
- **Suggested Fix**: Gate the BGSM `environment` fill on the chain's `environment_mapping`, as `forward_bgsm_env_map_scale` already does. Separately, decide whether an authoritative BGSM "env mapping off" should also suppress a NIF-sourced slot-4 cubemap. FO4 treats the BGSM as authoritative for material semantics, but the merge's documented rule is "NIF wins". That decision needs a source, as NIFAL-D8-03 argues for `glowmap`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
