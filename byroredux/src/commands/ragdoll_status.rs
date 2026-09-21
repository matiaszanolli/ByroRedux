//! Read-only live physics diagnostics; fixture-specific bounds belong to tests.
use super::shared::*;
use crate::components::AnimationTarget;
use byroredux_physics::{PhysicsWorld, Ragdoll};

pub(crate) struct RagdollStatusCommand;

pub(crate) struct NpcAppearanceCommand;
impl ConsoleCommand for NpcAppearanceCommand {
    fn name(&self) -> &str {
        "npc.appearance"
    }
    fn description(&self) -> &str {
        "Inspect corpse appearance restoration (npc.appearance <actor_id>)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        match args.trim().parse::<EntityId>() {
            Ok(actor) => {
                CommandOutput::line(crate::npc_spawn::loot_appearance::status(world, actor))
            }
            Err(_) => CommandOutput::line("usage: npc.appearance <actor_id>"),
        }
    }
}

impl ConsoleCommand for RagdollStatusCommand {
    fn name(&self) -> &str {
        "ragdoll.status"
    }
    fn description(&self) -> &str {
        "Inspect corpse physics (ragdoll.status <actor_id>)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        match status(world, args) {
            Ok(line) => CommandOutput::line(line),
            Err(error) => CommandOutput::line(format!("ragdoll.status: {error}")),
        }
    }
}

fn status(world: &World, args: &str) -> Result<String, &'static str> {
    let actor = args
        .trim()
        .parse::<EntityId>()
        .map_err(|_| "usage: ragdoll.status <actor_id>")?;
    // Copy ECS data before acquiring physics: do not introduce a reversed
    // PhysicsWorld -> component lock dependency into the debug drain.
    let origin = world
        .get::<GlobalTransform>(actor)
        .map(|t| t.translation)
        .ok_or("actor has no world placement")?;
    let skeleton = world
        .get::<AnimationTarget>(actor)
        .map_or(actor, |target| target.skeleton_root);
    let rag = world
        .get::<Ragdoll>(skeleton)
        .map(|r| r.clone())
        .ok_or("actor has no active ragdoll")?;
    let physics = world
        .try_resource::<PhysicsWorld>()
        .ok_or("physics unavailable")?;
    let mut finite = origin.is_finite();
    let mut live = 0;
    let mut max_distance = 0.0_f32;
    let mut max_speed = 0.0_f32;
    for (_, handle, scale) in &rag.bodies {
        let Some(body) = physics.bodies.get(*handle) else {
            continue;
        };
        live += 1;
        let position = body.translation();
        let distance = Vec3::new(position.x, position.y, position.z).distance(origin);
        let speed = body.linvel().norm();
        finite &= distance.is_finite()
            && speed.is_finite()
            && scale.is_finite()
            && body.rotation().coords.iter().all(|v| v.is_finite())
            && body.angvel().iter().all(|v| v.is_finite());
        max_distance = max_distance.max(distance);
        max_speed = max_speed.max(speed);
    }
    let complete = !rag.bodies.is_empty() && live == rag.bodies.len();
    Ok(format!("ragdoll.status: actor={actor} skeleton={skeleton} bodies={} live={live} complete={complete} finite={finite} max_distance={max_distance:.3} max_speed={max_speed:.3}", rag.bodies.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_physics::{build_ragdoll, ContactConfig, RagdollBodySpec, RagdollSpec};

    fn fixture() -> (World, EntityId, EntityId) {
        let mut world = World::new();
        world.register::<GlobalTransform>();
        world.register::<AnimationTarget>();
        world.register::<Ragdoll>();
        let actor = world.spawn();
        let skeleton = world.spawn();
        world.insert(
            actor,
            GlobalTransform {
                translation: Vec3::new(100.0, 0.0, 0.0),
                ..GlobalTransform::IDENTITY
            },
        );
        world.insert(
            actor,
            AnimationTarget {
                skeleton_root: skeleton,
                consumed_idle_serial: 0,
            },
        );
        let mut physics = PhysicsWorld::new();
        let rag = build_ragdoll(
            &mut physics,
            &RagdollSpec {
                bodies: vec![RagdollBodySpec {
                    entity: skeleton,
                    translation: Vec3::new(103.0, 4.0, 0.0),
                    rotation: Quat::IDENTITY,
                    scale: 1.0,
                    shape: CollisionShape::Ball { radius: 1.0 },
                    mass: 1.0,
                    linear_damping: 0.0,
                    angular_damping: 0.0,
                    friction: 0.5,
                    restitution: 0.0,
                }],
                constraints: vec![],
            },
            &ContactConfig::DEFAULT,
        );
        world.insert(skeleton, rag);
        world.insert_resource(physics);
        (world, actor, skeleton)
    }

    #[test]
    fn reports_physics_pose_relative_to_actor_not_stale_bone_globals() {
        let (world, actor, _) = fixture();
        let line = status(&world, &actor.to_string()).unwrap();
        assert!(
            line.contains("bodies=1 live=1 complete=true finite=true max_distance=5.000"),
            "{line}"
        );
    }

    #[test]
    fn missing_body_is_not_complete() {
        let (world, actor, skeleton) = fixture();
        let handle = world.get::<Ragdoll>(skeleton).unwrap().bodies[0].1;
        world.resource_mut::<PhysicsWorld>().remove_body(handle);
        let line = status(&world, &actor.to_string()).unwrap();
        assert!(line.contains("live=0 complete=false"), "{line}");
    }

    #[test]
    fn finite_runaway_body_reports_its_real_distance() {
        let (world, actor, skeleton) = fixture();
        let handle = world.get::<Ragdoll>(skeleton).unwrap().bodies[0].1;
        world.resource_mut::<PhysicsWorld>().bodies[handle]
            .set_translation([1_000_100.0, 0.0, 0.0].into(), true);
        let line = status(&world, &actor.to_string()).unwrap();
        assert!(
            line.contains("finite=true max_distance=1000000.000"),
            "{line}"
        );
    }

    #[test]
    fn empty_ragdoll_is_not_complete() {
        let (world, actor, skeleton) = fixture();
        world
            .query_mut::<Ragdoll>()
            .unwrap()
            .get_mut(skeleton)
            .unwrap()
            .bodies
            .clear();
        assert!(status(&world, &actor.to_string())
            .unwrap()
            .contains("bodies=0 live=0 complete=false"));
    }

    #[test]
    fn nonfinite_placement_is_not_hidden_by_float_max() {
        let (world, actor, _) = fixture();
        world
            .query_mut::<GlobalTransform>()
            .unwrap()
            .get_mut(actor)
            .unwrap()
            .translation
            .x = f32::NAN;
        assert!(status(&world, &actor.to_string())
            .unwrap()
            .contains("finite=false"));
    }

    #[test]
    fn absent_actor_and_bad_arguments_fail_closed() {
        let world = World::new();
        assert!(status(&world, "not-an-id").is_err());
        assert!(status(&world, "999").is_err());
    }
}
