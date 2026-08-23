# EX-16: Integrate REGN, NAVM, ambient audio, and AI with exterior streaming

**Repo**: matiaszanolli/ByroRedux
**Number**: #2372
**State**: OPEN
**Labels**: enhancement, ecs, medium, legacy-compat, gameplay, ai, terrain-exterior, audio

## Body

Plan: EX-16.

Problem
Exterior rendering is ahead of world simulation. Parsed region and navmesh data are not yet a complete streamed runtime contract for ambient state, actors, packages, and audio.

Acceptance
- REGN drives ambient sound, fog/weather overlays, ground cover, and encounter metadata with deterministic priority.
- NAVM tiles load/unload with cells and preserve cross-cell path connectivity.
- Actors/packages suspend, migrate, and resume across stream boundaries without duplication or dangling entity references.
- Ambient audio emitters and regions crossfade and reclaim ownership on unload.
- Debug telemetry reports active REGN/NAVM/audio/AI owners per cell.
- Boundary and soak tests cover unload/reload while an actor path crosses a cell edge.

Depends on cancellation/ownership soak and ground-cover streaming; coordinates with M42/M44 gameplay systems.
