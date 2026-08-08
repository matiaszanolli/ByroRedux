# REN-D18-NEW-02: In-flight WeatherTransitionRes is never collapsed or cleared on a worldspace change

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2511
**Finding ID**: REN-D18-NEW-02 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 18 — Sky/Weather
**Location**: `byroredux/src/scene/world_setup.rs::apply_worldspace_weather` / `insert_procedural_fallback_resources`; consumed in `byroredux/src/systems/weather.rs::weather_system`
**Status**: NEW

## Description
`WeatherTransitionRes` is a one-shot state machine (`elapsed_secs`, `duration_secs: 8.0`, `done`) that blends the live `WeatherDataRes` toward `target` and, on completion, promotes `target` into `WeatherDataRes`. Nothing ever removes it — `cell_loader/unload.rs` explicitly documents that worldspace-scoped weather resources are *not* released on cell unload (#1199), and the only writers are the single `insert_resource` in `apply_worldspace_weather` and the `done = true` latch in `weather_system`. Two paths mishandle a transition that is still in flight when a second worldspace change lands: (1) **WTHR branch retarget** — `insert_resource(WeatherTransitionRes { target: new_weather, elapsed_secs: 0.0, .. })` overwrites the in-flight transition while leaving `WeatherDataRes` at the *original* source snapshot, so the frame of the switch pops backwards by `t * (oldTarget - src)`. (2) **Procedural-fallback branch** — `insert_procedural_fallback_resources` replaces `WeatherDataRes` with `procedural_fallback_weather()` but leaves the in-flight transition installed; `weather_system` keeps blending the procedural sky toward the *previous worldspace's* target and, on completion, promotes that target's weather over the procedural fallback — a climateless worldspace ends up permanently rendering the prior worldspace's weather.

## Evidence
```rust
// world_setup.rs::apply_worldspace_weather — WTHR branch
if world.try_resource::<WeatherDataRes>().is_some() {
    world.insert_resource(WeatherTransitionRes {
        target: new_weather, elapsed_secs: 0.0, duration_secs: 8.0, done: false,
    });                       // <- clobbers an in-flight fade; WeatherDataRes still holds the old source
} else { ... }
```
Reachability: `app_step.rs:542` calls `apply_worldspace_weather` on every exterior-destination transition, plus `scene.rs:414` and `debug_load.rs:394`. Two exterior door transitions inside the 8-second window are enough. `grep -rn "WeatherTransitionRes"` confirms no `remove_resource` call site anywhere in the tree.

## Impact
Case 1 is a one-frame colour pop, self-healing within 8s — cosmetic. Case 2 is persistent wrong weather (palette, fog distances, wind-driven cloud scroll, DALC cube) on a climateless worldspace until the next worldspace change. Both require two worldspace transitions within 8 seconds; case 2 additionally requires the second worldspace to have no CLMT/default WTHR, so vanilla content is effectively immune. No crash, no NaN.

## Related
Extends the M33.1 crossfade state machine hardened by #1101 / #1102 / #1103 / REN-D15-NEW-07, none of which addressed transition *lifetime* across a worldspace boundary. #1199 (worldspace-scoped weather resource lifetime) is the reason nothing clears it.

## Suggested Fix
Before installing a new transition (or a procedural-fallback `WeatherDataRes`), collapse any in-flight one: write the current blended snapshot into `WeatherDataRes` (or, cheaply, `lerp` at the live `t`) and set `done = true` / reset `elapsed_secs`. A `collapse_weather_transition(world)` helper called at the top of both branches of `apply_worldspace_weather` covers both cases in one place.

## Completeness Checks
- [ ] **TESTS**: A regression test drives two exterior worldspace transitions within 8s and confirms no backward colour pop and no persistent wrong-weather state
