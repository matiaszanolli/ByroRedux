//! Particle billboard emission — extracted from `build_render_data` per #1115.
//!
//! Each live particle becomes one DrawCommand referencing the unit
//! particle quad mesh (handle passed in from the caller). Model matrix
//! is `translate(world_pos) · face_camera_rot · scale(size)`, so all
//! per-particle dynamics live in the model matrix.
//!
//! Batching depends on blend mode (#1649): **additive** emitters
//! (Gamebryo `dst_blend == ONE == 0` — the default, torch, ember and
//! magic-sparkle presets) are order-independent, so `draw_sort_key`
//! orders them mesh-before-depth and the instanced batch-merge from #272
//! collapses every same-mesh billboard into one indirect draw. **Alpha-
//! over** emitters (e.g. the smoke preset, `dst_blend == 7`) stay on the
//! depth-sorted per-particle path — their compositing order is visible,
//! so they cannot be reordered to batch.
//!
//! Color flows through `emissive_color * emissive_mult` — the
//! fragment shader's emissive add lights the quad with no scene-light
//! dependency. Particles default to additive blending
//! (src=SRC_ALPHA=6, dst=ONE=0) per ParticleEmitter defaults; per-emitter
//! overrides ride through the existing pipeline cache from #392.

use byroredux_core::ecs::{GlobalTransform, ParticleEmitter, RenderLayer, World};
use byroredux_core::math::{Mat4, Quat, Vec3, Vec4};
use byroredux_renderer::vulkan::context::DrawCommand;
use byroredux_renderer::MaterialTable;

use super::f32_sortable_u32;

/// Quantization step count for the particle color fade (#1795 / D2-NEW-02).
///
/// `material_hash` hashes `emissive_color`/`emissive_mult` at raw f32 bit
/// precision with no tolerance, so a continuous `age/life` fade produces a
/// distinct hash — and therefore a fresh `GpuMaterial` upload — for nearly
/// every live particle every frame, inverting the ~97% dedup-hit rate the
/// #781 fast path assumes. Snapping the fade parameter to 32 steps before
/// the color LERP collapses same-emitter particles onto ≤32 materials; the
/// banding is imperceptible on additive billboards.
const COLOR_FADE_STEPS: f32 = 32.0;

/// Snap a `0.0..=1.0` fade parameter to [`COLOR_FADE_STEPS`] discrete
/// steps, so distinct particles at nearly-identical ages collapse onto
/// the same quantized value and therefore the same `material_hash`.
/// #1795 / D2-NEW-02.
fn quantize_fade(t: f32) -> f32 {
    (t * COLOR_FADE_STEPS).round() / COLOR_FADE_STEPS
}

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
            // #1795 / D2-NEW-02 — quantize only the color's fade parameter
            // (not `t` itself, which still drives a smooth size LERP below)
            // so same-emitter particles collapse onto a handful of
            // `material_hash` values instead of one per particle per frame.
            let color_t = quantize_fade(t);
            let start_c = em.start_color;
            let end_c = em.end_color;
            let color = [
                start_c[0] + (end_c[0] - start_c[0]) * color_t,
                start_c[1] + (end_c[1] - start_c[1]) * color_t,
                start_c[2] + (end_c[2] - start_c[2]) * color_t,
                start_c[3] + (end_c[3] - start_c[3]) * color_t,
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

#[cfg(test)]
mod quantize_fade_tests {
    use super::{quantize_fade, COLOR_FADE_STEPS};

    #[test]
    fn snaps_nearby_values_to_the_same_step() {
        // Two ages one frame apart at ~60fps differ by a tiny fraction
        // of `t`; both must quantize to the same step so their
        // `material_hash` collides and dedup fires. #1795 / D2-NEW-02.
        let a = quantize_fade(0.500);
        let b = quantize_fade(0.503);
        assert_eq!(
            a, b,
            "nearby fade values within the same quantization bucket must \
             produce an identical result so material_hash collides"
        );
    }

    #[test]
    fn distinguishes_values_a_full_step_apart() {
        let step = 1.0 / COLOR_FADE_STEPS;
        let a = quantize_fade(0.0);
        let b = quantize_fade(step * 1.5);
        assert_ne!(
            a, b,
            "values more than half a step apart must land in different \
             buckets — the fade must still visibly progress"
        );
    }

    #[test]
    fn endpoints_are_stable() {
        assert_eq!(quantize_fade(0.0), 0.0);
        assert_eq!(quantize_fade(1.0), 1.0);
    }

    #[test]
    fn output_has_at_most_step_count_plus_one_distinct_values() {
        // Sweep the full input domain in fine increments and assert the
        // output only ever takes one of COLOR_FADE_STEPS + 1 values
        // (0..=COLOR_FADE_STEPS inclusive) — this is the actual dedup
        // guarantee the fix provides.
        let mut seen = std::collections::HashSet::new();
        let mut t = 0.0f32;
        while t <= 1.0 {
            seen.insert(quantize_fade(t).to_bits());
            t += 0.001;
        }
        assert!(
            seen.len() <= COLOR_FADE_STEPS as usize + 1,
            "expected at most {} distinct quantized values, got {}",
            COLOR_FADE_STEPS as usize + 1,
            seen.len()
        );
    }
}
