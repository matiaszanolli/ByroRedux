# FO3-2026-08-20-D4-01: FO3 bakes a four-level LOD ladder out to +/-64 cells but LodBandLadder::for_game returns None for Fallout3NV — the ring is pinned at level 4 / 48 cells, leaving 567 meshes + 832 textures unreachable

State: OPEN
Labels: bug,import-pipeline,medium,legacy-compat,game:fo3,terrain-exterior,esm-plugin

- **Severity**: MEDIUM
- **Dimension**: FO3 Dim 4 — Cell Loading End-to-End, exterior
- **Location**: `byroredux/src/cell_loader/lod_bands.rs:130-141` (`LodBandLadder::for_game` returns `None` for everything but Skyrim/FO4), `byroredux/src/cell_loader/terrain_lod.rs:286-310` (the `None` arm of `desired_lod_quads`, which emits only `k = LOD_BLOCK_CELLS`), `:53`/`:59` (`LOD_BLOCK_CELLS = 4`, `LOD_RADIUS_BLOCKS = 12`), `:254-257` (`lod_ring_reach_cells`)
- **Status**: NEW — residual of #3100 (CLOSED), narrower than #2371 (OPEN)

## Description

#3100 wired FO3's authored LOD **textures**, but only at the one level the ring can ask for.

`LodBandLadder::for_game` has arms for `Skyrim` and `Fallout4` and `_ => return None` for everything else. Its doc-comment states the premise:

> The ladder for `game`, or `None` for titles that ship **no baked quadtree LOD at all** (Oblivion / FO3 / FNV).

**That premise is false for FO3.** With no ladder, `desired_lod_quads` walks a fixed `LOD_RADIUS_BLOCKS`-deep square of level-4 blocks, so `lod_ring_reach_cells(Fallout3NV) = 12 × 4 = 48` cells and `level` is never anything but 4. FO3 ships **three coarser bands** whose whole purpose is the range beyond that.

`translate_terrain_lod_textures`' `FalloutLegacy` arm is already level-generic (`if level > 0`), so the constraint is entirely on the caller.

## Evidence

Archive census over `Fallout - Meshes.bsa` / `Fallout - Textures.bsa`:

```
meshes\landscape\lod   : level4 1661 · level8 480 · level16 71 · level32 16   (2 232)
textures\landscape\lod : level4 2560 · level8 640 · level16 160 · level32 32  (3 392 of 3 870)

wasteland level-4 quad coordinate range: x -64..60, y -64..60   (1 274 quads, 128x128 cells)
```

Ring reach **48 cells** vs authored extent **64 cells**. **567 meshes and 832 texture files sit at levels the ring cannot name.**

The `#3100` half is verified correct as far as it goes: `TerrainLodLayout::FalloutLegacy` (`env_translate.rs:51`, `:62`) names FO3's family, `legacy_landscape_lod_supported` (`cell_loader/lod_support.rs:82`) admits it, and the generated name matches vanilla exactly (`wasteland.n.level4.x-64.y-4.dds`, `.n.` infix universal across all 3 870 FO3 LOD textures, block origin always a multiple of 4).

## Impact

Capital Wasteland's horizon ends **~65 500 BU short** of the authored distance, and the coarse bands that vanilla uses to draw the far landmass are inert. Not a crash and not a regression — the level-4 ring is a working, and now correctly-textured, fallback — but the silhouette diverges from vanilla on **every FO3 exterior**, and #3100's closure reads as "FO3 legacy LOD is done" when one of four bands is wired.

**Applies to FNV identically** (same `GameKind::Fallout3NV`, same baked ladder convention).

## Related

- #3100 (CLOSED) — the texture-translation half, correct as far as it goes. This is its residual.
- #2371 (OPEN) — the distant-LOD umbrella, scoped to `.btr`/`.bto` bands; it does **not** name this ladder.
- #2086 (CLOSED) — placement LOD, correctly settled as Oblivion-only.

## Suggested Fix

Give `LodBandLadder::for_game` a `FalloutLegacy` arm whose refine boundaries come from **FO3's own `fBlockLevel<N>Distance` / `fBlockMaximumDistance` GMSTs** — they are in `Fallout3.esm`, so this needs **no guessed constant** (per the project's no-guessing rule).

Gate coarse-band availability on the authored quad existing — the same `has_btr`-shaped predicate the Skyrim/FO4 path already uses, pointed at `translate_terrain_lod_textures` instead.

Correct the `for_game` doc-comment: FO3/FNV *do* ship a baked ladder; only Oblivion belongs in the "no coarser source" bucket.

---
*Filed from `docs/audits/AUDIT_FO3_2026-08-20.md` (Dim 4). Verified against HEAD `bb0b92f2` — `for_game`'s `_ => return None` and its "no baked quadtree LOD at all (Oblivion / FO3 / FNV)" doc-comment are both live.*

## Completeness Checks
- [ ] **SIBLING**: FNV checked in the same change — it shares `GameKind::Fallout3NV` and the same authored ladder
- [ ] **CANONICAL-BOUNDARY**: band selection stays in `cell_loader`, the per-game *layout* stays in `translate_terrain_lod_textures`; no per-game branch pushed into the terrain pass. See `/audit-nifal`.
- [ ] **TESTS**: a regression test asserts `lod_ring_reach_cells(Fallout3NV) >= 64` and that `desired_lod_quads` emits at least one `level > 4` quad on a Capital Wasteland grid
- [ ] **NO-GUESSING**: the refine distances are read from `Fallout3.esm` GMSTs, not invented

