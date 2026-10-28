# ECS-2026-10-27-05: player_controller_system omits the PendingScriptStoryEvents write from its declaration

Labels: low, ecs, concurrency, bug
Source: docs/audits/AUDIT_ECS_2026-10-27.md

**Source**: `docs/audits/AUDIT_ECS_2026-10-27.md` finding `ECS-2026-10-27-05` (Dimension 5, system and scheduler wiring) · Severity LOW (declaration gap; no parallel peer touches the resource today) · Status NEW (recorded by the skill as a by-hand check; no issue or report has filed it).

**Location**: `byroredux/src/boot/schedule/early.rs:21-110` (the `player_controller_system` row, `writes::<StoryEvent>` at ~:81). Call path: `byroredux/src/systems/character.rs:1644-1664` (`apply_player_drowning_damage`) -> `byroredux/src/systems/story_events.rs` (`raise_kill_story_event`) -> `crates/scripting/src/story_manager.rs:150-170` (`raise_story_event`, collision arm: `try_resource_mut::<PendingScriptStoryEvents>()`).

## Description
The row declares `writes::<StoryEvent>` for the #5641 hop. It does not declare the queue that `raise_story_event` writes when the player entity already carries a `StoryEvent` that frame. The hop is cross-crate and has no turbofish at the call site, so `every_parallel_system_declares_everything_it_acquires` cannot see it. Only the Late dialogue exclusive declares the resource (`late.rs:678`).

## Evidence
```rust
// story_manager.rs
Some(event) => world
    .try_resource_mut::<PendingScriptStoryEvents>()
    .map(|mut pending| pending.0.push(event))
    .is_some(),
```
`early.rs` row: `.writes::<byroredux_scripting::story_manager::StoryEvent>()` with no `PendingScriptStoryEvents` resource write.

## Impact
None today. The Early parallel peer, `timer_tick_system`, does not touch the queue, so `known_conflict_count() == 0` still holds. A future parallel system that drains or appends to the queue would be scheduled beside the player controller with no conflict reported.

## Related
#5641, #5647, #5434.

## Suggested Fix
Add `.writes_resource::<byroredux_scripting::story_manager::PendingScriptStoryEvents>()` next to the `StoryEvent` write, with a `#5647` comment. Optionally list `raise_story_event` as a cross-file hop in `PARALLEL_SYSTEMS`.

## Completeness Checks
- [ ] **SIBLING**: Other parallel rows reaching `raise_story_event` / `raise_kill_story_event` (combat, hit, death paths) checked for the same missing declaration
- [ ] **LOCK_ORDER**: No acquisition order changes
- [ ] **TESTS**: `every_parallel_system_declares_everything_it_acquires` / the scheduler access tests stay green with the new declaration; the hop is added to the cross-file list if applicable

