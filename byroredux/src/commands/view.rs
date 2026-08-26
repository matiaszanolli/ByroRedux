//! Camera + selection / picking commands.
//!
//! `prid`, `cam.where`, `near`, `pick`, `cam.pos`, `cam.tp`,
//! `interaction.status`, `combat.status`, `combat.approach`, `input.press`,
//! `input.hold`, `input.look`,
//! `player.status`.

use super::shared::*;

/// `interaction.status` — inspect the camera-forward target and the last
/// canonical activation retained past transient event cleanup.
pub(crate) struct InteractionStatusCommand;
impl ConsoleCommand for InteractionStatusCommand {
    fn name(&self) -> &str {
        "interaction.status"
    }

    fn description(&self) -> &str {
        "Show the current interaction prompt and last activation result"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let target = world
            .try_resource::<crate::interaction::InteractionState>()
            .and_then(|state| state.target);
        let trace = world
            .try_resource::<crate::interaction::InteractionTrace>()
            .map(|trace| trace.clone())
            .unwrap_or_default();

        let mut lines = vec!["Interaction status:".to_string()];
        match target {
            Some(target) => {
                lines.push(format!(
                    "  target={} kind={:?} distance={:.2}",
                    target.entity, target.kind, target.distance
                ));
                lines.push(format!("  prompt=[E] {}", target.kind.verb()));
            }
            None => lines.push("  target=none prompt=none".to_string()),
        }
        lines.push(format!("  activations={}", trace.activation_count));
        if let Some(last) = trace.last {
            lines.push(format!(
                "  last_target={} kind={:?} activator={} event_emitted={}",
                last.target.entity,
                last.target.kind,
                last.activator
                    .map(|entity| entity.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                last.event_emitted,
            ));
            lines.push(format!("  outcome={}", last.outcome));
        } else {
            lines.push("  last_target=none".to_string());
        }
        CommandOutput::lines(lines)
    }
}

/// `input.press <action>` — queue one debug/smoke gameplay-key pulse.
///
/// The next Update tick maps the action's live keyboard binding through
/// `ActionBindings` and derives a normal pressed edge; this command never
/// invokes interaction or combat effects directly.
pub(crate) struct InputPressCommand;
impl ConsoleCommand for InputPressCommand {
    fn name(&self) -> &str {
        "input.press"
    }

    fn description(&self) -> &str {
        "Queue a one-frame gameplay input pulse (usage: input.press <action>)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        match crate::interaction::queue_debug_action_press(world, args) {
            Ok(message) => CommandOutput::line(message),
            Err(error) => CommandOutput::line(format!("input.press: {error}")),
        }
    }
}

/// `combat.status` — show timing plus the last resolved attack/hit/death.
pub(crate) struct CombatStatusCommand;
impl ConsoleCommand for CombatStatusCommand {
    fn name(&self) -> &str {
        "combat.status"
    }

    fn description(&self) -> &str {
        "Show melee cooldown, counters, and the last combat result"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let state = world
            .try_resource::<crate::combat::CombatState>()
            .map(|state| state.clone())
            .unwrap_or_default();
        let mut lines = vec!["Combat status:".to_string()];
        lines.push(format!(
            "  cooldown={:.3} blocking={} attacks={} hits={} kills={}",
            state.cooldown_remaining,
            state.blocking,
            state.attacks_started,
            state.hits_landed,
            state.kills,
        ));
        if let Some(last) = state.last {
            lines.push(format!(
                "  last_target={} damage={:.1} health_before={} health_after={} killed={}",
                last.target
                    .map(|entity| entity.to_string())
                    .unwrap_or_else(|| "none".to_owned()),
                last.damage,
                last.health_before
                    .map(|health| format!("{health:.1}"))
                    .unwrap_or_else(|| "none".to_owned()),
                last.health_after
                    .map(|health| format!("{health:.1}"))
                    .unwrap_or_else(|| "none".to_owned()),
                last.killed,
            ));
            lines.push(format!("  outcome={}", last.outcome));
        } else {
            lines.push("  last_target=none".to_owned());
        }
        CommandOutput::lines(lines)
    }
}

/// `combat.approach <entity|.>` — deterministic smoke-test positioning.
///
/// This command only moves the real character capsule and aims the normal
/// gameplay camera. It never emits a hit or mutates combat state; the attack
/// must still enter through `input.press attack` and the ordinary action,
/// ray-cast, event, damage, and death systems.
pub(crate) struct CombatApproachCommand;

const COMBAT_APPROACH_RADII_BU: [f32; 3] = [120.0, 96.0, 144.0];
const COMBAT_APPROACH_ANGLE_STEPS: usize = 16;
pub(super) const COMBAT_APPROACH_MAX_RAY_BU: f32 = 176.0;

