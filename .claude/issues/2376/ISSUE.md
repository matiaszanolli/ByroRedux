# EX-06/07: Exterior boundary benchmark and deadline-bounded streaming

**Repo**: matiaszanolli/ByroRedux
**Number**: #2376
**State**: OPEN
**Labels**: ecs, renderer, high, performance, terrain-exterior

## Body

Plan: EX-06 + EX-07.

Problem
Initial-radius foreground-first streaming exists, but there is no deterministic boundary-crossing benchmark. Remaining terrain/water/precombine setup and some mesh/upload/LOD operations are atomic, while LOD uses attempt counts rather than a measured wall-clock deadline.

Acceptance
- Deterministic camera/player path crosses at least two cell boundaries.
- Emit queue wait, worker parse, main-thread apply, unload, LOD, and whole-frame p50/p95/max timings per cell.
- Every NIF finalization, ordinary static placement, terrain/water/precombine phase, texture/mesh upload, BLAS build, and LOD provider yields under one shared measured deadline.
- Budget by elapsed time plus byte/mesh batches, not attempt count alone.
- Detect frame hitches above the agreed threshold and name the largest atomic unit.
- Run at minimum on FNV WastelandNV and one newer-engine worldspace (Skyrim Tamriel or FO4 Commonwealth).
