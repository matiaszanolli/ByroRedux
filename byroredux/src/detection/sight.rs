//! The actor-to-actor sight test (Q7) and the plain line test
//! (`docs/engine/sneak-detection.md` §5). The only callers of
//! `PhysicsWorld::line_of_sight_blocked` and of the detection range
//! (SNK-USE-007): every awareness question that does not have the player as
//! the target, and every line test (`GetLOS`, attraction), goes through here.

use byroredux_core::ecs::components::{Dead, GlobalTransform};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::Vec3;
use byroredux_plugin::esm::records::EsmIndex;

/// Detection range from the load order's GMSTs. Absent when the game does
/// not author `fSneakMaxDistance`, which disables ambient hostility.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DetectionConfig {
    /// `fSneakMaxDistance`, in BU.
    pub(crate) max_distance: f32,
    /// `fSneakExteriorDistanceMult`, or `1.0` (no scaling) when the game
    /// does not author it.
    pub(crate) exterior_mult: f32,
}

impl Resource for DetectionConfig {}

impl DetectionConfig {
    pub(crate) fn from_index(index: &EsmIndex) -> Option<Self> {
        let max_distance = index
            .game_setting_float("fSneakMaxDistance")
            .filter(|d| d.is_finite() && *d > 0.0)?;
        let exterior_mult = index
            .game_setting_float("fSneakExteriorDistanceMult")
            .filter(|m| m.is_finite() && *m > 0.0)
            .unwrap_or(1.0);
        Some(Self {
            max_distance,
            exterior_mult,
        })
    }

    pub(crate) fn range(&self, exterior: bool) -> f32 {
        if exterior {
            self.max_distance * self.exterior_mult
        } else {
            self.max_distance
        }
    }
}

/// Install (or clear) the [`DetectionConfig`] for a freshly loaded index.
pub(crate) fn install_detection_config(world: &mut World, index: &EsmIndex) {
    match DetectionConfig::from_index(index) {
        Some(config) => {
            world.insert_resource(config);
        }
        None => {
            world.remove_resource::<DetectionConfig>();
        }
    }
}

/// Whether the scene is an exterior (the detection range is scaled there).
pub(crate) fn in_exterior(world: &World) -> bool {
    world
        .try_resource::<crate::components::CellLightingRes>()
        .is_some_and(|cell| !cell.is_interior)
}

/// The detection range in this scene, or `None` when the game authors none.
pub(crate) fn detection_range(world: &World) -> Option<f32> {
    let config = world.try_resource::<DetectionConfig>().map(|c| *c)?;
    Some(config.range(in_exterior(world)))
}

/// The actor-to-actor sight test (Q7): `observer` sees `target` within the
/// detection range (`fSneakMaxDistance`, scaled outdoors), inside its view cone
/// where the game authors one (SNK-DET-008), and with nothing in the way.
/// `false` without a detection config (the game authors none), and for a dead
/// or unplaced actor.
pub(crate) fn sees(world: &World, observer: EntityId, target: EntityId) -> bool {
    let Some(range) = detection_range(world) else {
        return false;
    };
    let Some((from, to)) = sight_points(world, observer, target) else {
        return false;
    };
    from.distance(to) <= range && in_view_cone(world, observer, to - from) && sight_is_clear(world, from, to)
}

/// Whether `observer` sees the body of `corpse` (CK *Dead Body*: "an actor
/// discovers a dead body"): the sight test toward a dead target. No source
/// documents a corpse-specific range or cone, so the body is seen in range
/// through a clear line.
pub(crate) fn sees_body(world: &World, observer: EntityId, corpse: EntityId) -> bool {
    let Some(range) = detection_range(world) else {
        return false;
    };
    let centre = Vec3::Y * crate::npc_spawn::FALLBACK_ACTOR_CAPSULE_CENTRE_HEIGHT;
    let (Some((from, _)), Some(body)) = (
        sight_points(world, observer, observer),
        world.get::<GlobalTransform>(corpse).map(|t| t.translation + centre),
    ) else {
        return false;
    };
    from.distance(body) <= range && sight_is_clear(world, from, body)
}

/// Whether `observer` has an unobstructed line of sight to `target`, at any
/// range and angle (ObScript `GetLOS`); `false` for a dead or unplaced actor.
pub(crate) fn has_line_of_sight(world: &World, observer: EntityId, target: EntityId) -> bool {
    sight_points(world, observer, target).is_some_and(|(from, to)| sight_is_clear(world, from, to))
}

