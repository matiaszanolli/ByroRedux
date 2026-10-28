# RT-4: `m-exteriors fo3 boundary` hard-fails the terrain-seam gate on MegatonWorld

Labels: medium, terrain-exterior, bug, game:fo3
Source: docs/audits/AUDIT_RUNTIME_2026-10-27.md

**Report**: `docs/audits/AUDIT_RUNTIME_2026-10-27.md` · **Finding**: RT-4 · **Severity**: MEDIUM (gate red; visible seam cracks along the traversal; owner `/audit-exterior`) · **Dimension**: Gate matrix / exterior
**Location**: gate `docs/smoke-tests/m-exteriors.sh` (fo3 profile `fo3_run`: `--grid -1,-7 --wrld MegatonWorld`, `grid-cross`). Producer of the verdict line: the terrain-seam check in `byroredux/src/streaming_helpers.rs` (`pairs_checked` / `pairs_dirty` / `check_seam`, surfaced by the `terrain.seams` command).
**Game / Cell**: fo3 / MegatonWorld (-1,-7), three eastward crossings
**Note**: runtime-measured by the audit (engine launched headless); not re-executed by the prep pass.

## Description
In the FO3 boundary run, 5 of 6 adjacent loaded cell pairs disagree in edge height. MegatonWorld carries parent flags 0xFFC5, so Use Land Data is set. The loader logs `child worldspaces: 107 LAND-less cells take their parent's landscape`, so only LAND-less cells borrow the Wasteland landscape; cells with their own LAND keep it. If MegatonWorld's own LAND and Wasteland's disagree at shared edges, the mix produces exactly this kind of seam. That mechanism is **unverified**: the `terrain-seams:` line carries no pair coordinates. FNV, Oblivion and Skyrim pass the same gate; FO4 passes only through its documented #3306 exemption.

## Evidence
- Current: `terrain-seams: sampled=1 pairs_checked=6 pairs_dirty=5 height_mismatch_vertices=160 normal_mismatch_pairs=6 verdict=FAIL`. Boundary frame is a flat fog-green image (mean 0.143, sd 0.0063, 111 draws).
- At `a649b55b4` (2026-10-12): `pairs_checked=3 pairs_dirty=3 height_mismatch_vertices=99` - already failing, and has grown since.
- Log: `/tmp/audit/runtime/ext-boundary/fo3/debug.log` (audit scratch).

## Impact
Visible terrain cracks at FO3 child-worldspace seams. The FO3 boundary gate is red, so it cannot catch a new seam regression.

## Related
FO4-2026-10-27-01 (Use Land Data child worldspaces, FO4 leg); AUDIT_EXTERIOR_2026-10-26 EXT-D1 (FO3/FNV Use-Land-Data towns); EXT-D7-2026-10-27-01 (the seam line carries counts, not cells); #6089 (FO3 child-worldspace landscape inheritance).

## Suggested Fix
Make `terrain.seams` print the dirty pair coordinates. Then compare MegatonWorld's authored LAND against Wasteland's at those edges, to decide whether Use Land Data should source all cells from the parent rather than only LAND-less ones.

## Completeness Checks
- [ ] **SIBLING**: Same Use-Land-Data child-worldspace path checked on FO4 / FNV towns
- [ ] **TESTS**: `m-exteriors.sh fo3 boundary` green; a seam unit test pins the chosen landscape-source rule

