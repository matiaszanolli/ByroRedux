# FO3-D2-03: BSSegmentedTriShape segment table consumed and discarded with no bounds check and no downstream consumer

Filed from: `docs/audits/AUDIT_FO3_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2329

**Severity**: LOW
**Location**: `crates/nif/src/blocks/tri_shape/ni_tri_shape.rs:183-192`
**Status**: NEW

### Description
Wire layout is correct (verified against nif.xml and zero drift across 17,172 real FO3 NIFs). Two residual gaps: (a) every field is read into `let _`, so the per-segment body-part table never reaches `ImportedMesh` (benign today — FO3 dismemberment is driven by `BSDismemberSkinInstance` instead, which *is* imported); (b) `num_segments` has no sanity bound before the loop, unlike the sibling `parse_mesh_emitter`'s `check_alloc`.

Confirmed against current code: `parse_segmented` (`ni_tri_shape.rs:183-192`) reads `num_segments` then loops `for _ in 0..num_segments { let _flags = ...; let _index = ...; let _num_tris = ...; }` — all three fields discarded, no `check_alloc`/bound check on `num_segments` before the loop.

### Impact
(a) benign, nothing visible lost today. (b) bounded robustness gap only (corrupt count walks to EOF and errors, no UB/DoS).

### Suggested Fix
Add `stream.check_alloc(num_segments as usize * 9)?` before the loop for parity.

### Related
#146, FO3-D2-02

## Completeness Checks
- [ ] **SIBLING**: Check other fixed-layout-loop parsers for the same missing-`check_alloc` pattern (sibling `parse_mesh_emitter` already has it, use as the template)
- [ ] **TESTS**: A regression test pins "corrupt/huge `num_segments` is rejected by `check_alloc`, not walked to EOF"