/// The plain line test between two points (attraction's line test).
pub(crate) fn line_clear(world: &World, from: Vec3, to: Vec3) -> bool {
    sight_is_clear(world, from, to)
}

/// The points sight rays between two actors start and end at; `None` when
/// either is dead or unplaced.
pub(crate) fn sight_points(world: &World, observer: EntityId, target: EntityId) -> Option<(Vec3, Vec3)> {
    let player = crate::systems::PlayerEntity::get(world);
    let centre = Vec3::Y * crate::npc_spawn::FALLBACK_ACTOR_CAPSULE_CENTRE_HEIGHT;
    let sight_point = |entity: EntityId| -> Option<Vec3> {
        if world.get::<Dead>(entity).is_some() {
            return None;
        }
        let position = world.get::<GlobalTransform>(entity)?.translation;
        // The player body's transform is already its capsule centre; an NPC placement root stands at its feet.
        Some(if Some(entity) == player { position } else { position + centre })
    };
    Some((sight_point(observer)?, sight_point(target)?))
}

/// Whether `direction` lies inside `observer`'s view cone (`fDetectionViewCone`,
/// the full angle, around the actor's forward −Z, horizontally). True where the
/// game authors no cone, or the observer has no transform.
pub(crate) fn in_view_cone(world: &World, observer: EntityId, direction: Vec3) -> bool {
    let Some(cone) = world
        .try_resource::<byroredux_core::stealth::StealthRules>()
        .and_then(|rules| rules.view_cone_degrees())
    else {
        return true;
    };
    let Some(rotation) = world.get::<GlobalTransform>(observer).map(|t| t.rotation) else {
        return true;
    };
    let forward = rotation * Vec3::NEG_Z;
    let forward = Vec3::new(forward.x, 0.0, forward.z);
    let direction = Vec3::new(direction.x, 0.0, direction.z);
    if forward.length_squared() < 1e-8 || direction.length_squared() < 1e-8 {
        return true;
    }
    let angle = forward.normalize().dot(direction.normalize()).clamp(-1.0, 1.0).acos();
    angle.to_degrees() <= cone * 0.5
}

/// Whether nothing solid stands between two points (the player's own body
/// excepted); true without a physics world.
fn sight_is_clear(world: &World, from: Vec3, to: Vec3) -> bool {
    let player = crate::systems::PlayerEntity::get(world);
    let player_body = player.and_then(|player| {
        world.query::<byroredux_physics::RapierHandles>().and_then(|handles| handles.get(player).map(|h| h.body))
    });
    !world
        .try_resource::<byroredux_physics::PhysicsWorld>()
        .is_some_and(|physics| physics.line_of_sight_blocked(from, to, player_body))
}

#[cfg(test)]
mod tests {
    /// PRD §7.2 scenario 4 (SNK-USE-007): outside the detection module, no code in the binary or the
    /// library crates calls `PhysicsWorld::line_of_sight_blocked` or reads the detection range off
    /// `DetectionConfig`; `GetLOS` and attraction call the module's exported line test.
    #[test]
    fn the_detection_module_is_the_only_caller_of_the_line_test_and_the_range() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut offenders = Vec::new();
        let mut stack = vec![root.join("byroredux/src"), root.join("crates")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                let shown = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
                if path.is_dir() {
                    // The definition lives in the physics crate; the module itself is the one allowed caller.
                    if !(shown.ends_with("/target") || shown == "crates/physics" || shown == "byroredux/src/detection") {
                        stack.push(path);
                    }
                    continue;
                }
                if path.extension().is_none_or(|ext| ext != "rs") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).unwrap();
                for needle in ["line_of_sight_blocked(", "config.range(", ".range(exterior)"] {
                    if source.contains(needle) {
                        offenders.push(format!("{shown}: `{needle}`"));
                    }
                }
            }
        }
        assert!(offenders.is_empty(), "outside the detection module: {offenders:#?}");
        let queries = include_str!("../systems/script_queries.rs");
        assert!(queries.contains("crate::detection::has_line_of_sight"), "GetLOS calls the module's line test");
        let attraction = include_str!("../systems/attraction.rs");
        assert!(attraction.contains("crate::detection::line_clear"), "attraction calls the module's line test");
    }
}
