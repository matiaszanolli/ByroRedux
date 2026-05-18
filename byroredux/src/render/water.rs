//! Water-plane re-emit — extracted from `build_render_data` per #1115.
//!
//! Walks every `WaterPlane` entity, locates its already-emitted
//! `DrawCommand` (the main mesh-iteration loop above produced it
//! because water entities carry `MeshHandle`), flips its `is_water`
//! flag so the regular triangle path skips it, and emits a parallel
//! `WaterDrawCommand` whose `instance_index` matches the SSBO slot
//! the renderer will assign to that draw.
//!
//! The slot-index ↔ Vec position map relies on the renderer's 1:1
//! contract: `gpu_instances` is populated by iterating `draw_commands`
//! in order, and frustum-culled draws keep their SSBO slot per #516.
//! So the index into `draw_commands` equals `gl_InstanceIndex` after
//! upload.
//!
//! ⚠ **No-resort contract** (#1026 / F-WAT-05) — once the
//! `instance_index` captured below is written, `draw_commands` MUST
//! NOT be re-ordered before the renderer consumes it. The
//! defensive `debug_assert!` lives in `VulkanContext::draw_frame`
//! (using `byroredux_renderer::vulkan::water::water_commands_match_draw_slots`).
//! This function must run AFTER the draw_commands sort and BEFORE
//! the renderer consumes them.

use byroredux_core::ecs::components::water::{WaterFlow, WaterKind, WaterPlane};
use byroredux_core::ecs::{TotalTime, World};
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::vulkan::water::{WaterDrawCommand, WaterPush};

/// Re-emit water planes: flip the `is_water` flag on each plane's
/// already-emitted draw command and produce a matching
/// `WaterDrawCommand` referencing the same SSBO slot.
///
/// Linear scan over `draw_commands` per water entity is O(N×W);
/// typical N is ~thousands of draws and W is ≤ ~3 water planes per
/// cell, so this is well under a microsecond. A
/// `HashMap<EntityId, usize>` would be premature for the expected
/// scale.
pub(super) fn reemit_water_planes(
    world: &World,
    draw_commands: &mut Vec<DrawCommand>,
    water_commands: &mut Vec<WaterDrawCommand>,
) {
    let time_secs = world
        .try_resource::<TotalTime>()
        .map(|t| t.0)
        .unwrap_or(0.0);
    let Some(wq) = world.query::<WaterPlane>() else {
        return;
    };
    let fq = world.query::<WaterFlow>();
    for (entity, plane) in wq.iter() {
        let Some(idx) = draw_commands.iter().position(|c| c.entity_id == entity) else {
            // Entity has WaterPlane but no DrawCommand was emitted —
            // typically because the cell loader spawned the water
            // entity but the mesh wasn't yet uploaded, or the
            // entity is frustum-culled out of the regular emit
            // path. Skip silently.
            continue;
        };
        draw_commands[idx].is_water = true;

        let flow = fq.as_ref().and_then(|q| q.get(entity).copied());
        let (flow_dir, flow_speed) = match flow {
            Some(f) => (f.direction, f.speed),
            None => ([1.0, 0.0, 0.0], 0.0),
        };

        // ABI: matches `WaterPush` in `shaders/water.frag`.
        // Each vec4 maps to one std430-scalar slot — see
        // `crates/renderer/src/vulkan/water.rs::WaterPush` for
        // the layout contract.
        let mat = &plane.material;
        let push = WaterPush {
            timing: [
                time_secs,
                plane.kind as u8 as f32,
                mat.foam_strength,
                mat.ior,
            ],
            flow: [flow_dir[0], flow_dir[1], flow_dir[2], flow_speed],
            shallow: [
                mat.shallow_color[0],
                mat.shallow_color[1],
                mat.shallow_color[2],
                mat.fog_near,
            ],
            deep: [
                mat.deep_color[0],
                mat.deep_color[1],
                mat.deep_color[2],
                mat.fog_far,
            ],
            scroll: [
                mat.scroll_a[0],
                mat.scroll_a[1],
                mat.scroll_b[0],
                mat.scroll_b[1],
            ],
            tune: [
                mat.uv_scale_a,
                mat.uv_scale_b,
                mat.shoreline_width,
                0.0, // reserved (reflectivity moved to tint_reflect.w in #1069)
            ],
            misc: [
                mat.fresnel_f0,
                0.0,
                WaterPush::pack_normal_index(mat.normal_map_index),
                0.0,
            ],
            tint_reflect: [
                mat.reflection_tint[0],
                mat.reflection_tint[1],
                mat.reflection_tint[2],
                mat.reflectivity,
            ],
        };
        water_commands.push(WaterDrawCommand {
            mesh_handle: draw_commands[idx].mesh_handle,
            instance_index: idx as u32,
            push,
        });
        // Silence WaterKind-unused warning on builds where the
        // enum is only consumed by the f32 cast above.
        let _ = WaterKind::Calm;
    }
}
