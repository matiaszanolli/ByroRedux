# Renderer Audit — 2026-05-19 (Dim 15 focused)

**Scope**: Dimension 15 — Sky / Weather / Exterior Lighting (M33 / M33.1 / M34)
**Mode**: `--focus 15 --depth deep`
**Companion**: this morning's full 20-dim sweep at [AUDIT_RENDERER_2026-05-19.md](AUDIT_RENDERER_2026-05-19.md) covered Dim 15 only at the surface (noted #924 close, deferred bench verification). This is the deep follow-up.

## Executive Summary

**1 CRITICAL, 1 LOW, 4 INFO** verifications.

| Severity | Count |
|----------|-------|
| CRITICAL | 1 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 1 |
| INFO | 4 |

### Headline

**REN-DIM15-01 (CRITICAL)**: `unload_cell` removes worldspace-scoped weather/sky/lighting resources that no subsequent cell-load re-inserts. After the first M40-streaming cell-out-of-range event (typical: ~30–60 seconds of exterior walking), the entire exterior loses its TOD palette, sun arc, cloud scroll, fog distances, and ambient/directional lighting. `weather_system` early-returns from then on. This is almost certainly what the morning audit's deferred "Verify next bench cycle" would have surfaced.

The cell-load path was refactored under M40 to use `load_one_exterior_cell` for streaming, but the cell-unload path was not updated to recognise that `WeatherDataRes`/`SkyParamsRes`/`CellLightingRes`/`WeatherTransitionRes` are worldspace-scoped, not cell-scoped. An inline comment at [unload.rs:139-141](byroredux/src/cell_loader/unload.rs#L139-L141) encodes the original author's intent ("get replaced on the next `world.insert_resource` at cell load") — but no such re-insert exists in any cell-load entry point.

### Dedup status

- No open GitHub issue tracks the M40-streaming weather-resource wipe.
- Closest prior context: #803 / STRM-N2 lifted `CloudSimState` onto a separate, never-removed resource. That fix is the architectural template — the same pattern should apply to the four resources `unload_cell` is currently removing.
- Sole regression test in the area ([sky_params_cleanup_tests.rs:75-119](byroredux/src/cell_loader/sky_params_cleanup_tests.rs#L75-L119)) *manually* simulates the re-insert that production code does not perform — the test passes; production is broken.

---

## Rasterization & RT Pipeline Assessment

This dim is about ECS-resource lifetime upstream of the renderer, not about pipeline state, AS correctness, or shader math. Pipeline plumbing itself is unaffected: the UBO upload happily writes whatever `CellLightingRes` / `SkyParamsRes` carry. The problem is upstream — what *gets* uploaded after a cell unload is the prior frame's stale state (or, on a fresh UBO, zeros).

No new sync, AS, or shader findings in this pass.

---

## Findings

### CRITICAL

#### REN-DIM15-01: `unload_cell` removes worldspace-scoped weather/sky/lighting resources that no subsequent cell-load re-inserts

- **Dimension**: Sky / Weather / Exterior Lighting
- **Files**:
  - [byroredux/src/cell_loader/unload.rs:142-145](byroredux/src/cell_loader/unload.rs#L142-L145) — the remove sequence
  - [byroredux/src/cell_loader/exterior.rs:210](byroredux/src/cell_loader/exterior.rs#L210) — `load_one_exterior_cell` (no companion re-insert)
  - [byroredux/src/scene/world_setup.rs:191](byroredux/src/scene/world_setup.rs#L191) — `apply_worldspace_weather`, sole inserter
  - [byroredux/src/scene.rs:226](byroredux/src/scene.rs#L226) — sole caller, in streaming bootstrap
  - [byroredux/src/main.rs:758](byroredux/src/main.rs#L758) — streaming `unload_cell` call site
  - [byroredux/src/systems/weather.rs:323-337](byroredux/src/systems/weather.rs#L323-L337) — early-return on missing `WeatherDataRes`

- **Symptom**: After the player walks far enough that any exterior cell falls out of the streaming unload radius, the first `unload_cell` call removes `SkyParamsRes`, `CellLightingRes`, `WeatherDataRes`, and `WeatherTransitionRes`. No subsequent cell-load re-inserts them. `weather_system` early-returns at weather.rs:336 because `WeatherDataRes` is missing. Visual effect: exterior freezes its lighting once the player crosses the first cell boundary, and never recovers within the same session.

- **Cause**: Three facts collide:
  1. `unload_cell` (lines 142-145) unconditionally removes four worldspace-scoped resources, with a comment claiming they "get replaced on the next `world.insert_resource` at cell load".
  2. `load_one_exterior_cell` spawns only geometry (LAND, water, REFRs). It never touches these resources. Grep across `byroredux/src/cell_loader/` confirms zero `insert_resource(WeatherDataRes…)` or `apply_worldspace_weather(…)` call sites.
  3. `apply_worldspace_weather` is called exactly once, at scene.rs:226, during initial streaming bootstrap.

  The fallback at weather.rs:333-336 (`apply_neutral_exterior_fallback` writing into `CellLightingRes` when `WeatherDataRes` is missing) is also disabled, because `unload_cell` removes `CellLightingRes` at line 143 — there's nothing to mutate.

- **Fix** (Strategy 1, recommended): Stop removing these resources in `unload_cell`. They are worldspace-scoped, not cell-scoped. The texture-handle refcount drops at unload.rs:134-138 are correct (they consume `SkyParamsRes::texture_indices()` and feed the per-cell drop list) — leave those in place. But the `remove_resource` calls at 142-145 should go away. Process Drop will clean up at exit. Mirrors the #803/STRM-N2 pattern that already lifted `CloudSimState` out of the cell-unload remove path.

- **Fix** (Strategy 2, alternative): Conditionalise the removes — only fire when `state.loaded` is empty (player has left the worldspace, e.g. through a door to an interior). Higher risk: the "is this the last cell" check needs to be correct in both directions.

- **Estimated Impact**: High. Every M40 exterior bench past the first cell-boundary crossing renders with frozen lighting today. Likely the cause of the "verify next bench cycle" deferral in this morning's audit.

- **Regression Risk**: LOW for Strategy 1.
  - Texture-handle refcounts on `SkyParamsRes::texture_indices()` are still released via the texture_drops path at unload.rs:134-138 — those run regardless of whether the resource itself is removed.
  - The four resources are owned by `World`; they get dropped when World drops at process exit. No leak.
  - The "between-load query doesn't see stale state" concern from the original comment is moot: production code only queries these resources from `weather_system` and the renderer UBO upload, both of which actively replace the data each frame.

- **Testability**:
  - Add a 2-cell integration test (load A + B, unload A, assert all four resources still present). Flip the polarity of `cloud_sim_state_survives_sky_params_unload_reload` as a template.
  - Bench: `--esm FalloutNV.esm --grid 0,0 --radius 3 --bench-frames 600 --bench-hold`. Dump `cell_lit.directional_color` and `sky.sun_direction` per frame. Pre-fix: values freeze after the first cell unload event around frame ~200–300. Post-fix: TOD-driven sun arc advances across the full 600 frames.

- **Completeness Checks**:
  - [ ] **UNSAFE**: N/A — ECS resource-lifetime change, no unsafe surface
  - [ ] **SIBLING**: verify no other cell-unload variant (interior unload? master-load path?) replicates the same remove pattern
  - [ ] **DROP**: confirm `World` Drop still tears down the four resources (it does — they're standard `Resource` impls)
  - [ ] **LOCK_ORDER**: N/A
  - [ ] **FFI**: N/A
  - [ ] **TESTS**: regression test asserts 2-cell unload preserves all four resources; bench-side assertion that sun arc advances across cell-boundary crossings

---

### LOW

#### REN-DIM15-02: audit-checklist line reference `triangle.frag:1321` for the `radius=-1` interior-fill gate is stale

- **File**: [crates/renderer/shaders/triangle.frag:2228](crates/renderer/shaders/triangle.frag#L2228) (actual gate location)

- **Symptom**: The Dim-15 and Dim-20 audit checklists cite `triangle.frag:1321` as the interior-fill gate. Actual location today is line **2228** (`bool isInteriorFill = radius < 0.0;`), with the documentation comment at line 2205 and the cone-sample bypass at lines 2302/2306.

- **Cause**: Shader has grown significantly since the checklist was written. The gate exists and works correctly — pure documentation drift in the audit prompts.

- **Fix**: Update `.claude/commands/audit-renderer.md` Dim-15 and Dim-20 checklist text to reference the symbol `isInteriorFill` rather than a line number. The file is volatile; line numbers will keep drifting.

- **Estimated Impact**: 0 — no runtime effect. Drift risks future audits declaring findings STALE/UNVERIFIABLE when the code is fine and just moved.

- **Regression Risk**: N/A.

- **Testability**: `grep -n "isInteriorFill\|radius < 0" crates/renderer/shaders/triangle.frag`.

---

### INFO (verification pass-throughs)

#### REN-DIM15-03: `weather_system` game-time advance verified

- [byroredux/src/systems/weather.rs:287-298](byroredux/src/systems/weather.rs#L287-L298) — `game_time.hour += dt * time_scale / 3600.0`, monotonic with 24h wrap. Sun arc from CLMT TNAM via `climate_tod_hours` (world_setup.rs:351); not hardcoded.

#### REN-DIM15-04: WTHR fade-after-TOD-sample order verified

- [byroredux/src/systems/weather.rs:300-321](byroredux/src/systems/weather.rs#L300-L321) — `WeatherTransitionRes` timer advance with `done` latch (per #REN-D15-NEW-07, prevents elapsed-counter saturation to INF). Blend ratio with explicit `clamp(0.0, 1.0)`. Target snapshot applied AFTER the per-frame TOD sample.

#### REN-DIM15-05: Disabled-WTHR fallback (procedural Mojave) verified

- [byroredux/src/scene/world_setup.rs:424-525](byroredux/src/scene/world_setup.rs#L424-L525) — `insert_procedural_fallback_resources` inserts `CellLightingRes` + `SkyParamsRes` + synthetic `WeatherDataRes` + `GameTimeRes`. Pre-#542 the fallback omitted `GameTimeRes` and `WeatherDataRes`; that fix is in place. The synthetic NAM0 table fills the 7 groups `weather_system` reads (including `SKY_LOWER` per #541). No NaN, no pitch-black.
- Cross-ref REN-DIM15-01: this fallback is ALSO wiped by `unload_cell`. Strategy 1 protects this path too.

#### REN-DIM15-06: `CloudSimState` correctly survives cell unloads (#803 / STRM-N2)

- [byroredux/src/scene/world_setup.rs:340-342](byroredux/src/scene/world_setup.rs#L340-L342) — inserted only when not present; [unload.rs](byroredux/src/cell_loader/unload.rs) has no remove call for it. Cloud scroll persists across cell transitions correctly.
- This is the exact architectural pattern REN-DIM15-01 recommends for the four currently-mis-scoped resources.

---

## Cross-References

- **Dim 10** (Denoiser & Composite): fog-applied-to-direct-only invariant from #924 is downstream of `CellLightingRes.fog_*`. Once REN-DIM15-01 is fixed, that invariant becomes meaningful again across cell boundaries; today it's moot for streaming-exterior-only sessions because fog data is wiped on first unload.
- **Dim 18** (Volumetrics M55): scattering reads from TOD palette via `SkyParamsRes`. Same wipe applies post-first-unload — frozen output even when volumetrics is on.
- **Dim 20** (M-LIGHT v1 soft shadows): `sun_angular_radius` lives on `SkyParamsRes`. Once wiped, the cone sample reads stale UBO state. Soft shadows freeze with the sun direction.
- **Dim 9** (RT GI miss sky fill): the miss-fill reads zenith/horizon from `SkyParamsRes`. Same wipe path.

## Prioritized Fix Order

### Correctness (CRITICAL)

1. **REN-DIM15-01** — apply Strategy 1: drop the 4 `remove_resource` calls at unload.rs:142-145. Keep the texture-handle drops at 134-138. Add the regression test as a 2-cell integration test.

### Documentation hygiene (LOW)

2. **REN-DIM15-02** — update `.claude/commands/audit-renderer.md` checklist text in Dim-15 and Dim-20 to reference `isInteriorFill` rather than a stale line number.

### Deferred

The Dim-15 checklist items that did NOT get deep-traced in this pass (item 2 TOD interpolation easing; item 5 cloud parallax world-XY vs screen-space) should be revisited after REN-DIM15-01 lands and the streaming bench can validate downstream visual invariants. They are currently masked by the upstream wipe.

---

## Notes

- No alloc-hot-path findings; the dhat-infra gap is not load-bearing here.
- No speculative Vulkan barrier changes proposed. REN-DIM15-01's fix is pure ECS resource-lifetime; no GPU sync surface touched.
- The Dim-15 agent ran out of tool budget mid-investigation and did not write the dim_15.md file directly. The investigation continued in-thread with direct file reads; the agent's pre-truncation summary identified the correct CRITICAL site (`unload_cell` wipes worldspace resources) and the verification followed it through to confirmation in the load + apply paths.
