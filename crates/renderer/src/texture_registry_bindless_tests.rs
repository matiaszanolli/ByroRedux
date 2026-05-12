//! Regression tests pinning the bindless descriptor reflection
//! contract — the count of bindless slots must match the shader-side
//! `MAX_BINDLESS_TEXTURES` constant.

//! Regression tests for #950 / SAFE-25: the bindless texture
//! descriptor set layout (set=0, binding=0) must agree with the
//! shaders that sample it (`triangle.frag` + `ui.frag`).
//!
//! Production `TextureRegistry::new` calls `validate_set_layout`
//! before `vkCreateDescriptorSetLayout`, but that runtime check
//! only fires when a real Vulkan device exists. These tests pull
//! the binding through the same `build_bindless_descriptor_binding`
//! helper production uses and validate it against the
//! include_bytes!'d SPIR-V at `cargo test` time, so a future shader
//! refactor that drops or renames `textures[]` trips before the
//! first frame ever runs.
use super::*;

fn frag_shaders() -> [crate::vulkan::reflect::ReflectedShader<'static>; 2] {
    [
        crate::vulkan::reflect::ReflectedShader {
            name: "triangle.frag",
            spirv: crate::vulkan::pipeline::TRIANGLE_FRAG_SPV,
        },
        crate::vulkan::reflect::ReflectedShader {
            name: "ui.frag",
            spirv: crate::vulkan::pipeline::UI_FRAG_SPV,
        },
    ]
}

/// The bindless texture array binding must reflect cleanly against
/// both raster fragment shaders that read from it. `descriptor_count`
/// is the device-driven `max_textures` value at runtime; both
/// shaders declare the array as `OpTypeRuntimeArray`, so the
/// reflector returns `count == 0` (variable) which is compatible
/// with any layout count. 1024 is a representative value drawn
/// from the actual clamp ceiling.
#[test]
fn bindless_binding_matches_triangle_ui_frag() {
    let binding = build_bindless_descriptor_binding(1024);
    crate::vulkan::reflect::validate_set_layout(
        0,
        std::slice::from_ref(&binding),
        &frag_shaders(),
        "bindless textures (set=0)",
        &[],
    )
    .expect("bindless texture layout must match triangle.frag + ui.frag");
}

/// Synthetic drift: swapping the descriptor type away from
/// COMBINED_IMAGE_SAMPLER (the only type that pairs a sampler with
/// a sampled image in one descriptor) must trip a diagnostic. Pin
/// the rejection path so a Rust-side typo that picks SAMPLED_IMAGE
/// or STORAGE_IMAGE doesn't pass silently.
#[test]
fn wrong_descriptor_type_trips_diagnostic() {
    let binding = build_bindless_descriptor_binding(1024)
        .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE);
    let err = crate::vulkan::reflect::validate_set_layout(
        0,
        std::slice::from_ref(&binding),
        &frag_shaders(),
        "bindless textures (set=0, drift)",
        &[],
    )
    .expect_err("SAMPLED_IMAGE vs COMBINED_IMAGE_SAMPLER must fail");
    let msg = format!("{err}");
    assert!(
        msg.contains("binding=0"),
        "diagnostic must name binding 0: {msg}",
    );
}
