**Severity**: MEDIUM · **Dimension**: 11 — NIFAL Canonical-Translation Safety (NaN-on-GPU)
**Location**: `byroredux/src/material_translate.rs:232-233` (`normal_alpha_spec_roughness`) + `:286-288` (`resolve_normal_alpha_spec_roughness` writeback)
**Source**: `docs/audits/AUDIT_SAFETY_2026-06-14.md` (SAFE-D11-NEW-05; carryover of unpublished 2026-06-11 SAFE-D11-NEW-03)

## Description
The #1480 fix moved the normal-alpha-as-spec roughness derivation from per-draw render time to a once-at-spawn resolve — correct for the resolve-once contract, but the formula `(1.0 - glossiness / 100.0).clamp(0.05, 0.95)` (line 233) runs in `resolve_normal_alpha_spec_roughness`, which executes **after** `resolve_pbr()` and **overwrites** the resolved canonical roughness (`m.roughness = r`, line 287). `Material.glossiness` is a raw NIF binary float (`walker.rs:314` `shader.glossiness`, `:600` `mat.shininess`) with no `is_finite()` guard anywhere on its path, and Rust's `f32::clamp` **propagates NaN** (`NaN.clamp(a, b) == NaN`). A non-finite `glossiness` on an alpha-bearing-normal lit surface therefore ships `roughness = NaN` past the only NaN gate in the pipeline (`resolve_pbr`'s `is_nan` check at `material.rs:639`, which already ran) into the `GpuMaterial` SSBO. Code unchanged since June 11.

## Evidence
gate `normal_alpha_spec_applies` (`material_translate.rs:189-201`) checks `metalness`/`env_map_scale` (NaN comparisons are false, so those NaNs self-block) but **not** `glossiness`; `glossiness` is only consumed in the NaN-surviving `clamp` at line 233. The `specular_strength > 1.2` arm (line 234) self-blocks on NaN; the `normal_has_alpha` arm does not.

## Impact
NaN roughness on the GPU for the affected draw — NaN GGX terms poison the lit color, and through SVGF/TAA temporal accumulation a single NaN pixel contaminates history buffers (sticky, frame-persistent). Gate population is large (every Skyrim/Gamebryo-era lit surface with an alpha-bearing normal map and no gloss map); trigger needs a malformed/non-finite `glossiness`, consistent with the MEDIUM precedent (#1411/#1434).

## Related
#1434 (OPEN — same class, `NiPSysGrowFadeModifier.base_scale`); #1480 (CLOSED — created this code, did not flag the NaN path); #1500 (OPEN — adjacent `NORMAL_ALPHA_SPEC_BIT` lockstep pin, different concern); 2026-06-11 SAFE-D11-NEW-03 (never published).

## Suggested Fix
In `normal_alpha_spec_roughness`, early-return `None` when `!glossiness.is_finite()` (one line), or sanitize `glossiness` to a finite default at the `translate_material` boundary so every downstream consumer (including `resolve_pbr`'s classifier arm at `material.rs:579`) is protected.

## Completeness Checks
- [ ] **SIBLING**: Other roughness/spec derivation arms in the same function (`specular_strength` arm self-blocks; confirm no other NaN-survivor)
- [ ] **CANONICAL-BOUNDARY**: The fix keeps per-game logic at the `translate_material` / `Material::resolve_pbr` boundary — never pushed into shaders/renderer, never re-derived at render time (see `/audit-nifal`)
- [ ] **TESTS**: A regression test pins this fix (a non-finite `glossiness` yields a finite canonical `roughness`)
