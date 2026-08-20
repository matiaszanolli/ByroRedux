# PHYS-D5-2026-08-20-06: swim_vertical_velocity's damping is per-frame, not per-second — the new swim controller is frame-rate dependent

**Issue**: #3125 — https://github.com/matiaszanolli/ByroRedux/issues/3125
**Finding**: `PHYS-D5-2026-08-20-06`
**Labels**: bug, medium, legacy-compat
**Filed**: 2026-08-20 (comprehensive `/audit-suite` sweep, 25 reports)

---

**Audit**: `docs/audits/AUDIT_PHYSICS_2026-08-20.md` — Dimension 5 (Character Controller)
**Severity**: MEDIUM · **Status**: NEW — the whole function landed this cycle (`e7cf6373` / `c7561d74`)

## Location
`byroredux/src/systems/character.rs:964-984` (`swim_vertical_velocity`)

## Trigger conditions
Any frame in which the player is swimming (`swimlevel_reached` true), at any refresh rate other than the 60 Hz the tests use.

## Description
The integrator mixes a dt-scaled spring term with a **dt-free** multiplicative decay:

```rust
let target_y = surface_y - half_span * SWIM_HEIGHT_SCALE;
let spring = (target_y - center_y) * (5.0 + 7.0 * fraction.clamp(0.0, 1.0));
(prev_velocity * 0.72 + spring * dt).clamp(-120.0, 160.0)
```

`prev_velocity * 0.72` decays by a fixed 28 % **per frame** rather than per unit time. Its terrestrial sibling `integrate_vertical` (`character.rs:1053-1070`) is dt-correct (`prev + gravity*dt`), so the two halves of the same controller disagree on time discretization.

Verified at HEAD: `character.rs:981` is still `(prev_velocity * 0.72 + spring * dt).clamp(-120.0, 160.0)`.

## Evidence
The steady state of `v = 0.72·v + spring·dt` is `v* = spring·dt / 0.28`:

| Refresh | `v*` |
|---|---|
| 30 fps | `spring · 0.119` |
| 60 fps | `spring · 0.0595` |
| 144 fps | `spring · 0.0248` |

The swimmer's approach speed to the waterline therefore varies by ~**4.8×** across the refresh rates the project targets, and the standing depth offset needed to hold station scales with it.

All three pinning tests (`character.rs:1370`, `:1374`, `:1383`) pass `dt = 1.0/60.0` exclusively, so none can observe it.

## Impact
Swimming feels correct only at 60 fps. On a 144 Hz display the player rises to the surface roughly 2.4× slower and sinks lower before the spring catches them, which interacts with the drowning path — `head_submerged` is depth-derived, so a frame-rate-dependent rest depth makes breath drain frame-rate dependent too. Bounded to the player; no solver or NPC impact.

## Related
`integrate_vertical`'s dt invariants (#1698 substep budget is what makes dt spikes survivable elsewhere), `docs/engine/watal.md` character-swimming item, PHYS-D5-2026-08-20-08 (same new controller).

## Suggested fix
Replace the fixed factor with a dt-correct decay — `prev_velocity * (-SWIM_DAMPING * dt).exp()` (or the cheaper `prev_velocity / (1.0 + SWIM_DAMPING * dt)`) — naming `SWIM_DAMPING` so it reads in 1/s and choosing it to reproduce today's 60 fps behaviour (`0.72 == e^(−k/60)` → `k ≈ 19.7`).

Add a test asserting that two 1/120 s steps land within epsilon of one 1/60 s step.

## Completeness Checks
- [ ] **SIBLING**: Every other integrator in `byroredux/src/systems/character.rs` checked for the same dt-free decay (the jump branch at `:975-978` also mixes a raw `prev_velocity * 0.15` term)
- [ ] **TESTS**: A regression test pins this specific fix at more than one dt
