# SK-D2-LOW: BSA reader hardening bundle — debug allocs, decompressed-size assertion, dead bindings

## Finding: SK-D2-LOW (bundle of SK-D2-02 / 04 / 05 / 07)

- **Severity**: LOW (all items)
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-04-24.md`

All findings live in [crates/bsa/src/archive.rs](crates/bsa/src/archive.rs).

## SK-D2-02: genhash_* allocates Vec<u8> for a no-op lowercase pass per name

**Location**: archive.rs:24-29, 62-66

```rust
let lower: Vec<u8> = name.as_bytes().iter().map(|b| b.to_ascii_lowercase()).collect();
```

Inputs are already lowercased at lines 290 / 347 just before the call, so the inner copy is wasted. ~22k pointless heap allocations per Meshes0 open in debug builds (~6× slowdown vs release).

**Fix**: take `&[u8]` instead of `&str`, skip the inner copy. Or gate behind a feature flag instead of `cfg(debug_assertions)`.

## SK-D2-04: post-LZ4 decompressed length never asserted equal to prefix-declared size

**Location**: archive.rs:464-509

After LZ4 frame decompression, `buf.len()` is never compared against the prefix-declared `original_size`. Truncated frames fail late with a misleading downstream parser error (e.g. "NIF magic not found").

**Fix**: `if buf.len() != original_size as usize { return Err(...); }`.

## SK-D2-05: total_folder_name_length / total_file_name_length read into _-prefixed bindings

**Location**: archive.rs:180-181

Read but never validated against actual bytes consumed — would catch malformed archives early.

**Fix**: track running consumed lengths; assert at end of folder/file table parse.

## SK-D2-07: FolderRecord.hash + offset dead in release builds

**Location**: archive.rs:213-244

`FolderRecord.hash` and `FolderRecord.offset` are dead in release builds, generating persistent compiler warnings. Sibling `RawFileRecord.hash` already gates correctly with `#[cfg(debug_assertions)]`.

**Fix**: gate `FolderRecord.{hash, offset}` with `#[cfg(debug_assertions)]` to match the sibling pattern.

## Suggested PR Scope

All four are single-file changes in `archive.rs`. Bundle into one PR.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit BA2 reader (`crates/bsa/src/ba2.rs`) for the same hardening gaps.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: SK-D2-04 — synthetic archive with truncated LZ4 frame → assert clean error. SK-D2-05 — synthetic archive with mismatched name length → assert clean error.

_Filed from audit `docs/audits/AUDIT_SKYRIM_2026-04-24.md`._