pub(super) fn combat_approach_offsets() -> impl Iterator<Item = Vec3> {
    COMBAT_APPROACH_RADII_BU.into_iter().flat_map(|radius| {
        (0..COMBAT_APPROACH_ANGLE_STEPS).map(move |step| {
            let angle = step as f32 * std::f32::consts::TAU / COMBAT_APPROACH_ANGLE_STEPS as f32;
            Vec3::new(angle.sin() * radius, 0.0, -angle.cos() * radius)
        })
    })
}

fn find_walkable_combat_approach(
    world: &World,
    player: EntityId,
    target_pos: Vec3,
    controller: byroredux_physics::CharacterController,
) -> Option<(Vec3, f32)> {
    // DebugDrainSystem is Stage::Late exclusive, satisfying the narrow
    // registration helper's exclusivity contract (#3267).
    byroredux_physics::register_newcomers_and_refresh_queries(world);

    combat_approach_offsets().find_map(|offset| {
        let candidate = target_pos + offset;
        let surface_y = crate::scene::probe_walkable_floor_near(
            world,
            candidate.x,
            candidate.z,
            target_pos.y,
            controller,
            Some(player),
        )?;
        let body_pos = Vec3::new(
            candidate.x,
            crate::scene::character_spawn_center_y(world, surface_y, controller),
            candidate.z,
        );
        let camera_pos = body_pos + Vec3::Y * controller.eye_height;
        let aim_pos = target_pos + Vec3::Y * controller.eye_height * 1.15;
        (camera_pos.distance(aim_pos) <= COMBAT_APPROACH_MAX_RAY_BU)
            .then_some((body_pos, surface_y))
    })
}

impl ConsoleCommand for CombatApproachCommand {
    fn name(&self) -> &str {
        "combat.approach"
    }

    fn description(&self) -> &str {
        "Place the character in melee range of an actor (usage: combat.approach <entity|.>)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Ok(target) = resolve_console_entity(world, args.trim()) else {
            return CommandOutput::line("combat.approach: usage: combat.approach <entity_id|.>");
        };
        if world
            .get::<byroredux_core::ecs::components::ActorVitals>(target)
            .is_none()
        {
            return CommandOutput::line(format!(
                "combat.approach: entity {target} is not a damageable actor"
            ));
        }
        let Some(target_pos) = world
            .get::<GlobalTransform>(target)
            .map(|transform| transform.translation)
        else {
            return CommandOutput::line(format!(
                "combat.approach: entity {target} has no GlobalTransform"
            ));
        };
        let Some(player) = world
            .try_resource::<crate::systems::PlayerEntity>()
            .and_then(|player| player.0)
        else {
            return CommandOutput::line("combat.approach: no character player entity");
        };
        let Some(camera) = world.try_resource::<ActiveCamera>().map(|camera| camera.0) else {
            return CommandOutput::line("combat.approach: no active camera");
        };
        let controller = world
            .get::<byroredux_physics::CharacterController>(player)
            .map(|controller| *controller)
            .unwrap_or(byroredux_physics::CharacterController::HUMAN);

        // Keep the distance inside the 180-BU melee reach. The frozen ambush
        // actor is not itself standing over architecture, so a live engine
        // searches a deterministic ring for a nearby authored floor and
        // rejects candidates whose vertical separation would put the actor
        // outside the attack ray. Resource-light command tests retain the
        // direct authored-height fallback.
        let authored_floor_pos = target_pos - Vec3::Z * 120.0;
        let (body_pos, floor_y) = if world
            .try_resource::<byroredux_physics::PhysicsWorld>()
            .is_some()
        {
            let Some(approach) =
                find_walkable_combat_approach(world, player, target_pos, controller)
            else {
                return CommandOutput::line(
                    "combat.approach: no authored floor within grounded melee reach",
                );
            };
            approach
        } else {
            (
                authored_floor_pos + Vec3::Y * (controller.half_height + controller.radius),
                authored_floor_pos.y,
            )
        };
        let camera_pos = body_pos + Vec3::Y * controller.eye_height;
        let aim_pos = target_pos + Vec3::Y * controller.eye_height * 1.15;
        let (yaw, pitch) = look_at_yaw_pitch(camera_pos, aim_pos);
        let camera_rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);

        {
            let Some(mut transforms) = world.query_mut::<Transform>() else {
                return CommandOutput::line("combat.approach: Transform storage unavailable");
            };
            if let Some(player_transform) = transforms.get_mut(player) {
                player_transform.translation = body_pos;
                player_transform.rotation = Quat::IDENTITY;
            }
            let Some(camera_transform) = transforms.get_mut(camera) else {
                return CommandOutput::line(format!(
                    "combat.approach: camera entity {camera} has no Transform"
                ));
            };
            camera_transform.translation = camera_pos;
            camera_transform.rotation = camera_rotation;
        }
        if let Some(mut controllers) = world.query_mut::<byroredux_physics::CharacterController>() {
            if let Some(controller) = controllers.get_mut(player) {
                controller.vertical_velocity = 0.0;
                controller.is_grounded = false;
                controller.wants_jump = false;
            }
        }
        if let Some(mut input) = world.try_resource_mut::<InputState>() {
            input.yaw = yaw;
            input.pitch = pitch;
        }
        let physics_synced = byroredux_physics::set_kinematic_translation(world, player, body_pos);

        CommandOutput::lines(vec![
            format!(
                "Character placed in melee range of entity {target} at ({:.2}, {:.2}, {:.2})",
                target_pos.x, target_pos.y, target_pos.z,
            ),
            format!(
                "  body=({:.2}, {:.2}, {:.2}) floor_y={floor_y:.2} distance={:.2} \
                 physics_synced={physics_synced}",
                body_pos.x,
                body_pos.y,
                body_pos.z,
                Vec3::new(body_pos.x, target_pos.y, body_pos.z).distance(target_pos),
            ),
        ])
    }
}

