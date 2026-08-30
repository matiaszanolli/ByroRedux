# CONC-D3-2026-08-30-03: `scene.show` takes `SceneRegistry -> ScenePlayer`; `actor_quest_trigger_is_in_sequence` takes the reverse

**Issue**: #3650
**Labels**: bug, ecs, medium, concurrency
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D3-2026-08-30-03 (MEDIUM, D3 · ECS Lock Ordering & Deadlock).

**Location**: `byroredux/src/commands/quest.rs:474-491` and `crates/scripting/src/trigger.rs:341-346`.

## Description

Two sites acquire the same pair in **exactly opposite orders**, and both bind through `let ... else` / `world.get`, so both guards stay live while the pair is used together. Neither pair member appears in `docs/engine/ecs.md`'s canonical order table.

## Evidence

```rust
// crates/scripting/src/trigger.rs:341-346 — ScenePlayer → SceneRegistry
let Some(players) = world.query::<crate::ScenePlayer>() else { return true; };
let Some(registry) = world.try_resource::<crate::SceneRegistry>() else { return true; };
```
```rust
// byroredux/src/commands/quest.rs:474-491 — SceneRegistry → ScenePlayer
let Some(registry) = world.try_resource::<SceneRegistry>() else { ... };
let Some(definition) = registry.definition(form_id) else { ... };
let Some(entity) = registry.scene_entity(form_id) else { ... };
let Some(player) = world.get::<ScenePlayer>(entity) else { ... };
...
one_line_text(&definition.editor_id, 80)   // `registry` still borrowed here
```

## Trigger Conditions

Debug build with `BYRO_LOCK_ORDER_CHECK=1`: any frame in which a trigger volume is entered runs `actor_quest_trigger_is_in_sequence` (`trigger.rs:296`) and records `ScenePlayer -> SceneRegistry`; a `scene.show <formid>` from `byro-dbg` then records the reverse and panics.

## Impact

No live deadlock — `trigger_detection_dispatch` is `add_exclusive(Stage::Update)` (`boot.rs:877`) and `scene.show` runs in the exclusive `DebugDrainSystem`, so the holds cannot overlap.

The concrete cost is the debug-build detector abort, **plus the safety being entirely circumstantial**: promoting `trigger_detection_system` to `add_to_with_access` is a one-line change with no compile-time or test-time signal (`docs/engine/ecs.md:639-643`).

## Related

#2388, #3445; CONC-D3-2026-08-30-04 (the missing canonical-table entry).

## Suggested Fix

**Snapshot in `scene.show`** — clone the `SceneDefinition` fields (or just the `editor_id`) and the `scene_entity` id, `drop(registry)`, then `world.get::<ScenePlayer>`. That makes the command's order `SceneRegistry`-only, leaving `trigger.rs`'s `ScenePlayer -> SceneRegistry` as the single recorded direction.

## Completeness Checks
- [ ] **LOCK_ORDER**: The `SceneRegistry` guard is *dropped* before the `ScenePlayer` acquisition, not merely reordered
- [ ] **SIBLING**: The other scene/quest console commands in `byroredux/src/commands/quest.rs` audited for the same held-registry shape (`QuestStageState`, `QuestAdvanceOnActivate`)
- [ ] **TESTS**: `BYRO_LOCK_ORDER_CHECK=1` with a trigger-volume frame + `scene.show` in the same process must not panic
- [ ] **DOCS**: The cluster added to `docs/engine/ecs.md`'s canonical order table
