# EXT-D6-2026-10-26-01: Terrain-LOD skirts go stale at block seams, and a `.btr` block never gets any

Labels: medium,terrain-exterior,bug
Source: docs/audits/AUDIT_EXTERIOR_2026-10-26.md

- **Severity**: MEDIUM / **Dimension**: 6 (Distant LOD and trees) / **Tier Violated**: n/a
- **Game Affected**: all games with terrain LOD; the FO4 and Oblivion crack the commit fixes are the reported ones. Skyrim/FO4 `.btr` blocks are affected differently (see below).
- **Location**: `byroredux/src/cell_loader/terrain_lod.rs:1026-1050` (skirt edge discovery), `:205-218` (`block_hole_mask`), `:~625-660` (staleness test `prev_mask == mask`), `:~700-712` (`.btr` path).
- **Status**: NEW. Static trace; **unconfirmed on screen**.
- **Description**: `7eac91637` decides a skirt per LOD quad edge by asking whether the *neighbouring cell* is resident, including cells in the next block (`neighbour_is_full_detail`, "the neighbour may be in the next block, so go by cell"). But the only rebuild key is `LodBlock::hole_mask` = `block_hole_mask(...)`, which covers the block's own 16 cells. So:
  1. A block whose own mask is unchanged is never rebuilt when a resident cell *in the adjacent block* appears or leaves. The edge between them keeps whatever skirts it had at spawn time: when the resident region advances against a block edge, the crack returns on that edge (about every fourth step along each axis, where the newly resident cell sits on a block boundary).
  2. A finest-band block with `mask == 0` on a Combined game (Skyrim/FO4) takes the prebaked `.btr` path (`spawn_btr_block`), which has no skirt code at all. If its neighbour block holds the edge resident cells, that join is the same chord-above-edge geometry the commit describes.
- **Evidence**:
  ```rust
  // spawn_lod_block: edge belongs to the quad, neighbour looked up by cell, across blocks
  let neighbour_is_full_detail = |dr, dc| { ... cell_is_full_detail(bx0 + nc.div_euclid(spc), by0 + nr.div_euclid(spc), resident_full_cells) };
  // stream_lod_blocks: rebuild only when this block's own mask moved
  let mask = if level == k { block_hole_mask(cells_map, qx, qy, resident_full_cells) } else { 0 };
  Some(m) if m == mask => { missing_blocks.remove(..); continue; }
  ```
  The capture in the commit message / `watal.md` is a static pose at spawn time, which is the one case that works. The unit test covers `append_lod_skirts` geometry only; nothing pins the rebuild key.
- **Disproof attempted**: could another trigger rebuild the neighbour? `desired` only changes with the player's band selection, and the other invalidation path is the same `hole_mask` compare. No neighbour-residency term exists anywhere in the staleness check. A stale skirt after the neighbour *unloads* is harmless (the wall hangs under the neighbouring LOD surface); only the missing-skirt direction is visible.
- **Impact**: the pale-blue sky ribbon along the full-detail / LOD join reappears intermittently while streaming, and on `.btr` joins permanently. Visual only.
- **Related**: `7eac91637`, `docs/engine/watal.md` (2026-10-26 note), #6094.
- **Suggested Fix**: extend the rebuild key with the 16 outer-neighbour cell bits (a second `u16`/`u32` "skirt mask" stored on `LodBlock` and compared with `hole_mask`), and decide on `.btr` boundary blocks either by forcing the synth path when any outer neighbour is resident or by adding the same skirt to `terrain_lod_btr`. Add a pin that moves one cell across a block edge and asserts the neighbour is invalidated.

_Filed from `docs/audits/AUDIT_EXTERIOR_2026-10-26.md` (HEAD 3e6f6b66a). Static trace; not confirmed on screen unless stated._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader copies, other sun/lighting sites, other rings)
- [ ] **TESTS**: A regression test pins this specific fix
