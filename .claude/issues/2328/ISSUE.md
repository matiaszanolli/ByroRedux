# FO3-D1-06: Inherited-property precedence inversion — texture_clamp_mode/env_map_scale writes are unconditional

Filed from: `docs/audits/AUDIT_FO3_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2328

**Severity**: LOW
**Location**: `crates/nif/src/import/material/legacy_properties.rs:22,313,325,385,386,444-445,451-452,458,469,490`
**Status**: NEW

### Description
`apply_legacy_property_chain`'s documented intent is "shape properties first so they take priority" (#208), implemented via `is_none()`/`has_…` guards everywhere except `texture_clamp_mode` and `env_map_scale`, which use a bare `=` in all seven shader branches — an inherited parent-NiNode property silently overwrites the shape's own authored value, the opposite of the stated rule.

Confirmed against current code: `legacy_properties.rs:16-19` doc says "Shape properties first so they take priority (#208)", iterating `direct_properties.iter().chain(inherited_props.iter())`. Meanwhile `texture_clamp_mode`/`env_map_scale` assignments at lines 313, 325, 385, 386, 444-445, 451-452, 458, 469, 490 are unconditional `=` writes with no `_consumed` guard, unlike the `alpha_property_consumed` guard pattern used elsewhere (e.g. line 65 `if !info.alpha_property_consumed`).

### Impact
Only fires when a parent NiNode carries a `BSShader*Property` while the child shape also carries its own — uncommon in vanilla FO3, hence LOW. Compounds FO3-D1-01 when it does fire.

### Suggested Fix
Add `texture_clamp_mode_consumed`/`env_map_scale_consumed` bools mirroring `alpha_property_consumed`, gate both writes on them.

### Related
#208, #773, #1201/NIF-D4-NEW-05

## Completeness Checks
- [ ] **SIBLING**: Shared code path with FNV/Oblivion — same precedence bug applies there
- [ ] **TESTS**: A regression test pins "shape's own texture_clamp_mode/env_map_scale is not overwritten by an inherited parent NiNode property"
