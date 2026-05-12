use super::*;
use byroredux_core::ecs::components::material::ShaderTypeFields;
use byroredux_core::ecs::{ActiveCamera, Camera, GlobalTransform, World};

fn run_build(world: &World) -> Vec<DrawCommand> {
    let mut draw_commands = Vec::new();
    let mut gpu_lights = Vec::new();
    let mut bone_palette = Vec::new();
    let mut skin_offsets = HashMap::new();
    let mut palette_scratch = Vec::new();
    let mut material_table = byroredux_renderer::MaterialTable::new();
    let mut water_commands = Vec::new();
    let _ = build_render_data(
        world,
        &mut draw_commands,
        &mut water_commands,
        &mut gpu_lights,
        &mut bone_palette,
        &mut skin_offsets,
        &mut palette_scratch,
        &mut material_table,
        None,
    );
    draw_commands
}

/// Build a world with a single renderable mesh whose Material
/// supplies `shader_type_fields` for every variant slot. The
/// `material_kind` argument controls which variant the renderer
/// dispatches; the test caller picks 0 (default) to prove the
/// gate skips the pack, or 5/6/14 to prove the gate lets the
/// matching variant through.
fn world_with_variant_material(material_kind: u32) -> World {
    let mut world = World::new();

    // Camera entity — ActiveCamera is required by build_render_data.
    let cam = world.spawn();
    world.insert(cam, Transform::IDENTITY);
    world.insert(cam, GlobalTransform::IDENTITY);
    world.insert(cam, Camera::default());
    world.insert_resource(ActiveCamera(cam));

    // Renderable mesh — Material populates every variant field
    // with a non-default value so a passing pack would surface
    // those values on the DrawCommand.
    let mesh_e = world.spawn();
    world.insert(mesh_e, Transform::IDENTITY);
    world.insert(mesh_e, GlobalTransform::IDENTITY);
    world.insert(mesh_e, MeshHandle(1));
    world.insert(mesh_e, TextureHandle(1));
    world.insert(
        mesh_e,
        Material {
            material_kind,
            shader_type_fields: Some(Box::new(ShaderTypeFields {
                skin_tint_color: Some([0.9, 0.8, 0.7]),
                skin_tint_alpha: Some(0.5),
                hair_tint_color: Some([0.1, 0.2, 0.3]),
                eye_cubemap_scale: Some(0.42),
                eye_left_reflection_center: Some([1.0, 2.0, 3.0]),
                eye_right_reflection_center: Some([4.0, 5.0, 6.0]),
                parallax_max_passes: None,
                parallax_height_scale: None,
                multi_layer_inner_thickness: Some(0.7),
                multi_layer_refraction_scale: Some(0.3),
                multi_layer_inner_layer_scale: Some([2.0, 3.0]),
                multi_layer_envmap_strength: Some(0.55),
                sparkle_parameters: Some([0.1, 0.2, 0.3, 0.4]),
            })),
            ..Material::default()
        },
    );

    world
}

#[test]
fn default_kind_zero_skips_all_variant_packs() {
    let world = world_with_variant_material(0);
    let cmds = run_build(&world);
    assert_eq!(cmds.len(), 1, "exactly one DrawCommand expected");
    let c = &cmds[0];
    assert_eq!(
        c.skin_tint_rgba, [0.0; 4],
        "kind=0 must skip skin tint pack"
    );
    assert_eq!(c.hair_tint_rgb, [0.0; 3], "kind=0 must skip hair tint pack");
    assert_eq!(c.sparkle_rgba, [0.0; 4], "kind=0 must skip sparkle pack");
    assert_eq!(c.multi_layer_envmap_strength, 0.0);
    assert_eq!(c.multi_layer_inner_thickness, 0.0);
    assert_eq!(c.multi_layer_refraction_scale, 0.0);
    assert_eq!(c.multi_layer_inner_scale, [1.0, 1.0]);
    assert_eq!(c.eye_left_center, [0.0; 3]);
    assert_eq!(c.eye_right_center, [0.0; 3]);
    assert_eq!(c.eye_cubemap_scale, 0.0);
}

#[test]
fn kind_5_skin_tint_packs_only_skin_fields() {
    let world = world_with_variant_material(5);
    let cmds = run_build(&world);
    let c = &cmds[0];
    // SkinTint group lands.
    assert_eq!(c.skin_tint_rgba, [0.9, 0.8, 0.7, 0.5]);
    // Other groups stay default-zero — gate worked.
    assert_eq!(c.hair_tint_rgb, [0.0; 3]);
    assert_eq!(c.sparkle_rgba, [0.0; 4]);
    assert_eq!(c.multi_layer_envmap_strength, 0.0);
    assert_eq!(c.eye_left_center, [0.0; 3]);
}

