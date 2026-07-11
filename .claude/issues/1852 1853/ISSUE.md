# #1852: FNV-D7-04: Ragdoll writeback uses live gt.scale in the inverse while the seed captured scale at activation

Severity: LOW · legacy-compat
Location: `byroredux/src/ragdoll.rs:190` (activation seed), `:327` (writeback inverse)

`activate_ragdoll` composes each ragdoll body's seed pose using
`gt.scale` read at activation time. `ragdoll_writeback_system` inverts
that composition every frame using the bone's *current* `gt.scale`.
If GlobalTransform.scale changes between activation and a later frame,
the offset term de-composes with the wrong scale, displacing the bone
by `local_translation * Δscale`. Latent — vanilla content ships
constant uniform scale so the two reads always agree today.

Suggested fix: snapshot the seed-time scale into RagdollBodySpec at
activation and use that stored value in the writeback inverse instead
of re-reading live GlobalTransform.scale.

# #1853: FNV-D1-01: Stale doc comment claims FO3/FNV worldspace water default is unimplemented

Severity: LOW · documentation
Location: `byroredux/src/cell_loader/exterior.rs:41-50` (stale doc)

The doc on `default_water_height` (or equivalent field) says
FO3/FNV/Skyrim+ "are excluded pending DNAM parsing" — no longer true.
`crates/plugin/src/esm/cell/wrld.rs:131-138` parses DNAM's second f32
into `default_water_height`, and `env_translate.rs::
default_water_for_worldspace` already consumes it for all non-Oblivion
games (regression-tested). Doc-only fix: rewrite to state the current
implemented behavior. No new test needed per the issue.
