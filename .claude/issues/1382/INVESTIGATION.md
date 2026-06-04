# Investigation — #1382 NIFAL-S7 particle rate/start_size NaN guard

**Domain:** nif (NIFAL particles) / safety

## Finding
`particle_system` (`byroredux/src/systems/particle.rs`) guarded `life` via
`.max(0.05)` but not `em.rate` or `em.start_size`. A NaN `rate` permanently
poisons `spawn_accumulator` (`NaN − 0 = NaN`, so the emitter silently dies); a
NaN `start_size` pushes NaN-sized particles into the billboard render path.

## Fix
Wrapped ONLY the spawn step (3) in
`if em.rate.is_finite() && em.rate > 0.0 && em.start_size.is_finite() && em.start_size > 0.0`.
Chose a surgical guard over the issue's "continue at top of per-emitter block"
because the integrate + expire passes (steps 1-2) don't use rate/start_size — a
corrupt rate must not freeze existing live particles on screen; they should still
age out.

## Completeness
- [x] CANONICAL-BOUNDARY: guard at the runtime consumer of authored emitter
  params; per-game logic stays in `apply_emitter_params` (the NIFAL boundary).
- [x] TESTS: `non_finite_rate_or_size_spawns_nothing_and_keeps_accumulator_finite`
  (NaN/Inf rate → no spawn + accumulator stays finite; NaN start_size → no spawn).

Note: avoided `cargo fmt -p byroredux` — it reflows the whole package (the repo
isn't kept rustfmt-clean); hand-formatted the edit instead. cargo test 2802 passed.
