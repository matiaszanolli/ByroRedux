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

/// RT-enabled path: every binding 0..=13 (with TLAS at 2) must be
/// declared in `triangle.vert` ∪ `triangle.frag` with the matching
/// descriptor type. No `optional_shader_bindings` — every declared
/// binding must be consumed by the layout.
#[test]
fn rt_enabled_layout_matches_triangle_shaders() {
    let bindings = build_scene_descriptor_bindings(true);
    super::super::reflect::validate_set_layout(
        1,
        &bindings,
        &triangle_shaders(),
        "scene (set=1, rt=on)",
        &[],
    )
    .expect("scene descriptor layout (rt=on) must match triangle shaders");
}

/// RT-disabled path: TLAS binding (2) is intentionally absent from
/// the layout but still declared in the shader, gated at runtime by
/// the per-fragment `rayQuery` uniform flag. The validator must list
/// it in `optional_shader_bindings` so the shader-declared-but-
/// layout-absent case doesn't fire a false positive.
#[test]
fn rt_disabled_layout_matches_triangle_shaders_with_optional_tlas() {
    let bindings = build_scene_descriptor_bindings(false);
    // TLAS (binding 2) is shader-declared but absent from the
    // RT-disabled layout — and must not be in the bindings vec.
    assert!(
        !bindings.iter().any(|b| b.binding == 2),
        "rt_enabled=false must omit binding 2 (TLAS)",
    );
    super::super::reflect::validate_set_layout(
        1,
        &bindings,
        &triangle_shaders(),
        "scene (set=1, rt=off)",
        &[2],
    )
    .expect("scene descriptor layout (rt=off) must match triangle shaders");
}

/// All four shaders that consume the set=1 layout at draw time:
/// triangle.vert/frag (which cover every binding) plus water.vert/frag,
/// which reuse a subset (CameraUBO binding 1, InstanceBuffer binding 4, TLAS
/// binding 2). Mirrors the exact shader set `create_scene_descriptors` runs
/// through `validate_set_layout` at startup (#1561).
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

/// #1561 — pin the water shaders against the RT-enabled set=1 layout in the
/// same union `create_scene_descriptors` validates, so a water-shader binding
/// drift (e.g. water.frag declaring TLAS as the wrong descriptor type) is
/// caught device-free.
#[test]
fn rt_enabled_layout_matches_water_shaders() {
    let bindings = build_scene_descriptor_bindings(true);
    super::super::reflect::validate_set_layout(
        1,
        &bindings,
        &scene_shaders_with_water(),
        "scene (set=1, rt=on, water)",
        &[],
    )
    .expect("scene descriptor layout (rt=on) must match triangle + water shaders");
}

/// RT-disabled path: water.frag still statically declares TLAS (binding 2)
/// even though water is never created on a non-RT device. The validator must
/// treat binding 2 as optional, mirroring the triangle.frag case — pins the
/// `optional_shader_bindings=[2]` contract for the water-inclusive set.
#[test]
fn rt_disabled_layout_matches_water_shaders_with_optional_tlas() {
    let bindings = build_scene_descriptor_bindings(false);
    super::super::reflect::validate_set_layout(
        1,
        &bindings,
        &scene_shaders_with_water(),
        "scene (set=1, rt=off, water)",
        &[2],
    )
    .expect("scene descriptor layout (rt=off) must match triangle + water shaders with optional TLAS");
}

/// Synthetic drift: dropping binding 4 (instance SSBO) from the
/// layout must produce a descriptive failure. Pin the rejection
/// path so a future shader change that *removes* a binding without
/// also removing it from the production helper trips a clear
/// error rather than silently passing.
#[test]
fn dropping_instance_binding_fails_with_diagnostic() {
    let mut bindings = build_scene_descriptor_bindings(true);
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
        "scene (set=1, rt=on, drift)",
        &[],
    )
    .expect_err("dropping binding 4 must trip a layout drift error");
    let msg = format!("{err}");
    assert!(
        msg.contains("binding=4"),
        "diagnostic must name the offending binding (4): {msg}",
    );
}
