# ECS-followup: HorseTetherState/ActorCinematicState never terminate, so cinematic-retained entities have no re-adoption path

**Labels**: bug,ecs

## Description

Follow-up to #3254 (ECS-2026-08-24-06), which fixed the "any unrelated cell unload permanently orphans a live cinematic anywhere in the world" half of the retention bug (scoped the `CellRoot` strip to `victims ∩ retained` instead of the world's whole retained set).

That fix does **not** solve the underlying reversibility problem it explicitly deferred: once a cinematic entity's own home cell *does* unload while it's still retained, it permanently loses `CellRoot` with no path that ever gives it back. There are two structural gaps, confirmed against current source:

1. **`HorseTetherState` is never removed anywhere in production code.** `grep -rn "HorseTetherState" crates/scripting/ byroredux/` shows every non-test hit is either construction (`crates/scripting/src/trigger.rs`, `crates/scripting/src/fragment.rs`) or a field-only mutation of `route_target_form_id` (`byroredux/src/systems/cinematic.rs`). The retention predicate in `cinematic_retained_entities` (`byroredux/src/cell_loader/unload.rs`) can therefore never become false on its own for a tethered cart — its retention is permanent by construction, regardless of whether the in-game cart scene has actually finished.

2. **No re-adoption path exists.** Even if a tether legitimately ends, nothing re-stamps `CellRoot` on the previously-retained entity or otherwise reintegrates it into `CellRootIndex`. Once `strip_retained_cell_root` fires (now correctly scoped to the entity's own home-cell unload, post-#3254), the entity is permanently unowned: no future unload can find it via `CellRootIndex`, and no cell-load path re-associates a free-roaming already-spawned entity with a cell root.

## Impact

Vanilla Skyrim's opening cart convoy is the canonical trigger. After the convoy's own home cell unloads (which will eventually happen on any real playthrough — post-#3254 this at least requires the cart's *actual* cell to unload, not just any nearby one, but it is not a matter of *if*, only *when*), the cart, horse, every rider, and all their child bone/render entities become permanently resident and rendered at their last transform, across worldspace changes and interior transitions, with their GPU resources (mesh/texture/normal-map/skin-slot/morph-slot handles) held for the process lifetime.

## Suggested Fix (from #3254's original suggested-fix text, still unaddressed)

Either:
- (a) Reparent retained entities onto a dedicated long-lived "cinematic root" registered in `CellRootIndex`, so a tether-termination event can despawn (or reintegrate) the whole set through the normal unload path, or
- (b) Keep entities un-rooted while retained, but record them in a new `RetainedCinematicEntities` resource, and add a cinematic-completion system (currently missing entirely) that drains it — either re-stamping `CellRoot` at the entity's current position/cell or despawning the whole retained subtree once the tether/cinematic state is known to have ended.

Either path requires first determining **when** a `HorseTetherState`/`ActorCinematicState` should actually end — this needs research into the vanilla cart-script Papyrus fragment (or scene) that drives the tether, not a guess. Per this project's no-guessing policy, that research should happen before implementation, likely via the Fandom MCP connector or the legacy Gamebryo/Creation Engine source for the relevant quest/scene.

## Completeness Checks
- [ ] Research: identify the vanilla trigger for tether/cinematic termination (script fragment, scene phase, or distance-based heuristic Bethesda actually uses)
- [ ] Add the termination path once the trigger is known
- [ ] Add re-adoption (or clean despawn) for entities whose `CellRoot` was already stripped before termination is detected
- [ ] TESTS: a full lifecycle test — tether starts, cell unloads (entity retained, no longer rooted), tether ends, entity is either reintegrated or cleanly despawned, not left as a permanent zombie

_Split from #3254 (AUDIT_ECS_2026-08-24.md, ECS-2026-08-24-06) — that issue's mechanical, low-risk half (scope the strip to this cell's own victims) is fixed; this tracks the design-level half (make retention actually reversible) that needs research before it can be implemented safely._