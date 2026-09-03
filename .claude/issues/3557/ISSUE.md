# #3557 — RT-11: Oblivion emits two byte-identical synthetic `__max_default_light` directional emitters — double the intended synthetic contribution

**Severity**: LOW · **Dimension**: Runtime (renderer)
**Location**: `byroredux/src/cell_loader/spawn.rs::spawn_nif_lights`

## Investigation

`__max_default_light` is not a host-synthesized light — it's a real
`NiDirectionalLight` block, literally present and named that, in the
source NIF (a 3ds Max default-light artifact the exporter carried
through). Traced the spawn path: `spawn_nif_lights` (`spawn.rs`) is
called once per REFR placement, from `spawn_placed_instances` →
`spawn_synth_child` → the per-cell REFR loop
(`references/mod.rs::load_references_budgeted`), and separately once per
loose-NIF load (`scene/nif_loader.rs`, no cell/REFR context). Two REFRs
in a cell each placing a mesh that carries its own `__max_default_light`
node therefore each independently spawn a `LightSource` entity —
confirmed the doubling mechanism the issue describes.

## Fix

Applied the issue's own suggested fix ("de-duplicate synthetic default
lights by name at import"), scoped narrowly per the SIBLING checklist:
only names on a small, evidence-bound allowlist are deduplicated — an
ordinary content light is never affected even if it happens to share a
name with another.

- `is_known_exporter_artifact_light_name(name: &str) -> bool` — currently
  matches exactly `"__max_default_light"`, the one confirmed instance.
- In `spawn_nif_lights`'s loop, before spawning: if the light's name is
  on the allowlist AND `World::find_by_name` (an existing, general-purpose
  utility — `crates/core/src/ecs/world.rs`) already finds an entity with
  that name, skip spawning a duplicate.

**Why a `World`-wide name check is the right "per scene" scope**: the
issue's suggested fix offers either "de-duplicate by name" or "hoist to
one per scene." In this architecture the two are the same thing — cells
are loaded/unloaded as units, so at any moment the `World` only holds
entities for currently-loaded cells; checking "does any entity already
have this name" is exactly a per-currently-loaded-scene gate, without
needing to thread a new per-cell accumulator field through
`spawn_placed_instances`'s already-`#[allow(clippy::too_many_arguments)]`
signature and its two call sites (`references/synth_child.rs`,
`precombined.rs`).

**Why gated on the allowlist first**: the `find_by_name` scan is a linear
`Name` query — cheap in absolute terms (interior cells run ~1-50 lights),
but there's no reason to pay it for every ordinary content light when the
allowlist check is a single string comparison that's true for essentially
zero real content.

## SIBLING (issue's own checklist item — "other well-known exporter-
artifact light node names checked, not just `__max_default_light`")

Only `__max_default_light` is confirmed in this codebase's audit
evidence and corpus today. No other exporter-artifact light names are
documented anywhere in the repo (docs/legacy, nif.xml reference, or
prior audit findings), and per this project's no-guessing policy I did
not fabricate additional names. The allowlist function's own doc records
this explicitly and instructs future editors to extend it (not widen the
match to a heuristic) when another confirmed name turns up.

## CANONICAL-BOUNDARY (issue's own checklist item)

The de-dup happens once, at spawn time in `spawn_nif_lights` (the
NIF→ECS boundary) — not re-derived per frame in
`byroredux/src/render/lights.rs`, which reads whatever `LightSource`
entities already exist in the `World` without knowing or caring how they
got there.

## TESTS (issue's own checklist item — "two NIFs contributing the same
synthetic default light yield one emitter")

- `spawn_nif_lights_deduplicates_known_exporter_artifact_by_name` — the
  headline regression: two `spawn_nif_lights` calls (simulating two
  REFRs) each carrying an identical `__max_default_light` node must
  yield exactly one `LightSource` entity.
- `spawn_nif_lights_does_not_deduplicate_ordinary_named_lights` — the
  safety rail: two REFRs sharing an ordinary content light name must
  still both spawn (2 entities), proving the allowlist gate, not a
  blanket "first name wins" rule.
- `known_exporter_artifact_light_name_matches_only_the_documented_name`
  — pins the allowlist predicate directly, including that it's
  case-sensitive (evidence-bound to the exact confirmed string).

Widened the `#[cfg(test)]` re-export list in `byroredux/src/cell_loader.rs`
(the established pattern this file already uses so `nif_light_spawn_gate_
tests.rs`'s `use super::*;` sees `spawn.rs`'s `pub(crate)` helpers) to
include the new predicate.

**Reintroduce-and-revert verification** (two independent bugs, each
caught by its own test): (1) removed the dedup check from
`spawn_nif_lights`'s loop entirely — the headline test failed (`left: 2,
right: 1`); (2) made the allowlist predicate always return `true` — the
ordinary-lights safety-rail test failed (`left: 1, right: 2`). Restored
the real fix after each and reran — all 20 tests in
`nif_light_spawn_gate_tests` pass again.

## Verification

- `cargo check -p byroredux --tests`: clean, zero warnings.
- `cargo test -q -p byroredux --bin byroredux nif_light_spawn_gate_tests::`:
  20 passing, 0 failing (+3 new).
- `cargo test -q -p byroredux --bin byroredux`: 1889 passing, 0 failing
  (full binary crate, unaffected elsewhere).
- `cargo test -q --no-fail-fast` (full workspace): **7170 passing, 0
  failing**.
