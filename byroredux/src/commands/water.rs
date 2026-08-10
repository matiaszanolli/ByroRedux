//! Water render/physics diagnostics.
//!
//! `water.dump` exposes the canonical plane/material/volume selected around
//! the active camera. `water.contacts` exposes the physics sink, including the
//! same flow vector the solver consumes. Together they make WATAL's two ends
//! observable from a real-data smoke without reaching into renderer internals.

use super::shared::*;
use byroredux_core::ecs::components::water::{
    SubmersionState, WaterContact, WaterFlow, WaterPlane, WaterVolume,
};

fn active_camera_position(world: &World) -> Option<(EntityId, Vec3)> {
    let camera = world
        .try_resource::<ActiveCamera>()
        .map(|active| active.0)?;
    let position = world
        .query::<GlobalTransform>()
        .and_then(|query| query.get(camera).map(|transform| transform.translation))?;
    Some((camera, position))
}

fn format_flow(flow: Option<WaterFlow>) -> String {
    match flow {
        Some(flow) => format!(
            "[{:.3},{:.3},{:.3}]@{:.3}",
            flow.direction[0], flow.direction[1], flow.direction[2], flow.speed
        ),
        None => "none".to_string(),
    }
}

/// Inspect canonical water planes, their resolved material, and camera state.
pub(crate) struct WaterDumpCommand;

impl ConsoleCommand for WaterDumpCommand {
    fn name(&self) -> &str {
        "water.dump"
    }

    fn description(&self) -> &str {
        "List canonical water planes/materials and active-camera submersion"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let camera = active_camera_position(world);
        let camera_state = camera.and_then(|(entity, _)| {
            world
                .query::<SubmersionState>()
                .and_then(|query| query.get(entity).copied())
        });

        let plane_q = world.query::<WaterPlane>();
        let volume_q = world.query::<WaterVolume>();
        let flow_q = world.query::<WaterFlow>();
        let plane_count = plane_q.as_ref().map_or(0, |query| query.len());

        let mut lines = vec![format!("Water dump: planes={plane_count}")];
        match (camera, camera_state) {
            (Some((entity, position)), Some(state)) => lines.push(format!(
                "  camera={} pos=[{:.2},{:.2},{:.2}] submerged={} depth={:.2} material={}",
                entity,
                position.x,
                position.y,
                position.z,
                state.head_submerged,
                state.depth,
                state
                    .material
                    .map(|material| format!("0x{:08X}", material.source_form))
                    .unwrap_or_else(|| "none".to_string()),
            )),
            (Some((entity, position)), None) => lines.push(format!(
                "  camera={} pos=[{:.2},{:.2},{:.2}] submersion=missing",
                entity, position.x, position.y, position.z
            )),
            (None, _) => lines.push("  camera=unavailable".to_string()),
        }

        let Some(plane_q) = plane_q else {
            return CommandOutput::lines(lines);
        };
        let mut rows = Vec::with_capacity(plane_q.len());
        for (entity, plane) in plane_q.iter() {
            let volume = volume_q
                .as_ref()
                .and_then(|query| query.get(entity).copied());
            let flow = flow_q.as_ref().and_then(|query| query.get(entity).copied());
            let contains_camera = match (camera, volume) {
                (Some((_, position)), Some(volume)) => {
                    position.x >= volume.min[0]
                        && position.x <= volume.max[0]
                        && position.y >= volume.min[1]
                        && position.y <= volume.max[1]
                        && position.z >= volume.min[2]
                        && position.z <= volume.max[2]
                }
                _ => false,
            };
            rows.push((entity, *plane, volume, flow, contains_camera));
        }
        rows.sort_by_key(|row| row.0);

        for (entity, plane, volume, flow, contains_camera) in rows {
            let material = plane.material;
            let volume_text = volume.map_or_else(
                || "missing".to_string(),
                |volume| {
                    format!(
                        "[{:.1},{:.1},{:.1}]..[{:.1},{:.1},{:.1}]",
                        volume.min[0],
                        volume.min[1],
                        volume.min[2],
                        volume.max[0],
                        volume.max[1],
                        volume.max[2]
                    )
                },
            );
            lines.push(format!(
                "  plane={} camera_inside={} kind={:?} source=0x{:08X} volume={} flow={}",
                entity,
                contains_camera,
                plane.kind,
                material.source_form,
                volume_text,
                format_flow(flow),
            ));
            lines.push(format!(
                "    shallow=[{:.3},{:.3},{:.3}] deep=[{:.3},{:.3},{:.3}] fog={:.1}..{:.1} fresnel={:.4} reflect={:.3} normal={} wave={:.4}@{:.4}",
                material.shallow_color[0],
                material.shallow_color[1],
                material.shallow_color[2],
                material.deep_color[0],
                material.deep_color[1],
                material.deep_color[2],
                material.fog_near,
                material.fog_far,
                material.fresnel_f0,
                material.reflectivity,
                if material.normal_map_index == u32::MAX {
                    "procedural".to_string()
                } else {
                    material.normal_map_index.to_string()
                },
                material.wave_amplitude,
                material.wave_frequency,
            ));
        }

        CommandOutput::lines(lines)
    }
}

