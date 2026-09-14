# #4401 — NIFAL-D8-2026-09-14-02: #4235 moves base-texture precedence to the shader but leaves the paired clamp mode and the parallax slot first-writer-wins

**Labels**: low,nifal,import-pipeline,bug,game:fo3,game:fnv
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (vanilla renders unchanged; mod content exposed)
- **Dimension**: Shader-flags/Effects
- **Tier Violated**: single-boundary (per-role precedence split across two independent latches in one walker)
- **Game Affected**: FO3 / FNV
- **Location**: `crates/nif/src/import/material/legacy_properties.rs:58-70` (`claim_shader_texture`), `:432-437` (clamp latch), `:518` (parallax slot `is_none()`-gated); consumer `byroredux/src/cell_loader/spawn/mesh_instance.rs:933-938`
- **Status**: NEW (introduced by `8dfa78eee`, the fix for closed #4235)
- **Description**: After #4235 the base path always comes from `BSShaderTextureSet`, but `texture_clamp_mode` is still latched by whichever property ran first. When `NiTexturingProperty` is listed first, the shader's texture is sampled with the legacy property's address mode. The parallax/height role has the same split, and there is no in-code deferral comment for either.
- **Evidence**: `claim_shader_texture` clears only the `texturing_property_roles` bits. The clamp write at `:584` stays behind `!texture_clamp_mode_consumed`. The new precedence test asserts paths only.
- **Impact**: On mod content with differing paths: wrong edge wrap/clamp on the shader's texture, and normal/height pairs from different sources. The 5+5 co-bound vanilla shapes name identical paths.
- **Related**: #4235, #3517, #2328, #208.
- **Suggested Fix**: When `claim_shader_texture` displaces a base path, let that block re-latch `texture_clamp_mode`. Apply the same rule to parallax, or add a `#4235` deferral comment at `:518`. Extend the precedence test to assert clamp mode.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
