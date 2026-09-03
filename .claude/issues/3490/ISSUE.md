# #3490 — PHYS-D6-2026-08-27b-01: the current-volume containment test measures the body origin Y while the surface test measures the collider AABB centre - the exact split #2887 closed, 26 lines apart in the same loop

**Severity**: MEDIUM · **Dimension**: 6 — Water / Buoyancy
**Location**: `crates/physics/src/water.rs::apply_buoyancy_with_scratch`

## Fix

Verified the premise: the current-volume branch's Y containment test
(`pos.y >= v.min[1] && pos.y <= v.max[1]`) read the rigid-body origin
verbatim, while the surface branch a few lines below already fixed this
exact split for itself under #2887 (reads the collider AABB centre,
since `collision_shape_to_parts` attaches every compound part at its own
local isometry and ragdoll bones are offset by construction — the norm
for a bhk-imported body, not an edge case).

Applied the issue's own suggested fix: hoisted the collider AABB fetch
above `current_flow`'s computation so both branches share one
`aabb_y: Option<(f32, f32)>` (min_y, max_y) instead of each computing
their own — the surface branch previously called `compute_aabb()` a
second time for the same collider, so this is also a small perf win on
top of the correctness fix, exactly as the issue's suggested fix
predicted ("on the common path this is free"). `current_flow` now
derives `center_y` from the shared AABB and tests that instead of
`pos.y`. The missing-collider early-`continue` is preserved exactly —
pre-fix, a missing collider `continue`d past the whole loop iteration
(discarding an already-computed `current_flow` too, since `continue`
unwinds past both `let` bindings), so hoisting the fetch and continuing
before either branch computes is behavior-preserving for that case.

## SIBLING (issue's own checklist item)

Searched for other body-origin-vs-collider-centre Y reads in this file —
only the one already-corrected surface-branch site (`#2887`) and the one
this issue fixes. The one other `pos.y` use in the file
(`authored_wave_height_with_weather`'s sample point) is a wave-height
lookup at the body's horizontal position, unrelated to containment
testing, and outside this issue's premise.

## TESTS (issue's own checklist item — "a regression test pins this
specific fix")

`current_volume_flow_is_measured_from_the_collider_aabb_centre_not_the_body_origin`
— reuses the exact #2887 offset-compound fixture (a 40 BU-below-origin
leaf, so the AABB centre and body origin disagree by a known amount) and
authors a current-volume band containing ONLY the AABB centre.

Had to route around a fixture trap while writing it: `WaterContact::flow`
is **not** the right observable — when the body is also inside a
surface's water column (as this fixture's body is), `flow` is written
from the *surface's* own `WaterFlow` (`s.flow`), completely independent
of `current_flow`. The current-volume branch's actual effect
(`current_force`) applies directly to the Rapier body and is never
surfaced on `WaterContact` at all. Fixed by authoring the current
volume's flow along +Z — orthogonal to the base water plane's own +X
flow — and reading the resulting Z displacement over 600 ticks instead;
only a correctly-detected current volume can move the body along an axis
the surface's own flow never touches.

**Reintroduce-and-revert verification**: temporarily restored the
`pos.y`-based Y test — confirmed the new test failed with `z=0` exactly
(the current volume never fired, since the origin at -30 falls outside
the -80..-60 band). Restored the fix and reran — all 156 tests in
`byroredux-physics` pass again.

## Verification

- `cargo check -p byroredux-physics --tests`: clean, zero warnings.
- `cargo test -q -p byroredux-physics`: 156 passing, 0 failing (+1 new).
- `cargo test -q --no-fail-fast` (full workspace): **7176 passing, 0
  failing**.
