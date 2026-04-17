# FO4-D1-C2: BSSubIndexTriShape segment table wholesale-skipped via block_size — no dismemberment

**Issue**: #404 — https://github.com/matiaszanolli/ByroRedux/issues/404
**Labels**: bug, nif-parser, critical, legacy-compat

---

## Finding

`crates/nif/src/blocks/mod.rs:226-236` dispatches `BSSubIndexTriShape` by running the base `BsTriShape::parse` and then skipping the remainder via `block_size`:

```rust
"BSSubIndexTriShape" => {
    let start = stream.position();
    let shape = tri_shape::BsTriShape::parse(stream)?;
    if let Some(size) = block_size {
        let consumed = stream.position() - start;
        if consumed < size as u64 {
            stream.skip(size as u64 - consumed)?;
        }
    }
    Ok(Box::new(shape))
}
```

The segmentation payload is never decoded: `num_primitives` u32, `num_segments` u32, per-segment `{start_index, num_primitives, num_sub_segments, per-subseg data}`, optional SSF filename, per-segment user-slot-flags.

## Impact

1. **Dismemberment impossible.** FO4 actor meshes require per-segment bone-slot flags to map hits to body parts. Without walking the segment table, the renderer has zero body-part awareness — blocks combat/locational-damage wiring on the M-series roadmap.
2. **Fragile against any future refactor that stops plumbing `block_size`.** The current code has no fallback; if the caller passes `block_size = None` (in-stream path, malformed NIF), the parser silently misaligns. Every non-sized-block path becomes a potential realignment bug.

## Fix

Implement the full segment walk per the "FO4+ segmentation block" nif.xml schema:

```rust
pub struct BsTriShapeSegment {
    pub start_index: u32,
    pub num_primitives: u32,
    pub parent_array_index: u32,
    pub num_sub_segments: u32,
    pub sub_segments: Vec<BsTriShapeSubSegment>,
}
pub struct BsSubIndexTriShape {
    pub base: BsTriShape,
    pub num_primitives: u32,
    pub num_segments: u32,
    pub segments: Vec<BsTriShapeSegment>,
    pub ssf_filename: Option<String>,
    pub per_segment_flags: Vec<u32>,  // user slot flags
}
```

The existing test `bs_sub_index_tri_shape_consumes_segmentation_via_block_size` at `tri_shape.rs:1106` validates the **skip** — replace with a structured-decode test.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check every other block that currently uses `block_size` as a skip-to-end shortcut for unparsed payload. Grep `block_size.as_ref()` / `stream.skip(size` patterns in `blocks/mod.rs`.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Structured-decode test built from a real FO4 actor mesh (e.g., `actors\deathclaw\deathclaw.nif` which the Dim 5 probe confirmed contains BSSubIndexTriShape).

## Source

Audit: `docs/audits/AUDIT_FO4_2026-04-17.md`, Dim 1 C-2.