/// `input.hold <action> <frames>` — queue a bounded physical-key hold.
///
/// This is the traversal-smoke counterpart to [`InputPressCommand`]. It
/// resolves the action through the live keyboard bindings and lets the normal
/// action refresh plus character controller consume every held frame.
pub(crate) struct InputHoldCommand;
impl ConsoleCommand for InputHoldCommand {
    fn name(&self) -> &str {
        "input.hold"
    }

    fn description(&self) -> &str {
        "Hold a gameplay action for N frames (usage: input.hold forward 120)"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        const MAX_HOLD_FRAMES: u32 = 10_000;
        let mut parts = args.split_whitespace();
        let Some(action) = parts.next() else {
            return CommandOutput::line("usage: input.hold <action> <frames>");
        };
        let Some(frames) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
            return CommandOutput::line("usage: input.hold <action> <frames>");
        };
        if parts.next().is_some() || frames == 0 || frames > MAX_HOLD_FRAMES {
            return CommandOutput::line(format!(
                "input.hold: frames must be in 1..={MAX_HOLD_FRAMES}"
            ));
        }
        match crate::interaction::queue_debug_action_hold(world, action, frames) {
            Ok(message) => CommandOutput::line(message),
            Err(error) => CommandOutput::line(format!("input.hold: {error}")),
        }
    }
}

/// `input.look <yaw-degrees> [pitch-degrees]` — set the gameplay look
/// accumulator used by both mouse look and character movement alignment.
///
/// This is a deterministic smoke-test frontend for the normal `InputState`
/// boundary. It never writes camera or character transforms directly.
pub(crate) struct InputLookCommand;
impl ConsoleCommand for InputLookCommand {
    fn name(&self) -> &str {
        "input.look"
    }

    fn description(&self) -> &str {
        "Set gameplay yaw/pitch in degrees (usage: input.look <yaw> [pitch])"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut parts = args.split_whitespace();
        let Some(yaw_degrees) = parts.next().and_then(|value| value.parse::<f32>().ok()) else {
            return CommandOutput::line("usage: input.look <yaw-degrees> [pitch-degrees]");
        };
        let pitch_degrees = match parts.next() {
            Some(value) => match value.parse::<f32>() {
                Ok(value) => value,
                Err(_) => {
                    return CommandOutput::line("usage: input.look <yaw-degrees> [pitch-degrees]");
                }
            },
            None => 0.0,
        };
        if parts.next().is_some() || !yaw_degrees.is_finite() || !pitch_degrees.is_finite() {
            return CommandOutput::line("usage: input.look <yaw-degrees> [pitch-degrees]");
        }
        let Some(mut input) = world.try_resource_mut::<InputState>() else {
            return CommandOutput::line("input.look: InputState resource not present");
        };
        input.yaw = yaw_degrees.to_radians();
        input.pitch = pitch_degrees.clamp(-89.0, 89.0).to_radians();
        CommandOutput::line(format!(
            "input.look: yaw={:.1}° pitch={:.1}°",
            input.yaw.to_degrees(),
            input.pitch.to_degrees(),
        ))
    }
}

/// `player.status` — report the grounded traversal state used by P1 smokes.
pub(crate) struct PlayerStatusCommand;
impl ConsoleCommand for PlayerStatusCommand {
    fn name(&self) -> &str {
        "player.status"
    }

