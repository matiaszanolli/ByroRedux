# SAVE-D2-05: AnimationStack has no dedicated save/load round-trip test

**Labels**: medium, ecs, bug

**Severity**: MEDIUM
**Dimension**: Registry & (De)serialization Fidelity
**Source**: `docs/audits/AUDIT_SAVE_2026-07-16.md`

## Location
`crates/core/src/animation/stack.rs:14-33,84-88`; registered at `byroredux/src/save_io.rs:184`

## Description
`AnimationStack` (`Vec<AnimationLayer>` of 10+ fields each, plus `Option<EntityId> root_entity`) is registered for full save/restore and is the only registered type with both a nested `Vec` of a many-field struct and an `Option<EntityId>`. It's deliberately excluded from `MUTABLE_DELTA_COLUMNS` (`#1696`), and its structurally similar sibling `AnimationPlayer` *does* have a full-restore round-trip test (`anim_player_root_entity_not_clobbered_by_delta_apply`) — but `AnimationStack` itself is never constructed, saved, and asserted back in any test found. The only existing coverage (`crates/core/src/animation/stack.rs`) is a raw serde-json round trip of the struct itself, not a `save_world → encode → decode → restore_world` pass through the registry.

## Impact
A future serde-shape regression in `AnimationLayer`/`AnimationStack.root_entity` would not be caught by any existing test.

## Suggested Fix
Add a `crates/save/tests/round_trip.rs` case building a multi-layer `AnimationStack` (varying weight/blend timers/`reverse_direction`/`clip_handle`, with a `root_entity`), round-tripping through `save_world → encode → decode → restore_world`, asserting every field survives at the same entity id.

## Completeness Checks
- [ ] **SIBLING**: Mirror `AnimationPlayer`'s existing round-trip test pattern (`anim_player_root_entity_not_clobbered_by_delta_apply`)
- [ ] **TESTS**: A regression test pins this specific fix
