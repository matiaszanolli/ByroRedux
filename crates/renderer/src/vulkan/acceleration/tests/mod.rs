//! Unit tests for the acceleration submodules.
//!
//! Lifted from the monolithic `acceleration.rs::tests` block, then split
//! per-topic under #2977. Every test exercises a pure predicate or a
//! bookkeeping stand-in — none needs a live Vulkan context.
//!
//! - [`predicates_tests`]  — TLAS eligibility, BUILD-vs-UPDATE, refit validation, budget/eviction, flag pins
//! - [`tlas_tests`]        — instance mapping, transforms, shrink/handle lifecycle, build bookkeeping
//! - [`scratch_tests`]     — scratch sizing, alignment, growth/shrink, serialize-barrier invariant
//! - [`blas_static_tests`] — static BLAS build, eviction, deferred destroy

mod blas_static_tests;
mod predicates_tests;
mod scratch_tests;
mod tlas_tests;

use super::*;
use crate::vulkan::context::DrawCommand;

/// Byte-size units shared by the scratch- and TLAS-sizing tests.
pub(super) const KB: vk::DeviceSize = 1024;
pub(super) const MB: vk::DeviceSize = 1024 * 1024;

/// Minimal `DrawCommand` builder for the TLAS-eligibility unit tests.
/// Only `in_tlas` and `is_water` are read by
/// [`draw_command_eligible_for_tlas`]; every other field gets a
/// zero/default value. Same pattern as the `cmd` builder in
/// `context::draw::is_caustic_source_tests`.
pub(super) fn make_draw_command(in_tlas: bool, is_water: bool) -> DrawCommand {
    DrawCommand {
        mesh_handle: 0,
        texture_handle: 0,
        model_matrix: [0.0; 16],
        alpha_blend: false,
        src_blend: 6,
        dst_blend: 7,
        two_sided: false,
        wireframe: false,
        flat_shading: false,
        is_decal: false,
        render_layer: byroredux_core::ecs::components::RenderLayer::Architecture,
        bone_offset: 0,
        normal_map_index: 0,
        dark_map_index: 0,
        glow_map_index: 0,
        detail_map_index: 0,
        gloss_map_index: 0,
        parallax_map_index: 0,
        parallax_height_scale: 0.0,
        parallax_max_passes: 0.0,
        env_map_index: 0,
        env_mask_index: 0,
        alpha_threshold: 0.0,
        alpha_test_func: 0,
        roughness: 0.5,
        metalness: 0.0,
        ior: 1.5,        // #1248
        glass_fresnel_color: [1.0; 3],
        glass_refraction_scale: 0.05,
        glass_blur_scale: 0.4,
        glass_blur_scale_factor: 1.0,
        lighting_effect_1: 0.0,
        lighting_effect_2: 0.0,
        subsurface_rolloff: 0.0,
        rimlight_power: 0.0,
        backlight_power: 0.0,
        fresnel_power: 5.0,
        grayscale_to_palette_scale: 1.0,
        subsurface: 0.0, // #1249
        sheen: 0.0,
        sheen_tint: 0.0,
        anisotropic: 0.0, // #1250
        emissive_mult: 0.0,
        emissive_color: [0.0; 3],
        specular_strength: 0.0,
        specular_color: [0.0; 3],
        diffuse_color: [1.0; 3],
        ambient_color: [1.0; 3],
        vertex_offset: 0,
        index_offset: 0,
        vertex_count: 0,
        sort_depth: 0,
        in_tlas,
        in_raster: true,
        avg_albedo: [0.0; 3],
        material_kind: 0,
        z_test: true,
        z_write: true,
        z_function: 3,
        terrain_tile_index: None,
        entity_id: 0,
        uv_offset: [0.0; 2],
        uv_scale: [1.0; 2],
        material_alpha: 1.0,
        skin_tint_rgba: [0.0; 4],
        hair_tint_rgb: [0.0; 3],
        multi_layer_envmap_strength: 0.0,
        eye_left_center: [0.0; 3],
        eye_cubemap_scale: 0.0,
        eye_right_center: [0.0; 3],
        multi_layer_inner_thickness: 0.0,
        multi_layer_refraction_scale: 0.0,
        multi_layer_inner_scale: [0.0; 2],
        sparkle_rgba: [0.0; 4],
        effect_falloff: [0.0; 5],
        material_id: 0,
        vertex_color_emissive: false,
        effect_shader_flags: 0,
        greyscale_lut_index: 0,
        supplemental_texture_indices: [0; 16],
        translucency_subsurface_color: [0.0; 3],
        translucency_transmissive_scale: 0.0,
        translucency_turbulence: 0.0,
        shader_color: [0.0; 3],
        shader_float: 0.0,
        is_water,
    }
}
