# SAFE-D2-2026-09-21-02: `read_pod_vec_from`'s SAFETY argument rests on a false `io::Read` contract

**Labels**: medium, safety, nif-parser, nif, bug

Filed via /audit-publish from docs/audits/AUDIT_SAFETY_2026-09-21.md.

**Severity**: MEDIUM (a stated safety invariant that is false; the code is sound today only because of how it is instantiated) · **Dimension**: 2 — Memory corruption / UB
**Location**: `crates/nif/src/stream.rs`: `read_pod_vec_from` (~:805-838). This is the single POD `unsafe` site behind `NifStream::read_pod_vec` and `header::read_pod_vec_from_cursor`.
**Status**: NEW. The code has been like this since #3062; earlier audit runs passed it on the stated contract.
**Verified against**: HEAD `f97775ca8`

## Description

- `read_pod_vec_from` builds a `&mut [u8]` over the uninitialised capacity of `Vec::with_capacity(count)` and passes it to `reader.read_exact`.
- Its SAFETY comment justifies this with: "`read_exact` only WRITES through the slice — `Read::read_exact` is a pure writer interface per its contract — so no uninitialised byte is ever read."
- std documents the opposite. `Read` is a safe trait. Implementations "can make no assumptions about the contents of `buf`", and "*callers* of this method in unsafe code must not assume any guarantees about how the implementation uses `buf`. The trait is safe to implement, so it is possible that the code that's supposed to write to the buffer might also read from it. … Calling `read` with an uninitialized `buf` … is not safe, and can lead to undefined behavior." `read_exact`'s docs defer to that text.
- The function is generic over the reader (`reader: &mut impl io::Read`), so nothing limits it to a reader whose behaviour would make the argument true.

## Evidence

```rust
pub(crate) fn read_pod_vec_from<T: AnyBitPattern>(reader: &mut impl io::Read, count: usize, byte_count: usize) -> io::Result<Vec<T>> {
    let mut out: Vec<T> = Vec::with_capacity(count);
    // SAFETY: … `read_exact` only WRITES through the slice — `Read::read_exact` is a pure writer interface per its contract …
    let byte_slice: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(out.as_mut_ptr().cast::<u8>(), byte_count) };
    reader.read_exact(byte_slice)?;
```

- Both callers pass a `Cursor<&[u8]>`, whose `read_exact` is a `memcpy`. That instantiation is the only reason the code is sound today.
  - `NifStream::read_pod_vec` passes `&mut self.cursor`, a `Cursor<&'a [u8]>`.
  - `header::read_pod_vec_from_cursor` passes `cursor: &mut Cursor<&[u8]>`.
- Publish-time addition: the same block also misses `std::slice::from_raw_parts_mut`'s own precondition, "`data` must point to `len` consecutive properly initialized values of type `T`". Uninitialised `Vec` capacity does not meet it, whatever the reader does. Both std quotes were checked against the installed toolchain's `library/std/src/io/mod.rs` and `library/core/src/slice/raw.rs`.

## Impact

- There is no observed UB today: every instantiation reads from an in-memory cursor.
- The generic signature and the false comment invite a future caller to pass a decompressing, buffered or custom reader, and get undefined behaviour without touching the `unsafe` block.
- Every NIF geometry parse goes through this POD bulk-read path.

## Related

- #3062 (closed) removed the zero pre-fill that had made this sound by construction.
- #1439 (closed) added the `AnyBitPattern` bound.
- #4165 (closed) added `#[must_use]` to the cursor twin.
- `docs/audits/AUDIT_NIF_2026-09-21.md` re-confirms this finding without re-filing it.

## Suggested Fix

- Preferred: copy the cursor's remaining bytes (`cursor.get_ref()[pos..pos + byte_count]`, bounds-checked) into `out.spare_capacity_mut()` with `copy_nonoverlapping`, then advance the cursor and `set_len`. This never forms a `&mut [u8]` over uninitialised memory, so it satisfies both contracts, and it keeps #3062's no-pre-fill win.
- Minimum: narrow the parameter to `&mut Cursor<&[u8]>`, the only instantiation, and restate the SAFETY argument against that concrete impl. This alone does not address the `from_raw_parts_mut` precondition.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D2-2026-09-21-02)

## Completeness Checks
- [ ] **UNSAFE**: the rewritten block's `// SAFETY:` names only invariants that hold (concrete reader, bounds check, bytes initialised before `set_len`)
- [ ] **SIBLING**: no other `from_raw_parts_mut` over uninitialised `Vec` capacity in `crates/nif` or the other untrusted-input readers
- [ ] **TESTS**: the existing `read_pod_vec` round-trip and oversized-count tests still pass, and the short-input (`UnexpectedEof`) path of the new copy is covered
