# SK-D1-NEW-04: Duplicate half_to_f32 IEEE-754 binary16 decoder

**Severity**: LOW
**Source audit**: `docs/audits/AUDIT_SKYRIM_2026-05-11.md` Dim 1
**State**: OPEN

## Location

- `crates/nif/src/blocks/tri_shape.rs:1261` — `pub(crate) fn half_to_f32` (canonical)
- `crates/nif/src/import/mesh.rs:1565` — private re-declaration

## Fix

Promote `tri_shape::half_to_f32` to `pub` (or move to `crates/nif/src/util.rs`), delete the `import/mesh.rs` copy, `use` from the canonical path.

## Notes

The `import/mesh.rs` copy has a self-aware comment ("Re-declared so `import/mesh.rs` doesn't depend on a `pub(crate)` export in tri_shape that might churn") — the churn risk is unfounded since IEEE 754 binary16 is fully specified.
