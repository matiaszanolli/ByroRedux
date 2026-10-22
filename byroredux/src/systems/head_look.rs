//! Forced head-tracking (M47.3 ObScript `Look <target>` / `StopLook`).
//!
//! GECK *Look*: "forces the calling actor to head-track the target. This will override all normal
//! head-tracking behavior until StopLook is called, or the target leaves the loaded area." The engine has
//! no ambient head-tracking, so this is the whole of it: while a [`HeadLook`] is on an actor, its head
//! bone is turned from the body's heading toward the target, within the range a neck allows and at a
//! steady rate.
//!
//! The turn is applied as a *world-space delta on the animated pose*: the head keeps whatever the clip put
//! it at, and is rotated about the vertical by the yaw the target is off the body's heading and about the
//! body's right axis by the pitch — no knowledge of the bone's own axes is needed. The system runs after the
//! animation sets the bone's local rotation and before transform propagation, using the previous
//! frame's parent transform (the neck does not move between frames in a way that matters).

use byroredux_core::ecs::collect_subtree;
use byroredux_core::ecs::components::{GlobalTransform, Name, Parent, Transform};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_scripting::obscript_anim::HeadLook;

use crate::components::AnimationTarget;

/// The head can turn this far from the body's heading either way, and nod this far up or down.
const MAX_YAW: f32 = 70.0 * std::f32::consts::PI / 180.0;
const MAX_PITCH: f32 = 40.0 * std::f32::consts::PI / 180.0;
/// The rate the head turns toward the target, radians a second.
const TURN_RATE: f32 = 3.0;
/// A placement root faces its local -Z in the engine's Y-up space (Gamebryo's +Y, through the Z-up to
/// Y-up conversion).
const FORWARD: Vec3 = Vec3::NEG_Z;
/// The head bone of the humanoid and creature skeletons the games ship.
const HEAD_BONE: &str = "bip01 head";
/// How far down a skeleton the head bone can sit; deeper than any shipped rig.
const MAX_HEAD_DEPTH: usize = 32;

fn find_head(world: &World, root: EntityId) -> Option<EntityId> {
    let symbol = world.try_resource::<StringPool>()?.get(HEAD_BONE)?;
    // Walk `Children` first and take the `Name` query only afterwards: the rest of the tree
    // acquires `Children` before `Name`, and holding `Name` across the walk is the inverse
    // order the lock-order detector rejects (#6062).
    let subtree = collect_subtree(world, root);
    let names = world.query::<Name>()?;
    subtree.into_iter().find(|&entity| names.get(entity).is_some_and(|name| name.0 == symbol))
}

/// Whether `candidate` is still the head bone of `root`'s skeleton: it carries the head name and
/// `root` is among its ancestors. Cheap (a name read and a few parent hops), so a cached head is
/// re-proved every frame rather than trusted across a despawn / entity-id reuse.
fn is_head_of(world: &World, candidate: EntityId, root: EntityId, symbol: byroredux_core::string::FixedString) -> bool {
    if world.get::<Name>(candidate).is_none_or(|name| name.0 != symbol) {
        return false;
    }
    let mut at = candidate;
    for _ in 0..MAX_HEAD_DEPTH {
        match world.get::<Parent>(at).map(|parent| parent.0) {
            Some(parent) if parent == root => return true,
            Some(parent) => at = parent,
            None => return false,
        }
    }
    false
}

/// The head bone for `root`: the one `HeadLook` cached if it is still valid, else a subtree walk.
fn head_bone(world: &World, root: EntityId, cached: Option<EntityId>) -> Option<EntityId> {
    if let Some(candidate) = cached {
        let symbol = world.try_resource::<StringPool>()?.get(HEAD_BONE)?;
        if is_head_of(world, candidate, root, symbol) {
            return Some(candidate);
        }
    }
    find_head(world, root)
}

