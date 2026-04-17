# OBL-D3-H4: CELL XCLW (water height) not parsed — flooded Ayleid ruins render dry

**Issue**: #397 — https://github.com/matiaszanolli/ByroRedux/issues/397
**Labels**: bug, renderer, high, legacy-compat

---

## Finding

`parse_cell_group` at `crates/plugin/src/esm/cell.rs:315-423` handles EDID/DATA/XCLL/FULL on Oblivion CELL records. Oblivion has several more Oblivion-unique CELL sub-records that the walker silently drops:

| Sub-record | Count in Oblivion.esm | Rendering impact |
|---|---|---|
| **XCLW** | 288 | Water plane height (f32) — flooded Ayleid ruins, sewer interiors |
| XCMT | 1549 | Music type byte (0=default/1=public/2=dungeon) — reverb/music |
| XCWT | 160 | Water type (LTEX form ID) — water material selection |
| XCCM | 55 | Climate form ID for pseudo-exterior cells |
| XOWN | 820 | Owner — gameplay only |

**XCLW is rendering-critical.** Without it, any interior cell authored with a water plane (Ayleid ruins like Vilverin, Sardavar Leed; sewer systems; Shivering Isles corrupted chambers) renders without water — the plane depth is missing, so either no water geometry is spawned or it spawns at Y=0 and looks wrong.

**Note**: Skyrim's equivalent XCLW handling is tracked separately at #356. This is the Oblivion counterpart; the fix should be shared via `GameVariant` dispatch.

## Fix (~10 lines)

Add XCLW match arm alongside XCLL in `parse_cell_group`:

```rust
b"XCLW" if sub.data.len() >= 4 => {
    water_height = Some(f32::from_le_bytes(sub.data[0..4].try_into().unwrap()));
}
```

Add `water_height: Option<f32>` to `CellData`. Optionally: XCWT, XCMT, XCCM as Medium follow-ups (reverb/water material are not rendering-blocking).

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Share the XCLW parser with the Skyrim path tracked at #356 (same semantics — f32 water height).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Parse a synthetic Oblivion CELL with XCLW bytes `[00 00 20 41]` (f32 10.0) → `CellData.water_height == Some(10.0)`.

## Source

Audit: `docs/audits/AUDIT_OBLIVION_2026-04-17.md`, Dim 3 H4.
