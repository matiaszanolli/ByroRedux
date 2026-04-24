# #586: FO4-DIM2-01: Unchecked file_count / packed_size / unpacked_size allocations in BA2 reader (OOM hazard class)

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/586
**Labels**: bug, import-pipeline, medium, safety, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 2)
**Severity**: MEDIUM
**Location**: `crates/bsa/src/ba2.rs:185, 206, 300, 331, 356, 399, 431, 435, 456, 460`
**Hazard class**: same as closed #388 (NIF `Vec::with_capacity(u32)`), but the BA2 reader was not included in that 60-site sweep.

## Description

The BA2 reader allocates `Vec` / `HashMap` capacities directly from untrusted archive bytes:

- `Vec::with_capacity(file_count)` / `HashMap::with_capacity(file_count)` where `file_count = u32::from_le_bytes(hdr[12..16]) as usize`.
- `Vec::with_capacity(num_chunks)` where `num_chunks = u8 at DX10 base[13]` (max 255 — safe).
- `vec![0u8; packed_size as usize]` / `vec![0u8; unpacked_size as usize]` during extraction.
- `Vec::with_capacity(unpacked_size)` inside `decompress_chunk` zlib arm.

## Evidence

`ba2.rs:123`: `let file_count = u32::from_le_bytes(hdr[12..16]) as usize;` — used unchecked at lines 179/180/185/206. A malicious or corrupted archive with `file_count = 0xFFFF_FFFF` requests 4B-entry Vec/HashMap (abort on 64-bit, allocator panic on 32-bit). `unpacked_size = 0xFFFF_FFFF` on extract → 4 GB allocation.

## Impact

DoS on a corrupted-mid-download modded archive or a malicious BA2 planted in a Data folder. Zero vendor archives trigger it.

## Suggested Fix

Cap `file_count` at ~10M (vanilla max ≈600K in `MeshesExtra.ba2`; 10M is paranoid-safe). Cap `packed_size` / `unpacked_size` at 256 MB per chunk (vanilla max ~8 MB). Use `checked_mul` or an `allocate_vec`-style helper consistent with the #388 NIF sweep.

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: Same cap helper applied to `crates/bsa/src/archive.rs` (BSA v103/v104/v105 reader) — audit of same allocation class
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: Synthesize a BA2 with `file_count = u32::MAX` and assert graceful `InvalidData` error (not panic or abort).

## Related

- Closed #388 (NIF allocate_vec sweep — BA2 was out of scope).
