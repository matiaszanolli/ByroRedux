# #6237: SAVE-2026-10-21-01: The `RigidBodyData` delta column can never overlay a placed reference, so a scripted `SetMotionType` is lost on every live load (the #2379 fix only covers `restore_world`)

**Labels**: medium,save-load,physics,scripting,bug

**Source**: docs/audits/AUDIT_SAVE_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Snapshot Completeness & the Two Lists · **Location**: `byroredux/src/save_io/registry.rs:134`

- **Data-Loss Class**: silent-drop
- **Location**:
  - `byroredux/src/save_io/registry.rs:134` (`"RigidBodyData"` in `MUTABLE_DELTA_COLUMNS`) and `:397` (`register_component_preserving`).
  - `crates/save/src/driver.rs:273-282` (`build_form_id_remap` keys only `FormIdComponent`).
  - `byroredux/src/cell_loader/spawn.rs:1488-1497`, `:1545-1577`, `:288-306`, `:419-427` (every collision entity carries `PhysicsSourceForm`, never `FormIdComponent`).
  - `byroredux/src/systems/cinematic.rs:436-445` (the `SetMotionType` writer).
  - `byroredux/src/save_io/round_trip_tests.rs:766-814` (the #2379 regression test).
- **Status**: NEW (the fix for closed #2379 is incomplete; related #5686)
- **Description**:
  1. The live load overlays a column only onto entities that `build_form_id_remap` resolves, and it resolves only entities carrying a `FormIdComponent`.
  2. Every production `RigidBodyData` carrier for a placed reference is a separate collision entity: per-shape bhk colliders, graph-node bodies, the packed proxy ghost and the trimesh ghost. Each is stamped with `PhysicsSourceForm`, which is deliberately not `FormIdComponent`, so that `find_by_form_id` stays unambiguous (the `spawn.rs:1560-1571` comment).
  3. These rows are therefore captured by `save_world` (one row per collider, for every resident body) and then dropped by `apply_deltas`' `filter_map`. They are also not counted as "unresolved", because the body entity has no saved FormId row at all.
  4. `SetMotionType` writes `motion_type` on exactly these child bodies (`reference_body_entities` → `RigidBodyData` + `PhysicsWorld::set_motion_type`). A scripted motion-type change therefore survives only a `restore_world` round trip.
  5. `restore_world` is the test/loose path. `rigid_body_data_survives_save_load_round_trip` exercises only that path, so it stays green.
  6. The same mismatch makes #5686's `keep_reimported_part_surface_materials` hook and the `RigidBodyData` "NoReconcilerNeeded" disposition vacuous in production. The row #5686 described ("every FormID-matched collider") does not exist.
- **Evidence**:
  - `driver.rs:277-280`: `q.iter().filter_map(|(eid, comp)| pool.resolve(comp.0)...)` over `world.query::<FormIdComponent>()`.
  - `spawn.rs:1491-1497`: `world.insert(entity, body); … world.insert(entity, PhysicsSourceForm(fid));`. No production site inserts `RigidBodyData` on an entity that also gets a `FormIdComponent`. I grepped every `insert(.., RigidBodyData` in `byroredux/src`.
- **Impact**:
  - A quest that makes a prop dynamic, or freezes one keyframed, reverts to the authored motion type after any save/load.
  - `defaultDisableHavokOnLoad`-style scripts re-apply on `OnLoad` and self-heal, which is why this is MEDIUM and not the HIGH floor for "a load that drops gameplay state": no progression is lost.
  - Every save also carries one dead `RigidBodyData` row per resident collider (thousands in an exterior), and they bloat the payload for nothing.
  - #2379 reads as fixed while the production path never received the fix.
- **Related**: #2379, #5686, SAVE-2026-10-21-04 (the same column's `UNAUDITED` eviction row), PHYS-D3-2026-10-21-02 (bone bodies rebuilt from `RigidBodyData::default()`).
- **Suggested Fix**:
  - Persist scripted motion-type overrides the way enable and lock state are persisted: a FormID-keyed ledger resource (`ReferenceMotionTypeState`, placement FormID → motion type), written by the `SetMotionType` system, read at body spawn and restored before the reload (`PRE_RELOAD_RESOURCES`).
  - Then drop `RigidBodyData` from `MUTABLE_DELTA_COLUMNS` and from the registry, and retire the preserve hook.
  - Pin it with a live-path test: spawn a collider carrying `PhysicsSourceForm`, run `build_form_id_remap` + `apply_deltas`, and assert that the override survives.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
