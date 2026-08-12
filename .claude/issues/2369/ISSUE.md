# Issue #2369 — EX-14/15: Stream ground cover, persistent refs, parent worlds, and FO4 spatial data

State: OPEN
Labels: enhancement, renderer, import-pipeline, medium, legacy-compat

Plan: EX-14 + EX-15.

## Problem
Exterior geometry can load, but a world is not visually or structurally complete
without streamed grass/regions/trees, correct persistent-ref ownership, parent
worlds, and FO4 precombine/previs/occlusion behavior.

## Acceptance
- GRAS/REGN placement and full SpeedTree/tree rendering replace billboard-only coverage where source data exists.
- Density is deterministic, spatially stable, streamed, unloaded, and deadline-budgeted.
- Persistent and temporary refs retain correct ownership across parent worldspaces and boundary crossings.
- FO4 precombine/previs/occlusion data has explicit render, collision, fallback, and mod-invalidation coverage.
- No double geometry when absorbed refs and precombined meshes overlap.
- Soak telemetry proves these owners unload cleanly.

Depends on near terrain, streaming telemetry, and ownership soak.
