# #3791 — SAVE-D4-2026-08-30-03: validate_animation's clip-handle and root-entity checks stop at AnimationPlayer — two sibling saved columns carry the same two reference classes with no gate at all

**Severity**: MEDIUM · **Location**: `crates/save/src/validate.rs::validate_animation`
**Source**: `docs/audits/AUDIT_SAVE_2026-08-30.md` (SAVE-D4-2026-08-30-03)

`validate_animation` checked `AnimationPlayer.clip_handle` (resolves in the clip registry) and
`AnimationPlayer.root_entity` (spawned id), but two other **registered, saved** columns carry
the identical pair of reference classes with zero checks:

1. **`AnimationStack`** — `root_entity: Option<EntityId>` + `layers: Vec<AnimationLayer>`, each
   layer holding a `clip_handle: u32`. Forward-latent (only `#[cfg(test)]` populates it today)
   but registered, so a future producer would inherit an unguarded column.
2. **`Seated.animation_restore.clip_handle`** — production-populated (`sandbox_seat_system`
   captures it, `clear_ambient_behavior` writes it back). An `AnimationClipRegistry` index — the
   exact session-local handle class the registry's own allowlist entry rejects it for — riding
   to disk with no gate on the way out.

`Seated`'s exclusion rationale in `byroredux/src/save_io.rs` named only the `EntityId`
(`furniture`) hazard, giving no way for a maintainer extending the FormId-remap machinery to
learn about the second, independent handle hazard v9 (#3333) added.

## Fix implemented

- `validate_animation` now queries `AnimationStack` and `Seated` independently (each its own
  `if let Some(q) = world.query::<T>()`, matching `validate_saved_entity_references`'s
  per-column style — not gated behind `AnimationPlayer`'s query succeeding, which the previous
  `let Some(q) = ... else { return }` shape would have done for a world missing that one
  column). `AnimationStack.root_entity` and each layer's `clip_handle` get the same two checks
  `AnimationPlayer` already had; `Seated.animation_restore.clip_handle` gets the clip-registry
  check (`Seated.furniture` was already covered by `validate_saved_entity_references`).
- **LOCK_ORDER**: verified the existing `AnimationClipRegistry`-before-`AnimationPlayer` order
  (#3649) extends cleanly — `animation_system_inner` (`byroredux/src/systems/animation.rs`)
  acquires the registry at its top and only reaches its `AnimationStack` query much later in the
  same function body, so registry-before-stack is the established production order too.
  `Seated` has no known production co-acquisition with the registry at all
  (`sandbox_seat_system_inner` captures `AnimationPlayer` fields directly); registry-first is
  used there for consistency, not because an inversion is known-unsafe. The existing
  `validate_animation_takes_the_registry_before_the_player_query` source-scan test still passes
  unchanged.
- `byroredux/src/save_io.rs`'s `Seated` exclusion comment now names both hazards
  (`furniture`'s `EntityId` and `animation_restore.clip_handle`'s registry-handle), pointing at
  `validate_animation` as the pre-save gate for both.

Regression tests (the issue's own TESTS checklist item): a dangling `AnimationStack.root_entity`,
a stale `AnimationStack` layer `clip_handle`, a stale `Seated.animation_restore.clip_handle` —
each fails the gate — plus a healthy-stack/healthy-seat sanity test proving the new checks
reject only what's actually stale.

**SIBLING** (issue's own checklist item, scoped): grepped every `pub clip_handle: u32` field in
`crates/core/src/{ecs/components,animation}` — three exist (`AnimationPlayer`, `AnimationLayer`
inside `AnimationStack`, `SeatedAnimationRestore` inside `Seated`), all three now gated. A
fourth handle-bearing type, `AnimationController`, is already excluded from the save registry
entirely (`NOT_SAVED_BY_DESIGN`: "session-local clip handles... rebuilt with the actor"), so it
needs no validation gate — nothing to validate if it's never saved. A full audit of every
registered column against all nine gates (the issue's broader framing) was not run; this covers
the specific sibling class the issue names.

Full workspace: `cargo test --no-fail-fast` 7035 passing, 0 failing.
