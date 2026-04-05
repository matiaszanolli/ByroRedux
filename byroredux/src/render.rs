//! Per-frame render data collection from ECS queries.

use byroredux_core::ecs::{
    ActiveCamera, AnimatedVisibility, Camera, GlobalTransform, LightSource, MeshHandle,
    TextureHandle, Transform, World,
};
use byroredux_core::math::Mat4;
use byroredux_renderer::vulkan::context::DrawCommand;

use crate::components::{AlphaBlend, CellLightingRes, Decal, TwoSided};

/// Build the view-projection matrix and draw command list from ECS queries.
pub(crate) fn build_render_data(
    world: &World,
    draw_commands: &mut Vec<DrawCommand>,
    gpu_lights: &mut Vec<byroredux_renderer::GpuLight>,
) -> ([f32; 16], [f32; 3], [f32; 3]) {
    draw_commands.clear();
    gpu_lights.clear();

    // Get camera view-projection.
    let view_proj = if let Some(active) = world.try_resource::<ActiveCamera>() {
        let cam_entity = active.0;
        drop(active);

        let cam_q = world.query::<Camera>();
        let transform_q = world.query::<Transform>();

        let vp = match (cam_q, transform_q) {
            (Some(cq), Some(tq)) => {
                let cam = cq.get(cam_entity);
                let t = tq.get(cam_entity);
                match (cam, t) {
                    (Some(c), Some(t)) => c.projection_matrix() * Camera::view_matrix(t),
                    _ => Mat4::IDENTITY,
                }
            }
            _ => Mat4::IDENTITY,
        };
        vp.to_cols_array()
    } else {
        Mat4::IDENTITY.to_cols_array()
    };

    // Collect draw commands from entities with (GlobalTransform, MeshHandle).
    // TextureHandle is optional — entities without one use the fallback (0).
    if let Some((tq, mq)) = world.query_2_mut::<GlobalTransform, MeshHandle>() {
        let tex_q = world.query::<TextureHandle>();
        let alpha_q = world.query::<AlphaBlend>();
        let two_sided_q = world.query::<TwoSided>();
        let decal_q = world.query::<Decal>();
        let vis_q = world.query::<AnimatedVisibility>();
        for (entity, mesh) in mq.iter() {
            // Skip entities hidden by animation.
            let visible = vis_q
                .as_ref()
                .and_then(|q| q.get(entity))
                .map(|v| v.0)
                .unwrap_or(true);
            if !visible {
                continue;
            }

            if let Some(transform) = tq.get(entity) {
                let tex_handle = tex_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|t| t.0)
                    .unwrap_or(0);
                let alpha_blend = alpha_q
                    .as_ref()
                    .map(|q| q.get(entity).is_some())
                    .unwrap_or(false);
                let two_sided = two_sided_q
                    .as_ref()
                    .map(|q| q.get(entity).is_some())
                    .unwrap_or(false);
                let is_decal = decal_q
                    .as_ref()
                    .map(|q| q.get(entity).is_some())
                    .unwrap_or(false);
                draw_commands.push(DrawCommand {
                    mesh_handle: mesh.0,
                    texture_handle: tex_handle,
                    model_matrix: transform.to_matrix().to_cols_array(),
                    alpha_blend,
                    two_sided,
                    is_decal,
                });
            }
        }
    }
    // Sort: opaque → decal → alpha; decals drawn after base geometry at same depth.
    draw_commands.sort_unstable_by_key(|cmd| {
        (
            cmd.alpha_blend,
            cmd.is_decal,
            cmd.two_sided,
            cmd.texture_handle,
        )
    });

    // Collect lights from ECS.

    // Add cell directional light (primary interior illumination).
    if let Some(cell_lit) = world.try_resource::<CellLightingRes>() {
        gpu_lights.push(byroredux_renderer::GpuLight {
            position_radius: [0.0, 0.0, 0.0, 0.0],
            color_type: [
                cell_lit.directional_color[0],
                cell_lit.directional_color[1],
                cell_lit.directional_color[2],
                2.0,
            ], // 2 = directional
            direction_angle: [
                cell_lit.directional_dir[0],
                cell_lit.directional_dir[1],
                cell_lit.directional_dir[2],
                0.0,
            ],
        });
    }

    // Add placed point lights from LIGH records.
    if let Some((tq, lq)) = world.query_2_mut::<GlobalTransform, LightSource>() {
        for (entity, light) in lq.iter() {
            if let Some(t) = tq.get(entity) {
                gpu_lights.push(byroredux_renderer::GpuLight {
                    position_radius: [
                        t.translation.x,
                        t.translation.y,
                        t.translation.z,
                        light.radius,
                    ],
                    color_type: [light.color[0], light.color[1], light.color[2], 0.0], // 0 = point
                    direction_angle: [0.0, 0.0, 0.0, 0.0],
                });
            }
        }
    }

    // Log light count once.
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static LOGGED: AtomicBool = AtomicBool::new(false);
        if !LOGGED.swap(true, Ordering::Relaxed) {
            log::info!(
                "Lights collected: {} (first 3: {:?})",
                gpu_lights.len(),
                gpu_lights
                    .iter()
                    .take(3)
                    .map(|l| (l.position_radius, l.color_type))
                    .collect::<Vec<_>>(),
            );
        }
    }

    // Camera position.
    let camera_pos = if let Some(active) = world.try_resource::<ActiveCamera>() {
        let cam_entity = active.0;
        drop(active);
        let tq = world.query::<Transform>();
        tq.and_then(|q| {
            q.get(cam_entity)
                .map(|t| [t.translation.x, t.translation.y, t.translation.z])
        })
        .unwrap_or([0.0; 3])
    } else {
        [0.0; 3]
    };

    // Cell ambient color (or default).
    let ambient = world
        .try_resource::<CellLightingRes>()
        .map(|l| l.ambient)
        .unwrap_or([0.08, 0.08, 0.08]);

    (view_proj, camera_pos, ambient)
}
