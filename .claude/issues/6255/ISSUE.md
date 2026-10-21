# #6255: PHYS-D3-2026-10-21-01: FO4 decoded-ragdoll corpses have no collider that resolves to the actor, so the body cannot be targeted and can occlude itself

**Labels**: medium,bug,physics,gameplay,game:fo4

**Source**: docs/audits/AUDIT_PHYSICS_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Ragdoll & Constraint Seam · **Location**: `byroredux/src/ragdoll.rs:555-585` (stand-in retirement), `byroredux/src/npc_spawn.rs:353-383` (`keyframe_live_ragdoll_bones`: owner only on bones with a `CollisionShape`), `byroredux/src/interaction.rs:891-935` (`select_interaction_target`), `:999-1062` (`corpse_collider_actors`), `:1064-1079` (`ray_hit_actor`), `:1081-1105` (`ray_hit_entity`), `:1150-1172` (`collider_belongs_to_target`)

- **Severity**: MEDIUM
- **Dimension**: Ragdoll & Constraint Seam
- **Location**: `byroredux/src/ragdoll.rs:555-585` (stand-in retirement), `byroredux/src/npc_spawn.rs:353-383` (`keyframe_live_ragdoll_bones`: owner only on bones with a `CollisionShape`), `byroredux/src/interaction.rs:891-935` (`select_interaction_target`), `:999-1062` (`corpse_collider_actors`), `:1064-1079` (`ray_hit_actor`), `:1081-1105` (`ray_hit_entity`), `:1150-1172` (`collider_belongs_to_target`)
- **Status**: NEW. Introduced by `7c6e6c3e1` (#6070) on top of `e1bd40bcf`. No match in open or closed issues ("corpse ActorColliderOwner", "ragdoll corpse loot target").
- **Trigger Conditions**: an FO4 actor whose skeleton carries a decoded `bhkRagdollSystem` template (human, feral ghoul, super mutant, molerat, …) is dead and ragdolled. Its body has moved off its placement-root bound, or pokes out of that bound toward the player.
- **Description**: Every interaction and combat path maps a ray hit to an actor through `ActorColliderOwner`. `keyframe_live_ragdoll_bones` stamps that marker only on skeleton bones that carry a `CollisionShape`. FO4 bones carry none, because the articulation lives in the blob. That is exactly why the stand-in capsule (`install_fallback_actor_collider`) is installed, and the capsule is the actor's only owner-marked collider. At death, #6070 frees the capsule's body and `RigidBodyData`, and the decoded bodies take over. They hang on bone entities that have no owner marker. The results:
  1. `corpse_collider_actors` keeps only owners whose marked entity has a live collider. The capsule no longer has one, and the bones are not marked, so the FO4 corpse never enters `physical_corpses`.
  2. `ray_hit_actor` resolves a hit on a ragdoll body through `ray_hit_entity`'s `Ragdoll` fallback to the bone entity. `get::<ActorColliderOwner>(bone)` is `None`, so the "actor" is the bone, which is not a candidate. The physical-hit path never fires.
  3. The corpse is then targeted only through the placement-root bound. The code documents that bound as wrong for a ragdoll: "Collider-backed corpses must be targeted at the body, not also at the stale placement-root bound after the ragdoll has moved away" (`:906-908`) and "A ragdoll may fall away from the placement root's bound" (`:919-921`).
  4. `target_has_line_of_sight` then casts toward that bound. If the corpse's own body is the first thing hit, `collider_belongs_to_target(bone, actor)` fails: the bone is not the actor, has no owner marker and no `FormIdComponent` / `PhysicsSourceForm`. The corpse is reported as occluded by itself.

  Classic rigs are unaffected: their bones were owner-marked at spawn and keep the marker through activation. Before #6070 the FO4 corpse body was the capsule (the one-body template), which was owner-marked.
- **Evidence**: `npc_spawn.rs:372-377` (owner inserted only under `has_shape`). `ragdoll.rs:575-583` (the capsule's `RapierHandles` and `RigidBodyData` removed). `interaction.rs:1012-1020` (owners filtered to marked entities) and `:1037-1050` (only marked entities among the live colliders count). `:1076-1078`: `ActorColliderOwner(collider).map_or(collider, …)` returns the bone.
- **Impact**: An FO4 corpse that a hit or a fall has moved cannot be looted by aiming at it. It is only reachable by aiming at where it was placed, and it can even be rejected there. The same unresolvable mapping affects every `ActorColliderOwner` consumer that meets a decoded FO4 ragdoll body (`combat.rs:414` `resolve_actor_root`, `projectiles.rs:388`, `commands/view.rs:343`). Those mostly matter for corpses, because no FO4 path knocks a living actor down: `PushActorRequest` is raised only by ObScript.
- **Related**: #6070, #5015, #6134 (the retired capsule also becomes another shape-only entity of that class).
- **Suggested Fix**: When a template is installed (or at activation), insert `ActorColliderOwner(placement_root)` on every template bone that lacks one, so all body producers share the classic invariant. Alternatively, have `ray_hit_entity`'s `Ragdoll` fallback return the ragdoll's owner. The first fix also covers combat and projectiles. Add an interaction test with a decoded multi-body template and no shaped bones.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_PHYSICS_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
