# #2710: REN-D6-01: Effect-shader glass-carrier promotion lost its texture-keyword arm, and the test that pinned the FO4 direction was inverted in place rather than kept beside the new one

- **Severity**: MEDIUM
- **Dimension**: NIFAL Material
- **Location**: `byroredux/src/helpers.rs` — `classify_glass_into_material`
  (the `effect_glass_carrier` binding); test
  `glass_keyword_does_not_override_effect_shader_semantics` in the same file.
  Introduced by `322f33a8` (2026-08-10).
- **Status**: NEW
- **Description**: `classify_glass_into_material` is the alpha-aware glass
  classifier `translate_material` invokes after `resolve_pbr` — the last
  decision taken at the NIFAL boundary. Its effect-carrier arm now reads
  `material_kind == MATERIAL_KIND_EFFECT_SHADER && bgem_glass`; before
  `322f33a8` it read `… && (keyword_match || bgem_glass)`. Since `bgem_glass`
  can only be true when an external `.bgem` resolved and passed
  `bgem_uses_glass_behavior`, the promotion is now structurally unreachable for
  any `BSEffectShaderProperty` material with no external material file, and for
  BGEM-backed materials it is reachable only through that heuristic — never
  through the explicit semantic name/texture.

  The change itself is deliberate and well-motivated (Skyrim's alchemy-bench
  `InnerHaze` effect layers share `plainglasstile01.dds` with the surrounding
  shells; promoting them to glass erased their emission — the function's doc
  comment now says exactly this). The defect is that the **only** regression
  guard covering the opposite direction was rewritten in place instead of kept
  alongside: the prior test *glass_keyword_promotes_effect_shader_carrier*
  (FO4 `NukaCola_Glass:3` / `nukacola_glass.dds`, whose doc comment stated that
  FO4 commonly authors ordinary glass on a `BSEffectShaderProperty` with no
  BGEM glass flag) was renamed, its fixture swapped to `InnerHaze01:8`, and its
  assertions flipped. Nothing pins the FO4 behaviour in either direction now.
- **Evidence**:
  ```rust
  // byroredux/src/helpers.rs — live
  let keyword_match = texture_path.is_some_and(is_glass_keyword_path)
      || mesh_name.is_some_and(is_glass_keyword_path);
  let effect_glass_carrier =
      material.material_kind == byroredux_renderer::MATERIAL_KIND_EFFECT_SHADER && bgem_glass;
  ```
  `git show 322f33a8 -- byroredux/src/helpers.rs` removes
  `&& (keyword_match || bgem_glass)`, renames the test, and deletes
  `assert_eq!(m.material_kind, GLASS)`,
  `assert_eq!(m.roughness, GLASS_SURFACE_BEHAVIOR.roughness)` and
  `assert_eq!(m.ior, GLASS_SURFACE_BEHAVIOR.ior)`. `keyword_match` is still
  computed and now feeds only the non-effect arms.
- **Impact**: A cross-game divergence decided by a game-shaped rather than
  source-shaped discriminator, at the one boundary with all-game blast radius
  and no per-draw fallback to mask it. FO4/FO76 effect-shader glass whose BGEM
  misses `bgem_uses_glass_behavior`'s heuristic bundle renders as an effect
  surface with no dielectric IOR path. Magnitude is unmeasured — it depends on
  real BGEM authoring and no game archives were read — but the guard that would
  catch a regression in either direction no longer exists.
- **Related**: #2626 (`bgem_uses_glass_behavior` treats the raw refraction bit
  as an unconditional glass signal — the same predicate from the other side);
  #2477.
- **Suggested Fix**: Do not restore the bare keyword arm. Re-add it gated on
  external-material provenance (`keyword_match && from_bgsm`) — the signal that
  separates Skyrim's inline `BSEffectShaderProperty` (no `.bgem` exists
  pre-FO4) from FO4+ BGEM carriers — and restore the deleted Nuka-Cola
  assertions as a **second** test beside the `InnerHaze` one so both directions
  are pinned.

---

---
**Source**: `docs/audits/AUDIT_RENDERER_2026-08-12.md` (finding `REN-D6-01`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

