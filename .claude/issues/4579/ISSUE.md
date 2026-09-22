# REN-D6-2026-09-21-01: `c0b740ce7` silently disabled `translate_material_copies_every_canonical_field` — the NIFAL boundary's core copy-fidelity regression test

**Labels**: medium, renderer, nifal, test-gap, bug

Filed via /audit-publish from docs/audits/AUDIT_RENDERER_2026-09-21.md.

**Severity**: MEDIUM (a test gap, but on the single HIGH-floor NIFAL boundary with all-game blast radius; the production code is currently correct) · **Dimension**: NIFAL Material
**Location**: `byroredux/src/material_translate.rs`, `mod canonical_completeness_harness`: the orphaned doc block + `#[test]` (~:2722-2739), the new `an_msn_named_normal_slot_declares_model_space_normals` (~:2740-2775), and the now un-annotated `fn translate_material_copies_every_canonical_field()` (~:2777); meta-pin `every_source_derived_material_field_is_pinned_by_a_test` (~:3019)
**Status**: NEW (owner: `/audit-nifal` Dim 1; found by the renderer audit. The 2026-09-21 NIFAL report predates `c0b740ce7`.)
**Verified against**: HEAD `f97775ca8`

## Description

`c0b740ce7` ("Fix #4548: an _msn-named normal slot declares model-space normals at the translate boundary") added a new test (doc comment + `#[test]` + fn). It landed between the existing `#[test]` attribute of `translate_material_copies_every_canonical_field` and that fn's signature. At HEAD:

```rust
    /// The core regression: every canonical-tier field the boundary is
    /// documented to copy must carry its source value through unchanged. …
    #[test]                                   // orphaned: now applies to the fn below
    /// #4548 (…) — a material whose BGSM authors `model_space_normals = false` …
    #[test]
    fn an_msn_named_normal_slot_declares_model_space_normals() { … }

    fn translate_material_copies_every_canonical_field() {   // no #[test]
```

So `an_msn_named_normal_slot_declares_model_space_normals` carries two `#[test]` attributes. The core copy-fidelity test is now a plain fn that never runs.

## Evidence

- The source layout above.
- The audit listed both HEAD-built `byroredux` test harnesses (2353 tests). `an_msn_named_normal_slot_declares_model_space_normals` is listed twice (it ran twice: "2 passed"), and `translate_material_copies_every_canonical_field` is not listed at all.
- `every_source_derived_material_field_is_pinned_by_a_test` scans the test module's source text (`include_str!("material_translate.rs")`) for asserting statements that read each field. It stays green while the assertions it counts never execute.

## Impact

The #2214/#3462 contract is void for every field that only this test covers. That contract says that "deliberately reverting any single `source.X` → `material.X` line … fails the corresponding assertion". The affected fields include the `water_shader_flags`/`is_water_shader` NIFAL↔WATAL seam and the emissive, specular, diffuse, ambient, UV, alpha, env-map and vertex-colour copies. A boundary drop in any of those now ships green.

## Related

- #4548: the fix commit `c0b740ce7` belongs to.
- #4411 (open): the same meta-pin also counts comment prose as a pin. That is a second blind spot of the same instrument, so harden both together.
- #2214, #3462: the contract this test implements.

## Suggested Fix

- Move the orphaned doc block and `#[test]` back onto `fn translate_material_copies_every_canonical_field()`, and drop the duplicate attribute from `an_msn_named_normal_slot_declares_model_space_normals`.
- Harden the meta-pin so a de-attributed test cannot satisfy it: either assert that the fn is immediately preceded by `#[test]` in the source text, or have the meta-pin call the fn directly.
- Sweep the rest of the harness for the same orphaned-attribute shape.

Source: docs/audits/AUDIT_RENDERER_2026-09-21.md (REN-D6-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: every `#[test]` in `canonical_completeness_harness` (and in the other `include_str!` source-shape meta-pin modules) sits directly on its fn; no other doubled or orphaned attributes
- [ ] **CANONICAL-BOUNDARY**: the fix touches only the test module of `byroredux/src/material_translate.rs`; `translate_material` itself is unchanged
- [ ] **TESTS**: the meta-pin fails when `translate_material_copies_every_canonical_field` loses its `#[test]` (verified by temporarily removing it)
