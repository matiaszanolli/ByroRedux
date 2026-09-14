# #4391 — NIFAL-D1-2026-09-14-01: #4237's `window_env_mapping` glass signal promotes ~640 non-glass FNV/FO3 surfaces to refractive GLASS

**Labels**: high,nifal,renderer,bug,game:fnv,game:fo3
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: HIGH (NIFAL row: wrong `Material` out of `translate_material`)
- **Dimension**: Material
- **Tier Violated**: no-fabrication (a new positive glass signal landed with synthetic tests only, no census)
- **Game Affected**: FNV, FO3
- **Location**: `byroredux/src/helpers.rs:164` (gate), `byroredux/src/material_translate.rs:736` (argument), `crates/nif/src/import/material/legacy_properties.rs:41` (`legacy_window_env_mapping`)
- **Status**: NEW (introduced by `c9b02ba4a`, the fix for closed #4237)
- **Description**: Before `c9b02ba4a`, an alpha-covered dielectric became `MATERIAL_KIND_GLASS` only through a glass keyword in its texture path or mesh name, or a BGEM glass flag. The fix added `window_env_mapping` as a third, independent trigger "with the same standing as `bgem_glass`". That trigger fires on either the FO3/FNV `Window_Environment_Mapping` bit or the `Eye_Environment_Mapping` bit. The coverage gate accepts `has_alpha || alpha_test`, so any alpha-tested surface carrying either bit is promoted. Vanilla content authors these bits on env-mapped atlases and clutter, not just glass panes.
- **Evidence**: The orchestrator re-read `byroredux/src/helpers.rs:157-168`: `if !keyword_match && !bgem_glass && !window_env_mapping { return; }` is the only positive gate. The agent ran a census that mirrors the classifier's gates:
  - **FNV**: 776 meshes author a bit. 66 were already keyword glass. **458 meshes in 338 NIFs are promoted by the new signal alone** (448 via `alpha_test`). 401 of those use `textures\dlc05\dungeons\mz\mz03.dds`, the Old World Blues Big MT room and corridor shells. The rest include trash piles, `rowhousetrim01.dds` railings, office-building exteriors, gun cabinets, cameras, blood packs and cazador wings.
  - **FO3**: **182 meshes in 61 NIFs are newly promoted**, including 133 `rowhousetrim01.dds` (Georgetown trim) and 40 `storefrontquad01.dds`.
  - #4237's impact note assumed "most vanilla window meshes happen to match a glass keyword". The census shows the reverse: the new signal promotes about 7× more meshes than keywords already did.
- **Impact**: Hundreds of Big MT interior walls, Georgetown facade trim and clutter meshes now shade as smooth refractive dielectric glass through the physical-transmission path (`crates/renderer/shaders/include/ray_hit.glsl:436-439`). No per-draw fallback masks it.
- **Related**: #4237 (closed), #1280, #2315, NIFAL-D1-2026-09-14-02. Test gap: `every_source_derived_material_field_is_pinned_by_a_test` scans only the `Material` literal. None of the arguments `translate_material` passes to `classify_glass_into_material` has a translate-level test.
- **Suggested Fix**: Revert the third trigger, or narrow it to a measured population before landing it again. Options: Window bit only (not Eye); require blend coverage rather than `alpha_test`; or require a co-occurring keyword/name signal. Cite the census in the code comment. Add a translate-level test that drives each glass-classifier argument through `translate_material`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
