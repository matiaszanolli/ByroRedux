# NIF-07: BSNiAlphaPropertyTestRefController missing from dispatch (751 SE blocks)

**Severity**: HIGH | **Dimension**: Coverage Gaps | **Game**: Skyrim SE | **Audit**: docs/audits/AUDIT_NIF_2026-04-22.md § NIF-07

## Summary
Animates the alpha-test threshold on `NiAlphaProperty` (dissolve effects, fade transitions, ghost-reveal VFX). 751 blocks on Skyrim SE silently hold a fixed threshold.

## Evidence
`/tmp/audit/nif/skyrimse_unk.out`: 751.

## Location
`crates/nif/src/blocks/mod.rs`.

## Suggested fix
Subclass of `NiSingleInterpController` (nif.xml line 6242). ~15 LOC dispatch arm + test.

## Completeness Checks
- [ ] **TESTS**: Synthetic block round-trip
- [ ] **REAL-DATA**: SE unknown sweep drops this bucket to 0

Fix with: /fix-issue <number>
