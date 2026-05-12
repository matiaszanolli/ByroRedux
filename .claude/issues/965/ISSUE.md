# OBL-D3-NEW-01: WRLD record drops every exterior-bound and map-data field

**Labels**: bug, high, legacy-compat

**Audit**: `docs/audits/AUDIT_OBLIVION_2026-05-11_DIM3.md`
**Severity**: HIGH (single biggest blocker for Oblivion exterior render)
**Domain**: ESM / TES4 worldspace

## Premise

`parse_wrld_group` matches exactly two sub-records on the WRLD record itself: `EDID` and `CNAM`. Everything else falls to `_ => {}`.

[crates/plugin/src/esm/cell/wrld.rs:47-61](../../crates/plugin/src/esm/cell/wrld.rs#L47-L61)

## Gap

Missing for Oblivion (and cross-game where applicable):

- `WCTR` (center cell coords)
- `NAM0` + `NAM9` (usable grid bounds — min/max XY i32 pairs)
- `NAM2` (default water height)
- `ICON` (worldspace map texture)
- `MNAM` (map data block: usable rect + map cell offset)
- `PNAM` (parent worldspace FormID + inheritance flags)
- `DATA` (worldspace flags byte: no-LOD-water, small-world, fixed-dimensions, no-grass)
- `XEZN` (worldspace encounter zone)
- `SNAM` (default music)
- `RNAM` (region overrides)
- `OFST` (per-cell LAND offset table — perf optimization for streaming)

## Impact

- Cannot bound the Tamriel exterior grid (no NAM0/NAM9) — cell loader must guess from explicit cells map.
- Cannot resolve parent-worldspace inheritance (Shivering Isles, mod worldspaces derived from Tamriel).
- Cannot stream LAND in radius without OFST (perf only, not correctness).
- Cannot pick the right default music or map texture.

## Suggested Fix

Add a `WorldspaceRecord` type with at minimum:

```rust
pub struct WorldspaceRecord {
    pub usable_min: (i32, i32),
    pub usable_max: (i32, i32),
    pub parent_worldspace: Option<u32>,
    pub parent_flags: u8,
    pub default_music: Option<u32>,
    pub water_height: f32,
    pub map_texture: String,
    pub flags: u8,
}
```

Populate from `NAM0` / `NAM9` / `PNAM` / `DATA` / `NAM2` / `ICON` / `SNAM`. Store on `EsmCellIndex.worldspaces: HashMap<String, WorldspaceRecord>` keyed by lowercased EDID. The cell loader can then iterate `(min_x..=max_x, min_y..=max_y) ∩ requested_radius`. OFST is a separate consumer (LAND streamer) and can land later.

## Completeness Checks

- [ ] **SIBLING**: Verify Skyrim WRLD records still parse — the fix should not break Skyrim's worldspace flow.
- [ ] **SIBLING**: Verify FO3/FNV WRLD records still parse — same shape, NAM0/NAM9 should populate for `wasteland`.
- [ ] **TESTS**: Regression test parses `Oblivion.esm` and asserts `worldspaces["tamriel"]` has `usable_min`/`usable_max` non-default.
- [ ] **TESTS**: Regression test parses `Oblivion.esm` and asserts Shivering Isles' worldspace has `parent_worldspace = Some(<Tamriel FormID>)` if DLC archive is on the data path.
- [ ] **TESTS**: Regression test for FO3 `wasteland` worldspace NAM0/NAM9 round-trip.
- [ ] **DOCS**: Update CLAUDE.md "Compat correctness" section to note WRLD is fully decoded.
