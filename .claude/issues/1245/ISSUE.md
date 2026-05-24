# NIF-D6-NEW-01: collision/ragdoll.rs uses pre-#408 check_alloc + Vec::with_capacity idiom

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1245

**Source**: `docs/audits/AUDIT_NIF_2026-05-23.md` (Dim 6)
**Severity**: LOW (style consistency)
**Dimension**: Allocation Hygiene

## Description

Four sites in `crates/nif/src/blocks/collision/ragdoll.rs` (lines 59-60, 87-88, 95-96, 132-133) manually pair `stream.check_alloc(n.saturating_mul(K))?` with `Vec::with_capacity(n)`. Functionally identical to `stream.allocate_vec::<T>(n as u32)?` (both gate against `MAX_SINGLE_ALLOC_BYTES` + remaining bytes, both return a sized `Vec`), so the code is **correct and safe** — no leak, no missing gate. But the inconsistency:

1. Misses the `#[must_use]` warning if a future edit deletes the `let mut … =` binding;
2. Hardcodes element-size constants (`40` for BoneTransform, `4` for BlockRef) that drift from the type definition;
3. Makes a future blanket grep for `Vec::with_capacity` in production block parsers harder to interpret.

Not a regression — the file was added in commit 226f43d3 closing #980 — predates the #408 architectural pin's strict re-statement. Co-exists with the live `allocate_vec` pattern used elsewhere in the same module's sibling files (`compressed_mesh.rs`, `rigid_body.rs`).

## Suggested Fix

Replace each pair with `let mut transforms = stream.allocate_vec::<BoneTransform>(n as u32)?;` etc. (and remove the manual `check_alloc` — `allocate_vec`'s bound is implicit per `stream.rs:213-222`).

## Related

- #408 (CLOSED): blanket `allocate_vec` sweep architectural pin
- #831 (CLOSED): `#[must_use]` on `allocate_vec`
- #980 (CLOSED): the parser addition that this file ships under

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: grep for any other `check_alloc + Vec::with_capacity` pairs across `crates/nif/src/blocks/` — should be zero post-fix
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: existing ragdoll parser tests should pass unchanged; no behaviour change