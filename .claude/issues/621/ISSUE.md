# SK-D1-LOW: BsTriShape parser hardening — parse_dynamic VF_FULL_PRECISION + data_size mismatch wrong stride

## Finding: SK-D1-LOW (bundle of SK-D1-04 + SK-D1-05)

- **Severity**: LOW (both items)
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-04-24.md`

## SK-D1-04: parse_dynamic overwrites positions but never widens vertex_desc precision claim

**Location**: [crates/nif/src/blocks/tri_shape.rs:628-648](crates/nif/src/blocks/tri_shape.rs#L628)

`parse_dynamic` rewrites `shape.vertices` from the trailing Vector4 array but leaves `shape.vertex_desc` untouched. Downstream consumers reading `vertex_desc & VF_FULL_PRECISION` think positions are still half-precision, even though the dynamic array is full f32. Latent today (no consumer cross-checks); a future GPU-skinning path that re-uploads from the packed buffer would read stale half-precision metadata.

**Fix**: when `dynamic_count > 0`, OR `VF_FULL_PRECISION << 44` into `shape.vertex_desc` so the descriptor matches the post-overwrite reality.

## SK-D1-05: data_size mismatch warns but parse continues with the suspect stride

**Location**: [crates/nif/src/blocks/tri_shape.rs:426-441](crates/nif/src/blocks/tri_shape.rs#L426)

Logs a WARN if stored `data_size != vertex_size_quads * num_vertices * 4 + num_triangles * 6`, then unconditionally enters the per-vertex loop at lines 466-552 driven by `vertex_size_quads`. If `vertex_size_quads` is the misparsed field (the descriptor sits in the same u64 just read), the loop reads the wrong stride for every vertex. Block-size realignment in the dispatcher hides the slip from the parse-rate metric, so a misparsed `vertex_desc` corrupts geometry while stats still show 100%.

**Fix**: when the assertion fails, prefer the data_size-derived stride: `(data_size - num_triangles*6) / num_vertices`, OR hard-fail on mismatch and let the dispatcher recover via `block_size` skip.

## Related

- #341 (closed): BSDynamicTriShape data_size==0 — adjacent fix surface.
- #359 (closed): BSTriShape data_size sanity-checked — closed; this finding is the next step (act on the mismatch, not just warn).

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: After SK-D1-05, audit other parsers that WARN-and-continue on size mismatch (NiTriShape, BSDynamicTriShape).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: For SK-D1-04, synthetic BSDynamicTriShape with overwritten positions → assert `vertex_desc & VF_FULL_PRECISION != 0` post-parse. For SK-D1-05, synthetic BSTriShape with deliberate `data_size` mismatch → assert parse error or correct alternative stride.

_Filed from audit `docs/audits/AUDIT_SKYRIM_2026-04-24.md`._
