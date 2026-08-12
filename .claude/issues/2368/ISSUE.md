# Issue #2368 — EX-01/05: Cross-game exterior smoke matrix and image-health gate

State: OPEN
Labels: enhancement, renderer, high

Plan: EX-01 + the diagnostic portion of EX-05 in docs/engine/exterior-readiness-plan.md.

## Problem
Exterior claims are spread across audits and prose. There is no one-command,
artifact-producing gate across Oblivion, FO3, FNV, Skyrim, and FO4, so empty
CELL selection, white-out, missing archives, and import drift can recur
silently.

## Acceptance
- One command runs every installed primary profile and self-skips absent game data.
- Each profile records exact WRLD/grid/radius, command line, bench summary, debug telemetry, and PNG.
- Hard gates cover confirmed exterior lighting, meaningful entity/draw floors, non-empty screenshot, blank/white-out detection, panic, and Vulkan validation errors.
- Texture misses and failed NIF paths are classified soft evidence until per-profile ceilings are calibrated.
- Add a renderer-side zero-nonfinite-pixel counter; PNG health alone cannot observe pre-tonemap NaN/Inf.
- Retain artifacts for before/after comparison and document reproducible invocation.

## Initial evidence
FO3 MegatonWorld (-1,-7) r1: 3,201 entities, 1,093 draw commands, no missing
textures, healthy image mean/stddev 0.2725/0.0547, 19 failed NIF cache paths
(mostly ambient FX/furniture markers).
