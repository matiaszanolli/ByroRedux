# Issue #234 — NIF-04-11-M5

**Title**: FO4 morph controller variants (NiMorphWeightsController / NiMorphController / NiMorpherController) missing from dispatch
**Severity**: MEDIUM
**Dimension**: Coverage
**Audit**: `docs/audits/AUDIT_NIF_2026-04-11.md`
**Labels**: medium, nif-parser, bug, import-pipeline

## Location
`crates/nif/src/blocks/mod.rs` — dispatch table

## Summary
Three morph-animation driver types are not dispatched. Grep confirms zero occurrences in `crates/nif/src/`. FO4+ face morph animation silently falls through to NiUnknown (block_size recovery prevents a hard failure but semantic data is lost).

## Fix
1. Run `nif_stats --bsa` on FO4 vanilla Meshes to confirm prevalence.
2. If non-trivial, implement as `NiSingleInterpController` subtypes or dedicated parsers.
3. If near-zero, alias to `NiGeomMorpherController` or a generic stub.

## Game affected
FO4 (primary), FO76/Starfield (likely).

## Completeness checks
- [ ] CORPUS: run nif_stats before implementation
- [ ] TESTS: FO4 character NIF integration test
- [ ] SIBLING: confirm `NiGeomMorpherController` dispatch still works after additions

## Fix with
`/fix-issue 234`
