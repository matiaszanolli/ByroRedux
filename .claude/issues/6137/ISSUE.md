# ECS-SD-03: "reused entity id" comments contradict the never-recycled invariant

Labels: low,ecs,documentation,doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/6137

**Severity**: LOW (doc rot with a dead-code hint) · **Dimension**: 7 Component Lifecycles

## Location
`byroredux/src/npc_spawn/ai_package.rs` (the spawn-time `report_package_transition(world, placement_root, None, None)` and its comment), `crates/scripting/src/fragment/package.rs` (the comment on the pre-early-return `ActivePackages` write in `report_package_transition`)

## Description
Both comments justify clearing `ActivePackages` for a brand-new actor because "an earlier occupant of a reused entity id left" something. `World::spawn` never recycles ids (`world.rs`, #372), and the live engine never lowers `next_entity` (`set_next_entity` has callers only in `crates/save/src/driver.rs` and tests; the live load path reloads the cell and applies deltas instead of calling `restore_world`). `retire_active_packages` (#5870) already removes the unloaded actors' rows, so a fresh id can never find a stale `ActivePackages` entry. The `(None, None)` call is therefore a no-op that reads as a lifecycle safeguard. Added by #5864's change.

## Evidence
The two comments quoted above (verified at publish time: "This call only clears what an earlier occupant of a reused entity id left." in `ai_package.rs`; "...clear what an earlier occupant of a reused entity id left." in `package.rs`).

## Impact
None at runtime. It teaches the wrong ECS model (id reuse) - the exact misconception the ECS skill tells auditors not to propagate ("Do not describe it as generational").

## Related
#5864, #5870, #372.

## Suggested Fix
Reword both comments to name the real contract (unload retires rows via `retire_active_packages`; a restore-style `set_next_entity` rewind is test-only), or delete the spawn-time call and keep a test that a fresh actor has no row.

Source: docs/audits/AUDIT_ECS_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
