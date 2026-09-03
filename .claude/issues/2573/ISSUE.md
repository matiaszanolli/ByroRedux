# #2573 — OBL-D5-03: resolve_pbr's classifier backstop hardcodes specular_authored: false, diverging from the real Oblivion signal

**Severity**: LOW · **Dimension**: PBR classification / NIFAL canonical boundary
**Location**: `crates/core/src/ecs/components/material.rs::Material::resolve_pbr`

## Fix

Verified the premise against current code first: the importer
(`crates/nif/src/import/material/mod.rs::MaterialInfo::specular_authored`)
already carries the exact signal and already forwards it correctly into
`classify_pbr_keyword` at NIF-import time (`into_imported_material`'s
sibling call), but the field was dropped at the very next boundary —
`ImportedMaterial` (`crates/nif/src/import/types.rs`) had no field to
receive it, so `translate_material` had nothing to forward onto `Material`,
so `resolve_pbr`'s own classifier-backstop call hardcoded
`specular_authored: false`.

The issue's stated "impact today is nil" premise turned out to be stale:
`resolve_pbr`'s backstop is unreachable on the *Oblivion* path (confirmed),
but **not** in general — #2707 (SF-D8-01) already introduced a real,
shipping path where `metalness_override`/`roughness_override` arrive
`None` (the Starfield material-reference-stub case with no PBR signal at
all), which is exactly the "live divergence" scenario the issue's own text
predicted as a *future* risk. So this was fixed as a real, reachable
correctness gap, not a purely defensive change — and the "delete the
backstop arm" alternative the issue also offered was rejected for the same
reason: the backstop is genuinely load-bearing for Starfield stubs today.

Threaded `specular_authored: bool` through every layer of the boundary:

- `ImportedMaterial::specular_authored` (`crates/nif/src/import/types.rs`),
  defaulting `false` (matches the previous hardcoded assumption for every
  existing producer).
- Forwarded in `into_imported_material` (`crates/nif/src/import/material/mod.rs`)
  from `self.specular_authored` (`MaterialInfo`), which was already correct.
- `Material::specular_authored` (`crates/core/src/ecs/components/material.rs`),
  documented alongside `specular_color`.
- `translate_material` (`byroredux/src/material_translate.rs`) copies
  `source.specular_authored` onto the `Material` literal.
- `resolve_pbr`'s classifier-backstop call now reads `self.specular_authored`
  instead of hardcoding `false`.

## Save-format consequence

`Material` is a registered saved column, and `specular_authored` is a new
*required* field on it — the same shape of change as #3073's
`parallax_height_scale`/`parallax_max_passes` and v10's
`parallax_height_in_alpha`. Per this codebase's blanket
`serde_default_on_saved_struct_requires_format_major_bump` rule (no
per-field "but this default happens to be safe" judgement calls, #1714),
bumped `byroredux_save::FORMAT_MAJOR` 20 → 21 and updated the
`saved_type_shape_changes_require_format_major_bump` baseline
(`BASELINE_MAJOR` + `BASELINE_SHAPE_FINGERPRINT`) in
`byroredux/src/save_io/serde_default_guard_tests.rs`, with the same
"false happens to be correct for every pre-v21 snapshot, bump taken anyway"
reasoning `FORMAT_MAJOR`'s own doc comment already uses for its two closest
precedents.

## SIBLING (issue's own checklist item)

Searched for every other call site of `classify_pbr_keyword` — there is
exactly one (the NIF-import-time call in
`crates/nif/src/import/material/mod.rs`), and it was already correctly
forwarding `self.specular_authored`. `resolve_pbr`'s call was the only
hardcoded site.

## TESTS (issue's own checklist item — "confirms specular_authored is
forwarded correctly")

- `resolve_pbr_forwards_specular_authored_to_the_classifier`
  (`crates/core/src/ecs/components/material.rs`) — two fixtures sharing
  `env_map_scale > 0.3` (the branch `classify_pbr_keyword` actually
  consults `specular_authored` in) and a bright `specular_color`, differing
  only in `specular_authored`. Reverting `resolve_pbr` to hardcode `false`
  again collapses both to the same dielectric result, failing the
  `assert_ne!`.
- `translate_material_copies_every_canonical_field`
  (`byroredux/src/material_translate.rs`'s `canonical_completeness_harness`)
  extended: `kitchen_sink_source()` now sets `specular_authored: true` (the
  struct default is `false`) and the round-trip assertion checks
  `material.specular_authored` — this is also enforced structurally by
  that harness's own `every_source_derived_material_field_is_pinned_by_a_test`
  companion, which fails the build if any `source.` copy in the `Material`
  literal has no corresponding `assert*!`/`.expect(` on `material.<field>`.

**Reintroduce-and-revert verification** (both boundary layers, independently):
- Temporarily hardcoded `specular_authored: false` back into `resolve_pbr` —
  confirmed `resolve_pbr_forwards_specular_authored_to_the_classifier`
  failed with the expected message. Restored — all 45 tests in
  `ecs::components::material::tests` pass again.
- Temporarily reverted `translate_material`'s
  `specular_authored: source.specular_authored,` line to a hardcoded
  `false` — confirmed `translate_material_copies_every_canonical_field`
  failed (`#2573` assertion). Restored — all 53 `material_translate::`
  tests pass again.

## Verification

- `cargo check --workspace --tests`: clean, zero new warnings (the
  pre-existing unrelated `grup_walker.rs:469` `unused_mut` warning is
  present and out of scope, as in every prior session run).
- `cargo test -q -p byroredux-core ecs::components::material::`: 45
  passing, 0 failing (+1 new).
- `cargo test -q -p byroredux material_translate::`: 53 passing, 0 failing.
- `cargo test -q -p byroredux-nif`: 1229 passing, 0 failing (unaffected;
  confirms the `into_imported_material` forwarding change didn't disturb
  any existing NIF-import test).
- `cargo test -q -p byroredux save_io::`: 53 passing, 0 failing (the
  FORMAT_MAJOR/baseline bump verified in isolation).
- `cargo test -q --no-fail-fast` (full workspace): **7159 passing, 0
  failing** (+1 net new test — the kitchen-sink extension reused an
  existing test).
