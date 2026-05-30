**Severity:** LOW · **Dimension:** Version Handling (maintainability) · **Game Affected:** None (cosmetic)

From audit `docs/audits/AUDIT_NIF_2026-05-29.md` (finding NIF-2026-05-29-08).

## Description
Several sites compare `stream.bsver()` against bare decimals (`34`/`100`/`155`) that have named equivalents in `version::bsver` (`FO3_FNV`/`SKYRIM_SE`/`FO76`). The same files already use the named constants elsewhere — internally inconsistent. Residual after the #1042 / #1319 bare-literal naming sweeps.

## Location
- `crates/nif/src/blocks/particle.rs:350, 909, 912, 962, 1094, 1103`
- `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:264`

## Evidence
Confirmed present, e.g.:
- `particle.rs:350` `stream.bsver() >= 34` → `bsver::FO3_FNV`
- `particle.rs:909` `stream.bsver() >= 100` → `bsver::SKYRIM_SE`
- `particle.rs:912` `stream.bsver() == 155` → `bsver::FO76`
- `particle.rs:962` `> 34`, `:1094` `<= 34`, `:1103` `> 34` → `bsver::FO3_FNV`
- `bs_tri_shape.rs:264` `stream.bsver() == 155` → `bsver::FO76`

All seven are behaviorally correct against nif.xml; only the literal form differs.

## Impact
None at runtime. Maintainability: a future BSVER-band edit must hunt bare decimals as well as named constants; the named constants carry the nif.xml citation in their docstring.

## Suggested Fix
Replace each bare decimal with the corresponding `crate::version::bsver::*` constant. Pure mechanical substitution; no behavior change. Batch with the next tech-debt pass.

## Related
#1042, #1319.

## Completeness Checks
- [ ] **SIBLING**: After substitution, grep the whole `crates/nif/src/blocks/` tree for any remaining bare `bsver() <op> <decimal>` comparisons with a named equivalent
- [ ] **TESTS**: No behavior change — existing version-gate tests should stay green
