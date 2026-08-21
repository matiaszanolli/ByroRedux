//! Physics query-surface diagnostics.
//!
//! `phys.census` answers "what collision is under *this* point" and
//! `phys.stats` answers "is the simulation awake, and where is its static
//! geometry" — both from a live engine over `byro-dbg`.
//!
//! #2876 — `PhysicsWorld`'s whole query surface (`colliders_near_xz`,
//! `static_colliders_aabb`, `cast_capsule_down*`, `body_count`,
//! `awake_counts`) previously had zero console exposure, and the collider
//! census that consumes it was gated behind a single `if floor_probe_failed`
//! inside `setup_scene`'s door-teleport branch. That is the wrong moment: a
//! missing floor is noticed by falling through it somewhere in the cell, not
//! by reading a frame-0 log about the door. Same defect class as #518
//! (`tex.missing` / `mesh.cache` unreachable from `byro-dbg`).
//!
//! Both commands are pure reads of an already-`pub` API; `water.contacts` is
//! the existing template for a physics-reading command.

use super::shared::*;
use byroredux_physics::{CharacterController, PhysicsWorld};

/// Resolve the reference point a bare `phys.census` censuses: the player
/// body if one exists, else the active camera.
///
/// The player is preferred deliberately — the operator runs this command
/// *because* the thing that fell through the floor is the player, and in
/// Character mode the camera sits `eye_height` above the body.
fn reference_point(world: &World) -> Option<(&'static str, Vec3)> {
    let transforms = world.query::<GlobalTransform>()?;
    if let Some(player) = world
        .try_resource::<crate::systems::PlayerEntity>()
        .and_then(|player| player.0)
    {
        if let Some(transform) = transforms.get(player) {
            return Some(("player", transform.translation));
        }
    }
    let camera = world
        .try_resource::<ActiveCamera>()
        .map(|active| active.0)?;
    transforms
        .get(camera)
        .map(|transform| ("camera", transform.translation))
}

/// Census the colliders in a vertical column — the "why is there no floor
/// here" diagnostic.
pub(crate) struct PhysCensusCommand;

impl ConsoleCommand for PhysCensusCommand {
    fn name(&self) -> &str {
        "phys.census"
    }

    fn description(&self) -> &str {
        "Census colliders in a column: phys.census [x z [radius]] (defaults to the player/camera)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        if world.try_resource::<PhysicsWorld>().is_none() {
            return CommandOutput::error("phys.census: no PhysicsWorld resource");
        }
        let fields: Vec<&str> = args.split_whitespace().collect();
        let parse = |raw: &str, what: &str| {
            raw.parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("phys.census: {what} must be a finite number, got `{raw}`"))
        };

        // Argument shape is validated BEFORE resolving a reference point, so
        // a typo reports the typo rather than "no camera" — the reference is
        // only needed when XZ was omitted.
        //
        // `[x z]` overrides XZ only. Y stays the reference height because the
        // census sorts by distance from it and re-sweeps downward from it —
        // an operator naming a column has no reason to also guess its height.
        let explicit_xz = match fields.as_slice() {
            [] => None,
            [_] => {
                return CommandOutput::error("phys.census: expected `phys.census [x z [radius]]`")
            }
            [raw_x, raw_z, ..] => match (parse(raw_x, "x"), parse(raw_z, "z")) {
                (Ok(x), Ok(z)) => Some((x, z)),
                (Err(error), _) | (_, Err(error)) => return CommandOutput::error(error),
            },
        };
        let radius = match fields.get(2) {
            Some(raw) => match parse(raw, "radius") {
                Ok(radius) if radius > 0.0 => radius,
                Ok(_) => return CommandOutput::error("phys.census: radius must be positive"),
                Err(error) => return CommandOutput::error(error),
            },
            None => crate::scene::SPAWN_CENSUS_RADIUS_BU,
        };

        // A reference is still wanted even with explicit XZ: its Y is the
        // sweep origin. Falling back to y=0 when there is none keeps an
        // explicit `phys.census <x> <z>` usable on a world with no rig.
        let reference_point = reference_point(world);
        let (origin_label, reference) = match (reference_point, explicit_xz) {
            (Some(found), _) => found,
            (None, Some(_)) => ("y=0 (no rig)", Vec3::ZERO),
            (None, None) => {
                return CommandOutput::error(
                    "phys.census: no player or active camera to census around — pass an \
                     explicit `phys.census <x> <z>`",
                )
            }
        };
        let (x, z, source) = match explicit_xz {
            Some((x, z)) => (x, z, "argument"),
            None => (reference.x, reference.z, origin_label),
        };

        // Same capsule and walkability threshold the spawn rungs sweep with,
        // so a live census is directly comparable with the boot-time one.
        let controller = CharacterController::HUMAN;
        let probe = byroredux_physics::SpawnCensusProbe {
            x,
            y: reference.y + crate::scene::floor_probe_lift(controller),
            z,
            radius,
            capsule_half_height: controller.half_height,
            capsule_radius: controller.radius,
            max_distance: crate::scene::FLOOR_PROBE_CLEARANCE_BU
                + crate::scene::FLOOR_PROBE_REACH_BELOW_DOOR_BU,
            min_walkable_normal_y: crate::scene::min_walkable_normal_y(controller),
            // The live path has no NIF import cache to sum, so `0 colliders`
            // here cannot be split into "nothing authored" vs "dropped in
            // translation". The report says so rather than implying either.
            authoring: None,
        };

        let mut lines = vec![format!(
            "Collider census at XZ ({x:.1}, {z:.1}) ±{radius:.0} BU \
             (origin: {source}, y={:.1})",
            reference.y
        )];
        lines.extend(byroredux_physics::spawn_collider_census_report(
            world, probe,
        ));
        CommandOutput::lines(lines)
    }
}

/// Simulation-wide body/collider counts and the static-geometry bounds.
pub(crate) struct PhysStatsCommand;

impl ConsoleCommand for PhysStatsCommand {
    fn name(&self) -> &str {
        "phys.stats"
    }

    fn description(&self) -> &str {
        "Physics body/collider counts, awake islands, and static-collider bounds"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(pw) = world.try_resource::<PhysicsWorld>() else {
            return CommandOutput::error("phys.stats: no PhysicsWorld resource");
        };
        let bodies = pw.body_count();
        let (awake_dynamic, awake_kinematic) = pw.awake_counts();
        let statics = pw.static_colliders_aabb();
        let pending = pw.pending_wake();
        drop(pw);

        let mut lines = vec![
            format!("Physics stats: bodies={bodies}"),
            format!(
                "  awake: dynamic={awake_dynamic} kinematic={awake_kinematic} \
                 pending_wake={pending}"
            ),
        ];
        // `awake_dynamic == 0 && !pending_wake` is exactly the static-scene
        // fast path's condition, so surfacing both together tells the
        // operator whether `step` is running at all before they read
        // anything else as a physics result.
        if awake_dynamic == 0 && !pending {
            lines.push(
                "  → quiesced: `step` is taking the static-scene fast path (0 substeps/frame)"
                    .to_string(),
            );
        }
        match statics {
            Some((min, max, count)) => lines.push(format!(
                "  static colliders: {count} spanning x=[{:.0}, {:.0}] y=[{:.0}, {:.0}] \
                 z=[{:.0}, {:.0}]",
                min[0], max[0], min[1], max[1], min[2], max[2],
            )),
            None => lines.push(
                "  static colliders: NONE — nothing fixed is registered, so every ground \
                 probe in this cell will miss"
                    .to_string(),
            ),
        }
        CommandOutput::lines(lines)
    }
}
