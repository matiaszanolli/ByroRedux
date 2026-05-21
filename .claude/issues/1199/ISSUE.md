# #1199 — REN-DIM15-01: unload_cell wipes worldspace-scoped weather/sky/lighting resources

**Source**: docs/audits/AUDIT_RENDERER_2026-05-19_DIM15.md (Dim 15, CRITICAL)
**Severity**: critical
**Labels**: bug, critical, renderer
**State**: OPEN (filed 2026-05-19)

## Cause

`byroredux/src/cell_loader/unload.rs:142-145` removes 4 worldspace-scoped resources on every cell unload:
- `SkyParamsRes`
- `CellLightingRes`
- `WeatherDataRes`
- `WeatherTransitionRes`

No subsequent cell-load re-inserts them. `apply_worldspace_weather` is called exactly once at scene.rs:226 (streaming bootstrap). After the first M40 cell-out-of-range event, `weather_system` early-returns and exterior lighting freezes.

## Fix (Strategy 1, recommended)

Drop the 4 `remove_resource` calls at unload.rs:142-145. Keep the texture-handle drops at 134-138 (those are correct, load-bearing for #626). Resources are worldspace-scoped; `World` Drop handles them at process exit.

Mirrors #803 / STRM-N2 pattern (`CloudSimState` already lifted out of unload).

## Risk

LOW for Strategy 1. Texture refcounts still released via the texture_drops path. No leak.

## Estimated impact

HIGH. Every exterior bench past first cell-boundary crossing renders with frozen lighting today. Multiple downstream invariants (Dim 9/10/18/20) all depend on the wiped resources.

## Testability

1. 2-cell integration test (load A+B, unload A, assert all 4 resources still present). Polarity-flip `cloud_sim_state_survives_sky_params_unload_reload`.
2. Bench: `--esm FalloutNV.esm --grid 0,0 --radius 3 --bench-frames 600`. Dump `cell_lit.directional_color` per frame; pre-fix freezes around frame ~200-300, post-fix advances across full 600.
