//! Camera + selection / picking commands.
//!
//! `prid`, `cam.where`, `near`, `pick`, `cam.pos`, `cam.tp`.

use super::shared::*;

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
