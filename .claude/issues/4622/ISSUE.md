# NIF-D1-2026-09-21-03: BSTriShape's #621 data_size-derived stride override has no upper bound; a bogus data_size over-reads past the block and decodes garbage vertices

**Issue**: #4622
**Filed**: 2026-09-22 (audit-publish, AUDIT_NIF_2026-09-21.md)

**Severity**: MEDIUM. A consumed-byte mismatch that `block_size` reconciliation covers; the parsed geometry is still wrong.
**Dimension**: Stream Position Integrity
**Game Affected**: the mechanism applies to every BSTriShape band (SSE 100, FO4 130, FO76 155); observed only on FO4 third-party content. Vanilla shows zero BSTriShape drift: FO4 235,082 NIFs, FO76 102,968, SSE 32,902.
**Location**:
- `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:515-537`: the derived stride, and the #2598 guard `safe_stride = derived_stride.filter(|&s| s >= min_needed)`.
- `crates/nif/src/blocks/mod.rs:467`: `BsTriShape::parse(stream)` is called without `block_size`.

## Description
When `data_size` disagrees with `vertex_desc`, the override divides `data_size` to derive a per-vertex stride. The only check is a lower bound (`s >= min_needed`, `bs_tri_shape.rs:537`). A `data_size` that overstates the payload therefore produces a stride larger than the descriptor's, and the decode reads past the block. The parser never sees `block_size` at this call site, so it cannot notice.

Confirmed at HEAD `ee6d3fb39`: `bs_tri_shape.rs:537` is unchanged — `derived_stride.filter(|&s| s >= min_needed)` has no upper-bound arm, and `blocks/mod.rs:467` still calls `tri_shape::BsTriShape::parse(stream)?` with no `block_size` argument (unlike `parse_sub_index`, which does receive one).

## Evidence
Probe dump of `meshes\ss2c2\architecture\buildings\bld02addsinglewin01.nif` (`ss2_xpac_chapter2 - main.ba2`, bsver 130):
```
#1 size=226 consumed=442 drift=-216 BSTriShape vdesc=0x0003b00005430206 stride=24 ntri=2 nvert=4 data_size=324 expected=108
```
- The 226-byte block is exactly the 118 bytes of header fields plus the descriptor-stride payload (4 vertices × 24 B + 2 triangles × 6 B = 108 B).
- The override derives (324 − 12) / 4 = 78 bytes per vertex, reads 216 bytes past the block, and returns `Ok` with vertices 1–3 and both triangles decoded from wrong offsets.
- The sibling block #3 in the same file over-reads to EOF and falls to NiUnknown.

## Impact
Silently garbage geometry, visible only in `drift_histogram` and never in the clean-rate gate. Widely installed mods (Sim Settlements 2 family) are the realistic trigger.

## Related
#621 (closed — introduced the override), #2598 (closed — added the lower-bound-only guard), #359 (closed — original stride-mismatch finding).

## Suggested Fix
- Pass `block_size` into `BsTriShape::parse`, as `parse_sub_index` already receives it, and reject a derived stride whose payload exceeds the block.
- Otherwise, cap the override near the descriptor stride.
- Pin the dumped 226-byte block as a fixture.

Source: docs/audits/AUDIT_NIF_2026-09-21.md (NIF-D1-2026-09-21-03)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other data_size-derived overrides, `parse_dynamic`/`parse_lod` variants)
- [ ] **TESTS**: A regression test pins this specific fix (the 226-byte fixture from Sim Settlements 2)
