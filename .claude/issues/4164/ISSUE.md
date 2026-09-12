# NIF-D5-2026-09-04-04: BhkConvexListShape reads HavokMaterial via raw read_u32_le instead of the shared read_havok_material helper

URL: https://github.com/matiaszanolli/ByroRedux/issues/4164
Labels: bug, nif-parser, low, tech-debt, nif

---

**Severity**: LOW
**Dimension**: 5 — Collision/Shader Parsing
**Location**: `crates/nif/src/blocks/collision/shape_compound.rs:175`
**Status**: NEW (carry-forward of NIF-2026-09-04's Dimension 5 detail; re-verified this round to have zero behavioral effect, filed as a code-hygiene fix; no matching GitHub issue)

**Description**: `BhkConvexListShape` reads its `HavokMaterial` field via a raw `stream.read_u32_le()` instead of the shared `read_havok_material` helper every other Havok shape parser uses. Verified this round to have zero behavioral effect today (this block type's version scope never reaches the helper's version-dependent gate), but it is an unexplained inconsistency that could silently diverge if the helper's gate ever moves to cover this block type's version range.

**Evidence** (`shape_compound.rs:170-175`):
```rust
let mut sub_shapes = stream.allocate_vec_sized::<BlockRef>(num_sub_shapes)?;
for _ in 0..num_sub_shapes {
    sub_shapes.push(stream.read_block_ref()?);
}
let material = stream.read_u32_le()?;
```
vs. sibling shape parsers using `read_havok_material(stream, bsver)` (see `crates/nif/src/blocks/collision/mod.rs`).

**Impact**: None currently measured (zero behavioral effect verified) — pure consistency/maintainability fix that removes a future silent-divergence risk.

**Suggested Fix**: Swap the raw `read_u32_le()` for the shared `read_havok_material` helper, matching every sibling shape parser.

## Completeness Checks
- [ ] **SIBLING**: Confirm every other `bhk*Shape` parser already uses the shared helper after this fix

