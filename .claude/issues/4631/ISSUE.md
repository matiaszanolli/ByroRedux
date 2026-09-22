# NIF-D6-2026-09-21-03: KFM read_cstring allocates vec![0u8; len] for any i32 length with no remaining-bytes or cap check

**Issue**: #4631
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIF_2026-09-21.md)

**Severity**: LOW
**Dimension**: 6 (Allocation Hygiene)
**Location**: `crates/nif/src/kfm.rs:766-778`
**Status**: NEW

## Description
KFM `read_cstring` allocates `vec![0u8; len as usize]` for any non-negative `i32` length with no remaining-bytes or `MAX_SINGLE_ALLOC_BYTES` cap check, unlike the header and stream string readers, which both check first.

Confirmed at HEAD `ee6d3fb39`: `read_cstring` (`kfm.rs:765-782`) reads an `i32` length, rejects only `len < 0`, then does `let mut buf = vec![0u8; len as usize]; self.cursor.read_exact(&mut buf)?;` with no bound against the remaining stream bytes or `MAX_SINGLE_ALLOC_BYTES`. A crafted `len` near `i32::MAX` (~2 GB) can be requested from a tiny file.

`kfm.rs` has no engine consumer today — `grep -rln "kfm::" --include="*.rs" .` outside `crates/nif/src/kfm.rs` itself returns nothing, so this is unreachable from any production load path, same reachability class as NIF-D6-2026-09-21-02's Havok packfile finding.

## Evidence
`kfm.rs:775`: `let mut buf = vec![0u8; len as usize];` — no `check_alloc` or remaining-bytes comparison, in contrast to `NifStream::allocate_vec`/`allocate_vec_sized` (`stream.rs:278,329`) which both bound against `remaining` and (for the sized variant) `MAX_SINGLE_ALLOC_BYTES`.

## Impact
No current impact (no production caller). Same DoS-shaped hygiene gap as the other allocation-hygiene findings in this report (NIF-D6-2026-09-21-01, -02) if `.kfm` loading is ever wired up.

## Related
NIF-D6-2026-09-21-01, NIF-D6-2026-09-21-02 (same allocation-hygiene class, filed in this report)

## Suggested Fix
Bound `len` against the stream's remaining bytes (and/or `MAX_SINGLE_ALLOC_BYTES`) before allocating, matching the pattern already used by `NifStream`'s string/array readers.

Source: docs/audits/AUDIT_NIF_2026-09-21.md (NIF-D6-2026-09-21-03)

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix (forged-length rejection), if `.kfm` loading gains a caller
