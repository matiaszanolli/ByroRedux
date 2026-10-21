# #6326: PHYS-D3-2026-10-21-02: `stand_up` rebuilds bone bodies from `RigidBodyData::default()` instead of the authored rows activation removed

**Labels**: low,bug,physics

**Source**: docs/audits/AUDIT_PHYSICS_2026-10-21.md · **Severity**: LOW · **Dimension**: Ragdoll & Constraint Seam · **Location**: `byroredux/src/systems/revive.rs:82-90` (bones), `:92-101` (stand-in, which matches its install recipe, so it is fine); compare `byroredux/src/ragdoll.rs:549-553` (activation removes the authored rows) and `crates/core/src/ecs/components/collision.rs:102-167` (fields)

- **Severity**: LOW
- **Dimension**: Ragdoll & Constraint Seam
- **Location**: `byroredux/src/systems/revive.rs:82-90` (bones), `:92-101` (stand-in, which matches its install recipe, so it is fine); compare `byroredux/src/ragdoll.rs:549-553` (activation removes the authored rows) and `crates/core/src/ecs/components/collision.rs:102-167` (fields)
- **Status**: NEW. `revive.rs` was added after the baseline (`d1b298921`, 2026-10-18). GAME-D4-2026-10-21-01/-02 cover the animation and idle half of `knock_down` / `stand_up`, not the collision rows.
- **Trigger Conditions**: a revived actor (`Resurrect`) or a knocked-down actor getting up (`get_up_system`) whose skeleton bones had authored bodies (FO3/FNV/Oblivion via `PushActorAway`, any game via `Resurrect`).
- **Description**: Activation deletes each body bone's `RigidBodyData`. That row is the imported row with only `motion_type` flipped to Keyframed (`keyframe_live_ragdoll_bones`). `stand_up` re-inserts `RigidBodyData { motion_type: Keyframed, ..Default::default() }`. That loses `collidable` (a bone authored on the Havok non-collidable layer comes back solid instead of as a sensor), `havok_layer` (→ 0 "Unidentified", which `ApplyHavokImpulse` scaling reads), `surface_material`, `part_surface_materials`, friction, restitution and mass. It also inserts the row for every template bone. On FO4 those bones never had a row: they have no `CollisionShape` and never register, so the rows are inert strays.
- **Evidence**: `revive.rs:84-88`. `collision.rs:149-167` (`static_body()`: friction 0.5, restitution 0.3, `havok_layer` 0, `collidable` true).
- **Impact**: After a get-up or revive, a live actor's keyframed bone colliders differ from the ones it spawned with. The visible effects are small (contact coefficients against clutter, the sensor/solid flag, the layer-scaled impulse). The state is still not what the content authored.
- **Related**: GAME-D4-2026-10-21-01, GAME-D4-2026-10-21-02, #6134.
- **Suggested Fix**: Have activation park the removed rows (for example a `RetiredBoneBodies(Vec<(EntityId, RigidBodyData)>)` component on the skeleton root) and have `stand_up` restore exactly those. Bones that had no row stay without one.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_PHYSICS_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
