# SK-D5-NEW-07: BSLODTriShape parser realignment fires on real Skyrim tree LODs — distant-LOD size triplet may be misread

## Description

Unlike the by-design BSLagBoneController case in #SK-D5-NEW-03, `BSLODTriShape` HAS a dedicated parser (`tri_shape::BsTriShape::parse_lod`, dispatch at `blocks/mod.rs:275`) that explicitly reads the trailing 3 × u32 LOD-size triplet (covered by `tri_shape_skin_vertex_tests.rs:719`). But on Skyrim Meshes0/1 + the single-NIF dump of `dlc02\landscape\trees\treepineforestashl02.nif`, every parse fires the per-block `consumed != block_size` realignment for BSLODTriShape.

Either the on-disk SE LOD layout has additional trailing fields the parser doesn't model, or the FO4-targeted `parse_lod` has a per-version offset gap on Skyrim.

## Location

`crates/nif/src/blocks/tri_shape.rs:813` (`BsTriShape::parse_lod`)

## Evidence

- Single-tree dump: `BSLODTriShape=2` realigned (treepine NIF extracted via `dump_nif`).
- Sweep totals on Meshes0: 11 WARN events covering ~14 blocks.
- Test at `crates/nif/src/blocks/tri_shape_skin_vertex_tests.rs:719` covers the FO4 layout with a synthesised 3-u32 trailer — but doesn't cover the SE on-disk layout (no real-Skyrim-NIF regression test).

## Impact

Distant-LOD0/1/2 size fields are realigned via `block_size` — values may be partially or wholly recovered depending on alignment, but trust in their values is unverified. Affects SpeedTree distant LOD switching (`BSTreeNode` + tree-LOD pyramid). Game-visual consequence: tree LODs may pop or pick the wrong tier at draw distance. Hard to pin without RenderDoc.

## Suggested Fix

Pull a Skyrim BSLODTriShape via `trace_block` and diff its on-disk byte range against the synthesised test layout at `tri_shape_skin_vertex_tests.rs:732`. Likely either:

- (a) extra padding pre-LOD trailer on SE
- (b) a per-stream-version branch missing in `parse_lod`

Add a real-NIF regression once the layout is pinned.

## Related

- SK-D5-NEW-03 (BSLagBoneController by-design noise — drowns out this real drift)
- SK-D5-NEW-05 (per-block realignment invisible to nif_stats gate)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check `BSMeshLODTriShape` and `BSSubIndexTriShape` for the same per-version trailer issue — all three are SE/FO4 LOD variants
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a real-Skyrim-NIF regression test using a fixture extracted from `meshes\landscape\trees\treepineforest01.nif` (or equivalent) — assert `parse_lod` consumes exactly `block_size` bytes

## Source Audit

`docs/audits/AUDIT_SKYRIM_2026-05-05_DIM5.md` — SK-D5-NEW-07