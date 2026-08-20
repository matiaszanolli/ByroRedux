# #2371: EX-10/11: Near-terrain correctness and complete distant LOD bands

**Labels**: enhancement, renderer, high, legacy-compat
**State**: OPEN

## Plan
EX-10 + EX-11.

## Problem
Near LAND and a first LOD ring are wired, but exterior continuity is not complete: the roadmap still calls out missing 8/16/32 bands and .btr normals, and there is no cross-game crack/overlap gate.

## Acceptance
- Real-data guards cover LAND height, normal, vertex color, splat/material, holes, and collider/render agreement per game.
- Adjacent near cells have no geometric cracks or texture discontinuities.
- Select 4/8/16/32 terrain and object LOD bands with stable hysteresis.
- Wire .btr normal maps and VWD full-model culling.
- Define far-plane/reversed-Z policy for the full exterior scale.
- Automated captures detect near/LOD overlap, double draws, holes, and thrash while crossing boundaries.
- Calibrate on Oblivion Tamriel, FNV WastelandNV, Skyrim Tamriel, and FO4 Commonwealth.
