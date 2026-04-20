# Issue #440

FO3-5-01: NiTriShapeData tangent mask 0xF000 over-reads — FO3 NPC FaceGen heads invisible

---

## Severity: High

**Location**: `crates/nif/src/blocks/tri_shape.rs:755`

## Problem

`parse_geometry_data_base_inner` skips `array_count * 24` bytes for tangents + bitangents whenever `data_flags & 0xF000 != 0`. Per nif.xml `NiGeometryData.Tangents`, tangents/bitangents are present only when **bit 12 specifically** is set (`NBT_METHOD == 0x1000`). Bits 13/14/15 mean something else.

FO3 FaceGen heads set bit 13/14 for morph pointers (VAS_MATERIAL_DATA / VAS_MORPH_DATA), not NBT. The current mask triggers a 4,024 B skip past the real data → stream misalignment → outer loop demotes the block to `NiUnknown` via `block_sizes` recovery (`lib.rs:202-213`).

## Evidence

`target/release/examples/nif_stats /tmp/audit/fo3/probes/headfemalefacegen.nif`:
```
Block 6 'NiTriShapeData' (size 90126, offset 453, consumed 94150):
skip(64126) at position 94603 would exceed data length 136938 —
seeking past block and inserting NiUnknown
```

The FaceGen head loses NiTriShapeData entirely — no vertices, triangles, UV, normals. Every FO3 NPC face silently renders as empty geometry.

## Impact

~200 vanilla FaceGen heads in `Fallout - Meshes.bsa`. The ROADMAP "Megaton 1609 entities, 199 textures" baseline covered props, not NPCs — the NPC regression has been invisible.

## Fix

One-character change at line 755:
```rust
if has_normals && data_flags & 0x1000 != 0 {  // NBT_METHOD == 0x1000
```

Add regression probe: `/tmp/audit/fo3/probes/headfemalefacegen.nif` must parse with zero `warn!` lines and 0 `NiUnknown` blocks (currently has both).

## Completeness Checks

- [ ] **TESTS**: Regression test for FaceGen head parse — zero warnings, all blocks resolved
- [ ] **SIBLING**: Check other NiGeometryData callsites for the same mask bug
- [ ] **DOCS**: Add nif.xml NBT_METHOD reference comment at the fix site

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-5-01)
