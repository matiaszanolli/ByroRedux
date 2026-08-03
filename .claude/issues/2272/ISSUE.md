# SAFE-2026-08-03-02: NIF header POD-read overflow guard is a caller contract, not a construction guarantee

Severity: low
Source audit: docs/audits/AUDIT_SAFETY_2026-08-03.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2272

**Dimension**: 2 (Memory Corruption / UB)
**Source**: `docs/audits/AUDIT_SAFETY_2026-08-03.md` (SAFE-2026-08-03-02)
**Status**: NEW
**Location**: `crates/nif/src/header.rs:360-385` (`read_pod_vec_from_cursor`)

## Description
Unlike `NifStream::read_pod_vec` (`crates/nif/src/stream.rs:340-372`, which calls
`self.check_alloc(byte_count)` internally before allocating), the header-phase
mirror `read_pod_vec_from_cursor` deliberately omits an internal allocation-size
cap — documented in its own doc comment ("Caller is responsible for the
byte-budget bounds check... every header call site already does this against
`total_bytes - cursor.position()`, see #388") — and relies on each caller checking
remaining bytes first. Both current call sites (`header.rs:213` and `:237`) do
this correctly. But a third caller added later without the preceding budget check
would allocate up to ~4 GB (`u32::MAX` blocks × 4 B) via `vec![T::default(); count]`
before `read_exact` fails on a malformed/adversarial NIF header.

## Evidence
```
crates/nif/src/header.rs:360-385   read_pod_vec_from_cursor — no check_alloc call, caller-contract only
crates/nif/src/header.rs:213       let indices = read_pod_vec_from_cursor::<u16>(&mut cursor, num_blocks as usize)?;
crates/nif/src/header.rs:237       read_pod_vec_from_cursor::<u32>(&mut cursor, num_blocks as usize)?
crates/nif/src/header.rs:394       check_header_alloc — the guard that exists but isn't called from read_pod_vec_from_cursor
crates/nif/src/stream.rs:340-350   read_pod_vec's equivalent: self.check_alloc(byte_count)? called internally before allocating
```

## Impact
No live defect — both callers are correct today and are covered by
`check_header_alloc_rejects_oversized_len` (header.rs tests). Risk is purely in a
future caller of this private (crate-internal) helper that skips the preceding
bounds check, silently reintroducing the class of unbounded-allocation OOM that
#388 fixed in `stream.rs`.

## Suggested Fix
Pass `total_bytes` (or the cursor's remaining-length) into
`read_pod_vec_from_cursor` and call the existing `check_header_alloc` internally,
matching `stream.rs::read_pod_vec`'s pattern, so the guard can't be forgotten by a
new call site.

## Related
None — no open issue overlaps this finding (checked against 47 open issues,
`/tmp/audit/issues.json`). Distinct from the closed `stream.rs`-side #388 fix this
finding cites as precedent.

## Completeness Checks
- [ ] **UNSAFE**: The `unsafe { std::slice::from_raw_parts_mut(...) }` block in
      `read_pod_vec_from_cursor` keeps its SAFETY comment accurate once the
      allocation is gated internally rather than by caller contract
- [ ] **SIBLING**: Confirm no other private POD-read helper in `crates/nif` (or
      elsewhere) follows the same caller-contract-only pattern without a
      compensating internal guard
- [ ] **TESTS**: A regression test exercises a call site *without* a preceding
      bounds check and confirms the internal guard now rejects an oversized `count`
