# NIF-13: Misc tail types — BSRefractionFirePeriodController, NiFogProperty, BSMultiBoundSphere, BSWArray

**Severity**: LOW | **Dimension**: Coverage Gaps | **Game**: FNV (primarily), FO3, Oblivion | **Audit**: docs/audits/AUDIT_NIF_2026-04-22.md § NIF-13

## Summary
Four remaining tail types with low counts but distinct semantics:

- `BSRefractionFirePeriodController` (25 FNV blocks) — animates refraction fire-period on shader effects
- `NiFogProperty` (1 FO3 block) — legacy per-node fog override
- `BSMultiBoundSphere` — sibling of BSMultiBoundAABB/OBB, already dispatched
- `BSWArray` — wide-array extra-data variant

## Evidence
Low-count long-tail buckets in FNV/FO3/Oblivion unknown sweeps.

## Location
`crates/nif/src/blocks/mod.rs` — missing dispatch arms.

## Suggested fix
Four thin parsers (~15 LOC each). BSMultiBoundSphere in particular is a near-copy of the existing BSMultiBoundAABB — direct sibling extension.

## Completeness Checks
- [ ] **SIBLING**: BSMultiBoundSphere goes next to BSMultiBoundAABB/OBB — same file
- [ ] **TESTS**: One round-trip per type

Fix with: /fix-issue <number>
