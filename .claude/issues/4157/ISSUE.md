# PERF-D6-2026-09-11-01: NiPixelData mipmap array bypasses the amplification guard allocate_vec_sized exists to close

URL: https://github.com/matiaszanolli/ByroRedux/issues/4157
Labels: bug, nif-parser, medium, nif, game:oblivion

---

**Severity**: MEDIUM
**Dimension**: 6 — Allocation Hygiene
**Game Affected**: Oblivion (the only current source of embedded `NiPixelData` textures); mechanism is version-agnostic
**Location**: `crates/nif/src/blocks/texture.rs:210,288`
**Status**: NEW

**Description**: Both `NiPixelData`/`PixelFormatPrelude` mipmap-descriptor loops pre-size with the loose `stream.allocate_vec::<MipMapInfo>(num_mipmaps)` — a 1-byte-per-element bound with no `check_alloc`/`MAX_SINGLE_ALLOC_BYTES` routing at all — for a 12-byte struct (`width`/`height`/`offset`, all `u32`) that `allocate_vec_sized`'s own doc comment names as exactly the case it exists to bound correctly.

**Evidence** (`texture.rs:209-215` and the parallel `:287-292` site):
```rust
let mut mipmaps: Vec<MipMapInfo> = stream.allocate_vec(num_mipmaps)?;
for _ in 0..num_mipmaps {
    let width = stream.read_u32_le()?;
    let height = stream.read_u32_le()?;
    let offset = stream.read_u32_le()?;
    ...
```

**Impact**: A crafted `NiPixelData` block with an inflated `num_mipmaps` can trigger a `Vec::with_capacity` request up to 12× the remaining file bytes, unconstrained by the crate's 256 MB hard cap — an OOM/large-allocation DoS vector on untrusted input, not covered by any of the three `heap_allocation_bounds*.rs` dhat gates.

**Suggested Fix**: `stream.allocate_vec_sized::<MipMapInfo>(num_mipmaps)?` at both call sites — the type has no heap indirection or version-dependent size variability, so the exact `size_of`-based bound applies cleanly.

## Completeness Checks
- [ ] **TESTS**: A dhat allocation-bounds test pins both call sites at the `size_of::<MipMapInfo>()` bound

