# MAT-D6-02: "scrap" classifier keyword is an unbounded substring match (potential over-match on metallic scrap clutter)

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1925

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/core/src/ecs/components/material.rs:472` — `if contains_any_ci(path, &["scrap"]) { return PbrMaterial { roughness: 0.85, metalness: 0.0 }; }`
**Status**: NEW

## Description
The arm matches "scrap" anywhere in the path and forces a fully dielectric matte result before the metal arm can run. Its intended target is FNV/FO3 `metalscrap*` painted-tin cladding. However, genuine scrap-metal clutter (e.g. FNV "Scrap Metal" misc-item textures, FO4 scrap piles) also contains the token and would be forced to `metalness 0.0`, potentially reading as flat non-metal where a dull-but-conductive surface is wanted.

## Evidence
The arm is unconditional on any "scrap" substring; the metal arm sits below it and is unreachable for such paths.

## Impact
At worst a dull-vs-slightly-metallic look difference on scrap clutter — bounded, non-divergent (same result on both load paths), inside a classifier that is explicitly heuristic (legacy NIFs carry no PBR ground truth). Low-confidence finding — no content sampling done.

## Related
MAT-D6-01

## Suggested Fix
If a real regression surfaces, narrow to "metalscrap" (the actual cladding token) so genuine scrap-metal clutter can still reach the metal arm.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr`, or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
