# #3478 — NIF-2026-08-27-D6-01: #2550's `hkSubPartData` table uses the 1-byte-per-element allocation bound for a 12-byte element

Source: `docs/audits/AUDIT_NIF_2026-08-27.md`
Filed: 2026-08-27 via `/audit-publish`
Labels: low, nif-parser, nif, bug

---

Audit: `docs/audits/AUDIT_NIF_2026-08-27.md` — Dimension 6 (Allocation Hygiene). Severity **LOW**. Games: FO3 / FNV / Skyrim / FO4+ (any `version >= 20.2.0.7` packed collision mesh). Introduced by `84dbf1bf`, 2026-08-27.

## Location
`crates/nif/src/blocks/collision/shape_mesh.rs:215` (re-verified at publish time: `sub_parts = stream.allocate_vec(num_sub_shapes as u32)?;`).

## Description
```rust
sub_parts = stream.allocate_vec(num_sub_shapes as u32)?;
```

`NifStream::allocate_vec` documents itself as bounding `count` "as if every element cost 1 byte" (`crates/nif/src/stream.rs:270-283`) and explicitly points at `allocate_vec_sized` for "fixed-size element types with no heap indirection — plain scalar/array/tuple-of-scalars structs … whose `size_of::<T>()` is an honest on-disk element size". `HkSubPartData` is exactly that: three `u32`s (`havok_filter`, `num_vertices`, `material` at `shape_mesh.rs:69-73`), 12 bytes on disk and in memory. The sibling arrays in the same parser already go through the sized helpers.

## Evidence
`num_sub_shapes` is a `u16`, so the worst case is a 65,535-entry `Vec::with_capacity` (786 KB) accepted on a stream with only 65,535 bytes remaining — a 12× over-allocation, and it also skips the `MAX_SINGLE_ALLOC_BYTES` cap that `allocate_vec_sized` applies via `check_alloc`.

## Impact
Bounded and small (u16 count), so this is hardening rather than a live risk — but it is the precise pattern #2523 / PERF-D8-NEW-01 introduced `allocate_vec_min_bytes` to eliminate.

## Related
#2550 (the commit that added the `hkSubPartData` table), #831 / #2523 (the helper family).

## Suggested Fix
`stream.allocate_vec_sized::<HkSubPartData>(num_sub_shapes as u32)?`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other four `allocate_vec` sites in `shape_mesh.rs` at :42/:81/:148/:188, and the rest of `blocks/collision/` for fixed-size element types still on the 1-byte bound)
- [ ] **TESTS**: A regression test pins this specific fix
