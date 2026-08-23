# #2371 — EX-10/11: Near-terrain correctness and complete distant LOD bands

State: OPEN
Labels: enhancement, renderer, high, legacy-compat, terrain-exterior

Plan: EX-10 + EX-11.

## Problem
Near LAND and a first LOD ring are wired, but exterior continuity is not
complete: the roadmap still calls out missing 8/16/32 bands and .btr
normals, and there is no cross-game crack/overlap gate.

## Acceptance
- Real-data guards cover LAND height, normal, vertex color, splat/material,
  holes, and collider/render agreement per game.
- Adjacent near cells have no geometric cracks or texture discontinuities.
- Select 4/8/16/32 terrain and object LOD bands with stable hysteresis.
- Wire .btr normal maps and VWD full-model culling.
- Define far-plane/reversed-Z policy for the full exterior scale.
- Automated captures detect near/LOD overlap, double draws, holes, and
  thrash while crossing boundaries.
- Calibrate on Oblivion Tamriel, FNV WastelandNV, Skyrim Tamriel, and FO4
  Commonwealth.

---

# #2369 — EX-14/15: Stream ground cover, persistent refs, parent worlds, and FO4 spatial data

State: OPEN
Labels: enhancement, renderer, import-pipeline, medium, legacy-compat, game:fo4, terrain-exterior

Plan: EX-14 + EX-15.

## Problem
Exterior geometry can load, but a world is not visually or structurally
complete without streamed grass/regions/trees, correct persistent-ref
ownership, parent worlds, and FO4 precombine/previs/occlusion behavior.

## Acceptance
- GRAS/REGN placement and full SpeedTree/tree rendering replace
  billboard-only coverage where source data exists.
- Density is deterministic, spatially stable, streamed, unloaded, and
  deadline-budgeted.
- Persistent and temporary refs retain correct ownership across parent
  worldspaces and boundary crossings.
- FO4 precombine/previs/occlusion data has explicit render, collision,
  fallback, and mod-invalidation coverage.
- No double geometry when absorbed refs and precombined meshes overlap.
- Soak telemetry proves these owners unload cleanly.

Depends on near terrain, streaming telemetry, and ownership soak.

---

# #2372 — EX-16: Integrate REGN, NAVM, ambient audio, and AI with exterior streaming

State: OPEN
Labels: enhancement, ecs, medium, legacy-compat, gameplay, ai, terrain-exterior, audio

Plan: EX-16.

## Problem
Exterior rendering is ahead of world simulation. Parsed region and navmesh
data are not yet a complete streamed runtime contract for ambient state,
actors, packages, and audio.

## Acceptance
- REGN drives ambient sound, fog/weather overlays, ground cover, and
  encounter metadata with deterministic priority.
- NAVM tiles load/unload with cells and preserve cross-cell path
  connectivity.
- Actors/packages suspend, migrate, and resume across stream boundaries
  without duplication or dangling entity references.
- Ambient audio emitters and regions crossfade and reclaim ownership on
  unload.
- Debug telemetry reports active REGN/NAVM/audio/AI owners per cell.
- Boundary and soak tests cover unload/reload while an actor path crosses
  a cell edge.

Depends on cancellation/ownership soak and ground-cover streaming;
coordinates with M42/M44 gameplay systems.
