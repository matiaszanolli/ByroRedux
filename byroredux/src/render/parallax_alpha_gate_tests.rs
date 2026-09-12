//! #3562 — `PARALLAX_ALPHA_HEIGHT_BIT` must only be set when the bound
//! normal texture actually carries an alpha channel.
//!
//! #3530's `APPLY_HILIGHT2` route binds the NORMAL map into the height slot
//! and reads the height out of its alpha, because Oblivion ships no separate
//! height texture. It reused `NORMAL_ALPHA_SPEC_BIT`'s pattern "verbatim"
//! except for the one thing that made that pattern safe: the
//! `normal_has_alpha` gate.
//!
//! Without it, a BC1/BC4/BC5 normal (for which `dds::format_has_alpha` is
//! false and the sampler returns `A = 1.0` by format) yields a constant
//! height of 1.0. `parallaxDisplaceUV`'s `currentDepth >= sampledHeight`
//! guard then never fires, the marcher runs every step, and it returns
//! `uv - planarSlide` — the FULL slide at every fragment, view-dependent per
//! frame. `sampleUV` feeds every later fetch, so the whole material swims.
//! Mixed-block BC1 is worse still: 3-colour blocks decode `A = 0` and
//! 4-colour blocks `A = 1`, tearing the surface along block boundaries.
//!
//! The exposed population on vanilla Oblivion is empty today (0 of 1,430
//! `APPLY_HILIGHT2` properties carry a normal slot), so this is a
//! correctness/robustness pin, not a visual-bug repro.
//!
//! #4260 (OB-D4-01) — withholding only `PARALLAX_ALPHA_HEIGHT_BIT` above
//! turned out to be half the fix: `parallax_map_index` itself stayed bound
//! to the normal map's own handle, so POM still ran and sampled the
//! normal's red channel as a fabricated height field. Measured on the real
//! Oblivion + Shivering Isles corpus at 100 of 1,274 `APPLY_HILIGHT2`
//! properties (7.8%) with a BC1/DXT1 normal sibling — nonzero unlike the
//! synthetic 0/1,430 figure above, which only covered normal-slot presence,
//! not the alpha-format split. The slot must now be zeroed entirely in this
//! case, not just left flag-less.

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

/// One renderable mesh on the `APPLY_HILIGHT2` route: a height slot bound to
/// the normal texture, `parallax_height_in_alpha` set by the importer, and
/// `normal_has_alpha` reporting what the DDS format actually provides.
fn world_with_alpha_height_material(normal_has_alpha: bool) -> World {
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
            parallax_height_in_alpha: true,
            ..Material::default()
        },
    );
    let mut textures = byroredux_nif::import::MaterialTextureSet::<u32>::default();
    textures.normal = 7;
    // The `APPLY_HILIGHT2` route binds the normal map into the height slot.
    textures.height = 7;
    world.insert(
        mesh_e,
        MaterialTextureHandles {
            textures,
            normal_has_alpha,
            parallax_height_scale: 0.04,
            parallax_max_passes: 4.0,
        },
    );

    world
}

fn parallax_index(world: &World) -> u32 {
    let cmds = run_build(world);
    let cmd = cmds
        .iter()
        .find(|cmd| cmd.parallax_map_index != 0)
        .expect("the mesh must emit a draw with a bound height slot");
    cmd.parallax_map_index
}

/// #4260 (OB-D4-01) — an alpha-less normal must not just withhold the flag
/// bit; `parallax_map_index` itself must be zeroed. Before this fix the
/// slot stayed bound to the normal map's own handle (7, same as
/// `textures.normal` in the fixture below) even with the bit correctly
/// masked off, so POM still ran and sampled the normal's red channel as a
/// fabricated height field — the exact defect class #3562's bit-only gate
/// was meant to close but didn't finish closing.
#[test]
fn alpha_less_normal_zeroes_the_parallax_slot_entirely() {
    let cmds = run_build(&world_with_alpha_height_material(false));
    assert_eq!(cmds.len(), 1, "fixture spawns exactly one mesh");
    assert_eq!(
        cmds[0].parallax_map_index, 0,
        "a BC1/BC4/BC5 normal has no real height data anywhere to sample — \
         POM must not run at all, not merely skip the alpha-height flag bit \
         while staying bound to the normal map's handle (#4260)"
    );
}

/// And the route must still work where it is legitimate — a DXT3/DXT5 normal
/// with real alpha is exactly what #3530 exists to serve.
#[test]
fn normal_with_alpha_still_sets_the_alpha_height_bit() {
    let index = parallax_index(&world_with_alpha_height_material(true));
    assert_ne!(
        index & crate::material_translate::PARALLAX_ALPHA_HEIGHT_BIT,
        0,
        "an authored-alpha normal must still take the #3530 height path"
    );
    assert_eq!(
        index & !crate::material_translate::PARALLAX_ALPHA_HEIGHT_BIT,
        7,
        "the bit must ride alongside the index, not replace it"
    );
}

/// #3567 — the end-to-end half of the same channel question. This fixture is
/// exactly the `APPLY_HILIGHT2` shape: one texture bound into both the normal
/// and the height slot, `parallax_height_in_alpha` set, alpha present. Every
/// precondition of the normal-alpha-as-spec binding is therefore satisfied by
/// construction, and before the fix the draw carried BOTH bits — the shader
/// read that one texel as displacement in `sampleParallaxHeight` and as a
/// specular-intensity mask in `triangle.frag`, so specular tracked the height
/// field.
///
/// Pinned on the emitted `DrawCommand` rather than only on the predicate,
/// because the two claims are made fifty lines apart in the same draw loop:
/// the predicate test alone would not catch a future site re-deriving the
/// binding locally.
#[test]
fn a_height_bearing_normal_alpha_is_not_also_bound_as_the_spec_mask() {
    let cmds = run_build(&world_with_alpha_height_material(true));
    let cmd = cmds
        .iter()
        .find(|cmd| cmd.parallax_map_index != 0)
        .expect("the mesh must emit a draw with a bound height slot");
    assert_ne!(
        cmd.parallax_map_index & crate::material_translate::PARALLAX_ALPHA_HEIGHT_BIT,
        0,
        "precondition: this draw must be on the #3530 alpha-height route"
    );
    assert_eq!(
        cmd.gloss_map_index & crate::material_translate::NORMAL_ALPHA_SPEC_BIT,
        0,
        "the normal alpha is already claimed as parallax height; binding it \
         as the per-pixel specular mask too makes specular strength track \
         displacement — crevices matte, raised brickwork glossy (#3567)"
    );
}