    fn description(&self) -> &str {
        "Show player mode, body/camera positions, exterior grid, and grounding"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let mode = world
            .try_resource::<crate::systems::PlayerMode>()
            .map(|mode| *mode)
            .unwrap_or_default();
        let player = world
            .try_resource::<crate::systems::PlayerEntity>()
            .and_then(|player| player.0);
        let camera_entity = world.try_resource::<ActiveCamera>().map(|active| active.0);
        let camera = camera_entity
            .and_then(|entity| world.get::<Transform>(entity))
            .map(|transform| transform.translation);
        let look = world
            .try_resource::<InputState>()
            .map(|input| (input.yaw.to_degrees(), input.pitch.to_degrees()));

        let mut lines = vec!["Player status:".to_string()];
        lines.push(format!(
            "  mode={mode:?} player={}",
            player
                .map(|entity| entity.to_string())
                .unwrap_or_else(|| "none".to_string())
        ));
        if let Some(player) = player {
            let body = world
                .get::<Transform>(player)
                .map(|transform| transform.translation);
            let controller = world.get::<byroredux_physics::CharacterController>(player);
            match (body, controller) {
                (Some(body), Some(controller)) => {
                    let grid = crate::streaming::world_pos_to_grid(body.x, body.z);
                    lines.push(format!(
                        "  body=({:.2}, {:.2}, {:.2}) grid=({},{}) grounded={} vertical_velocity={:.2}",
                        body.x,
                        body.y,
                        body.z,
                        grid.0,
                        grid.1,
                        controller.is_grounded,
                        controller.vertical_velocity,
                    ));
                }
                (Some(body), None) => lines.push(format!(
                    "  body=({:.2}, {:.2}, {:.2}) controller=none",
                    body.x, body.y, body.z
                )),
                (None, _) => lines.push("  body=none".to_string()),
            }
        }
        lines.push(match camera {
            Some(camera) => format!(
                "  camera=({:.2}, {:.2}, {:.2})",
                camera.x, camera.y, camera.z
            ),
            None => "  camera=none".to_string(),
        });
        lines.push(match look {
            Some((yaw, pitch)) => format!("  look=(yaw={yaw:.1}°, pitch={pitch:.1}°)"),
            None => "  look=none".to_string(),
        });
        lines.push(format!(
            "  input_hold_frames_remaining={}",
            crate::interaction::injected_hold_frames_remaining(world)
        ));
        CommandOutput::lines(lines)
    }
}

