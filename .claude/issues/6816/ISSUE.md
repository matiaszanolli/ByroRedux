# NIFAL-2026-11-08-01: Env-mask classifier change (2ed1d91ce) has no wiring test, an unmeasured non-FNV population, and is unrecorded in the spec

GitHub: #6816 (snapshot as filed; GitHub is authoritative for state)
Labels: low,nifal,renderer,shaders,test-gap,bug,game:fnv,game:fo3,game:skyrim

- **Severity**: LOW
- **Dimension**: Material (found by Dim 1 and Dim 8)
- **Tier Violated**: harness-gap (secondary: no-fabrication, measurement covers FNV only)
- **Game Affected**: FO3, FNV (intended); Skyrim type-1 EnvironmentMap and FO4 non-BGSM shapes (side effect)
- **Location**: `crates/core/src/ecs/components/material.rs:1415` (mask ceiling), `:1566` (`resolve_pbr` hardcodes `has_env_mask: false`); `crates/nif/src/import/material/mod.rs:1463`; `byroredux/src/material_translate.rs:678`; `crates/renderer/shaders/triangle.frag:1803-1809`
- **Status**: NEW
- **Description**: The change caps an env-mapped surface that authors an environment mask at roughness 0.55 (`ENV_MASK_ROUGHNESS_CEILING`), and the fragment shader blends metalness toward 0.9 by the mask texel. Three gaps. (1) The only tests of `has_env_mask` set it by hand on the classifier input (`material.rs:2344-2350`). Nothing exercises the two production wires, `classify_legacy_pbr` (`mod.rs:1463`) and the overlay recompute (`material_translate.rs:678`). Setting either to `false` leaves every test green and silently reverts the FNV fix. `resolve_pbr` also passes a bare `false` with no comment. (2) The only cited measurement is FNV (3,576 meshes, 591 reflective). The classifier arm is not game- or kind-gated, so Skyrim and FO4 shapes that fill the `environment_mask` role also change: the Skyrim SE ebony cuirass goes from roughness 0.8 to 0.55, across the renderer's `roughness < 0.6` reflection gate. That may be a fix, but it is unmeasured. The shader metalness blend is gated on `materialKind == 0u`, so kind-1 Skyrim gets the lower roughness without the metalness weighting. The census has no committed tool. (3) The GLSL constants `0.3` and `0.9` mirror Rust values with no pin, and neither `nifal.md` nor `material-abstraction.md` mentions the new classifier input.
- **Failure scenario**: A refactor drops `has_env_mask` from `classify_legacy_pbr` or the recompute arm. FNV pistols, typewriters and gurneys return to roughness 0.8, above the reflection gate, and CI stays green.
- **Suggested fix**: Add one nif-crate test (`MaterialInfo` with `env_map_scale = 1.0`, `env_mask = Some`, no authored specular, expect roughness == `ENV_MASK_ROUGHNESS_CEILING`) and one translate-level test for the recompute arm. Comment the `false` in `resolve_pbr`. Pin the 0.3 / 0.9 / 0.6 couplings the way `nif_importer_material_kind_literals_match_renderer_constants` pins kinds. Run the census on Skyrim SE / FO4, commit the tool beside `crates/nif/examples/synthetic_height_census.rs`, and either gate `has_env_mask` to the kind-0 population or record the result in `nifal.md`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test or guard pins this specific fix


_Source: `docs/audits/AUDIT_NIFAL_2026-11-08.md` (HEAD 51ce1c2f9)._
