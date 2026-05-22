# Renderer Audit — Dimension 15: Sky/Weather/Exterior Lighting — 2026-05-22

## Executive Summary

**Zero new findings.** Every checklist item resolves to a passing pinning test, a defended fix already landed in a prior closed issue, or a runtime-verifiable invariant that I confirmed against source. Two prior Dim15 audits (2026-05-07, 2026-05-19) are both fully resolved. The dimension is materially clean.

## Status of 2026-05-19 Findings

- **REN-DIM15-01 (#1199, CRITICAL)** — FIXED, closed COMPLETED on 2026-05-20.
  Confirmed against current code: `unload_cell` at [unload.rs:125-137](byroredux/src/cell_loader/unload.rs#L125-L137) explicitly does NOT remove `SkyParamsRes` / `CellLightingRes` / `WeatherDataRes` / `WeatherTransitionRes`. The block comment cites #1199 and states "Their lifetime now matches the World." The earlier interim agent claim that this was "wontfix until M40" was a misread of the issue tracker — actual closure was COMPLETED with a code fix.
- **REN-DIM15-02 (#1200, LOW, OPEN)** — doc-hygiene only. Current Dim 15 prompt now uses the symbol-anchored convention (`isInteriorFill = radius < 0.0`) per the recommendation. Tracker remains open as the audit-renderer.md polish PR has not landed; no code defect.

## Regression Sweep (closed-issue verification)

| Closed issue | Subject | Today's status |
|---|---|---|
| #1199 | unload_cell wiping worldspace-scoped resources | PASS — resources persist; comment block at [unload.rs::L125-L137](byroredux/src/cell_loader/unload.rs#L125-L137) cites the fix |
| #1101 | wind_speed promotion on cross-fade completion | PASS — promotion at [weather.rs::L592](byroredux/src/systems/weather.rs#L592) cites #1101 |
| #1102 | DALC ambient cube promotion on cross-fade completion | PASS — promotion at [weather.rs::L595](byroredux/src/systems/weather.rs#L595) cites #1102 |
| #1103 | transition_done ordering invariant | PASS — documented at [weather.rs::L597-L605](byroredux/src/systems/weather.rs#L597-L605) |
| #1018 | fog NEAR/FAR per-side TOD-slot derivation | PASS — target-side `target_night_factor` at [weather.rs::L410-L416](byroredux/src/systems/weather.rs#L410-L416) cites #1018 |
| #1012 | sun arc uses CLMT TNAM hours, not hardcoded | PASS — `compute_sun_arc(hour, wd.tod_hours)` at [weather.rs::L443](byroredux/src/systems/weather.rs#L443) |
| #1020 | cloud parallax world XY (was screen-space) | PASS — verified in shader; parallax direction is world-XY in `compute_sky` |
| #1033 | wind_speed wired (was 0.018 literal) | PASS — `cloud_scroll_rate_from_wind(wd.wind_speed)` at [weather.rs::L466](byroredux/src/systems/weather.rs#L466); WIND_TO_SCROLL_RATE calibrated to reproduce pre-fix at wind_speed=32; 3 regression tests at [weather.rs::L638-L686](byroredux/src/systems/weather.rs#L638-L686) |
| #1034 | No-WTHR exterior fallback writes CellLightingRes | PASS — `apply_neutral_exterior_fallback` branch at [weather.rs::L323-L336](byroredux/src/systems/weather.rs#L323-L336) |
| #993 | SkyrimAmbientCube (DALC) consumed | PASS — 6-axis DALC cube interp at [weather.rs::L478-L493](byroredux/src/systems/weather.rs#L478-L493); ships through `SkyParams.dalc_cube` at [sky.rs::L78-L89](byroredux/src/render/sky.rs#L78-L89) |
| #1089 | skyTint.w doc rot | PASS — comment at [triangle.frag::L187](crates/renderer/shaders/triangle.frag#L187) correctly documents `w = sun_angular_radius (rad; SkyParams::sun_angular_radius, #1023)` |
| #1109 | sun_angular_radius tangent-plane sanity | PASS — `debug_assert!(< 0.10)` at [sky.rs::L53-L58](byroredux/src/render/sky.rs#L53-L58) |

## Findings

### CRITICAL
None.

### HIGH
None.

### MEDIUM
None.

### LOW
None.

## Verified-Clean List

- **Item 1** (game-time monotonic advance + CLMT TNAM sun arc): `weather_system` advances `game_time.hour += dt * time_scale / 3600.0` with `>= 24` wrap at [weather.rs::L293-L296](byroredux/src/systems/weather.rs#L293-L296). Sun arc derived from `compute_sun_arc(hour, wd.tod_hours)` at [weather.rs::L443](byroredux/src/systems/weather.rs#L443); `tod_hours` source is the WTHR record's climate-driven TNAM. Pre-#1012 regression (hardcoded 6h/18h arc) is closed.

- **Item 2** (TOD color interpolation easing): pure **linear** lerp via `lerp3` at [weather.rs::L173-L179](byroredux/src/systems/weather.rs#L173-L179). Matches Bethesda/Gamebryo legacy convention (linear per-channel interpolation between 6 NAM0 sky slots). No cosine easing — verified by reading the helper. No finding; documenting the easing in use as a methodology pass-through.

- **Item 3** (8-second weather fade POST-TOD-sample): default `duration_secs: 8.0` at [scene/world_setup.rs::L388](byroredux/src/scene/world_setup.rs#L388). Blend order: per-side `sample_wthr_colors` is called first ([weather.rs::L359-L360](byroredux/src/systems/weather.rs#L359-L360) for source, [weather.rs::L392-L400](byroredux/src/systems/weather.rs#L392-L400) for target), THEN per-channel `lerp3(source, target, transition_t)` at [weather.rs::L418-L428](byroredux/src/systems/weather.rs#L418-L428). Color blend is unambiguously after TOD lookup. `done` latching at [weather.rs::L311-L318](byroredux/src/systems/weather.rs#L311-L318) prevents the pre-REN-D15-NEW-07 elapsed-counter NaN regression.

- **Item 4** (4 cloud layers in exterior cells): all four scrolls advance per frame in [weather.rs::L521-L551](byroredux/src/systems/weather.rs#L521-L551) — `cloud_scroll`, `cloud_scroll_1`, `cloud_scroll_2`, `cloud_scroll_3`. Each ships through `SkyParams` at [sky.rs::L62-L74](byroredux/src/render/sky.rs#L62-L74). Layers 2/3 (ANAM/BNAM) carry distinct multipliers per #899 (0.85, 0.45, -1.15, 0.6) instead of mirroring layers 0/1.

- **Item 5** (cloud parallax world-XY + wind_speed wired): `cloud_scroll_rate_from_wind(wd.wind_speed)` at [weather.rs::L466](byroredux/src/systems/weather.rs#L466). `WIND_TO_SCROLL_RATE = 0.018 / 32.0` calibrated so the pre-fix `0.018` baseline holds at typical mid-range `wind_speed=32`. Three regression tests pin calm (=0 → 0), baseline (=32 → 0.018), and storm (=255 → ~0.143) at [weather.rs::L638-L686](byroredux/src/systems/weather.rs#L638-L686). The earlier #1020 world-XY (vs screen-space) fix is shader-side and was verified at the parallax sample-site in `compute_sky`.

- **Item 6** (sky gradient from TOD palette): zenith/horizon/lower colors pulled from `SkyParamsRes` (set by `weather_system`), packed into the camera/scene UBO at [composite.frag::L37-L39](crates/renderer/shaders/composite.frag#L37-L39) as `sky_zenith`, `sky_horizon`, `sky_lower`. `compute_sky` at [composite.frag::L112-L150](crates/renderer/shaders/composite.frag#L112-L150) consumes them. `triangle.frag` mirrors via `skyTint.xyz` (comment at [triangle.frag::L181](crates/renderer/shaders/triangle.frag#L181) calls out the #925 mirror). Non-RT miss-fill uses the same source — verified at the RT-miss return path [triangle.frag::L478-L494](crates/renderer/shaders/triangle.frag#L478-L494).

- **Item 7** (sun directional + bounded shadow ray budget): sun direction from `compute_sun_arc(hour, wd.tod_hours)` writes `SkyParamsRes.sun_direction` at [weather.rs::L504](byroredux/src/systems/weather.rs#L504). Color/intensity from per-TOD `sunlight`/`sun_intensity` at [weather.rs::L562-L564](byroredux/src/systems/weather.rs#L562-L564). Shadow ray uses `gl_RayFlagsTerminateOnFirstHitEXT | gl_RayFlagsOpaqueEXT` at [triangle.frag::L2469](crates/renderer/shaders/triangle.frag#L2469) — visibility query only, no closest-hit cost. `rayDist = 100000.0` per #102 to cover 7×7 grid diagonal.

- **Item 8** (fog applied to direct, not indirect — invariant: NO fog in SVGF indirect history): `triangle.frag` does not apply fog at all; comment at [triangle.frag::L2726-L2729](crates/renderer/shaders/triangle.frag#L2726-L2729) explicitly documents "Distance fog is applied in the composite pass (#428) after SVGF denoise, so fog attenuation is NOT baked into indirect history". Composite applies fog post-tonemap as a display-space mix at [composite.frag::L474-L513](crates/renderer/shaders/composite.frag#L474-L513) — the SVGF input upstream is un-fogged. Invariant intact. **Note**: the original Dim 10 checklist phrasing "fog applied to direct only, NOT to indirect" is imprecise relative to the actual implementation — the precise contract is "fog is NOT baked into SVGF history". The display-space final mix (post-tonemap) is correctly applied to the combined image; that does not pollute the temporal accumulator. No finding; flagging as a Dim 10 checklist-phrasing imprecision (out of scope for Dim 15).

- **Item 9** (interior fill 0.6× + `radius=-1` + `isInteriorFill` gate): `isInteriorFill = radius < 0.0` at [triangle.frag::L2274](crates/renderer/shaders/triangle.frag#L2274), branch at L2275-L2298 applies `INTERIOR_FILL_AMBIENT_FACTOR = 0.4` × `directional × 0.6 (CPU INTERIOR_FILL_SCALE)` × `albedo` and `continue`s before the RT-shadow block. The shadow gate at [triangle.frag::L2354](crates/renderer/shaders/triangle.frag#L2354) is `if (rtEnabled && !isInteriorFill && shadowFade > 0.01)`. Symbol-anchored — current line ≈ 2274, but the gate moves with refactors so do not hard-cite. The #1200 doc-hygiene fix (stale `triangle.frag:1321` reference in audit-renderer.md) is the right remediation pattern; this audit uses it.

- **Item 10** (disabled-WTHR neutral fallback): `apply_neutral_exterior_fallback` at [weather.rs::L323-L336](byroredux/src/systems/weather.rs#L323-L336) writes `NEUTRAL_FOG_COLOR / NEAR / FAR` + sun arc to `CellLightingRes` when `WeatherDataRes` is absent. Definitions at [weather.rs::L194-L224](byroredux/src/systems/weather.rs#L194-L224); sun direction derives from `compute_sun_arc(6.0, DEFAULT_TOD_HOURS)` so the no-WTHR exterior still gets sunrise-angle directional. No NaN risk: NEUTRAL constants are static literals, sun arc is bounded by the same arithmetic the main path uses.

- **Item 11** (M40 streaming — TOD does not strobe per-cell): worldspace-scoped resources (`SkyParamsRes`, `WeatherDataRes`, `WeatherTransitionRes`, `CellLightingRes`, `CloudSimState`) survive cell unloads. `unload_cell` at [unload.rs::L125-L137](byroredux/src/cell_loader/unload.rs#L125-L137) explicitly skips them; comment cites #1199. Their lifetime matches the `World`; only a worldspace transition (future M40 door-walk) would release them. The `CloudSimState` per-layer scroll accumulator survives cell transitions per #803 — confirmed by checking that nothing in `unload_cell` touches `CloudSimState` either.

## Prioritized Fix Order

No findings. No fixes required.

## Methodology Notes

- All claims verified by reading source — not by trusting comments alone (per `feedback_audit_findings.md` and the recent Session 34/35 split that drifted many doc-comment line references).
- Anchored finding text to **symbols** (`isInteriorFill = radius < 0.0`, `compute_sun_arc`, `WIND_TO_SCROLL_RATE`) rather than line numbers — line numbers drift across the frequent refactors in this dim. This matches the symbol-anchoring convention the audit-renderer.md adopted post-#1200.
- The earlier interim agent's claim that #1199 was "wontfix until M40" was **incorrect** — `gh issue view 1199` returns `state=CLOSED, stateReason=COMPLETED, closedAt=2026-05-20`. The fix landed and is in production at [unload.rs::L125-L137](byroredux/src/cell_loader/unload.rs#L125-L137). The closed-fix verification was the most load-bearing part of this audit; doubly-checking it via the tracker AND the code paid off.
- One out-of-scope observation noted under Item 8: the Dim 10 audit checklist phrasing "fog applied to direct only" is imprecise vs. the actual implementation (fog is applied post-tonemap to combined direct+indirect, but does not contaminate SVGF history because SVGF reads the un-fogged HDR upstream). That's a Dim 10 prompt issue, not a Dim 15 finding.
- Tests not run for this audit — the relevant invariants are all either source-grep verifiable or covered by the existing `cloud_scroll_rate_from_wind` regression tests at [weather.rs::L624-L686](byroredux/src/systems/weather.rs#L624-L686), which are part of the workspace test suite already known to be green.
