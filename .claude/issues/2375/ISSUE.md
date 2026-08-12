# #2375 EX-02/04: Exterior foreground readiness, safe spawn, and terrain-collision gate

State: OPEN
Labels: bug, ecs, high

Plan: EX-02 + EX-04.

Problem
A requested exterior CELL can exist but be an empty/dummy tile. FO3 MegatonWorld (0,0) has no LAND, references, or precombines; bootstrap previously returned a zero center and default Character mode free-fell indefinitely.

Acceptance
- Foreground readiness is a typed result, not an implicit Vec3::ZERO sentinel.
- Missing/empty/terrainless requested cells report CELL presence, LAND/ref/precombine counts, and deterministic nearest viable coordinates.
- Default Character mode starts only from a content-backed foreground and a verified walkable ground probe; otherwise use FlyCam or a clear error. Explicit --player may override with a warning.
- Capture terrain/static collider count plus initial ground-probe result in smoke telemetry.
- Oblivion, FO3, FNV, Skyrim, and FO4 are grounded at frame 0 and after at least one boundary crossing.
- Keep regression coverage for missing, empty, LAND-only, ref-only, and precombine-only CELLs.

Reproduction
Fallout3.esm --wrld MegatonWorld --grid 0,0 --radius 1. Nearest populated Megaton cells include (-1,-5), (0,-6), and (-1,-6); the curated visual profile remains (-1,-7).
