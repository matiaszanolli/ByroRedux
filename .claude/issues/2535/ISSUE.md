# SAVE-D4-02: HorseTetherState.horse / ActorCinematicState.vehicle entity references are invisible to every validate_world check

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2535
**Finding ID**: SAVE-D4-02

**Severity**: MEDIUM
**Dimension**: 4 — Validation Gates
**Data-Loss Class**: reference-break
**Location**: `crates/scripting/src/cinematic.rs:120-144` (`ActorCinematicState.vehicle: Option<EntityId>`), `:171-179` (`HorseTetherState.horse: EntityId`); `byroredux/src/save_io.rs:273-274` (both registered); `crates/save/src/validate.rs` (no check touches either type)
**Status**: NEW

## Description
Both components carry a direct `EntityId` reference to another entity (mounted vehicle / tethered horse). None of `validate_world`'s four reference-class checks inspect them — `validate_hierarchy` only walks `Parent`/`Children`, `validate_equipment` only walks `EquipmentSlots`↔`Inventory`, `validate_animation` only walks `AnimationPlayer`, `validate_inventory_instances` only walks `Inventory.items[].instance`. A save with `HorseTetherState.horse` (or `ActorCinematicState.vehicle`) pointing at an id `>= next_entity`, or at a live-but-unrelated entity, currently passes `validate_world` cleanly and is written with no diagnostic. Both types are also deliberately excluded from `MUTABLE_DELTA_COLUMNS` (a separate, already-addressed finding), so the live `execute_pending_save_loads` path never overlays a stale value onto a reloaded cell and carries no live risk from this specific gap — the residual window is: (a) a pre-write save capturing an already-dangling reference (e.g. the tethered horse despawned mid-session while the tether component survived) is written silently, and (b) the `restore_world` test/loose path restores components verbatim at saved ids with the same blind spot in its post-load diagnostic re-run.

## Evidence
Confirmed directly: `crates/save/src/validate.rs` declares exactly `validate_hierarchy`, `validate_equipment`, `validate_animation`, `validate_inventory_instances` — no fifth check. Consumption is defensive (`byroredux/src/systems/cinematic.rs:271,306` — `transforms.get(tether.horse)?` / `transforms.get(state.vehicle?)?`), so a dangling id fails the `?` gracefully rather than panicking — this caps severity at MEDIUM (silently-skipped pose sync, not a crash) rather than HIGH/CRITICAL.

## Impact
The subsystem's defense-in-depth thesis ("the gate sees every reference class that could go stale") is not actually true for these two types; today's runtime consequence is silent and non-crashing, but a dangling reference is written and reloaded with zero diagnostic anywhere in the pipeline.

## Suggested Fix
Add a fifth `validate_world` sub-check (e.g. `validate_entity_refs`) that walks any component known to carry a bare `EntityId` field — starting with these two — and flags `id >= next_entity` the same way `validate_hierarchy`/`validate_animation` already do. Establishes a pattern future `EntityId`-bearing components can slot into instead of each needing a bespoke check.

## Completeness Checks
- [ ] **TESTS**: A regression test constructs a save with a dangling `HorseTetherState.horse`/`ActorCinematicState.vehicle` and confirms `validate_world` now flags it
- [ ] **SIBLING**: Confirm no other `EntityId`-bearing component has the same validation blind spot
