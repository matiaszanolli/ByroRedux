# Investigation — #1450 WAT-01 submersion hysteresis

**Domain:** renderer / water (`byroredux/src/systems/water.rs`)

## Note on the "do not fix speculatively" guidance
The issue (and the project no-speculative-fix policy) recommend not fixing
without a repro. The user explicitly chose to add the hysteresis band now during
`/fix-issue`, so it was implemented as a low-risk, well-bounded defensive change.

## Finding
`head_submerged = depth > 0.0` is a hard threshold with no band. With the camera
parked at the waterline, `depth` dithers across 0 and the boolean strobes.
Important nuance confirmed during investigation: the underwater *fog/tint* is
driven by `state.depth` itself (`compute_underwater_params` returns `depth` as
the 4th component, and the composite shader gates on `underwater.w > 0`), so the
tint **intensity self-fades to ~0 at the waterline** — it does not hard-strobe.
The only thing that hard-toggles is the `head_submerged` boolean (swim-state /
FX-enable). That is what the hysteresis stabilizes.

## Fix
- New module const `WATERLINE_HYSTERESIS = 4.0` (Bethesda world units,
  ~1.43 cm/unit → ~5.7 cm; imperceptible, swamps sub-frame dither).
- New pure helper `resolve_head_submerged(was, depth) -> bool`: enter at
  `depth > +eps`, stay submerged while a candidate exists down to `depth > -eps`,
  dry when outside every volume (`None`).
- The vertical AABB upper bound is relaxed by the *same* const
  (`cam_pos.y > volume.max[1] + WATERLINE_HYSTERESIS`) so a candidate still
  exists up to one band above the surface — without this, the eye crossing the
  surface drops straight to the no-candidate (`None` → dry) path and the exit
  side of the band would never engage. Exit therefore fires precisely at the
  band edge.
- `depth` is written through unchanged for the fog path.

## Completeness checks
- [x] **Repro**: not captured; fix made at user's explicit direction (decision
  recorded in the conversation).
- [x] **SIBLING**: audited all `SubmersionState` / `head_submerged` consumers.
  The only FX is the composite underwater tint via `compute_underwater_params`,
  which is depth-driven through the same `head_submerged` gate — there is no
  other independent waterline-gated boolean toggle to band. Covered.
- [x] **TESTS**: 4 unit tests on `resolve_head_submerged` (outside-volume-dry,
  enter-requires-full-band, stays-submerged-across-waterline, non-degenerate
  band).

## Verification
`cargo check` clean (no warnings in touched file). `cargo test` (workspace):
2790 passed, 0 failed (incl. the 4 new tests).

## Residual note
A perceptual repro at the waterline is still the right way to tune
`WATERLINE_HYSTERESIS`; 4.0 wu is a conservative starting value, easily adjusted.
