# OB-D5-01: Oblivion's APPLY_HILIGHT2 arm fabricates detached literal copies of the canonical parallax defaults instead of leaving them unset

**Issue**: #4263 — https://github.com/matiaszanolli/ByroRedux/issues/4263
**Labels**: low,nifal,nif-parser,game:oblivion,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 5 — NIFAL Canonical Material Translation for Oblivion
**Location**: `crates/nif/src/import/material/legacy_properties.rs:303-308`
**Status**: NEW

## Description
Oblivion's `APPLY_HILIGHT2` arm sets `info.parallax_max_passes = Some(4.0)` and `info.parallax_height_scale = Some(0.04)` as detached literal copies of the canonical `DEFAULT_PARALLAX_MAX_PASSES`/`DEFAULT_PARALLAX_HEIGHT_SCALE` constants, converting "unauthored" into "authored" at the raw import tier — precisely the pattern #3073 was introduced to prevent (resolving defaults once, downstream, via `Material::resolve_pbr`, not at every raw-import call site).

## Evidence
`crates/nif/src/import/material/legacy_properties.rs:303-308`: `if info.parallax_max_passes.is_none() { info.parallax_max_passes = Some(4.0); } if info.parallax_height_scale.is_none() { info.parallax_height_scale = Some(0.04); }` — literal duplicates of the canonical defaults, not a reference to the constants.

## Impact
Numerically inert today: the values match the canonical defaults exactly, and the branch measured 0/35,322 imported meshes with `parallax_height_in_alpha` actually set before #3596 made the route reachable. Becomes a live per-game divergence the moment either canonical default is retuned, since this Oblivion-specific literal would silently stop matching the (now-changed) canonical default.

## Related
Adjacent to #3073 (which introduced the canonical constants this duplicates) and #3596 (which made this arm reachable).

## Suggested Fix
Reference `DEFAULT_PARALLAX_MAX_PASSES`/`DEFAULT_PARALLAX_HEIGHT_SCALE` directly here (or, per #3073's intended pattern, leave the fields `None` and let `Material::resolve_pbr` apply the canonical defaults downstream) instead of duplicating the literal values.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_OBLIVION_2026-09-11.md — findings verified against live code during this publish run.*
