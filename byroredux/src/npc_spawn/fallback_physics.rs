//! Fallback collider / ragdoll install for actors with no authored physics, and live-ragdoll bone keyframing.

use super::*;

/// #1698 — keyframe a live NPC's ragdoll bones.
///
/// Skyrim (and FO3/FNV/Oblivion) author each skeleton ragdoll bone's bhk body
/// as `MO_SYS_DYNAMIC`, but a *living* actor's ragdoll must be **keyframed to
/// the animation** — driven by the animated skeleton, inert and hittable —
/// and only free-simulate on death. Importing them as free Dynamic bodies lets
/// ~18 bones/NPC collapse and free-fall (nothing drives them, no floor beneath
/// the spawn), and ~480+ such bodies across a dense interior pin
/// `physics_sync_system`'s dynamic solver at ~140 ms/frame for ~28 s
/// (Dragonsreach RT-1).
///
/// Flip each skeleton bone's Dynamic collision body to [`MotionType::Keyframed`]
/// **before** the first `physics_sync_system` registers it with Rapier, so it
/// registers as a kinematic body. `push_kinematic` then drives it from the
/// bone's animation-written `GlobalTransform` each frame (skipping idle bones),
/// keeping it out of the dynamic solver entirely. Death-time ragdoll activation
/// (`RagdollActive` / `build_ragdoll`) is unaffected — it rebuilds the
/// simulated ragdoll separately.
///
/// #2873 — each skeleton bone that carries a collision shape also gets an
/// [`ActorBoneCollider`](byroredux_physics::ActorBoneCollider) marker, so
/// registration files its colliders under
/// [`ACTOR_BONE_GROUP`](byroredux_physics::ACTOR_BONE_GROUP) and downward
/// floor probes skip them. Without it, `locomotion::step_toward`'s ground-snap
/// ray — cast from 256 BU above the actor's own root — meets the actor's
/// upper-body bone before it reaches the floor and re-seats the root at that
/// bone's height; because the bones are then driven from the root's
/// `GlobalTransform`, the next tick casts from higher still. That is a
/// monotonic elevator, not a one-off offset: walking NPCs ascend out of the
/// cell. `exclude_rigid_body` cannot fix it — each bone is a separate body.
pub(super) fn keyframe_live_ragdoll_bones(
    world: &mut World,
    actor_root: EntityId,
    skel_map: &std::collections::HashMap<std::sync::Arc<str>, EntityId>,
) -> Option<EntityId> {
    use byroredux_core::ecs::components::collision::CollisionShape;

    let mut has_gameplay_collider = false;
    for &bone in skel_map.values() {
        if let Some(body) = world.get_mut::<RigidBodyData>(bone) {
            if body.motion_type == MotionType::Dynamic {
                body.motion_type = MotionType::Keyframed;
            }
        }
        // Tag every bone that actually registers a collider, whatever its
        // authored motion type — a bone shipped Keyframed upstream is just as
        // self-hittable as one flipped above.
        let has_shape = world
            .query::<CollisionShape>()
            .is_some_and(|q| q.contains(bone));
        if has_shape {
            has_gameplay_collider = true;
            world.insert(bone, byroredux_physics::ActorBoneCollider);
            world.insert(bone, byroredux_physics::ActorColliderOwner(actor_root));
        }
    }

    if !has_gameplay_collider {
        Some(install_fallback_actor_collider(world, actor_root))
    } else {
        None
    }
}

/// The shape-less-actor fallback capsule's half-height (BU, excludes caps).
/// `pub(crate)`: the NPC KCC (`systems/locomotion.rs`) derives its sweep
/// capsule from the same pair so walking NPCs sweep with the body the
/// combat ray targeting already assumes (#4689 — the values were copied
/// twice with only doc comments tying them together; #2885 already moved
/// them once and the copies silently aged).
pub(crate) const FALLBACK_ACTOR_CAPSULE_HALF_HEIGHT: f32 = 32.0;
/// See [`FALLBACK_ACTOR_CAPSULE_HALF_HEIGHT`] — total height
/// `2 * (32 + 20) = 104` BU ≈ 1.5 m at the 70 BU/m Havok scale.
pub(crate) const FALLBACK_ACTOR_CAPSULE_RADIUS: f32 = 20.0;
/// Height of the fallback capsule's centre above the placement root (the
/// actor's feet): the capsule rests on the floor. Also where ambient faction
/// hostility (#4414) aims its sight rays — torso height, inside the body the
/// combat ray targets.
pub(crate) const FALLBACK_ACTOR_CAPSULE_CENTRE_HEIGHT: f32 =
    FALLBACK_ACTOR_CAPSULE_HALF_HEIGHT + FALLBACK_ACTOR_CAPSULE_RADIUS;

