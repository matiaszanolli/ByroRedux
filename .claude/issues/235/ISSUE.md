# Issue #235 — NIF-04-11-M6

**Title**: FO3+ specialized controllers missing from dispatch
**Severity**: MEDIUM
**Dimension**: Coverage
**Audit**: `docs/audits/AUDIT_NIF_2026-04-11.md`
**Labels**: medium, nif-parser, bug, animation

## Location
`crates/nif/src/blocks/mod.rs` — dispatch table

## Missing types
- BSLagBoneController (hair/cloth physics lag)
- BSKeyframeController
- BSFrustumFOVController
- BSMaterialEmittanceMultController
- BSRefractionStrengthController
- BSProceduralLightningController

## Game affected
FO3, FNV, FO4, FO76, Starfield. Falls through to NiUnknown on FO3+ (block_size recovery).

## Fix
Most are `NiSingleInterpController` subtypes — one wrapper parser covers all. `BSLagBoneController` likely needs its own because of physics coefficients.

## Completeness checks
- [ ] CORPUS: nif_stats on FNV + FO4 to confirm prevalence
- [ ] SIBLING: reuse `BSEffectShaderPropertyFloatController` dispatch pattern
- [ ] TESTS: hair NIF for LagBone, camera NIF for FrustumFOV

## Fix with
`/fix-issue 235`
