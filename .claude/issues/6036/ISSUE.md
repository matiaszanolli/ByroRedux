# ESM-2026-10-18-EXT-01: LTEX to TXST path and texture-set resolution runs per plugin, so a DLC LTEX that points at a master TXST never resolves

Labels: medium,esm-plugin,terrain-exterior,bug,game:fnv
Source: docs/audits/AUDIT_ESM_2026-10-18.md

- **Severity**: MEDIUM. A vanilla DLC loses terrain layers; the loader logs "not in landscape_textures map; skipping layer".
- **Dimension**: ESM→ECS Handoff / FormID & Load Order
- **Record / Sub-record**: `LTEX` / `TNAM`
- **Location**:
  - `crates/plugin/src/esm/records/parse.rs:472-476`: `landscape_textures` is filled from `ltex_to_txst` joined with this plugin's own `txst_textures`
  - `crates/plugin/src/esm/records/parse.rs:487-492`: `landscape_texture_sets`, same join against this plugin's own `texture_sets`
  - `crates/plugin/src/esm/cell/index.rs:509-513`: `merge_from` only `extend`s the already-resolved maps
- **Status**: NEW. Not in the 119-issue snapshot. #3314 (closed) fixed the FormID remap of the `TNAM` value, not this join.
- **Description**: `ltex_to_txst` is a local of `parse_esm_with_load_order`. It is joined against the same plugin's TXST maps and then dropped, and it is not a field of `EsmCellIndex`. A plugin whose LTEX references a TXST authored in a master therefore produces no `landscape_textures` or `landscape_texture_sets` entry at all. The merge cannot repair it. An override that re-points `TNAM` at a different master TXST would also leave the master's stale path in place.
- **Evidence** (my census, `ltex_dlc.py`, `ltex_cmp.py`, `hh_land.py`):
  - `HonestHearts.esm` has 6 LTEX. `0x0100CD37 ScrubBarrenWastes02GrassMix` has `TNAM = 0x000357BD`, a `FalloutNV.esm` TXST. Its own LAND records use it in 550 `ATXT` layers.
  - `Update.esm` has 15 LTEX, 13 of which point at `Skyrim.esm` TXSTs. All 13 are no-op overrides, identical to the master's `TNAM`. Only the two that point at Update's own TXST change anything, and they resolve locally.
  - So the defect is live on real DLC data; Skyrim only escapes because the overrides happen to be identical.
- **Impact**: Fallout New Vegas Honest Hearts (Zion) drops about 550 splat layers of `ScrubBarrenWastes02GrassMix`. Any mod LTEX that reuses a vanilla TXST (the common authoring pattern) renders with a missing terrain layer.
- **Related**: #3314, #5075-class "keyed in global space" invariants.
- **Suggested Fix**: keep `ltex_to_txst` (a `HashMap<u32, u32>`) on `EsmCellIndex`, merge it last-writer-wins in `merge_from`, and run the TXST join once after the final merge. Pin it with a two-plugin test where the LTEX is in the DLC and the TXST is in the master.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, other callers)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_ESM_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
