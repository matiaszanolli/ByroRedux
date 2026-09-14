# #4392 — NIFAL-D1-2026-09-14-02: #4255's authored-shader-type guard splits identical Skyrim glass by `shader_type` — alchemy-lab glass loses GLASS, Tolfdir's alembic keeps it

**Labels**: high,nifal,renderer,bug,game:skyrim
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: HIGH (NIFAL row: divergent `Material` for the same authored surface)
- **Dimension**: Material
- **Tier Violated**: no-fabrication ("authored `shader_type` 1..=20 ⇒ not glass" is an unmeasured proxy rule)
- **Game Affected**: Skyrim LE/SE (inline `BSLightingShaderProperty`, `from_bgsm == false`)
- **Location**: `byroredux/src/helpers.rs:118` (`lit_carrier_authored_dispatch`), `byroredux/src/helpers.rs:123-129` (early return)
- **Status**: NEW (introduced by `4a8875b39`, the fix for closed #4255; structural root is Existing: #4256)
- **Description**: `classify_glass_into_material` now returns before any glass gate whenever `material_kind` (the verbatim Skyrim `shader_type`) is in `1..=20` and no external BGSM resolved. That protects MultiLayerParallax (11) ice as intended. It also blocks EnvironmentMap (1), which is how Skyrim authors ordinary glass apparatus. The canonical classification of one physical surface therefore depends on which shader variant the artist picked.
- **Evidence**: The orchestrator re-read `byroredux/src/helpers.rs:104-129` and confirmed the unconditional `1..=20` range. The agent's census of Skyrim SE `Meshes0/1.bsa` (22,047 NIFs) counted glass-eligible meshes by authored kind:
  - Totals: kind 0: 29, **kind 1: 80**, kind 2: 1, kind 11: 7.
  - Kind 1 covers `InnerGlass02` / `OuterGlass02` / `Liquid02:*` on `textures\clutter\plainglasstile01.dds` in `alchemyworkstation.nif`, `alchemyworkbench.nif`, `workbenches\alchemyworkbench01.nif`, `workbenches\alchemyworkstation01.nif`, the load-screen and Hearthfire variants, and `winterholdbookcase01.nif` (26 glass rows).
  - The same sub-mesh names on the same texture in `alchemytolfdirsalembic01.nif` are authored as kind 0 and still classify as GLASS.
  - The guard does remove real false positives on kind 1 (ice wraith, `dragon_snow`, ice floes). Identical false positives remain on kind 0 (`dragon_icelake`, `icevine01*`, Solitude `swindow02` shutters). The guard is therefore neither a glass signal nor a false-positive filter; it is a shader-type partition.
- **Impact**: Every player-usable alchemy lab renders its glass apparatus as an alpha-blended env-mapped shell (kind 1 gets only an env-cubemap response, `crates/renderer/shaders/triangle.frag:2778`). Tolfdir's alembic, the same asset family, renders as refractive glass.
- **Related**: #4255 (closed), #4256 (OPEN), #2710 / `322f33a8` (the `InnerHaze` effect-layer precedent — kind 101, a different population), NIFAL-D1-2026-09-14-01.
- **Suggested Fix**: Scope the guard to kinds whose dispatch really conflicts with glass. MultiLayerParallax (11) is measured; census the others first. Do not guard EnvironmentMap (1). Handle keyword false positives (`ice` on dragons and wraiths) in the keyword list or name gate, not by shader type. Pin the fix with a test built from the two alchemy meshes.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
