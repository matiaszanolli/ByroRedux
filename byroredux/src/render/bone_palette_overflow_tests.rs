use super::*;
use byroredux_core::ecs::{GlobalTransform, SkinnedMesh, World};
use byroredux_core::math::Mat4;

fn make_skinned_world(num_meshes: usize) -> World {
    let mut world = World::new();
    for _ in 0..num_meshes {
        // Each mesh declares MAX_BONES_PER_MESH bones with self-
        // EntityId pointers. The palette closure looks up
        // GlobalTransform on each bone — we don't insert any, so
        // every bone falls back to identity (the test only cares
        // about overflow accounting, not the matrix values).
        let mesh_entity = world.spawn();
        world.insert(mesh_entity, GlobalTransform::IDENTITY);
        let bones = vec![Some(mesh_entity); MAX_BONES_PER_MESH];
        let binds = vec![Mat4::IDENTITY; MAX_BONES_PER_MESH];
        world.insert(mesh_entity, SkinnedMesh::new(None, bones, binds));
    }
    world
}

fn run_build(world: &World) -> (Vec<[[f32; 4]; 4]>, HashMap<EntityId, u32>) {
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
    (bone_palette, skin_offsets)
}

#[test]
fn at_capacity_fills_palette_completely() {
    // `MAX_SKINNED = MAX_TOTAL_BONES / MAX_BONES_PER_MESH`. At
    // post-#900 sizing that's 32768 / 128 = 256 meshes. The
    // overflow check fires only when adding the NEXT mesh would
    // exceed `MAX_TOTAL_BONES`; `MAX_SKINNED - 1` meshes plus the
    // 1 identity slot at index 0 fit exactly at the boundary, so
    // the palette completes without truncation. Document the
    // exact off-by-one. (Pre-#900 this comment hardcoded the old
    // 32-mesh / 4096-bone ceiling — REN-D12-NEW-02.)
    let max_skinned = MAX_TOTAL_BONES / MAX_BONES_PER_MESH;
    let world = make_skinned_world(max_skinned - 1);
    let (palette, offsets) = run_build(&world);
    assert_eq!(
        offsets.len(),
        max_skinned - 1,
        "all {} meshes must register a bone offset",
        max_skinned - 1
    );
    // 1 identity slot + (max_skinned - 1) × MAX_BONES_PER_MESH
    let expected_slots = 1 + (max_skinned - 1) * MAX_BONES_PER_MESH;
    assert_eq!(palette.len(), expected_slots);
}

#[test]
fn over_capacity_breaks_loop_and_truncates_offsets() {
    // `MAX_SKINNED + 1` meshes × `MAX_BONES_PER_MESH` bones
    // requests one mesh past the bone-palette ceiling. The guard
    // at the top of the loop trips before the offending mesh
    // gets its offset registered, so `skin_offsets` holds
    // strictly fewer entries than were requested and the
    // palette stays at or below `MAX_TOTAL_BONES`. (Pre-#900
    // this comment hardcoded the old 32-mesh / 4096-bone
    // ceiling — REN-D12-NEW-02.)
    let max_skinned = MAX_TOTAL_BONES / MAX_BONES_PER_MESH;
    let world = make_skinned_world(max_skinned + 1);
    let (palette, offsets) = run_build(&world);
    assert!(
        offsets.len() < max_skinned + 1,
        "overflow guard must drop at least one mesh; got {} offsets for {} meshes",
        offsets.len(),
        max_skinned + 1
    );
    assert!(
        palette.len() <= MAX_TOTAL_BONES,
        "palette must never exceed MAX_TOTAL_BONES={}; got {}",
        MAX_TOTAL_BONES,
        palette.len()
    );
}
