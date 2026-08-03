# SF-D9-2026-08-03-01: BGEM legacy glass-bundle detection reads a shadowed field that is structurally always false for version >= 10

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2358
**Labels**: bug,import-pipeline,medium,legacy-compat

---

**Severity**: MEDIUM
**Dimension**: 9 — BGSM/BGEM External Material Flow (Starfield audit, 2026-08-03)
**Location**: `byroredux/src/asset_provider/material.rs:126-142` (`bgem_uses_glass_behavior`); shadowed field pair at `crates/bgsm/src/base.rs:209-213` vs `crates/bgsm/src/bgem.rs:131-134`
**Status**: NEW, CONFIRMED against current code

## Description

`BaseMaterial::parse_after_magic` (`base.rs:209-213`) only populates `environment_mapping`/`environment_mapping_mask_scale` for `version < 10`; for `version >= 10` it hardcodes `(false, 1.0, depth_bias)`. `BgemFile` separately re-reads the same two values into its own `BgemFile::environment_mapping` field (`bgem.rs:131-134`, populated for `version >= 10`) — a shadowing pair. `bgem_uses_glass_behavior` (`material.rs:126`) reads the *base* copy, which is structurally `false` for `version >= 10`. Combined with the classifier's own `version < 21` gate (`material.rs:138`), the legacy glass-bundle arm can only ever fire for BGEM `version < 10`, leaving versions 10-20 (Skyrim SE = v20) a dead window where neither the legacy bundle nor `glass_enabled` (`version >= 21` only) can trigger.

## Evidence

- Confirmed via direct read: `base.rs:209`: `let (environment_mapping, environment_mapping_mask_scale, depth_bias) = if version < 10 { (r.read_bool()?, r.read_f32()?, false) } else { (false, 1.0, r.read_bool()?) };`
- `bgem.rs:131-134`: `if version >= 10 { out.environment_mapping = r.read_bool()?; out.environment_mapping_mask_scale = r.read_f32()?; }` — a separate `BgemFile::environment_mapping` field.
- `grep -rn "\.environment_mapping\b"` across the workspace: `BgemFile::environment_mapping` (the field the parser actually populates for `version >= 10`) has **zero consumers** anywhere outside its own parser — confirmed. The only test coverage hand-builds a struct literal bypassing the parser, so it cannot observe the shadowing.

## Impact

A BGEM `v10-v20` authoring a transparent environment-mapped shell falls through to opaque-plastic classification unless a `glass` keyword happens to match. Not Starfield-facing (Starfield uses `.mat`/CDB, not BGEM), but cross-cutting for Skyrim SE/FO76-era BGEM content.

## Suggested Fix

Add a version-aware accessor (`BgemFile::env_mapping_enabled()`) that reads the correct field per version, plus a parser-driven (not struct-literal) regression fixture at v20.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other BGSM/BGEM shadowed-field pairs exist from the same base/subtype split (this audit's Dimension 9 checked the others and found them correctly traced)
- [ ] **CANONICAL-BOUNDARY**: Fix stays in `crates/bgsm/src/bgem.rs` + the classifier in `byroredux/src/asset_provider/material.rs`, upstream of `translate_material`. See `/audit-nifal`.
- [ ] **TESTS**: Parser-driven (not struct-literal) regression fixture at v20 pins the fix
