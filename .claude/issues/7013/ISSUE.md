# #7013 — PHYS-D2-02: `movable_body_blocks_line` does not exclude the target's (or shooter's) multi-body ragdoll, so a knocked-down target blocks its own shot

**Labels**: low,physics,ai,combat,bug · **Filed from**: `docs/audits/AUDIT_PHYSICS_2026-11-15.md` (2026-11-15)

- **Severity**: LOW
- **Dimension**: Dim 2 (scene-query contract, `47207ecb0`) / caller `byroredux/src/systems/faction_hostility.rs:113-124`
- **Location**: `crates/physics/src/world/queries.rs:428-451`; caller `byroredux/src/systems/faction_hostility.rs:117-123`
- **Trigger Conditions**: A UseWeapon package with Hold Fire When Blocked (`options.hold_fire_when_blocked`, `byroredux/src/systems/script_requests.rs:122`) whose target is a ragdolled actor (knocked down by `push_actor_system`, or any actor with `RagdollActive`).
- **Status**: NEW
- **Description**: `excluded_bodies` is built from ONE handle per actor, `RapierHandles.body` of the shooter and target entities (the stand-in capsule or placement body). A live actor's bone bodies are keyframed (kinematic), so they are not "movable" and the test is correct for them. When an actor is ragdolled, its bones are separate DYNAMIC bodies on their own entities and none are in `excluded_bodies`. `sight_points` aims at `placement root + FALLBACK_ACTOR_CAPSULE_CENTRE_HEIGHT` (`detection/sight.rs:121-133`), which lies inside the ragdoll's torso. The ray therefore first-hits one of the target's own dynamic bone colliders, and `is_dynamic()` is true. The doc comment promises "a dynamic ragdoll target would otherwise block its own shot", but the exclusion only covers the single capsule body. The unit test uses a one-body target and cannot see this.
- **Evidence**: queries.rs 437-445 filter `!collider.parent().is_some_and(|body| excluded_bodies.contains(&body))`; the caller passes `[shooter, target].filter_map(handles.get(entity).map(|h| h.body))`. Ragdoll bodies live on `Ragdoll.bodies` entities (`knockdown.rs:106`). Nothing maps them back to the owning actor, although `ActorColliderOwner` stamps every ragdoll body with the placement root (#6255).
- **Impact**: The `UseWeaponRun` "waits without giving up" (script_requests.rs:122-125), so a shooter holds fire for as long as its target is down, normally the get-up window, then resumes. Behaviourally wrong for a few seconds per knockdown. Also, a third actor's ragdoll corpse between shooter and target counts as a "movable object". That may be intended; confirm against the GECK text.
- **Related**: #6656 (AIP-UW-003), #6255 (`ActorColliderOwner`).
- **Suggested Fix**: Resolve exclusion by owner, not by handle. In the caller, add the `Ragdoll.bodies` handles of shooter and target to `own_bodies`, or key the predicate on `ActorColliderOwner`. Add a test with a two-body (jointed or dynamic) target.

Source: `docs/audits/AUDIT_PHYSICS_2026-11-15.md`

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
