//! Particle billboard emission — extracted from `build_render_data` per #1115.
//!
//! Each live particle becomes one DrawCommand referencing the unit
//! particle quad mesh (handle passed in from the caller). Model matrix
//! is `translate(world_pos) · face_camera_rot · scale(size)`, so all
//! per-particle dynamics live in the model matrix and the existing
//! instanced batching from #272 collapses every particle (consecutive
//! in the sorted list, sharing mesh+pipeline) into a single instanced
//! cmd_draw_indexed.
//!
//! Color flows through `emissive_color * emissive_mult` — the
//! fragment shader's emissive add lights the quad with no scene-light
//! dependency. Particles default to additive blending
//! (src=SRC_ALPHA, dst=ONE) per ParticleEmitter defaults; per-emitter
//! overrides ride through the existing pipeline cache from #392.

use byroredux_core::ecs::{GlobalTransform, ParticleEmitter, RenderLayer, World};
use byroredux_core::math::{Mat4, Quat, Vec3, Vec4};
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::MaterialTable;

use super::f32_sortable_u32;

/// Emit one DrawCommand per live particle into `draw_commands`.
///
/// Skipped entirely when `particle_quad_handle == None` (no unit-quad
/// mesh registered yet) or when no entity carries `ParticleEmitter`.
///
/// Must run BEFORE the draw_commands sort (the emitted commands need
/// the same sort treatment as the rest of the frame's draws).
pub(super) fn emit_particles(
    world: &World,
    particle_quad_handle: Option<u32>,
    cam_pos: Vec3,
    vp_mat: Mat4,
    draw_commands: &mut Vec<DrawCommand>,
    material_table: &mut MaterialTable,
) {
    let Some(particle_mesh) = particle_quad_handle else {
        return;
    };
    let (Some(gtq), Some(eq)) = (
        world.query::<GlobalTransform>(),
        world.query::<ParticleEmitter>(),
    ) else {
        return;
    };
    for (entity, em) in eq.iter() {
        let _ = gtq.get(entity); // transform sampled by the system at spawn
        if em.particles.is_empty() {
            continue;
        }
        let particle_count = em.particles.len();
        for i in 0..particle_count {
            let p = em.particles.positions[i];
            let world_pos = Vec3::new(p[0], p[1], p[2]);
            // Face-camera rotation: align the quad's local +Z
            // (its outward normal — see `quad_vertices` which
            // sets normals to (0,0,1)) toward the camera.
            let to_cam = cam_pos - world_pos;
            let rot = if to_cam.length_squared() > 1.0e-6 {
                Quat::from_rotation_arc(Vec3::Z, to_cam.normalize())
            } else {
                Quat::IDENTITY
            };
            // LERP color and size against age/life so particles
            // fade out smoothly and grow/shrink as configured.
            let t = (em.particles.ages[i] / em.particles.lifes[i]).clamp(0.0, 1.0);
            let start_c = em.start_color;
            let end_c = em.end_color;
            let color = [
                start_c[0] + (end_c[0] - start_c[0]) * t,
                start_c[1] + (end_c[1] - start_c[1]) * t,
                start_c[2] + (end_c[2] - start_c[2]) * t,
                start_c[3] + (end_c[3] - start_c[3]) * t,
            ];
            let size = em.start_size + (em.end_size - em.start_size) * t;

            let model = Mat4::from_scale_rotation_translation(Vec3::splat(size), rot, world_pos);
            let pos_clip = vp_mat * Vec4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);
            let sort_depth = f32_sortable_u32(pos_clip.w);

            let mut cmd = DrawCommand {
                mesh_handle: particle_mesh,
                texture_handle: 0,
                model_matrix: model.to_cols_array(),
                alpha_blend: true,
                src_blend: em.src_blend,
                dst_blend: em.dst_blend,
                two_sided: true, // billboard quads are single-faced; cull-off avoids back-face flicker on extreme angles
                // Particles never use wireframe (sprites don't render
                // line-by-line) or flat-shading (no per-face geometry
                // — billboards are screen-aligned quads). #869.
                wireframe: false,
                flat_shading: false,
                is_decal: false,
                // Particles ride emissive + alpha-blend with depth-write
                // off — they never z-fight surfaces, so Architecture
                // (zero bias) is correct. See `RenderLayer::depth_bias`.
                render_layer: RenderLayer::Architecture,
                bone_offset: 0,
                normal_map_index: 0,
                dark_map_index: 0,
                glow_map_index: 0,
                detail_map_index: 0,
                gloss_map_index: 0,
                parallax_map_index: 0,
                parallax_height_scale: 0.04,
                parallax_max_passes: 4.0,
                env_map_index: 0,
                env_mask_index: 0,
                alpha_threshold: 0.0,
                alpha_test_func: 0,
                roughness: 1.0,
                metalness: 0.0,
                // #1248 — generic dielectric default (η = 1.5 → F0 ≈ 0.04).
                // Particles don't author IOR; the default reproduces the
                // pre-#1248 hardcoded vec3(0.04) shader behaviour.
                ior: 1.5,
                // #1249 — Disney diffuse off (particles use the legacy
                // Lambert path; MAT_FLAG_BGSM_PBR never fires here).
                subsurface: 0.0,
                sheen: 0.0,
                sheen_tint: 0.0,
                // #1250 — isotropic GGX (legacy ax = ay = roughness²).
                anisotropic: 0.0,
                // Emissive carries the particle color * alpha so the
                // existing fragment-shader emissive add lights the quad
                // with no scene-light dependency. Alpha is folded into
                // emissive_mult so the LERP-to-0 end-color drives a
                // true fade-out.
                emissive_mult: color[3],
                emissive_color: [color[0], color[1], color[2]],
                specular_strength: 0.0,
                specular_color: [0.0, 0.0, 0.0],
                // Particles ride emissive; identity diffuse + ambient
                // so the tint/ambient multipliers don't interact with
                // the emissive add (#221).
                diffuse_color: [1.0, 1.0, 1.0],
                ambient_color: [1.0, 1.0, 1.0],
                vertex_offset: 0,
                index_offset: 0,
                vertex_count: 0,
                sort_depth,
                in_tlas: false,
                // Particles are drawn every frame they're alive; no
                // frustum cull here (small, transient).
                in_raster: true,
                // Deterministic tiebreaker for same-emitter particles
                // sharing depth bucket and color. XOR keeps the emitter
                // grouping intact while giving each particle its own
                // ordering slot.
                entity_id: entity ^ (i as u32),
                // Particles use identity UV + full alpha — the
                // billboard quad is a unit square and the emitter's
                // per-frame RGBA color already rides on emissive_color
                // / emissive_mult above.
                uv_offset: [0.0, 0.0],
                uv_scale: [1.0, 1.0],
                material_alpha: 1.0,
                avg_albedo: [0.0, 0.0, 0.0],
                material_kind: 0,
                // Particles render with depth test on, depth write off
                // (alpha-blended billboards). Default LESSEQUAL.
                // See #398.
                z_test: true,
                z_write: false,
                z_function: 3,
                terrain_tile_index: None,
                // Particles are never Skyrim+ variant shading.
                skin_tint_rgba: [0.0; 4],
                hair_tint_rgb: [0.0; 3],
                multi_layer_envmap_strength: 0.0,
                eye_left_center: [0.0; 3],
                eye_cubemap_scale: 0.0,
                eye_right_center: [0.0; 3],
                multi_layer_inner_thickness: 0.0,
                multi_layer_refraction_scale: 0.0,
                multi_layer_inner_scale: [1.0, 1.0],
                sparkle_rgba: [0.0; 4],
                // #620 — particles never carry an effect-shader
                // falloff cone; identity-pass-through.
                effect_falloff: [1.0, 1.0, 1.0, 1.0, 0.0],
                material_id: 0,
                // Particles ride the emissive accumulator through
                // emissive_color / emissive_mult already; no per-vertex
                // emissive payload (#695).
                vertex_color_emissive: false,
                // #890 Stage 2 — particles never carry
                // BSEffectShaderProperty flag bits.
                effect_shader_flags: 0,
                // #890 Stage 2c — particles never carry the greyscale
                // palette LUT either; the bindless 0 slot signals
                // "no LUT" in the shader.
                greyscale_lut_index: 0,
                translucency_subsurface_color: [0.0; 3],
                translucency_transmissive_scale: 0.0,
                translucency_turbulence: 0.0,
                is_water: false,
            };
            // #781 / PERF-N4 — dedup material payload.
            cmd.material_id =
                material_table.intern_by_hash(cmd.material_hash(), || cmd.to_gpu_material());
            draw_commands.push(cmd);
        }
    }
}
