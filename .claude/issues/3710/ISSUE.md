# #3710 — ECS-P2-07: two distinct PlayerEntity resources describe the same fact with different null semantics

**Severity**: LOW · **Dimension**: P2 Gameplay Slice / Resource shape
**Location**: `byroredux/src/systems/character.rs` (`PlayerEntity(pub Option<EntityId>)`) and
`crates/scripting/src/papyrus_demo/mod.rs` (`PlayerEntity(pub EntityId)`)

## Fix, scoped to the issue's "at minimum" fallback

Full type unification (merging the two shapes, or having scripting
re-export byroredux's `Option`-wrapped type) would touch 50+ call sites
across `crates/scripting` that universally do `.0` direct access assuming
the player entity always exists — a much larger, riskier change than a
LOW-severity finding justifies in one pass. Took the issue's own narrower
fallback instead: renamed the scripting-side type to
**`PapyrusPlayerEntity`**, disambiguating the two by name everywhere,
mechanically (same shape, same fields, same trait impls — a pure
identifier rename), verified safe by the compiler at every step (a missed
or wrong site would have been a type-mismatch compile error, since the
two types' field shapes genuinely differ).

Scope: 24 files — 17 inside `crates/scripting` (the rename is unambiguous
there; `systems::PlayerEntity` doesn't exist in that crate at all) plus 7
on the `byroredux` side where only the fully-qualified
`byroredux_scripting::papyrus_demo::PlayerEntity` occurrences were
touched, never the bare `crate::systems::PlayerEntity` ones living in the
same files.

**Caught by the compiler, not by inspection**: a rename that touches only
fully-qualified `use` imports leaves any *bare* reference elsewhere in the
same function pointing at whatever's now in scope — `save_io/
round_trip_tests.rs:780` had exactly this (a bare `PlayerEntity(player)`
call site whose governing `use` statement got renamed to
`PapyrusPlayerEntity` a few lines above it), and it surfaced immediately
as a compile error rather than silently resolving to the wrong type or
failing to resolve at all.

## An unrelated, deeper bug found while chasing the issue's own TESTS ask

Investigating "how does the save-registry completeness guard currently
key on the short type name" (per the issue's own framed mechanism)
revealed the true situation was **worse** than "one row ambiguously
covers both": `registry_completeness_tests.rs`'s `impl_target_type` only
recognizes lines starting with the bare `"impl Component for "` /
`"impl Resource for "` prefix. Scripting's resource used a
fully-qualified `impl byroredux_core::ecs::resource::Resource for
PlayerEntity {}` — which **never matched that prefix at all**, so the
type was completely invisible to the scanner, not merely
name-colliding with byroredux's entry. Confirmed by re-running the guard
immediately after the rename: it still passed with zero `PapyrusPlayerEntity`
entries anywhere, which would have been impossible if the scanner had
ever actually been discovering it.

Fixed by normalizing the impl to the bare form every other impl in that
file already uses (`use byroredux_core::ecs::resource::Resource;` +
`impl Resource for PapyrusPlayerEntity {}`), matching the codebase-wide
convention rather than widening the scanner's regex to tolerate an
outlier syntax. Verified: the guard immediately started flagging
`PapyrusPlayerEntity` as an unclassified offender once the impl became
visible, confirming the normalization actually closed the blind spot.

## SIBLING (issue's own checklist item — "every consumer of either `PlayerEntity` audited for null-semantics assumptions")

Every one of the 24 touched files was a pure identifier substitution —
no null-semantics changed at any call site (`.0` direct access on
`PapyrusPlayerEntity` behaves identically to before; `Option`-unwrapping
on `crate::systems::PlayerEntity` is completely untouched, since none of
those sites were renamed). The full-workspace compile pass is the
completeness proof here: a null-semantics-relevant site would only be
silently wrong if the SAME identifier resolved to a *different* type
after the rename, which the type checker cannot let through without an
explicit, deliberate `.0`/`Option` conversion — none was added.

## TESTS (issue's own checklist item — "pins that the save registry-completeness allowlist distinguishes the two")

Classified the now-discoverable `PapyrusPlayerEntity` with its own
`NOT_SAVED_BY_DESIGN` row (same process-local-identity posture as
`PlayerEntity`'s existing entry — both are set from the same call site in
`scene.rs`), and added a direct assertion inside
`every_component_or_resource_impl_is_saved_or_explicitly_allowlisted`
pinning that both `"PlayerEntity"` and `"PapyrusPlayerEntity"` carry
distinct rows — since `allowlisted` is built as a `HashSet`, a
reintroduced short-name collision wouldn't otherwise surface as a
duplicate-key error, it would just silently classify both types under
whichever reason happened to be listed.

Verified the guard actually catches the regression (this session's
established quality bar): temporarily collapsed the two rows back into
one shared `"PlayerEntity"` name, reran — the new assertion failed with
exactly the expected message, then restored the two distinct rows and
confirmed a clean pass again.

## Verification

- `cargo check --workspace --tests`: clean (one pre-existing, unrelated
  `unused_mut` warning in `esm/records/grup_walker.rs` predates this fix).
- `cargo test -q -p byroredux-scripting`: 409 tests passing, 0 failing.
- `cargo test -q -p byroredux --bin byroredux`: 1,872 tests passing, 0
  failing.
- `cargo test -q --no-fail-fast` (full workspace): **7093 passing, 0
  failing** (unchanged — the new assertion lives inside an existing test
  function).
