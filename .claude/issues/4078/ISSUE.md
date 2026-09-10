# #4078 — ESM-2026-09-09-D5-04

an `ATXT` with no following `VTXT` silently drops the whole terrain texture layer (14 measured in `Oblivion.esm`)

Filed 2026-09-09 by `/audit-publish` from `docs/audits/AUDIT_ESM_2026-09-09.md`.
Snapshot of the issue **as filed** — GitHub is authoritative for current state
(`gh issue view 4078 --json state`).

---

- **Severity**: LOW
- **Dimension**: CELL / WRLD Walkers
- **Record / Sub-record**: `LAND.ATXT` / `LAND.VTXT`
- **Location**: `crates/plugin/src/esm/cell/walkers.rs:1179`, `:1218-1245`
- **Status**: NEW
- **Description**: `parse_land_record` stashes each `ATXT` header in `pending_atxt` and only
  pushes a `TerrainTextureLayer` from inside the `b"VTXT"` arm. An `ATXT` not followed by a
  `VTXT` is therefore overwritten by the next `ATXT` and the layer — LTEX FormID, quadrant and
  layer index included — disappears with no log line, even though `TerrainTextureLayer::alpha`
  is already `Option<Vec<f32>>` and could express "header authored, no alpha rows"
  (`mod.rs:143-152`; no `alpha: None` is constructed anywhere in the workspace). Secondary:
  the `ATXT` arm only assigns `pending_atxt` when `quadrant < 4`, without clearing it, so an
  out-of-range quadrant would leave a stale header for the next `VTXT` to attach to.
- **Evidence**: `land_census.py` over the exterior/interior `LAND` bodies (zlib-inflating the
  compressed ones):

  | master | ATXT | VTXT | ATXT with no following VTXT |
  |---|---|---|---|
  | `Oblivion.esm` | 358,094 | 358,080 | **14** |
  | `Fallout3.esm` | 63,456 | 63,456 | 0 |
  | `FalloutNV.esm` | 63,240 | 63,240 | 0 |
  | `Skyrim.esm` (SE) | 102,968 | 102,968 | 0 |
  | `Fallout4.esm` | 56,859 | 56,859 | 0 |

  The 14 (from `atxt_probe.py`, `(LAND form, next sub-record, LTEX, quadrant, layer)`) are all
  immediately followed by another `ATXT`, and 10 of them name a real LTEX:
  `0x53854 ltex=0x1c7b6 q=0 layer=1`, `0x53853 ltex=0x1c7b7 q=1 layer=0`,
  `0x53852 ltex=0x1c7b7 q=3 layer=0`, `0x47c8f ltex=0x27362 q=1 layer=0`,
  `0x47c8f ltex=0x27363 q=1 layer=1`, `0x4a702 ltex=0x27362 q=1 layer=0`,
  `0x4a702 ltex=0x27363 q=1 layer=1`, `0x4a70a ltex=0x27363 q=1 layer=2`,
  `0x4a909 ltex=0x27362 q=1 layer=0`, `0x4a909 ltex=0x27363 q=1 layer=1`
  (the remaining 4 carry `ltex=0x0`, i.e. default dirt). Measured `ATXT_quadrant_ge_4` and
  `BTXT_quadrant_ge_4` are 0 in all five masters, so the stale-`pending_atxt` path is
  currently unreachable on shipped data.
- **Impact**: 10 authored Oblivion terrain layers across 9 exterior tiles never reach
  `TerrainQuadrant::layers`; those tiles render with one fewer splat layer than authored.
  Visual-only, Oblivion-only, sub-0.01 % of layers. The real cost is that the drop is
  unattested: no citation establishes what the engine should do with a `VTXT`-less `ATXT`
  (zero-alpha layer? full-opacity layer?), and no test pins the current choice, so the
  behaviour is a silent default rather than a decision.
- **Related**: #3720 (the neighbouring LAND soft-fail path), `TerrainTextureLayer::alpha`
  (`mod.rs:151`).
- **Suggested Fix**: Flush a pending `ATXT` when the next sub-record is not its `VTXT` —
  either push it with `alpha: None` or `log::debug!` and drop it deliberately — and clear
  `pending_atxt` on an out-of-range quadrant. Whichever semantics is chosen needs one test
  and one citation (xEdit `wbDefinitionsTES4.pas` `LAND` ordering) rather than the current
  implicit drop.

---
