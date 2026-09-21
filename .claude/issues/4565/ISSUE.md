# NIFAL-D7-2026-09-21-04: Harness doc cites a nonexistent test module unsanitized_clip_scalar_tests

**Labels**: low, nifal, animation, documentation, doc-rot

**Severity**: LOW · **Dimension**: Animation / controllers (guard doc rot) · **Tier Violated**: — · **Game Affected**: all
**Location**: `byroredux/src/anim_convert.rs:1124`
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
The `canonical_animation_completeness_harness` module doc says the scalar sanitizers "have their own tests in `unsanitized_clip_scalar_tests` above". No such module exists — the tests live in `clip_frequency_tests`, `clip_phase_tests`, `clip_duration_weight_tests` (`anim_convert.rs:942-1102`).

### Evidence
`grep -rn unsanitized_clip_scalar_tests byroredux/src` → single hit at `:1124`.

### Impact
A reader following the pointer lands nowhere and may conclude the sanitizers are untested.

### Related
#4405 (the harness rework that added the sentence)

### Suggested Fix
Repoint to the three real module names.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix
