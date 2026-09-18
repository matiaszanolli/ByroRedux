//! #4423 (REN-2026-09-16-D7-02) — `TINT_ALPHA_WEIGHT_BIT` must only be set
//! when the bound tint texture actually carries an alpha channel.
//!
//! The tint role's only vanilla producer is the Skin Tint shader's
//! `*_sk.dds` map (Beyond Skyrim, *NetImmerse Format/Texture Slots*: slot 2
//! under the Skin Tint type "simulates the diffusion of light inside a
//! translucent medium" — a subsurface input, not an albedo multiplier). The
//! audit's full BC1 decode found all nine vanilla `_sk` paths are DXT1 with
//! zero alpha-0 texels, so a bare `tintSample.a` reads the format default
//! 1.0 and the weighted `mix(albedo, albedo * tint.rgb, tint.a)` multiply
//! ran at full force — Skyrim head/body/hand skin collapsed to between 1/5
//! and 1/200 of its diffuse per channel. The bit is the packer's statement
//! that the weight is authored data; an alpha-less tint stays inert until a
//! real subsurface consumer exists (M56).

use super::*;
use byroredux_core::ecs::{
    ActiveCamera, Camera, GlobalTransform, Material, MeshHandle, TextureHandle, World,
};

use crate::components::MaterialTextureHandles;

fn run_build(world: &World) -> Vec<DrawCommand> {
    let mut draw_commands = Vec::new();
    let mut gpu_lights = Vec::new();
    let mut bone_world = Vec::new();
    let mut skin_offsets = rustc_hash::FxHashMap::default();
    let max_skinned = ((byroredux_renderer::vulkan::scene_buffer::MAX_TOTAL_BONES
        / byroredux_core::ecs::components::MAX_BONES_PER_MESH)
        - 1) as u32;
    let mut skin_slot_pool = byroredux_core::ecs::resources::SkinSlotPool::new(max_skinned);
    let mut material_table = byroredux_renderer::MaterialTable::new();
    let mut water_commands = Vec::new();
    let _ = build_render_data(
        world,
        &mut draw_commands,
        &mut water_commands,
        &mut gpu_lights,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut bone_world,
        &mut skin_offsets,
        &mut skin_slot_pool,
        &mut material_table,
        None,
    );
    draw_commands
}

/// One renderable mesh with the tint role bound (the Skyrim Skin Tint
/// shape): tint handle 9, `tint_has_alpha` reporting what the DDS format
/// actually provides.
fn world_with_tint_material(tint_has_alpha: bool) -> World {
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
    world.insert(mesh_e, Material::default());
    let mut textures = byroredux_nif::import::MaterialTextureSet::<u32>::default();
    textures.tint = 9;
    world.insert(
        mesh_e,
        MaterialTextureHandles {
            textures,
            normal_has_alpha: false,
            tint_has_alpha,
            parallax_height_scale: 0.04,
            parallax_max_passes: 4.0,
        },
    );

    world
}

fn tint_index(world: &World) -> u32 {
    let cmds = run_build(world);
    let cmd = cmds
        .iter()
        .find(|cmd| cmd.supplemental_texture_indices
            [byroredux_renderer::vulkan::material::supplemental_texture_slot::TINT]
            != 0)
        .expect("the mesh must emit a draw with a bound tint slot");
    cmd.supplemental_texture_indices
        [byroredux_renderer::vulkan::material::supplemental_texture_slot::TINT]
}

/// The vanilla shape: a BC1 `_sk` map has no alpha channel, so the packer
/// must NOT claim the sample weight is authored data. The index stays bound
/// (diagnostics still see the slot) but the shader's bit gate keeps the
/// multiply off — the pre-`1d94eb246` behavior that was never reported
/// broken, not a fabricated 1/50 albedo.
#[test]
fn alpha_less_tint_does_not_claim_an_authored_alpha_weight() {
    let index = tint_index(&world_with_tint_material(false));
    assert_eq!(index & !crate::material_translate::TINT_ALPHA_WEIGHT_BIT, 9);
    assert_eq!(
        index & crate::material_translate::TINT_ALPHA_WEIGHT_BIT,
        0,
        "a BC1/BC4/BC5 tint has no usable `.a` weight anywhere — leaving the \
         bit set makes the shader multiply the subsurface input into albedo \
         at full force (#4423)"
    );
}

/// And the route must still work where it is legitimate — an alpha-bearing
/// tint (BC3/BC7, or a future consumer that ships one) keeps its authored
/// weight.
#[test]
fn tint_with_alpha_still_sets_the_alpha_weight_bit() {
    let index = tint_index(&world_with_tint_material(true));
    assert_ne!(
        index & crate::material_translate::TINT_ALPHA_WEIGHT_BIT,
        0,
        "an authored-alpha tint must still take the weighted-mix path"
    );
    assert_eq!(
        index & !crate::material_translate::TINT_ALPHA_WEIGHT_BIT,
        9,
        "the bit must ride alongside the index, not replace it"
    );
}
