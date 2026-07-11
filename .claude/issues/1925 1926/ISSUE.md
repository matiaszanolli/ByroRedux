# #1925: MAT-D6-02 — "scrap" classifier keyword is an unbounded substring match

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/core/src/ecs/components/material.rs:475` — `if contains_any_ci(path, &["scrap"]) { ... }`

## Description
The arm matched "scrap" anywhere in the path and forced a fully dielectric matte
result before the metal arm could run. Its intended target is FNV/FO3
`metalscrap*` painted-tin cladding. Genuine scrap-metal clutter (e.g. FNV/FO4
"Scrap Metal" texture naming) also contains the token and would be force-matte'd
even when conductive.

## Suggested Fix
Narrow to "metalscrap" (the actual cladding token) so genuine scrap-metal
clutter can still reach the metal arm.

---

# #1926: REN-D8-01 — composite fog fallback branch is dead code

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/shaders/composite.frag:510-549` (fog fallback);
`crates/renderer/src/vulkan/volumetrics.rs:154` (`VOLUMETRIC_OUTPUT_CONSUMED = true`);
`crates/renderer/src/vulkan/context/draw.rs:3510` (host mirror)

## Description
The composite aerial-perspective fog fallback branch is gated
`params.depth_params.x > 0.5 && depth < 0.9999 && params.depth_params.z < 0.5`.
Since `VOLUMETRIC_OUTPUT_CONSUMED` is `true`, `depth_params.z` is permanently
pinned to 1.0, so `depth_params.z < 0.5` is always false — the branch is dead.
The author's own comment says to drop it in lockstep with the flip; that
removal never happened.

## Suggested Fix
Remove the fog fallback branch; keep `fog_color`/`fog_params` in the UBO as
reserved-and-unconsumed for the future REGN-driven density-tint feature.
