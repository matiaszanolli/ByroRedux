# EXT-D6-2026-09-27-01: Legacy distant-terrain quads are sampled with the wrong image orientation — FO3/FNV rotated 180°, Oblivion flipped N/S, for diffuse and normal

**Issue**: #4898
**Filed**: 2026-09-27 (audit-publish, AUDIT_EXTERIOR_2026-09-27.md)
**Labels**: high,terrain-exterior,bug,game:oblivion,game:fo3,game:fnv

**Severity**: HIGH. This is a wrong canonical UV contract at the LOD boundary, affecting every legacy exterior, with no render-time fallback.
**Dimension**: Distant LOD and trees
**Tier Violated**: single-boundary / no-fabrication
**Game Affected**: Oblivion, FO3, FNV
**Status**: NEW. The convention dates from `b29e62751` (#1745), whose message records no orientation check.
**Location**:
- `byroredux/src/cell_loader/terrain_lod.rs:800-810` (`u = (wx−ox)/S`, `v = 1 − (wy−oy)/S`, one convention for all three games).
- `byroredux/src/env_translate.rs:72-81` (`TranslatedTerrainLodTexture` carries no orientation).
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-27.md` (HEAD `0e0d35b96`)

## Description
Three independent measurements agree, and all contradict the engine's assumption that image-right is east and image-top is north. The texture upload does no row flip, so v = 0 is the first stored row.
1. **Bethesda's own LOD mesh UVs**, the authoring contract:
   - FNV `wastelandnv.level4.x0.y56` / `x44.y60` and FO3 `wasteland.level4.x0.y0` / `x-20.y16` all map (0,0)→(1,0) and (S,0)→(0,0), which is **u = 1−x, v = y**.
   - Oblivion `60.00.00.32.nif` gives **u = x, v = y**.
2. **LAND ground truth.** The decoded LOD normal texels were correlated with VHGT height gradients.
   - FNV, 5 quads at u=1−x, v=y: corr(R, east) is +0.92…+0.97 and corr(G, north) is +0.96…+0.99. At the engine mapping the correlation is about 0.
   - Oblivion Tamriel, 5 quads at u=x, v=y: corr(R, east) is +0.55…+0.88. At the engine mapping it is ≤ +0.13.
3. **Seam continuity.** This is the mean absolute luminance step across shared quad edges.
   - FNV diffuse: 0.2–2.1 under the corrected mapping against 1.6–50.9 under the engine's.
   - Oblivion: v=y gives 8.4–12.1 against 16.3–43.7 for v=1−y.

## Evidence
- The audit's scratch scripts (not retained; re-derivable from the method described here): `land_heights*.py`, `land_vs_lod*.py`, `edge_orient.py`, the `lodnm/` UV fit and `nm_convention.py`.
- The main-context re-check confirmed the single hard-coded mapping, and that #1745 carried no orientation evidence.

## Impact
- Every legacy distant-terrain quad paints its baked colour 180° away (FO3/FNV) or mirrored N/S (Oblivion) within its footprint, with hard seams at every quad border.
- The normal map is sampled at the same wrong texel, so distant relief is lit from an unrelated patch of terrain.

## Suggested Fix
- Add a per-layout orientation to the translate output: Oblivion u=x, v=y; FO3/FNV u=1−x, v=y.
- Give the synthesized LOD vertices an explicit tangent: +X east, w = +1, not `new_terrain`'s LAND value of −1. Today they carry a zero tangent and shade through the derivative frame, which flips once the UV is corrected and would invert the relief.
- Pin both with a corpus test.
- Confirm with a captured distant-terrain frame (`m-exteriors.sh fnv static`).

## Related
#1745, `b50a8e6a9`, #3100, #2822. This answers the baseline's open NIFAL-D1-21b-01 question: the basis matches, but the sampling location is wrong.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawners / translate arms / games)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the EXAL/WATAL translate boundary (`env_translate.rs`, `groundcover_translate.rs`, the `cell_loader` spawn sites) — never pushed into shaders/renderer, never re-derived at render time. See `/audit-exterior`.
- [ ] **TESTS**: A regression test pins this specific fix
