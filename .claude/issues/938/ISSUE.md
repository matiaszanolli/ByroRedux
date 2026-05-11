# NIF-D2-NEW-04: Feature-flag predicates duplicate parse-time bsver checks (foot-gun)

**Severity**: MEDIUM (current usage is safe; the predicates are an architectural foot-gun)
**Source audit**: `docs/audits/AUDIT_NIF_2026-05-10.md` (Dim 2)

## Location

`crates/nif/src/version.rs:140-205` — `compact_material()`, `has_emissive_mult()`, `has_shader_emissive_color()`

## Why it's a bug

These three public predicates are documented as "callers should use," then immediately followed by "but prefer `stream.bsver() >= N` directly." `grep -rn` confirms zero production call sites — every parse site queries `stream.bsver()` directly.

They coexist with predicates that ARE used externally (`has_shader_alpha_refs`, `has_material_crc`) under the same naming scheme — no way for a future contributor to know which are blessed.

The predicates' results disagree with the parse path by one bsver step at boundary versions (combined with NIF-D2-NEW-01's `Fallout3.bsver() == 21` quirk).

## Fix

Either:
- (a) Delete the three unused predicates.
- (b) Split into `AuthoritativeFeatures` (variant-only) and `BsverFeatures` (called on `stream`, queries header bsver directly).

The current "pick whichever flavor you like" surface invites the same regression class as #323.

## Completeness Checks

- [ ] **SIBLING**: Confirm via `grep -rn` that `compact_material`, `has_emissive_mult`, `has_shader_emissive_color` are unused outside `version.rs` tests before deletion
- [ ] **TESTS**: Remove or update `version.rs` tests that pin the deleted predicates
