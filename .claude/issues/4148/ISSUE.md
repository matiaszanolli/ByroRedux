# NIF-D1-2026-09-11-01: Unbounded recursion in read_and_skip_bounding_volume's UNION arm

URL: https://github.com/matiaszanolli/ByroRedux/issues/4148
Labels: bug, nif-parser, high, nif

---

**Severity**: HIGH
**Dimension**: 1 — Stream Position Integrity
**Game Affected**: Pre-Gamebryo/NetImmerse content only (v ≤ 4.2.2.0) — Morrowind-era and non-Bethesda mod content, not reachable by any of the seven vanilla titles Redux targets (all ship at v20.0.0.4+)
**Location**: `crates/nif/src/blocks/base.rs:306-337` (`read_and_skip_bounding_volume`)
**Status**: NEW (carry-forward of NIF-2026-09-04-D1-01, independently re-derived this session; no matching GitHub issue found via dedup search)

**Description**: The `UNION` bounding-volume arm (`bv_type == 4`) recurses once per declared child with no depth limit — 8 on-disk bytes per level (`bv_type` + `count`), so an N-byte crafted file can drive recursion to depth N/8. This is native Rust call-stack recursion, not a bounded loop or heap allocation, so none of the crate's `allocate_vec`/`MAX_SINGLE_ALLOC_BYTES` guards apply.

**Evidence** (`crates/nif/src/blocks/base.rs:306-337`):
```rust
fn read_and_skip_bounding_volume(stream: &mut NifStream) -> io::Result<()> {
    let bv_type = stream.read_u32_le()?;
    match bv_type {
        ...
        4 => {
            // UNION: num_bv(u32) + BoundingVolume[num_bv]
            let count = stream.read_u32_le()?;
            for _ in 0..count {
                read_and_skip_bounding_volume(stream)?;
            }
        }
        ...
    }
    Ok(())
}
```
Confirmed present verbatim by direct code reading during publish.

**Impact**: Process abort (uncatchable stack overflow, not a returned `Err`) on crafted/fuzzed/untrusted mod content reaching Redux's loose-`.nif` CLI load path. Blast radius: archive sweep harnesses, the loose-NIF CLI arm, third-party mod content in this version band.

**Related**: NIF-D1-2026-09-11-02 (same function, filed separately); precedent for a depth cap on recursive NIF structures at `#3237` (ESM GRUP walker, already capped at 64 levels).

**Suggested Fix**: Thread a `depth: u32` parameter through the function, `Err(InvalidData)` past a small cap (e.g. 32).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other recursive block-tree readers)
- [ ] **TESTS**: A regression test pins this specific fix (crafted deeply-nested UNION bounding volume)