/// Inspect dynamic-body contact with canonical water volumes.
pub(crate) struct WaterContactsCommand;

impl ConsoleCommand for WaterContactsCommand {
    fn name(&self) -> &str {
        "water.contacts"
    }

    fn description(&self) -> &str {
        "List physics bodies with derived water contact, depth, and current"
    }

    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        let Some(contact_q) = world.query::<WaterContact>() else {
            return CommandOutput::line("Water contacts: total=0 wet=0 submerged=0");
        };
        let mut rows: Vec<(EntityId, WaterContact)> = contact_q
            .iter()
            .map(|(entity, contact)| (entity, *contact))
            .collect();
        rows.sort_by_key(|row| row.0);

        let wet = rows
            .iter()
            .filter(|(_, contact)| contact.submerged_fraction > 0.0)
            .count();
        let submerged = rows
            .iter()
            .filter(|(_, contact)| contact.head_submerged)
            .count();
        let mut lines = vec![format!(
            "Water contacts: total={} wet={} submerged={}",
            rows.len(),
            wet,
            submerged
        )];
        for (entity, contact) in rows {
            lines.push(format!(
                "  entity={} depth={:.2} fraction={:.3} head_submerged={} flow={} material={}",
                entity,
                contact.depth,
                contact.submerged_fraction,
                contact.head_submerged,
                format_flow(contact.flow),
                contact
                    .material
                    .map(|material| format!("0x{:08X}", material.source_form))
                    .unwrap_or_else(|| "none".to_string()),
            ));
        }
        CommandOutput::lines(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::water::{WaterKind, WaterMaterial};
    use byroredux_core::math::Quat;

    #[test]
    fn dump_reports_camera_plane_flow_and_canonical_material() {
        let mut world = World::new();
        let camera = world.spawn();
        world.insert_resource(ActiveCamera(camera));
        world.insert(
            camera,
            GlobalTransform::new(Vec3::new(2.0, -3.0, 4.0), Quat::IDENTITY, 1.0),
        );
        let material = WaterMaterial {
            source_form: 0x1234,
            ..WaterMaterial::default()
        };
        world.insert(
            camera,
            SubmersionState {
                depth: 3.0,
                head_submerged: true,
                material: Some(material),
            },
        );

        let water = world.spawn();
        world.insert(
            water,
            WaterPlane {
                kind: WaterKind::River,
                material,
            },
        );
        world.insert(
            water,
            WaterVolume {
                min: [-10.0, -20.0, -10.0],
                max: [10.0, 0.0, 10.0],
            },
        );
        world.insert(
            water,
            WaterFlow {
                direction: [1.0, 0.0, 0.0],
                speed: 8.0,
            },
        );

        let output = WaterDumpCommand.execute(&world, "").lines.join("\n");
        assert!(output.contains("Water dump: planes=1"), "{output}");
        assert!(
            output.contains("submerged=true depth=3.00 material=0x00001234"),
            "{output}"
        );
        assert!(
            output.contains("camera_inside=true kind=River source=0x00001234"),
            "{output}"
        );
        assert!(
            output.contains("flow=[1.000,0.000,0.000]@8.000"),
            "{output}"
        );
    }

    #[test]
    fn contacts_report_wet_and_dry_bodies() {
        let mut world = World::new();
        let wet = world.spawn();
        let dry = world.spawn();
        world.insert(
            wet,
            WaterContact {
                depth: 5.0,
                submerged_fraction: 0.75,
                head_submerged: true,
                flow: Some(WaterFlow {
                    direction: [0.0, 0.0, 1.0],
                    speed: 2.0,
                }),
                material: Some(WaterMaterial {
                    source_form: 0xABCD,
                    ..WaterMaterial::default()
                }),
            },
        );
        world.insert(dry, WaterContact::default());

        let output = WaterContactsCommand.execute(&world, "").lines.join("\n");
        assert!(
            output.contains("Water contacts: total=2 wet=1 submerged=1"),
            "{output}"
        );
        assert!(
            output.contains(&format!("entity={wet} depth=5.00")),
            "{output}"
        );
        assert!(
            output.contains("flow=[0.000,0.000,1.000]@2.000"),
            "{output}"
        );
        assert!(output.contains("material=0x0000ABCD"), "{output}");
        assert!(
            output.contains(&format!("entity={dry} depth=0.00")),
            "{output}"
        );
    }
}
