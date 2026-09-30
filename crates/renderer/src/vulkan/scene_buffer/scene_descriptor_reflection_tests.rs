//! Scene-descriptor-layout reflection tests.
//!
//! Compares the runtime descriptor-set layout against shader binding
//! declarations parsed from `triangle.vert/frag` + `ui.vert` SPIR-V
//! reflection.

use super::buffers::build_scene_descriptor_bindings;

fn triangle_shaders() -> [super::super::reflect::ReflectedShader<'static>; 2] {
    [
        super::super::reflect::ReflectedShader {
            name: "triangle.vert",
            spirv: super::super::pipeline::TRIANGLE_VERT_SPV,
        },
        super::super::reflect::ReflectedShader {
            name: "triangle.frag",
            spirv: super::super::pipeline::TRIANGLE_FRAG_SPV,
        },
    ]
}

/// Every binding 0..=13 (with TLAS at 2) must be
/// declared in `triangle.vert` ∪ `triangle.frag` with the matching
/// descriptor type. No `optional_shader_bindings` — every declared
/// binding must be consumed by the layout.
#[test]
fn scene_layout_matches_triangle_shaders() {
    let bindings = build_scene_descriptor_bindings();
    super::super::reflect::validate_set_layout(
        1,
        &bindings,
        &triangle_shaders(),
        "scene (set=1)",
        &[],
    )
    .expect("scene descriptor layout must match triangle shaders");
}

#[test]
fn bone_palette_is_visible_to_primary_and_secondary_hit_shading() {
    let bindings = build_scene_descriptor_bindings();
    let palette = bindings
        .iter()
        .find(|binding| binding.binding == 3)
        .unwrap();
    assert_eq!(
        palette.descriptor_type,
        ash::vk::DescriptorType::STORAGE_BUFFER
    );
    assert!(
        palette
            .stage_flags
            .contains(ash::vk::ShaderStageFlags::VERTEX | ash::vk::ShaderStageFlags::FRAGMENT)
    );
}

/// All four shaders that consume the set=1 layout at draw time:
/// triangle.vert/frag (which cover every binding) plus water.vert/frag,
/// which reuse CameraUBO, InstanceBuffer, TLAS, MaterialBuffer, and global
/// geometry bindings for material-aware water-ray hit reconstruction.
/// Mirrors the exact shader set `create_scene_descriptors` runs through
/// `validate_set_layout` at startup (#1561).
fn scene_shaders_with_water() -> [super::super::reflect::ReflectedShader<'static>; 4] {
    let [tv, tf] = triangle_shaders();
    [
        tv,
        tf,
        super::super::reflect::ReflectedShader {
            name: "water.vert",
            spirv: super::super::water::WATER_VERT_SPV,
        },
        super::super::reflect::ReflectedShader {
            name: "water.frag",
            spirv: super::super::water::WATER_FRAG_SPV,
        },
    ]
}

/// #1561 — pin the water shaders against the set=1 layout in the
/// same union `create_scene_descriptors` validates, so a water-shader binding
/// drift (e.g. water.frag declaring TLAS as the wrong descriptor type) is
/// caught device-free.
#[test]
fn scene_layout_matches_water_shaders() {
    let bindings = build_scene_descriptor_bindings();
    super::super::reflect::validate_set_layout(
        1,
        &bindings,
        &scene_shaders_with_water(),
        "scene (set=1, water)",
        &[],
    )
    .expect("scene descriptor layout must match triangle + water shaders");
}

/// Synthetic drift: dropping binding 4 (instance SSBO) from the
/// layout must produce a descriptive failure. Pin the rejection
/// path so a future shader change that *removes* a binding without
/// also removing it from the production helper trips a clear
/// error rather than silently passing.
#[test]
fn dropping_instance_binding_fails_with_diagnostic() {
    let mut bindings = build_scene_descriptor_bindings();
    let before = bindings.len();
    bindings.retain(|b| b.binding != 4);
    assert_eq!(
        bindings.len(),
        before - 1,
        "fixture must actually drop binding 4",
    );
    // After removing binding 4 from the Rust side, the shader still
    // declares it — validate must flag the shader's extra binding
    // since it is not in `optional_shader_bindings`.
    let err = super::super::reflect::validate_set_layout(
        1,
        &bindings,
        &triangle_shaders(),
        "scene (set=1, drift)",
        &[],
    )
    .expect_err("dropping binding 4 must trip a layout drift error");
    let msg = format!("{err}");
    assert!(
        msg.contains("binding=4"),
        "diagnostic must name the offending binding (4): {msg}",
    );
}

/// #4298 — `shader-pipeline.md`'s Set-1 rows must list exactly the bindings
/// `build_scene_descriptor_bindings` declares, with the same descriptor
/// type. Binding 20 (the SKYAL cubemap) went undocumented for a release
/// because the existing doc pins (#4019) check only the "Used by" cells;
/// a `PARTIALLY_BOUND` binding missing from the table is exactly the one a
/// reader rebuilding the scene set would forget to write.
#[test]
fn shader_pipeline_doc_lists_every_scene_set_binding() {
    const DOC: &str = include_str!("../../../../../docs/engine/shader-pipeline.md");
    let documented: Vec<(u32, String)> = DOC
        .lines()
        .filter_map(|line| line.strip_prefix("| 1 | "))
        .filter_map(|rest| {
            let (binding, rest) = rest.split_once(" | ")?;
            let binding: u32 = binding.parse().ok()?;
            let ty = rest.split('`').nth(1)?.to_string();
            Some((binding, ty))
        })
        .collect();
    let declared: Vec<(u32, String)> = build_scene_descriptor_bindings()
        .iter()
        .map(|b| {
            let ty = format!("{:?}", b.descriptor_type);
            (b.binding, ty.trim_end_matches("_KHR").to_string())
        })
        .collect();
    assert_eq!(
        documented, declared,
        "shader-pipeline.md's Set-1 rows (left) no longer match the scene \
         descriptor layout (right) — add or correct the row in the same \
         change that touches `build_scene_descriptor_bindings` (#4298)"
    );
}
