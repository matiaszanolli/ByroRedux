# NIF-12: Havok tail types missing (bhkLiquidAction / bhkAabbPhantom / bhkPCollisionObject / bhkConvexListShape / bhkBreakableConstraint / bhkOrientHingedBodyAction)

**Severity**: LOW | **Dimension**: Coverage Gaps | **Game**: Oblivion, FO3, FNV, Skyrim SE | **Audit**: docs/audits/AUDIT_NIF_2026-04-22.md § NIF-12

## Summary
Six rare Havok types appear in NiUnknown buckets in small counts across all four games. Each individually is low-impact, but bundled together they close Havok coverage to a meaningful level for physics-driven content that the roadmap hasn't shipped yet.

## Evidence
Per-type counts: bhkLiquidAction ~35, bhkAabbPhantom ~42, bhkPCollisionObject ~28, bhkConvexListShape ~22, bhkBreakableConstraint ~18, bhkOrientHingedBodyAction ~15 (numbers approximate — pulled from the four unknown sweeps).

## Location
`crates/nif/src/blocks/mod.rs` + `crates/nif/src/blocks/collision.rs`.

## Suggested fix
Six thin parsers per nif.xml. All are leaf types (no inheritance chain beyond bhkShape / bhkConstraint). ~120 LOC combined + dispatch arms.

## Completeness Checks
- [ ] **TESTS**: One synthetic round-trip per type
- [ ] **SIBLING**: Audit existing `crates/nif/src/blocks/collision.rs` for shared base-class reuse
- [ ] **REAL-DATA**: All four sweeps drop these buckets

Fix with: /fix-issue <number>