#[test]
fn kind_11_multilayer_parallax_packs_only_multilayer_fields() {
    // Stub variant — shader doesn't consume yet, but the pack
    // still runs so when the shader branch lands the data is
    // already there. See triangle.frag:797-813.
    let world = world_with_variant_material(11);
    let cmds = run_build(&world);
    let c = &cmds[0];
    assert_eq!(c.multi_layer_inner_thickness, 0.7);
    assert_eq!(c.multi_layer_refraction_scale, 0.3);
    assert_eq!(c.multi_layer_inner_scale, [2.0, 3.0]);
    assert_eq!(c.multi_layer_envmap_strength, 0.55);
    // Other groups stay default-zero.
    assert_eq!(c.skin_tint_rgba, [0.0; 4]);
    assert_eq!(c.hair_tint_rgb, [0.0; 3]);
    assert_eq!(c.sparkle_rgba, [0.0; 4]);
    assert_eq!(c.eye_left_center, [0.0; 3]);
}

#[test]
fn kind_16_eye_envmap_packs_only_eye_fields() {
    let world = world_with_variant_material(16);
    let cmds = run_build(&world);
    let c = &cmds[0];
    assert_eq!(c.eye_left_center, [1.0, 2.0, 3.0]);
    assert_eq!(c.eye_right_center, [4.0, 5.0, 6.0]);
    assert_eq!(c.eye_cubemap_scale, 0.42);
    // Other groups stay default-zero.
    assert_eq!(c.skin_tint_rgba, [0.0; 4]);
    assert_eq!(c.hair_tint_rgb, [0.0; 3]);
    assert_eq!(c.multi_layer_envmap_strength, 0.0);
}

/// Regression for #620 / SK-D4-01. Material with
/// `effect_falloff = Some(...)` and `material_kind = 101`
/// (`MATERIAL_KIND_EFFECT_SHADER`) must surface the falloff cone
/// on the resulting `DrawCommand.effect_falloff`. Identity defaults
/// stay in place when `material_kind != 101` even if the Material
/// carries `effect_falloff` (the gate is on `material_kind`, not
/// on the option).
#[test]
fn effect_shader_kind_packs_falloff_cone() {
    use byroredux_core::ecs::components::material::EffectFalloff;
    let mut world = World::new();
    let cam = world.spawn();
    world.insert(cam, Transform::IDENTITY);
    world.insert(cam, GlobalTransform::IDENTITY);
    world.insert(cam, Camera::default());
    world.insert_resource(ActiveCamera(cam));

    let mesh_e = world.spawn();
    world.insert(mesh_e, Transform::IDENTITY);
    world.insert(mesh_e, GlobalTransform::IDENTITY);
    world.insert(mesh_e, MeshHandle(1));
    world.insert(mesh_e, TextureHandle(1));
    world.insert(
        mesh_e,
        Material {
            // `MATERIAL_KIND_EFFECT_SHADER` (101) — engine-synthesized
            // upstream of the renderer, see render.rs:603.
            material_kind: 101,
            effect_falloff: Some(EffectFalloff {
                start_angle: 0.95,
                stop_angle: 0.30,
                start_opacity: 1.0,
                stop_opacity: 0.0,
                soft_falloff_depth: 8.0,
            }),
            ..Material::default()
        },
    );

    let cmds = run_build(&world);
    assert_eq!(cmds.len(), 1);
    let c = &cmds[0];
    assert_eq!(
        c.effect_falloff,
        [0.95, 0.30, 1.0, 0.0, 8.0],
        "effect-shader DrawCommand must carry the captured cone"
    );
}

/// Companion: when `material_kind != 101` the Material's
/// `effect_falloff` is ignored and the DrawCommand emits the
/// identity-pass-through tuple. Pre-fix the gate was missing — a
/// non-effect mesh authored with stale `effect_falloff` would have
/// faded incorrectly.
#[test]
fn non_effect_kind_emits_identity_falloff_even_when_material_has_it() {
    use byroredux_core::ecs::components::material::EffectFalloff;
    let mut world = World::new();
    let cam = world.spawn();
    world.insert(cam, Transform::IDENTITY);
    world.insert(cam, GlobalTransform::IDENTITY);
    world.insert(cam, Camera::default());
    world.insert_resource(ActiveCamera(cam));

    let mesh_e = world.spawn();
    world.insert(mesh_e, Transform::IDENTITY);
    world.insert(mesh_e, GlobalTransform::IDENTITY);
    world.insert(mesh_e, MeshHandle(1));
    world.insert(mesh_e, TextureHandle(1));
    world.insert(
        mesh_e,
        Material {
            material_kind: 0, // default lit, NOT EffectShader
            effect_falloff: Some(EffectFalloff {
                start_angle: 0.5,
                stop_angle: 0.1,
                start_opacity: 1.0,
                stop_opacity: 0.0,
                soft_falloff_depth: 4.0,
            }),
            ..Material::default()
        },
    );

    let cmds = run_build(&world);
    assert_eq!(cmds.len(), 1);
    assert_eq!(
        cmds[0].effect_falloff,
        [1.0, 1.0, 1.0, 1.0, 0.0],
        "non-effect kind must emit identity-pass-through falloff \
         regardless of Material.effect_falloff content"
    );
}
