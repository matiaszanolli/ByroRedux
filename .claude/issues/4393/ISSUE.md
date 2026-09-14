# #4393 — NIFAL-D8-2026-09-14-01: #4250 fabricates `env_map_scale = 1.0` for a wire field Skyrim does not have, flipping every inline Skyrim effect shader into the "authored environment mapping" PBR arm

**Labels**: high,nifal,nif-parser,bug,game:skyrim
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: HIGH (NIFAL row: wrong `Material` out of `translate_material`). The dimension agent proposed MEDIUM because every current GPU consumer short-circuits for `material_kind == 101`. The orchestrator re-floored it to HIGH: `.claude/commands/_audit-severity.md` makes the NIFAL row a minimum, and this canonical-tier error surfaces the moment the deferred `base_color_scale` effect render path lands.
- **Dimension**: Shader-flags/Effects
- **Tier Violated**: no-fabrication
- **Game Affected**: Skyrim LE + SE (every `BSEffectShaderProperty` with BSVER < 130); FO4+ reads the real field
- **Location**: `crates/nif/src/blocks/shader.rs:1936-1956` (placeholder now `1.0`), `crates/nif/src/import/material/dedicated_shader.rs:535` and `:583-584` (copy + latch), `crates/nif/src/import/material/mod.rs:1407-1416` (`classify_legacy_pbr`), `crates/core/src/ecs/components/material.rs:1209` (`env_map_scale > 0.3` arm)
- **Status**: NEW (introduced by `797e82124`, the fix for closed #4250)
- **Description**: nif.xml defines `Env Map Scale` on `BSEffectShaderProperty` for FO4+ only. #4250 changed the parser's not-present placeholder from `0.0` to `1.0`, calling `1.0` "neutral". The engine's convention for *unauthored* environment mapping is `0.0`:
  - `legacy_env_map_scale` returns `0.0` when no env flag is authored.
  - `ImportedMaterial::default()` is `0.0`.
  - `translate_texture_only_material` deliberately overrides `Material::default()`'s `1.0` to `0.0`.
  - The water translator treats `0` as the absence sentinel.

  The fabricated `1.0` latches (#4251) and crosses the classifier's `> 0.3` gate, whose own comment reserves it for surfaces that "DO author real environment mapping". The canonical `Material` gets `env_map_scale 1.0` and `roughness 0.80` instead of `0.0 / 0.85`.
- **Evidence**: The orchestrator confirmed `crates/nif/src/blocks/shader.rs:1955` returns `1.0` for the non-FO4 arm. The agent ran `material_dump` on live Skyrim SE content:
  - `fxambbeamdust00.nif`: `BeamMeshDust05:0` and `BeamMeshStatic04` both import as `kind 101, env 1.00, rghO 0.80`.
  - `fxglowfillroundmid.nif`: `GlowMesh01:0` imports as `kind 101, env 1.00, rghO 0.80`.
  - None of the three paths matches a keyword.
- **Impact**: The same semantic state ("no env mapping authored") is canonical `env 0.0 / rough 0.85` on FO3/FNV and `env 1.0 / rough 0.8` on Skyrim. Nothing changes on screen today: the kind-101 raster branch returns at `crates/renderer/shaders/triangle.frag:1108`, RT hits add emission and break, and shadow rays skip effect cards. The wrong values are visible in `mat.dump` / `material_dump` tooling and in the completeness-harness fill rates. Any future consumer of `Material.env_map_scale` / `roughness` on effect surfaces would get them too.
- **Corroboration (merged from Dim 9, formerly NIFAL-D9-2026-09-14-01)**:
  - The cross-game completeness harness moved Skyrim SE `metO`/`rghO` from **93.8% to 99.0%** this window. 93.8% was also the 2026-08-23 and 2026-09-11 figure. Every other game stayed identical on every column.
  - A per-mesh probe over 198 identical LE/SE paths counted **36 meshes per install (72 total)** with no texture, normal, gloss or authored specular, yet `metalness_override = Some(0.0)` / `roughness_override = Some(0.8)`. Every one is `material_kind = 101` with `env = 1`. Under #2707's `has_no_pbr_classifier_signal` (`crates/nif/src/import/material/mod.rs:1455-1461`), each of these had no signal before #4250. This is the #2707 / #2352 fabrication class re-entered through a different input.
  - The harness asserts only lower bounds (`crates/nif/tests/translation_completeness.rs:220-231`), so the fabricated fill passed silently and reads as improved coverage.
- **Related**: #4250, #4251, #2707, #2352, #2315/#2555 (established 0.0 = unauthored), #4043.
- **Suggested Fix**: Give the harness a drift *band* for `metO`/`rghO` rather than a floor, so an upward jump fails as loudly as a drop. Separately, revert the placeholder to `0.0`. Alternatively, make the parsed field `Option<f32>` and write `MaterialInfo.env_map_scale` only when BSVER ≥ 130. Keep the #4251 latch on the authored path only. Replace the #4250 test with one asserting that a Skyrim effect shader stays on the default-matte arm. If `BsEffectShaderData::default()`'s `1.0` was the source of confusion, document it as the FO4 on-disk default, not an absence value.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
