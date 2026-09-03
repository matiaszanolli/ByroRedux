# #3706 — ECS-2026-08-30-D10-06 (LATENT): sample_blended_transform has no single-layer short-circuit — 2x the per-bone lookup cost on the steady-state stack

**Severity**: LOW · **Dimension**: Animation Runtime · **LATENT** (`AnimationStack` is not registered in production — see ECS-D10-01/02/03; no live repro exists or was sought)
**Location**: `crates/core/src/animation/stack.rs::sample_blended_transform`

## Fix

Verified the premise: the function always runs two full passes over
`stack.layers` — a weight/max-priority pass and a blend pass — each doing
`registry.get()`, `effective_weight()`, and a `clip.channels.get()` hash
probe per layer. For the common steady-state case (one layer, no active
fade) this pays every lookup twice for a result the second pass could
have produced alone.

Applied the issue's own suggested fix: added an early branch for
`stack.layers.len() == 1` that resolves the same four gates the two-pass
path applies (registry lookup, weight cull, channel lookup, key-presence)
exactly once, then returns the raw sampled `(translation, rotation,
scale)` triple directly — skipping the normalisation divide entirely
rather than computing the no-op `w = ew / total_weight = 1.0` the
two-pass path would otherwise perform. This is not just an optimization
of the same result: for a single true contributor, `total_weight == ew`
exactly, so `ew / ew` is the mathematically exact IEEE 754 value `1.0`
(no rounding, since dividing a finite nonzero float by itself always
lands on exactly-representable `1.0`), and multiplying by `1.0` is itself
lossless — so the short-circuit's raw values and the two-pass path's
normalised values are bit-identical, not merely close.

## SIBLING (issue's own checklist item — "the float/color/bool channel
blend siblings checked for the same duplicated probe")

Searched the entire workspace for other `sample_blended_*` functions —
`sample_blended_transform` is the **only** multi-layer blending function
in the codebase (`sample_float_channel`/`sample_color_channel`/
`sample_bool_channel` in `interpolation.rs` are single-channel
interpolators with no cross-layer blending pass at all, called only from
within this one function). The issue's premise of a sibling to check
against does not currently hold — there is nothing else to fix.

## TESTS (issue's own checklist item — "pins that the one-layer
short-circuit produces bit-identical output to the two-pass path")

- `single_layer_short_circuit_matches_two_pass_output` — the core proof:
  a two-layer stack where the second layer is excluded via the weight
  cull (distinct from the existing `#3471` keyless-channel exclusion
  test) still exercises the real two-pass path (`layers.len() == 2`);
  an otherwise-identical **one**-layer stack carrying only the real
  contributor takes the new short-circuit. Asserts `assert_eq!` (exact,
  not approximate) equality between the two outputs.
- `single_layer_short_circuit_respects_the_weight_cull` — a lone
  below-threshold layer must still return `None`.
- `single_layer_short_circuit_respects_channel_has_keys` — a lone
  all-empty channel must still return `None` (the `#3471` exclusion,
  now pinned for the short-circuit path specifically).

**Reintroduce-and-revert verification** (three independent bugs, each
caught by its own test): temporarily (1) removed the weight-cull check
from the short-circuit — `single_layer_short_circuit_respects_the_
weight_cull` failed; (2) removed the `channel_has_keys` check —
`single_layer_short_circuit_respects_channel_has_keys` failed; (3)
hardcoded the translation sample's time argument to `0.0` instead of
`layer.local_time` — `single_layer_short_circuit_matches_two_pass_output`
failed with the exact mismatched translation value. Restored the real
fix after each and reran — all 81 tests in `byroredux-core`'s
`animation::` tests pass again.

## Verification

- `cargo check -p byroredux-core --tests`: clean, zero warnings.
- `cargo test -q -p byroredux-core animation::`: 81 passing, 0 failing
  (+3 new).
- `cargo test -q --no-fail-fast` (full workspace): **7165 passing, 0
  failing**.