/// Marks the stand-in targetable capsule [`install_fallback_actor_collider`]
/// gives an actor whose skeleton authors no collision bodies (#6070).
///
/// The capsule exists so a *living* shape-less actor can be hit. When the
/// skeleton instead carries a decoded articulation (FO4's `bhkRagdollSystem`),
/// the death ragdoll's own bodies take over and the capsule must go: left as
/// a keyframed body it overlaps every limb, and the solver ejects them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FallbackActorCollider;

impl byroredux_core::ecs::Component for FallbackActorCollider {
    type Storage = byroredux_core::ecs::SparseSetStorage<Self>;
}

/// Give an otherwise shape-less live actor one conservative, targetable body.
///
/// FO4's shipped skeletons can contain no bhk bone bodies at all.  Their
/// placement roots still have `ActorVitals`, but physical combat only resolves
/// a ray through a registered collider; without this child every swing misses.
/// The fallback is deliberately a child rather than a shape on the root: the
/// placement root is at the actor's feet while the target volume is centred at
/// torso height.  It joins the actor-bone query group so floor and clearance
/// probes ignore it, exactly as they ignore authored live-actor bone shapes.
pub(super) fn install_fallback_actor_collider(world: &mut World, actor_root: EntityId) -> EntityId {
    use byroredux_core::ecs::components::collision::CollisionShape;

    let collider = world.spawn();
    world.insert(
        collider,
        Transform::new(
            Vec3::new(0.0, FALLBACK_ACTOR_CAPSULE_CENTRE_HEIGHT, 0.0),
            Quat::IDENTITY,
            1.0,
        ),
    );
    world.insert(collider, GlobalTransform::IDENTITY);
    world.insert(
        collider,
        CollisionShape::Capsule {
            half_height: FALLBACK_ACTOR_CAPSULE_HALF_HEIGHT,
            radius: FALLBACK_ACTOR_CAPSULE_RADIUS,
        },
    );
    world.insert(
        collider,
        RigidBodyData {
            motion_type: MotionType::Keyframed,
            ..Default::default()
        },
    );
    world.insert(collider, byroredux_physics::ActorBoneCollider);
    world.insert(collider, byroredux_physics::ActorColliderOwner(actor_root));
    world.insert(collider, FallbackActorCollider);
    world.insert(collider, Parent(actor_root));
    add_child(world, actor_root, collider);
    collider
}

/// Install the one-body corpse template associated with a fallback collider.
///
/// This is intentionally separate from collider installation: a shape-less
/// actor can still be targeted while alive even if its skeleton failed to
/// import. A real skeleton root is required before the normal death path can
/// own a `RagdollTemplate` and the existing ragdoll/status/save machinery can
/// treat the body as a corpse.
pub(crate) fn install_fallback_ragdoll_template(
    world: &mut World,
    skeleton_root: EntityId,
    fallback_body: EntityId,
) {
    use byroredux_core::ecs::components::collision::CollisionShape;

    world.insert(
        skeleton_root,
        crate::ragdoll::RagdollTemplate {
            bodies: vec![crate::ragdoll::RagdollTemplateBody {
                bone: fallback_body,
                local_translation: Vec3::ZERO,
                local_rotation: Quat::IDENTITY,
                shape: CollisionShape::Capsule {
                    half_height: 32.0,
                    radius: 20.0,
                },
                mass: 70.0,
                linear_damping: 0.35,
                angular_damping: 1.0,
                friction: 0.8,
                restitution: 0.0,
                pose_bone_id: None,
            }],
            constraints: Vec::new(),
        },
    );
}
