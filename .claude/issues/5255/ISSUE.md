# #5255 — SAVE-D1-2026-10-05-01: The save registry-completeness guard only sees literal impl Component/Resource lines — 8 fully-qualified impls are unclassified, and PendingGearRelease survives a load on the player

- **Labels**: low,save-load,test-gap,bug
- **Filed from**: `docs/audits/AUDIT_SAVE_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5255

- **Severity**: LOW
- **Dimension**: Snapshot Completeness & the Two Lists
- **Data-Loss Class**: none (guard gap; one latent presentation path)
- **Location**:
  - `byroredux/src/save_io/registry_completeness_tests.rs:128-137` (`impl_target_type`), `:526` (stale `ItemEventBatch` reason);
  - `byroredux/src/npc_spawn.rs:1166` (`PendingGearRelease`);
  - `byroredux/src/save_io.rs:1869` (only `PendingGearImport` is removed);
  - `byroredux/src/app_events.rs:1036` vs `:1062` (`step_save_loads` precedes `gear_import_loader.step`).
- **Status**: NEW. This audit owns GAME-D7-2026-10-05-01 (Related), as `AUDIT_GAMEPLAY_2026-10-05.md` routed it here. It is the same class as the closed #3166 (guard reach).
- **Description**:
  1. `impl_target_type` strips only the two literal prefixes. `rg 'impl [a-z_:]+::(Resource|Component) for'` finds 13 fully-qualified impls. Five are classified only because a row was added by hand (`NpcEquipmentPart`, `NpcSkeletonBones`, `ActorBodyClass`, `PendingGearImport`, `DialogueSurfaceState`). Eight are classified nowhere: `TriggerOccupancyState`, `GracefulExitRequested`, `SceneEffectSoftCache`, `ScaleformHudDiag`, `PendingGearRelease`, `ScriptProvider`, `HudControl`, `CellLoadPhaseTimings`. Classified by hand today, all eight are correctly unsaved:
     - `TriggerOccupancyState` is `EntityId`-keyed detector scratch;
     - `GracefulExitRequested` is a process flag;
     - `SceneEffectSoftCache` is a render cache;
     - `ScaleformHudDiag` and `CellLoadPhaseTimings` are diagnostics;
     - `ScriptProvider` is an asset provider;
     - `HudControl` holds HUD debug pins;
     - `PendingGearRelease` is handoff scratch.
  2. The guard does not check the reverse direction (allowlisted name ↔ discovered impl). A hand row survives its type being renamed or deleted, and the hand rows above would stay green even if a type's impl were rewritten.
  3. `ItemEventBatch`'s reason ("no reader yet (#4713)") is stale: #5028's `queue_gear_releases` reads it (`loot_appearance.rs:306`). The classification is still right.
  4. Concrete consequence: after the overlay, #5034 removes the player's in-flight `PendingGearImport` because it would attach the pre-load session's gear. The sibling `PendingGearRelease` is not removed. It is queued in Late and drained by `gear_import_loader.step` *after* `step_save_loads`. A release queued in the load's frame would therefore despawn a gear root that `reconcile_worn_gear` just revealed for the loaded slots. Nothing re-imports it without a new equip event. The path is latent: GAME-D1-2026-10-05-02 shows the queue has no production producer yet.
- **Impact**: the next fully-qualified component or resource that does need saving passes the guard silently. Once a player-side item removal lands, a load in the same frame as a worn-item removal leaves the restored armor invisible.
- **Suggested Fix**:
  - Match any path prefix, e.g. `impl\s+(?:[\w:]+::)?(Component|Resource)\s+for\s+(\w+)`.
  - Allowlist the eight types with reasons.
  - Add the reverse check (every allowlisted name is discovered).
  - Fix the `ItemEventBatch` reason.
  - Remove `PendingGearRelease` from the player beside the `PendingGearImport` removal at `save_io.rs:1869`.

This finding also owns GAME-D7-2026-10-05-01 from `AUDIT_GAMEPLAY_2026-10-05.md` (same guard-reach defect; not filed separately).

_Source: `AUDIT_SAVE_2026-10-05.md` (SAVE-D1-2026-10-05-01), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
