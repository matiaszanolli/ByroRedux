//! Regression tests pinning the bindless descriptor reflection
//! contract — the count of bindless slots must match the shader-side
//! `MAX_BINDLESS_TEXTURES` constant.

//! Regression tests for #950 / SAFE-25: the bindless texture
//! descriptor set layout (set=0, 2D binding 0 + cube binding 1) must agree with the
//! shaders that sample it (`triangle.frag` + `ui.frag`).
//!
//! Production `TextureRegistry::new` calls `validate_set_layout`
//! before `vkCreateDescriptorSetLayout`, but that runtime check
//! only fires when a real Vulkan device exists. These tests pull
//! the bindings through the same `build_bindless_descriptor_bindings`
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
    let bindings = build_bindless_descriptor_bindings(1024);
    crate::vulkan::reflect::validate_set_layout(
        0,
        &bindings,
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
    let mut bindings = build_bindless_descriptor_bindings(1024);
    bindings[0] = bindings[0].descriptor_type(vk::DescriptorType::SAMPLED_IMAGE);
    let err = crate::vulkan::reflect::validate_set_layout(
        0,
        &bindings,
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

/// M39 — the first-level table (binding 2) is read by every shader that binds
/// this set at set 0 and samples through `residentLod`: the raster, water and
/// ground-cover fragment shaders. Each must declare it as the storage buffer
/// the layout holds, or `residentLod` reads an unbound binding.
#[test]
fn first_level_table_matches_every_set0_bindless_consumer() {
    let consumers = [
        crate::vulkan::reflect::ReflectedShader {
            name: "triangle.frag",
            spirv: crate::vulkan::pipeline::TRIANGLE_FRAG_SPV,
        },
        crate::vulkan::reflect::ReflectedShader {
            name: "water.frag",
            spirv: crate::vulkan::water::WATER_FRAG_SPV,
        },
        crate::vulkan::reflect::ReflectedShader {
            name: "groundcover_blade.frag",
            spirv: include_bytes!("../shaders/groundcover_blade.frag.spv"),
        },
    ];
    // Only binding 2 is checked here: `bindless_binding_matches_triangle_ui_frag`
    // validates the whole set against triangle/ui, and water.frag reflects
    // `cubemaps[]` with the fixed size glslang gives a constant-indexed
    // unsized array, which Vulkan accepts but the strict count check does not.
    for shader in &consumers {
        let reflected = crate::vulkan::reflect::reflect_bindings(shader.spirv).unwrap();
        assert!(
            reflected.iter().any(|b| b.set == 0
                && b.binding == FIRST_LEVEL_BINDING
                && b.descriptor_type == vk::DescriptorType::STORAGE_BUFFER
                && b.count <= 1),
            "{} must declare the first-level table at set 0 binding {FIRST_LEVEL_BINDING}",
            shader.name,
        );
    }
}

/// The binding flags and the pool sizes are hand-written beside the bindings;
/// one entry missing or a count short is a create-time VUID that `cargo test`
/// cannot otherwise see. The first-level table must not be
/// `UPDATE_AFTER_BIND`: the device enables that only for sampled images
/// (`device.rs`), and the table's descriptor is written before its set is
/// first bound.
#[test]
fn bindless_flags_and_pool_sizes_cover_every_binding() {
    let max_textures = 1024;
    let bindings = build_bindless_descriptor_bindings(max_textures);
    assert_eq!(BINDLESS_BINDING_FLAGS.len(), bindings.len());
    for (binding, flags) in bindings.iter().zip(BINDLESS_BINDING_FLAGS) {
        let uab = flags.contains(vk::DescriptorBindingFlags::UPDATE_AFTER_BIND);
        assert_eq!(
            uab,
            binding.descriptor_type == vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            "binding {}: only the sampler arrays may be UPDATE_AFTER_BIND",
            binding.binding,
        );
    }
    let pool = bindless_pool_sizes(max_textures);
    for ty in [
        vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        vk::DescriptorType::STORAGE_BUFFER,
    ] {
        let needed: u32 = bindings
            .iter()
            .filter(|b| b.descriptor_type == ty)
            .map(|b| b.descriptor_count * MAX_FRAMES_IN_FLIGHT as u32)
            .sum();
        let sized: u32 = pool
            .iter()
            .filter(|p| p.ty == ty)
            .map(|p| p.descriptor_count)
            .sum();
        assert_eq!(
            sized, needed,
            "{ty:?}: pool must hold exactly every set's descriptors"
        );
    }
    assert_eq!(
        bindings[FIRST_LEVEL_BINDING as usize].descriptor_type,
        vk::DescriptorType::STORAGE_BUFFER
    );
}
