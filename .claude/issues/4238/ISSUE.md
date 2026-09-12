# FO4-2026-09-11-D4-03: psg_vertex_stride reads vertex-attribute field without the 12-bit mask its sibling applies

Issue: https://github.com/matiaszanolli/ByroRedux/issues/4238

**Severity**: LOW
**Dimension**: 4 — NIF BSVER 130 + Half-Float + FO4 Collision
**Location**: `crates/nif/src/import/precombine.rs:93-101` (vs `:122`)
**Status**: NEW

**Description**: `psg_vertex_stride` computes `attrs = (vertex_desc >> 44) as u16` (unmasked), while `decode_shared_geom_object` twenty lines later correctly masks with `& 0xFFF`. `BSVertexDesc`'s Vertex Attributes field is 12 bits at position 44; the unmasked form leaks 4 bits of the adjacent "Unused 2" field.

**Evidence**: Confirmed in current code — `psg_vertex_stride` (`precombine.rs:95`): `let attrs = (vertex_desc >> 44) as u16;` vs `decode_shared_geom_object` (`precombine.rs:122`): `let attrs = ((vertex_desc >> 44) & 0xFFF) as u16;`.

**Impact**: None today — every `VF_*` constant is below `0x800`, so the leaked bits can't change the current `attrs & VF_FULL_PRECISION` result. Latent trap for the first consumer testing a bit ≥ `0x1000`.

**Suggested Fix**: Apply `& 0xFFF` at the stride function too, matching `decode_shared_geom_object`.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other `vertex_desc >> 44` site in the crate is similarly unmasked
- [ ] **TESTS**: A regression test pins a `vertex_desc` with bits set above `0xFFF` at position 44 producing the same stride as the masked form