fn signed_angle(from: f32, to: f32) -> f32 {
    let mut delta = to - from;
    while delta > std::f32::consts::PI {
        delta -= std::f32::consts::TAU;
    }
    while delta < -std::f32::consts::PI {
        delta += std::f32::consts::TAU;
    }
    delta
}

pub(crate) fn head_look_system(world: &World, dt: f32) {
    let looks: Vec<(EntityId, HeadLook)> = match world.query::<HeadLook>() {
        Some(query) if !query.is_empty() => query.iter().map(|(actor, look)| (actor, *look)).collect(),
        _ => return,
    };
    let dt = if dt.is_finite() { dt.clamp(0.0, 0.25) } else { 0.0 };
    for (actor, look) in looks {
        // "or the target leaves the loaded area": a target with no placement is gone.
        let Some(target_position) = world.get::<GlobalTransform>(look.target).map(|t| t.translation) else {
            if let Some(mut looks) = world.query_mut::<HeadLook>() {
                looks.remove(actor);
            }
            continue;
        };
        let Some(actor_global) = world.get::<GlobalTransform>(actor).map(|t| *t) else { continue };
        let root = world.get::<AnimationTarget>(actor).map_or(actor, |target| target.skeleton_root);
        let Some(head) = head_bone(world, root, look.head) else { continue };
        let Some(parent) = world.get::<Parent>(head).map(|parent| parent.0) else { continue };
        let Some(parent_global) = world.get::<GlobalTransform>(parent).map(|t| *t) else { continue };
        let Some(local) = world.get::<Transform>(head).map(|t| *t) else { continue };

        // Where the head is, in the world (last frame's parent, this frame's animated local).
        let head_world_rotation = parent_global.rotation * local.rotation;
        let head_position = parent_global.translation + parent_global.rotation * (local.translation * parent_global.scale);
        let to_target = target_position - head_position;
        if to_target.length_squared() < 1e-6 {
            continue;
        }
        let desired = to_target.normalize();
        let forward = actor_global.rotation * FORWARD;
        let forward_flat = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
        if forward_flat == Vec3::ZERO {
            continue;
        }
        let heading = forward_flat.x.atan2(forward_flat.z);
        let want_yaw = signed_angle(heading, desired.x.atan2(desired.z)).clamp(-MAX_YAW, MAX_YAW);
        let want_pitch = desired.y.clamp(-1.0, 1.0).asin().clamp(-MAX_PITCH, MAX_PITCH);
        let step = TURN_RATE * dt;
        let yaw = look.yaw + (want_yaw - look.yaw).clamp(-step, step);
        let pitch = look.pitch + (want_pitch - look.pitch).clamp(-step, step);
        if let Some(mut looks) = world.query_mut::<HeadLook>() {
            if let Some(stored) = looks.get_mut(actor) {
                stored.yaw = yaw;
                stored.pitch = pitch;
                stored.head = Some(head);
            }
        }
        // Turn about the vertical by the yaw, nod about the body's (turned) right axis by the pitch.
        let turn = Quat::from_rotation_y(yaw);
        let right = (turn * forward_flat).cross(Vec3::Y).normalize_or_zero();
        let nod = Quat::from_axis_angle(right, pitch);
        let new_world = nod * turn * head_world_rotation;
        let new_local = parent_global.rotation.inverse() * new_world;
        if let Some(mut transforms) = world.query_mut::<Transform>() {
            if let Some(transform) = transforms.get_mut(head) {
                transform.rotation = new_local.normalize();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::Children;

    fn bone(world: &mut World, parent: Option<EntityId>, name: &str, at: Vec3) -> EntityId {
        let entity = world.spawn();
        let symbol = world.resource_mut::<StringPool>().intern(name);
        world.insert(entity, Name(symbol));
        world.insert(entity, Transform::new(at, Quat::IDENTITY, 1.0));
        world.insert(entity, GlobalTransform::new(at, Quat::IDENTITY, 1.0));
        if let Some(parent) = parent {
            world.insert(entity, Parent(parent));
            if let Some(mut children) = world.query_mut::<Children>() {
                match children.get_mut(parent) {
                    Some(list) => list.0.push(entity),
                    None => {
                        children.insert(parent, Children(vec![entity]));
                    }
                }
            }
        }
        entity
    }

    fn fixture(target_at: Vec3) -> (World, EntityId, EntityId, EntityId) {
        let mut world = World::new();
        world.insert_resource(StringPool::new());
        world.register::<Name>();
        world.register::<Parent>();
        world.register::<Children>();
        world.register::<HeadLook>();
        world.register::<AnimationTarget>();
        let actor = bone(&mut world, None, "Actor", Vec3::ZERO);
        let head = bone(&mut world, Some(actor), "Bip01 Head", Vec3::new(0.0, 1.7, 0.0));
        let target = bone(&mut world, None, "Target", target_at);
        world.insert(actor, HeadLook { target, yaw: 0.0, pitch: 0.0, head: None });
        (world, actor, head, target)
    }

    fn head_forward(world: &World, head: EntityId) -> Vec3 {
        world.get::<Transform>(head).unwrap().rotation * FORWARD
    }

    /// #6313 — the head bone is found once and cached on the look; a cached bone that stops being
    /// the skeleton's head (renamed, despawned, id reused elsewhere) is replaced by a fresh walk.
    #[test]
    fn the_head_bone_is_cached_and_revalidated() {
        let (mut world, actor, head, _target) = fixture(Vec3::new(-10.0, 1.7, -10.0));
        head_look_system(&world, 0.016);
        assert_eq!(world.get::<HeadLook>(actor).unwrap().head, Some(head), "found once, kept");

        // The skeleton is rebuilt: the old bone loses its name, a new head takes its place.
        world.query_mut::<Name>().unwrap().remove(head);
        let replacement = bone(&mut world, Some(actor), "Bip01 Head", Vec3::new(0.0, 1.7, 0.0));
        head_look_system(&world, 0.016);
        assert_eq!(world.get::<HeadLook>(actor).unwrap().head, Some(replacement), "stale cache re-found");
    }

    /// The body faces -Z. A target to its left (-X) turns the head toward -X at a steady rate, a target
    /// far behind is limited to the neck's range, and a target that is gone ends the look.
    #[test]
    fn the_head_turns_toward_the_target_within_the_necks_range() {
        let (mut world, actor, head, target) = fixture(Vec3::new(-10.0, 1.7, -10.0));
        // 45 degrees to the left of the heading; the turn takes 45/(3 rad/s) ~ 0.26 s.
        // The animation resets the head's local rotation every frame; stand in for it.
        let animate = |world: &World| world.query_mut::<Transform>().unwrap().get_mut(head).unwrap().rotation = Quat::IDENTITY;
        for _ in 0..30 {
            animate(&world);
            head_look_system(&world, 0.016);
        }
        let facing = head_forward(&world, head);
        assert!(facing.x < -0.6 && facing.z < -0.6, "the head faces the target: {facing:?}");
        // Behind the actor: the head stops at the neck's limit rather than spinning round.
        world.get_mut::<GlobalTransform>(target).unwrap().translation = Vec3::new(0.0, 1.7, 10.0);
        for _ in 0..200 {
            animate(&world);
            head_look_system(&world, 0.016);
        }
        let yaw = world.get::<HeadLook>(actor).unwrap().yaw.abs();
        assert!((yaw - MAX_YAW).abs() < 1e-3, "limited to {MAX_YAW}: {yaw}");
        // The target leaves the loaded area (no placement any more): the look ends.
        world.query_mut::<GlobalTransform>().unwrap().remove(target);
        head_look_system(&world, 0.016);
        assert!(!world.has::<HeadLook>(actor));
    }
}
