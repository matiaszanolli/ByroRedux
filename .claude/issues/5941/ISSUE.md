# PHYS-D4-2026-10-16-02: Console movers accept non-finite coordinates: `actor.walk` (NaN duration never expires) and `cam.pos` via `place_character_body`

Labels: low,physics,bug
Source: docs/audits/AUDIT_PHYSICS_2026-10-16.md

- **Severity**: LOW
- **Dimension**: Character & NPC Controller
- **Location**: `byroredux/src/commands/anim_event.rs` (`ActorWalkCommand::execute`), `byroredux/src/systems/graph_locomotion.rs:73-90` (`console_walk_system`), `byroredux/src/commands/view.rs:1190-1215` (`cam.pos`), `byroredux/src/systems/character.rs:921-950` (`place_character_body`)
- **Status**: NEW
- **Description**: `actor.walk` parses `speed`/`seconds` with `f32::parse` (accepts `nan`/`inf`) and stores them with no bounds. In `console_walk_system`, `step = dt.min(walk.remaining).max(0.0)` with `remaining = NaN` is `dt`, and `remaining -= step` stays NaN, so `remaining <= 0.0` is never true: the actor walks forever (until it dies, `reconcile_dead_actor` removes `ConsoleWalk`). An `inf` speed writes an infinite translation straight into the actor's `Transform` (no KCC, no `translation_is_sane`). Likewise `cam.pos nan 0 0` flows camera -> `snap_character_body_to_camera` -> `place_character_body`, which, now being the single home of the player teleport, writes the position to `Transform`/`GlobalTransform` unvalidated (`apply_player_pose` validates with `PhysicsWorld::translation_is_sane` only for the save path).
- **Evidence**: see Description; `cargo test` pins none of this. `console_walk_system` and `ActorWalkCommand` have no non-finite test.
- **Impact**: Debug/console surface only (same exposure as every other console mutator); the failure is a NaN/inf player or actor transform or a perpetual forced walk, recoverable by another teleport/kill. Reported because #5591 created the single point where one guard would cover every player teleport.
- **Trigger Conditions**: An operator (or `byro-dbg` script) passing `nan`/`inf` to those commands.
- **Related**: #5530 (apply_player_pose guard), `PhysicsWorld::translation_is_sane`
- **Suggested Fix**: In `ActorWalkCommand`, reject non-finite `speed`/`seconds`/`turns` and cap `seconds`; in `place_character_body`, return `None` (and log) when `!PhysicsWorld::translation_is_sane(pos)` so `apply_player_pose`'s check becomes defence in depth rather than the only guard.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the sibling producer or consumer
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Filed from `docs/audits/AUDIT_PHYSICS_2026-10-16.md`. Fix with `/fix-issue <number>`._
