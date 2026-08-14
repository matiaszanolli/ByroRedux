# PHYS-D6-03

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2872

---

Found by `/audit-physics` Dimension 6 (WATAL Physics Sink). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: MEDIUM · **Status**: NEW · **This is a cross-layer SEAM — reported once, here.**
**Location**: `byroredux/src/env_translate.rs:475-488` (`resolve_water_material` flow synthesis), consumed at `crates/physics/src/water.rs:147` and `crates/core/src/ecs/components/water.rs:214-218` (the unit contract)

## Trigger Conditions
Any cell whose XCWT resolves to a WATR whose EDID matches the `rapid`/`waterfall`/`falls`/`river`/`stream` heuristic (`env_translate.rs:462-471`) — i.e. every river/rapids plane, in every game. Calm water carries no `WaterFlow` and is unaffected.

## Description
`WaterFlow::speed` is documented as *"World units per second. Typical: 0.5 (calm river) ... 8.0 (whitewater rapids) ... 25.0 (Tamriel-tall waterfall sheet)"* (`components/water.rs:216-218`). The single translate site assigns it `rec.params.wind_speed.abs().max(0.5)` — the WATR `DATA`/`DNAM` wind-velocity float, copied verbatim with **no scale factor and no documented unit** at the parse boundary (`crates/plugin/src/esm/records/misc/water.rs:96` says only "wind speed", default `1.0`).

The **same** scalar is then also used as a shader scroll rate in the very next lines:

```rust
// env_translate.rs:477-488
let speed = rec.params.wind_speed.abs().max(0.5);
flow = Some(WaterFlow { direction: [cos_theta, 0.0, sin_theta], speed });
mat.scroll_a = [cos_theta * speed * 0.5, sin_theta * speed * 0.5];   // vs default [0.020, 0.011]
```

against a `scroll_a` default of `[0.020, 0.011]` documented as *"world-space scroll vectors ... (xy = m/s)"*. Physics consumes it directly as a velocity target: `speed_error = target_speed - body_velocity.dot(direction)` (`water.rs:148`).

A value that is simultaneously a ~0.02-magnitude UV scroll rate and a 0.5-25 BU/s world velocity **cannot be dimensionally correct in both consumers**. Nothing in the repo establishes which reading is right, and there is no clamp or sanity band on either.

## Impact
The physics current's terminal drift speed is set by a field of unverified unit.
- If WATR wind velocity is the small normalised float the `scroll_a` defaults imply, the `.max(0.5)` floor pins every real river/rapids plane at ~0.5 BU/s ~ 7 mm/s — authored currents are effectively **inert** in the physics sink, and the "downstream drift" behaviour only exists in the hand-authored `speed: 8.0` test (`water.rs:726-729`).
- If it is a large float on some records, an unclamped `speed` is the **unbounded** terminal velocity clutter converges to.

**Vanilla WATR values were deliberately NOT verified on disk** — that is the disproof step this finding explicitly leaves open, per the no-guessing rule. What is proven from code alone is the two-consumers-one-scalar inconsistency and the total absence of unit documentation, conversion, or clamp.

## Suggested Fix
Establish the WATR wind-velocity unit from the Gamebryo 2.3 / nif.xml / UESP reference **first** (No-Guessing), then either:
- (a) apply an explicit BU/s conversion at the single `resolve_water_material` site and derive `scroll_a` from the canonical `WaterFlow` rather than the raw field, or
- (b) if the field genuinely is a scroll rate, stop feeding it to `WaterFlow.speed` and synthesize the physics current from a documented engine constant x `WaterKind`.

Clamp the result to the documented 0.5-25 BU/s band either way.

## Seam owners
- Decode side: `/audit-esm` Dim 5 (WATR `DATA`/`DNAM` field semantics)
- `scroll_a`/`scroll_b` consumer: `/audit-renderer` Dim 15 (cf. the already-open #2787 on the neighbouring `ampScale`/`freqScale` sentinels)

`docs/engine/watal.md` §4 lists `WaterFlow` as "SYNTHESIZED from wind" for Oblivion/FO3/FNV and "AUTHORED flow" for Skyrim — the synthesis is currently the only path for all games, since no DNAM linear-velocity decode exists.
