# #2702: FO4-D2-03: The BGSM Phase-1 flag-forwarding tests are mirror tests that re-implement the merge locally and cannot fail when production diverges

- **Severity**: LOW
- **Dimension**: 2 — test integrity
- **Location**: `byroredux/src/asset_provider/tests.rs:784-816`, `:820-856`, `:862-894`
- **Status**: NEW
- **Description**: All three (`bgsm_merge_forwards_phase1_shader_flags`, `bgsm_merge_does_not_set_phase1_flags_from_false`, `bgsm_merge_phase1_flags_honor_child_first_chain`) declare local `let mut is_pbr = false;` bools and hand-copy the production gate rather than invoking `merge_external_material`. They assert their own copy of the logic and are guaranteed passes regardless of what the merge does — which is exactly how the `is_pbr` contract flip in FO4-D2-01 landed with a green suite while a test comment kept stating the opposite behaviour.
- **Impact**: Zero regression coverage on the three BGSM shader-flag forwards the tests claim to pin, plus an actively misleading doc comment.
- **Related**: FO4-D2-01, #2595 (same class in the neighbouring `fill_from_bgsm` helper).
- **Suggested Fix**: Rewrite against the real function using the existing test-provider fixture path (`insert_bgem_for_test` has the shape to mirror for BGSM).

---
**Source**: `docs/audits/AUDIT_FO4_2026-08-12.md` (finding `FO4-D2-03`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

