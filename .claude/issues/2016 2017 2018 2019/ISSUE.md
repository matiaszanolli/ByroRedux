# Batch: #2016, #2017, #2018, #2019 (all from AUDIT_SAVE_2026-07-16.md)

## #2016 — SAVE-D2-05: AnimationStack has no dedicated save/load round-trip test
- Severity: MEDIUM · Labels: bug, ecs, medium
- Location: `crates/core/src/animation/stack.rs:14-33,84-88`; registered at
  `byroredux/src/save_io.rs:184`
- AnimationStack (Vec<AnimationLayer> + Option<EntityId> root_entity) is the
  only registered type with both a nested Vec-of-many-field-struct AND an
  Option<EntityId>, deliberately excluded from MUTABLE_DELTA_COLUMNS (#1696).
  AnimationPlayer (sibling) has a full-restore round-trip test
  (anim_player_root_entity_not_clobbered_by_delta_apply); AnimationStack does
  not — only a raw serde-json round trip of the struct itself exists.
- Fix: add a crates/save/tests/round_trip.rs case building multi-layer
  AnimationStack (varying weight/blend timers/reverse_direction/clip_handle,
  with root_entity), round-tripping through save_world → encode → decode →
  restore_world, asserting every field survives at the same entity id.
- Domain: byroredux-save (integration test crate)

## #2017 — SAVE-D4-NEW-01: Quicksave ring cursor advances even when validation aborts
- Severity: MEDIUM · Labels: bug, medium, tech-debt
- Location: `byroredux/src/save_io.rs:396-421` (SaveCommand::execute)
- `state.ring.advance()` runs BEFORE validate_world/validate_form_ids for a
  blank-slot quicksave. If validation fails, function aborts without writing,
  but ring cursor already permanently advanced — breaks "next quicksave lands
  one slot after last SUCCESSFUL one" invariant.
- Fix: move `state.ring.advance()` after the validation gate — use a
  non-mutating peek for the abort-message path, only call the mutating
  advance() once issues.is_empty() and write is about to proceed.
- Domain: byroredux (binary crate, save_io.rs)

## #2018 — SAVE-D6-03: apply_player_pose reverts FlyCam-saved pose within one frame
- Severity: MEDIUM · Labels: bug, medium, tech-debt
- Location: `byroredux/src/save_io.rs:288-338` (apply_player_pose); interacts
  with `byroredux/src/systems/character.rs:358-454` (camera_follow_system)
- Branch gated on `pose.character_mode && character_now` — only drives body
  when BOTH save-time and live mode are Character. FlyCam-saved + live
  Character-mode → camera-only fallback reposition, body untouched;
  camera_follow_system (Stage::Late) re-derives camera from body's
  GlobalTransform every frame, silently reverting after 1 frame. Same
  mechanism as closed #1874 (door-transition), never patched here.
- Fix: in fallback branch, when character_now is true and body exists, also
  relocate the body (mirror snap_character_body_to_camera, camera→body
  direction) so camera_follow_system re-derives the restored position.
  Suggested simplest form: branch on character_now alone.
- Domain: byroredux (binary crate, save_io.rs + systems/character.rs)

## #2019 — SAVE-D6-04: build_form_id_remap silently drops unresolvable saved deltas
- Severity: MEDIUM · Labels: bug, ecs, medium
- Location: `crates/save/src/driver.rs:143-178` (build_form_id_remap)
- A saved FormIdPair that doesn't resolve in the reloaded cell is silently
  absent from the remap, zero logging. Every MUTABLE_DELTA_COLUMNS row keyed
  to that entity dropped silently by ApplyFn's filter_map. Doc comment covers
  "no form id at save time" case but not this one.
- Fix: log::warn! the count (and bounded identities) of saved rows that fail
  to resolve — mirror the log::warn! already present in same file's
  FormIdComponent save closure for the symmetric case.
- Domain: byroredux-save (crates/save)

## Domain classification
- #2016 → `byroredux-save` (crates/save/tests/round_trip.rs)
- #2017 → `byroredux` (binary crate, save_io.rs)
- #2018 → `byroredux` (binary crate, save_io.rs + systems/character.rs)
- #2019 → `byroredux-save` (crates/save/src/driver.rs)
