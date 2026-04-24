# #597: FO4-DIM2-07: BA2 DX10 num_mips = 0 silently clamps to 1 in synthesized DDS

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/597
**Labels**: bug, import-pipeline, low, 

---

**From**: `docs/audits/AUDIT_FO4_2026-04-23.md` (Dim 2)
**Severity**: LOW
**Location**: `crates/bsa/src/ba2.rs:515-517, 544`

## Description

`num_mips` is read as a raw u8 from `base[20]`. BA2 archives with `num_mips = 0` (technically malformed, seen in some mods' packed textures) flow through the DDS synthesizer as:
- `flags` does NOT include `DDSD_MIPMAPCOUNT` (since `num_mips > 1` is false).
- `dwMipMapCount` is written as `num_mips.max(1) = 1`.
- `caps1` does NOT include `DDSCAPS_MIPMAP | DDSCAPS_COMPLEX`.

Produces a DDS with `dwMipMapCount=1` but no flag bit telling the loader to honor it. Spec says: if `DDSD_MIPMAPCOUNT` unset, loader **must** assume 1 mip regardless of field value — so behavior is correct, but the invariant is silent-corrected rather than flagged.

## Evidence

```rust
// ba2.rs:515-517
if num_mips > 1 { flags |= DDSD_MIPMAPCOUNT; }
// ba2.rs:544
hdr.extend_from_slice(&(num_mips.max(1) as u32).to_le_bytes());
```

## Impact

Silent "correction" of `num_mips = 0`. Current vanilla FO4 archives all have `num_mips ≥ 1`, so this never fires. Malformed archives (third-party tools writing 0) lose the signal.

## Suggested Fix

Log a `warn!` when `num_mips == 0` is encountered during record parse, or reject the record. Document the `max(1)` as intentional clamping.

## Completeness Checks

- [ ] **UNSAFE**: n/a
- [ ] **SIBLING**: n/a
- [ ] **DROP**: n/a
- [ ] **LOCK_ORDER**: n/a
- [ ] **FFI**: n/a
- [ ] **TESTS**: Unit test with synthetic BA2 record `num_mips = 0`, assert warn/reject path.
