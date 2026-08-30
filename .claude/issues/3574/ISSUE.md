# #3574 — REN-2026-08-30-D17-01: `MAT_FLAG_TRANSLUCENCY`'s #1147 Phase 2b subsurface term is unreachable — the per-light contribution gate `continue`s on exactly the `−N·L > 0` geometry that is the term's only non-zero domain

**Labels**: `medium,renderer,shaders,bug`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3574 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Disney BSDF (Phase 2b sibling gating)
- **Location**: `crates/renderer/shaders/triangle.frag` (contribution gate, lines 2865–2877; translucency block, lines 2913–2954), `crates/renderer/shaders/include/lighting.glsl` (`bethesdaDiffuseLightFactor` line 80, `bethesdaRimFactor` line 98, `bethesdaBackFactor` line 106)
- **Status**: NEW. Not in the 159-issue OPEN set (keyword sweep of `issues.json` for `translucen|sss|subsurface|bsdf` returns only #3452/#3448/#3071, none of which is this) and not in `docs/audits/AUDIT_RENDERER_2026-08-27.md` (`grep -n "translucen\|backDotL\|TRANSLUCENCY"` → no hits). Predates the 2026-08-25 Bethesda work — `git log -L 2864,2876:triangle.frag` shows the gate was `float contribution = NdotL * atten;` before `ceb69d24` widened it, so the defect is longstanding, not a regression of that commit.
- **Description**: Inside the cluster light loop, the per-light early-out is

  ```glsl
  float rawNdotL = dot(N, L);
  float NdotL = max(rawNdotL, 0.0);
  vec3 diffuseGate = bethesdaDiffuseLightFactor(mat, lightingMask, rawNdotL);
  float legacyGate = max(bethesdaRimFactor(mat, NdotV, NdotL),
                         bethesdaBackFactor(mat, rawNdotL));
  float contribution = max(max(diffuseGate.r, max(diffuseGate.g, diffuseGate.b)),
                           legacyGate) * atten;
  if (contribution < 0.001) { continue; }
  ```

  55 lines further down, still in the same iteration, the Phase 2b block runs
  `float backDotL = max(-dot(N, L), 0.0);` and accumulates
  `sssTint * translucencyTransmissiveScale * thicknessShape * turbMod * unshadowedRadiance`.
  `backDotL` is non-zero **iff** `rawNdotL < 0`. For a material that carries
  `MAT_FLAG_TRANSLUCENCY` and nothing else from the Bethesda lighting-response
  family, all three gate terms are identically zero on that half-space:
  `bethesdaDiffuseLightFactor` returns `vec3(max(rawNdotL, 0.0))` when
  `MAT_FLAG_SOFT_LIGHTING` is clear (lighting.glsl:84-86);
  `bethesdaRimFactor` returns `0.0` when `MAT_FLAG_RIM_LIGHTING` is clear
  (line 99) and is otherwise multiplied by `frontNdotL == 0`;
  `bethesdaBackFactor` returns `0.0` when `MAT_FLAG_BACK_LIGHTING` is clear
  (line 107). So the loop `continue`s. On the complementary half-space
  (`rawNdotL >= 0`) the gate passes but `backDotL == 0`, so the term is zero
  there too. The translucency contribution is therefore identically zero at
  every fragment, for every light.
- **Evidence**:
  - The three flags are genuinely disjoint on real content, so the "SOFT_LIGHTING rescues it" escape does not exist. `MAT_FLAG_TRANSLUCENCY` comes from `ImportedMaterial::has_translucency`, set only from `bgsm.translucency` (`byroredux/src/asset_provider/material.rs:40-41`), which `crates/bgsm/src/bgsm.rs:207-213` reads only when `version >= 8`. `MAT_FLAG_SOFT_LIGHTING` comes from `ImportedMaterial::soft_lighting`, set either from `bgsm.subsurface_lighting` (`forward_bgsm_rim_subsurface`, `byroredux/src/asset_provider/material.rs:92-97`), which `bgsm.rs:214-219` reads only in the `else` arm (`version < 8`), or from `skyrim_slsf2::SOFT_LIGHTING` — and `crates/nif/src/import/material/dedicated_shader.rs:170-181` extracts those three SLSF2 bits **only** for `TextureSlotLayout::Skyrim`, a family that ships no BGSM. Same argument for `MAT_FLAG_BACK_LIGHTING`.
  - Bit packing confirmed at `byroredux/src/cell_loader.rs:255-276` (`pack_imported_material_flags`) and `crates/renderer/src/shader_constants_data.rs:407-422`.
  - `grep -n "MAT_FLAG_TRANSLUCENCY" crates/renderer/shaders/**` → the only shading consumer is triangle.frag:2913 (2933/2946 are the `THICK_OBJECT`/`MIX_ALBEDO` sub-branches inside it; 1547 is the `viewMaterialLobe` debug colour). There is no second, ungated evaluation site.
  - The feed is fully wired and non-trivial: `translucency_subsurface_{r,g,b}`, `translucency_transmissive_scale`, `translucency_turbulence` are parsed (`bgsm.rs:209-213`), merged (`asset_provider/material.rs:1501-1514`), uploaded (`crates/renderer/src/vulkan/material.rs`), and offset-pinned. All of it terminates in dead shader code.
- **Impact**: The entire #1147 Phase 2b subsurface feature produces zero output on 100% of loaded content. FO4 foliage, paper, thin cloth, skin and frost-rimed glass — every vanilla BGSM v≥8 material that authors `bTranslucency` — never shows the back-lit wraparound the flag exists to produce, and the failure is silent: the flag is set, the fields are non-zero in the GPU material, `mat.dump` shows a correctly translated material, and `viewMaterialLobe` paints the fragment magenta ("translucency"). Nothing in `cargo test` can see it, because the defect is a control-flow ordering between two blocks that are each independently correct.
- **Suggested Fix**: Fold the translucency driver into the gate rather than moving the block. Add a fourth term alongside `legacyGate`, e.g.
  `float sssGate = ((mat.materialFlags & MAT_FLAG_TRANSLUCENCY) != 0u) ? max(-rawNdotL, 0.0) * mat.translucencyTransmissiveScale : 0.0;`
  and include it in the `max(...)` that forms `contribution`. That keeps the early-out doing its job (it exists to skip lights that cannot contribute) while making it agree with the set of lobes evaluated below it. Do **not** simply lower the `0.001` threshold — the gate would still be zero, because the driver is absent from it, not merely small. Pin the result with a shader-source contract test in `shader_contract_tests.rs` in the style of `disney_sheen_keeps_its_relative_weight_in_canonical_direct_path`: assert that the `contribution` expression mentions a translucency term, so a future edit cannot silently re-orphan the block.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D17-01

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