/// `prid <entity_id>` — pick a reference (Bethesda console heritage).
///
/// Sets the world-scoped [`SelectedRef`] to the given entity so that
/// follow-up commands operate on it by default. Today's consumers:
/// `inspect` (no args), `cam.tp` (no args). The natural workflow:
///
/// ```text
/// byro> entities Inventory          # list NPCs with equip state
/// byro> prid 42                     # pick one
/// byro> cam.tp                      # frame it
/// byro> inspect                     # dump every component on it
/// byro> skin.coverage               # read coverage against this view
/// ```
///
/// With no arg, `prid` prints the current selection (`SelectedRef`
/// resource state). The selection is not implicitly cleared on cell
/// unload — a re-issued generational `EntityId` could re-bind to a
/// new entity. This is a known dev-tool sharp edge that matches
/// Bethesda's own `prid` semantics; M40 cell streaming will need an
/// explicit clear-on-unload pass later.
pub(crate) struct PridCommand;
impl ConsoleCommand for PridCommand {
    fn name(&self) -> &str {
        "prid"
    }
    fn description(&self) -> &str {
        "Pick a reference for follow-up commands (usage: prid <entity_id>)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let trimmed = args.trim();
        if trimmed.is_empty() {
            let Some(sel) = world.try_resource::<SelectedRef>() else {
                return CommandOutput::line("SelectedRef resource not present");
            };
            return match sel.0 {
                None => CommandOutput::line("no entity selected (usage: prid <entity_id>)"),
                Some(entity) => {
                    let name = resolve_entity_name(world, entity);
                    drop(sel);
                    CommandOutput::line(match name {
                        Some(n) => format!("selected: entity {entity} ({n})"),
                        None => format!("selected: entity {entity}"),
                    })
                }
            };
        }
        let Ok(target) = trimmed.parse::<EntityId>() else {
            return CommandOutput::line(format!(
                "prid: failed to parse entity id from `{trimmed}`"
            ));
        };
        // Validate the entity exists. Transform is the closest thing
        // to "every entity has it" in this ECS — placement roots,
        // NPCs, bones, cameras all carry one. A bone with only a
        // hierarchy parent + Name (rare) would fail this check;
        // that's a deliberately conservative bar to keep `prid` from
        // accepting typos silently. Falls back to GlobalTransform to
        // catch the bone case.
        let has_transform = world
            .query::<Transform>()
            .map(|q| q.contains(target))
            .unwrap_or(false);
        let has_global = world
            .query::<GlobalTransform>()
            .map(|q| q.contains(target))
            .unwrap_or(false);
        if !has_transform && !has_global {
            return CommandOutput::line(format!(
                "prid: entity {target} has no Transform/GlobalTransform — \
                 does it exist? (use `entities` to list)"
            ));
        }
        let Some(mut sel) = world.try_resource_mut::<SelectedRef>() else {
            return CommandOutput::line("SelectedRef resource not present");
        };
        sel.0 = Some(target);
        drop(sel);
        let name = resolve_entity_name(world, target);
        CommandOutput::line(match name {
            Some(n) => format!("selected: entity {target} ({n})"),
            None => format!("selected: entity {target}"),
        })
    }
}
/// `cam.where` — print the active camera's world position + yaw/pitch.
///
/// Use to capture the current viewpoint before teleporting elsewhere
/// so you can return to it (`cam.pos x y z`). Pairs with `skin.
/// coverage` for documenting which viewpoint produced a given coverage
/// reading.
pub(crate) struct CamWhereCommand;
impl ConsoleCommand for CamWhereCommand {
    fn name(&self) -> &str {
        "cam.where"
    }
    fn description(&self) -> &str {
        "Print active camera position + yaw/pitch (radians)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(active) = world.try_resource::<ActiveCamera>() else {
            return CommandOutput::line("ActiveCamera resource not present");
        };
        let cam_entity = active.0;
        drop(active);
        let pos = world
            .query::<Transform>()
            .and_then(|q| q.get(cam_entity).map(|t| t.translation));
        let Some(pos) = pos else {
            return CommandOutput::line(format!("Camera entity {cam_entity} has no Transform"));
        };
        let (yaw, pitch) = if let Some(input) = world.try_resource::<InputState>() {
            (input.yaw, input.pitch)
        } else {
            (0.0, 0.0)
        };
        CommandOutput::lines(vec![
            format!("Camera entity: {}", cam_entity),
            format!("  position: ({:.2}, {:.2}, {:.2})", pos.x, pos.y, pos.z),
            format!("  yaw:      {:.4} rad ({:.1}°)", yaw, yaw.to_degrees()),
            format!("  pitch:    {:.4} rad ({:.1}°)", pitch, pitch.to_degrees()),
        ])
    }
}
/// `near [radius]` — list entities with `GlobalTransform` within
/// `radius` units of the active camera, sorted by distance ascending.
/// Default radius 300; the cap on the result list is 30 rows.
///
/// Use when there's no raycast picker and you need to identify the
/// REFR you're looking at — walk close to it, run `near 100`, eyeball
/// the closest hits for matching `texture_path` / `material_path` /
/// `Name`, then `prid <entity_id>` for the full inspect. The native
/// REFR rotation chain `(rx, ry, rz)` is NOT directly visible from
/// this command — for that, follow up with `prid` and then look up
/// the source REFR in the ESM via `dump_prospector_saloon_refrs`-
/// style tooling.
///
/// Output columns: distance, entity_id, `Name` (or `Material`-derived
/// label), texture/material path (whichever populated first), pos.
pub(crate) struct NearCommand;
impl ConsoleCommand for NearCommand {
    fn name(&self) -> &str {
        "near"
    }
    fn description(&self) -> &str {
        "List entities near the camera, sorted by distance (usage: near [radius=300])"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let radius: f32 = args.trim().parse().unwrap_or(300.0);
        let Some(active) = world.try_resource::<ActiveCamera>() else {
            return CommandOutput::line("ActiveCamera resource not present");
        };
        let cam_entity = active.0;
        drop(active);
        let cam_pos = world
            .query::<Transform>()
            .and_then(|q| q.get(cam_entity).map(|t| t.translation));
        let Some(cam_pos) = cam_pos else {
            return CommandOutput::line(format!("Camera entity {cam_entity} has no Transform"));
        };
        let Some(gtq) = world.query::<GlobalTransform>() else {
            return CommandOutput::line("GlobalTransform storage not present");
        };
        let r2 = radius * radius;
        let mut hits: Vec<(f32, EntityId, Vec3)> = Vec::new();
        for (entity, gt) in gtq.iter() {
            if entity == cam_entity {
                continue;
            }
            let pos = gt.translation;
            let d2 = (pos - cam_pos).length_squared();
            if d2 <= r2 {
                hits.push((d2.sqrt(), entity, pos));
            }
        }
        drop(gtq);
        hits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        if hits.is_empty() {
            return CommandOutput::line(format!(
                "no entities within {:.1} units of camera ({:.1},{:.1},{:.1})",
                radius, cam_pos.x, cam_pos.y, cam_pos.z
            ));
        }
        let take_n = 30.min(hits.len());
        let mut lines = Vec::with_capacity(take_n + 2);
        lines.push(format!(
            "camera at ({:.1},{:.1},{:.1}) — {} entities within {:.1} units \
             (showing nearest {}):",
            cam_pos.x,
            cam_pos.y,
            cam_pos.z,
            hits.len(),
            radius,
            take_n
        ));
        lines.push(format!(
            "{:>7}  {:>6}  {:<28}  {:<48}  {}",
            "dist", "id", "name", "tex/mat path", "position"
        ));
        for (dist, entity, pos) in hits.iter().take(take_n) {
            let name_str = resolve_entity_name(world, *entity).unwrap_or_else(|| "-".to_string());
            let path = world
                .get::<Material>(*entity)
                .and_then(|m| {
                    m.texture_path
                        .as_deref()
                        .or(m.material_path.as_deref())
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();
            lines.push(format!(
                "{:>7.1}  {:>6}  {:<28.28}  {:<48.48}  ({:>+6.1},{:>+6.1},{:>+6.1})",
                dist, entity, name_str, path, pos.x, pos.y, pos.z
            ));
        }
        CommandOutput::lines(lines)
    }
}
/// `pick [count]` — ray-cast from the active camera along its forward
/// direction and list entities whose `WorldBound` sphere the ray
/// intersects, sorted by ray-parameter (closest first). Default count
/// 10.
///
/// Use this to identify "the thing I'm looking at" without the noise
/// of `near` (which lists everything within a radial sphere). Pair
/// with `mesh.info <id>` for the full inspect on the top hit.
///
/// Caveat: matches against bounding spheres only — a hit at the
/// nearest sphere's edge can register before a small geometry inside
/// a bigger sphere. The first 2-3 hits are usually what you want.
pub(crate) struct PickCommand;
impl ConsoleCommand for PickCommand {
    fn name(&self) -> &str {
        "pick"
    }
    fn description(&self) -> &str {
        "Ray-cast from camera forward; list entities the ray pierces (usage: pick [count=10])"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let count: usize = args.trim().parse().unwrap_or(10);
        let Some(active) = world.try_resource::<ActiveCamera>() else {
            return CommandOutput::line("ActiveCamera resource not present");
        };
        let cam_entity = active.0;
        drop(active);
        let cam_pos = world
            .query::<Transform>()
            .and_then(|q| q.get(cam_entity).map(|t| t.translation));
        let Some(cam_pos) = cam_pos else {
            return CommandOutput::line(format!("Camera entity {cam_entity} has no Transform"));
        };
        // Camera forward derived from InputState (yaw, pitch) the way
        // fly_camera_system computes it: forward = R_y(yaw)·R_x(pitch)·-Z.
        let (yaw, pitch) = world
            .try_resource::<InputState>()
            .map(|i| (i.yaw, i.pitch))
            .unwrap_or((0.0, 0.0));
        let cy = yaw.cos();
        let sy = yaw.sin();
        let cp = pitch.cos();
        let sp = pitch.sin();
        // forward = R_y(yaw) * R_x(pitch) * (0,0,-1)
        // R_x(pitch) * (0,0,-1) = (0, sin(pitch), -cos(pitch))
        // R_y(yaw)   * (0, sin(pitch), -cos(pitch)) =
        //   ( -sin(yaw)·cos(pitch), sin(pitch), -cos(yaw)·cos(pitch) )
        let forward = Vec3::new(-sy * cp, sp, -cy * cp);

        // Tier 1: proper WorldBound sphere — counts as a real hit.
        // Tier 2: GlobalTransform-only fallback — many entities ship
        // with `WorldBound::default()` (zero center, zero radius) when
        // the NIF importer didn't surface a usable local sphere. We
        // still want those entities in the pick list, so we synthesise
        // a 32-unit sphere at the entity's GlobalTransform.translation
        // (1 m at FNV scale — wide enough to catch a wall the camera
        // is hugging, tight enough to avoid grabbing the whole room).
        // Synthetic hits are flagged with `~` in the radius column so
        // the operator knows they're approximate, not authored.
        const SYNTH_RADIUS: f32 = 32.0;

        let Some(gtq) = world.query::<GlobalTransform>() else {
            return CommandOutput::line(
                "GlobalTransform storage not present (no entities to test against)",
            );
        };

        // Ray r(t) = cam_pos + t · forward; sphere center c, radius R.
        // Intersect when |r(t) - c|² = R². Quadratic in t:
        //   a = forward·forward = 1
        //   b = 2 · (cam_pos - c) · forward
        //   c = |cam_pos - c|² - R²
        // disc = b² - 4·a·c. disc >= 0 → at least one real root; take
        // the smaller positive root as the hit distance.
        let mut hits: Vec<(f32, EntityId, Vec3, f32, bool)> = Vec::new();
        for (entity, gt) in gtq.iter() {
            if entity == cam_entity {
                continue;
            }
            // Prefer authored WorldBound when present + non-degenerate.
            let (center, radius, synthetic) = match world.get::<WorldBound>(entity) {
                Some(wb) if wb.radius > 0.0 => (wb.center, wb.radius, false),
                _ => (gt.translation, SYNTH_RADIUS, true),
            };
            let oc = cam_pos - center;
            let b = 2.0 * oc.dot(forward);
            let cc = oc.length_squared() - radius * radius;
            let disc = b * b - 4.0 * cc;
            if disc < 0.0 {
                continue;
            }
            let sqrt_disc = disc.sqrt();
            let t0 = (-b - sqrt_disc) * 0.5;
            let t1 = (-b + sqrt_disc) * 0.5;
            // Pick the closer non-negative root. If both negative, the
            // sphere is entirely behind the camera — skip.
            let t = if t0 >= 0.0 {
                t0
            } else if t1 >= 0.0 {
                // Camera inside sphere — still counts as a hit but at
                // t=0 (we're inside it right now).
                0.0
            } else {
                continue;
            };
            hits.push((t, entity, center, radius, synthetic));
        }
        drop(gtq);
        hits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        if hits.is_empty() {
            return CommandOutput::line(format!(
                "no WorldBound spheres along ray from ({:.1},{:.1},{:.1}) dir ({:+.3},{:+.3},{:+.3})",
                cam_pos.x, cam_pos.y, cam_pos.z, forward.x, forward.y, forward.z
            ));
        }
        let take_n = count.min(hits.len()).max(1);
        let mut lines = Vec::with_capacity(take_n + 2);
        lines.push(format!(
            "ray from ({:.1},{:.1},{:.1}) dir ({:+.3},{:+.3},{:+.3}) — \
             {} hits (top {}):",
            cam_pos.x,
            cam_pos.y,
            cam_pos.z,
            forward.x,
            forward.y,
            forward.z,
            hits.len(),
            take_n
        ));
        lines.push(format!(
            "{:>7}  {:>6}  {:<28}  {:<48}  {:>7}  {}",
            "t", "id", "name", "tex/mat path", "r", "sphere center"
        ));
        for (t, entity, center, radius, synthetic) in hits.iter().take(take_n) {
            let name_str = resolve_entity_name(world, *entity).unwrap_or_else(|| "-".to_string());
            let path = world
                .get::<Material>(*entity)
                .and_then(|m| {
                    m.texture_path
                        .as_deref()
                        .or(m.material_path.as_deref())
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();
            let radius_str = if *synthetic {
                format!("~{:.0}", radius)
            } else {
                format!("{:.1}", radius)
            };
            lines.push(format!(
                "{:>7.1}  {:>6}  {:<28.28}  {:<48.48}  {:>7}  ({:>+6.1},{:>+6.1},{:>+6.1})",
                t, entity, name_str, path, radius_str, center.x, center.y, center.z
            ));
        }
        CommandOutput::lines(lines)
    }
}
/// `cam.pos x y z` — teleport the active camera to an absolute world
/// position (renderer Y-up). Leaves rotation untouched.
///
/// `fly_camera_system` early-returns when the mouse isn't captured
/// (the default for `--bench-hold`), so the new position persists
/// across frames. With mouse capture active the camera still moves
/// relative to WASD input, so this command sets the *anchor* for that
/// frame's worth of input rather than locking the camera in place.
pub(crate) struct CamPosCommand;
impl ConsoleCommand for CamPosCommand {
    fn name(&self) -> &str {
        "cam.pos"
    }
    fn description(&self) -> &str {
        "Teleport camera to absolute world position (usage: cam.pos x y z)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let parts: Vec<&str> = args.split_whitespace().collect();
        if parts.len() != 3 {
            return CommandOutput::line("usage: cam.pos <x> <y> <z>  (renderer Y-up coordinates)");
        }
        let parse = |s: &str| -> Option<f32> { s.parse::<f32>().ok() };
        let (Some(x), Some(y), Some(z)) = (parse(parts[0]), parse(parts[1]), parse(parts[2]))
        else {
            return CommandOutput::line(format!(
                "cam.pos: failed to parse coordinates from `{args}`"
            ));
        };
        let Some(active) = world.try_resource::<ActiveCamera>() else {
            return CommandOutput::line("ActiveCamera resource not present");
        };
        let cam_entity = active.0;
        drop(active);
        let Some(mut tq) = world.query_mut::<Transform>() else {
            return CommandOutput::line("Transform storage not present");
        };
        let Some(transform) = tq.get_mut(cam_entity) else {
            return CommandOutput::line(format!("Camera entity {cam_entity} has no Transform"));
        };
        transform.translation = Vec3::new(x, y, z);
        CommandOutput::line(format!("Camera teleported to ({x:.2}, {y:.2}, {z:.2})"))
    }
}
/// `cam.tp <entity_id>` — teleport the active camera to look at the
/// given entity. The camera lands ~200 units back along the target's
/// -Z axis at +50 Y for a reasonable over-the-shoulder framing on
/// FNV / Skyrim+ NPCs (~100 unit tall humanoids).
///
/// Both `Transform.rotation` and `InputState.{yaw, pitch}` are
/// updated so the orientation survives the next `fly_camera_system`
/// tick even when the mouse is captured.
///
/// The natural usage with `skin.coverage`: spawn a multi-NPC cell with
/// `--bench-hold`, `cam.tp <npc_entity_id>` to frame the actor, then
/// `skin.coverage` reads the new viewpoint's dispatches_total.
pub(crate) struct CamTpCommand;
impl ConsoleCommand for CamTpCommand {
    fn name(&self) -> &str {
        "cam.tp"
    }
    fn description(&self) -> &str {
        "Teleport camera to look at entity (usage: cam.tp <entity_id>)"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let trimmed = args.trim();
        let target_id = if trimmed.is_empty() {
            // Fall back to the picked reference (`prid <id>` workflow).
            // No selection AND no arg → user error, point them at the
            // shorter path.
            let Some(sel) = world.try_resource::<SelectedRef>() else {
                return CommandOutput::line("SelectedRef resource not present");
            };
            let Some(id) = sel.0 else {
                return CommandOutput::line(
                    "usage: cam.tp <entity_id>  (or `prid <id>` then `cam.tp`)",
                );
            };
            id
        } else {
            let Ok(id) = trimmed.parse::<EntityId>() else {
                return CommandOutput::line(format!(
                    "cam.tp: failed to parse entity id from `{trimmed}`"
                ));
            };
            id
        };
        let Some(active) = world.try_resource::<ActiveCamera>() else {
            return CommandOutput::line("ActiveCamera resource not present");
        };
        let cam_entity = active.0;
        drop(active);
        // Read the target's world position. GlobalTransform is updated
        // by `transform_propagation_system` each frame — for entities
        // freshly spawned this frame the value may still be the
        // identity-default, but for cell-stable entities it's the
        // resolved position. Read-only — no lock contention with the
        // mutate below.
        let target_pos = world
            .query::<GlobalTransform>()
            .and_then(|q| q.get(target_id).map(|gt| gt.translation));
        let Some(target_pos) = target_pos else {
            return CommandOutput::line(format!(
                "Entity {target_id} has no GlobalTransform (does it exist? `entities` to list)"
            ));
        };
        // Land ~200 units back + 50 up. World-space offset, not local
        // — keeps the over-the-shoulder framing predictable regardless
        // of the target's own orientation.
        let camera_pos = target_pos + Vec3::new(0.0, 50.0, 200.0);
        let (yaw, pitch) = look_at_yaw_pitch(camera_pos, target_pos);
        let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
        // Apply Transform mutation under its own scope so the input-
        // state mutation doesn't hold two write guards simultaneously.
        {
            let Some(mut tq) = world.query_mut::<Transform>() else {
                return CommandOutput::line("Transform storage not present");
            };
            let Some(transform) = tq.get_mut(cam_entity) else {
                return CommandOutput::line(format!("Camera entity {cam_entity} has no Transform"));
            };
            transform.translation = camera_pos;
            transform.rotation = rotation;
        }
        // Sync InputState so the next fly_camera tick under mouse
        // capture reads back the same yaw/pitch instead of overwriting
        // the look direction with stale accumulator values.
        if let Some(mut input) = world.try_resource_mut::<InputState>() {
            input.yaw = yaw;
            input.pitch = pitch;
        }
        CommandOutput::lines(vec![
            format!(
                "Camera teleported to look at entity {target_id} at \
                 ({:.2}, {:.2}, {:.2})",
                target_pos.x, target_pos.y, target_pos.z,
            ),
            format!(
                "  camera now at ({:.2}, {:.2}, {:.2}) yaw {:.1}° pitch {:.1}°",
                camera_pos.x,
                camera_pos.y,
                camera_pos.z,
                yaw.to_degrees(),
                pitch.to_degrees(),
            ),
        ])
    }
}
