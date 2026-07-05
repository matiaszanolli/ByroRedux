**Severity**: LOW · **Dimension**: Allocation Hygiene · **Source**: `docs/audits/AUDIT_NIF_2026-07-05.md` (NIF-D6-001)
**Game Affected**: Pre-Gamebryo / old NetImmerse content hitting `NiBlendInterpolator::parse_legacy` (10.1.0.x bands) — reachable on Oblivion-era and older streams.
**Status**: NEW
**Location**: `crates/nif/src/blocks/interpolator.rs` (`parse_legacy`, the `Vec::with_capacity(array_size as usize)` site)

## Description
Every other file-driven count in the crate routes its pre-allocation through `NifStream::allocate_vec` (or `read_array_of` / `read_pod_vec`), which bounds the claimed count against the bytes remaining in the stream before reserving capacity. `parse_legacy` instead calls `Vec::with_capacity(array_size as usize)` directly on a count read straight from the stream.

## Evidence
```rust
let array_size = if int_priority {
    let n = stream.read_u16_le()?;
    let _array_grow_by = stream.read_u16_le()?;
    n
} else {
    stream.read_u8()? as u16
};
let mut items = Vec::with_capacity(array_size as usize);
for _ in 0..array_size { ... }
```

## Impact
Bounded and low. `array_size` is a `u16` (≤ 65535) or `u8`, so it cannot express the `0xFFFFFFFF`-style over-commit the `allocate_vec` guard exists to catch; worst-case transient reservation is ~1 MB and each per-element read is EOF-checked, so a corrupt count runs the reader to EOF → block-size recovery, not an OOM. This is a pattern-consistency divergence, not a live vulnerability or a measurable perf regression.

## Suggested Fix
Replace with `let mut items = stream.allocate_vec::<_>(array_size as u32)?;` (or `read_array_of`) so the legacy blend path shares the one bound-check idiom. One-line change; keeps the loop body identical.

## Related
#388/#764/#768 (the allocate_vec sweep this call site was not swept into); mirrors `read_block_ref_list` which does use `allocate_vec`.

## Completeness Checks
- [ ] **SIBLING**: Grep `interpolator.rs` (and other 10.1.0.x legacy parse paths) for any other raw `Vec::with_capacity` on a stream-read count
- [ ] **TESTS**: Existing `parse_legacy` coverage exercises the path; verify green after the swap
