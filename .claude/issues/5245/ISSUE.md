# #5245 — WATAL W3-01: flowing water sweeps cross-current — weather transport composes at full wind rate over the flow term (plus #4929's 44x Rapids arm)

https://github.com/matiaszanolli/ByroRedux/issues/5245

Source: live user report + White River measurement 2026-10-04. Full body on the issue.

## Resolution (2026-10-04)

`WaterKind::FLOWING_WATER_WEATHER_TRANSPORT = 0.35` damps atmospheric transport on River/Rapids/Waterfall in BOTH the renderer upload and the CPU crest sampler (#3207 coherence); chop amplitude stays full. The Rapids third layer consumes the CPU-baked `normalScrollC` (#4929). Live A/B on the White River: cross-screen texture motion ~30 px/s -> ~8 px/s with along-flow motion preserved. Gates: m-exteriors fnv/skyrim water green; pins `flowing_kinds_damp_the_weather_transport_but_calm_keeps_it` + `water_rapids_third_layer_uses_world_xz_flow` (updated) + `rapids_third_layer_uses_the_authored_scroll_rate`.
**Live report (user, 2026-10-04):** "Water texture movement speed is way too aggressive when waving sideways in rivers" / "Rivers and rapids have more sideways than straight water movement."

- **Severity**: MEDIUM (visual motion quality on every flowing surface in wind)
- **Dimension**: W3 shading/motion tuning — surface-transport composition.
- **Location**:
  - `crates/core/src/ecs/components/water.rs` — `WEATHER_SCROLL_PER_BU_PER_S` (0.0015) applies the atmospheric wind at full rate to every kind's normal-layer transport.
  - `byroredux/src/render/water.rs` — the per-layer weather composition (1.0 / 0.65 / 0.45).
  - `crates/renderer/shaders/water.frag` — the Rapids third-layer arm (`push.flow.xz * push.flow.w * 2.0`), open as #4929.
  - `byroredux/src/systems/water.rs` + `crates/physics/src/water.rs` — the CPU crest sampler composing the same weather scroll (#3207 coherence).
- **Measured live** (Skyrim White River fixture, grid (4,-11), `RiverWaterFlowNE`, kind=River, flow `[0.883, 0, −0.469] @ 2.876` BU/s): the weather term at a 200 BU/s wind scrolls up to **0.33 UV/s in the wind direction** against a **~0.13 UV/s downstream** flow term — the surface visibly sweeps cross-current several times faster than it flows. Cross-correlated `water_normal` captures show ~30 px/s of texture motion with dominant cross-screen components. On Rapids the #4929 arm additionally strobes at raw BU/s×2 (44× the translate's intent), leaving the wind scroll as the only coherent motion.
- **Fix**:
  - `WaterKind::FLOWING_WATER_WEATHER_TRANSPORT = 0.35` (core, beside the other kind-owned presentation anchors): flowing kinds (River/Rapids/Waterfall) damp the atmospheric *transport* to 35%; the wind keeps its full chop *amplitude* (`wind_wave_scale`). Applied identically in the renderer upload and the CPU crest sampler so #3207 holds.
  - #4929's arm: Rapids third layer consumes `normalScrollC` (the CPU-baked flow-biased authored rate).
- **Guards**: `flowing_kinds_damp_the_weather_transport_but_calm_keeps_it` (behavior, all three layers + amplitude-full pin) and `rapids_third_layer_uses_the_authored_scroll_rate` (GLSL source shape).

## Completeness Checks
- [x] **TESTS**: behavior + GLSL pins as above
- [x] **LIVE**: White River two-shot cross-correlation re-measured post-fix; `m-exteriors.sh` water gates green
- [x] **DOCS**: the constant's doc carries the measurement; watal.md §2 motion paragraph updated
