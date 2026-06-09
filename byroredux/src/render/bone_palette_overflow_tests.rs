use super::*;
use byroredux_core::ecs::{GlobalTransform, SkinnedMesh, World, MAX_BONES_PER_MESH};
use byroredux_core::math::Mat4;
use byroredux_renderer::vulkan::scene_buffer::MAX_TOTAL_BONES;

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
        world.insert(
            mesh_entity,
            SkinnedMesh::new_with_global(None, bones, binds, Mat4::IDENTITY),
        );
    }
    world
}

fn run_build(world: &World) -> (Vec<[[f32; 4]; 4]>, HashMap<EntityId, u32>) {
    let mut draw_commands = Vec::new();
    let mut gpu_lights = Vec::new();
    // M29.6 — pre-multiplied palette → sparse bone_world (slot-indexed)
    // + persistent bind_inverses SSBO (GPU-only). The overflow guard
    // now lives on the SkinSlotPool: when capacity is exhausted,
    // allocate() returns None and the entity is dropped. We assert
    // bone_world.len() reaches the resized ceiling = max_used_slot ×
    // MBPM, which is byte-equivalent to the pre-M29.6 dense-pack
    // length at full saturation.
    let mut bone_world = Vec::new();
    let mut skin_offsets = HashMap::new();
    let mut material_table = byroredux_renderer::MaterialTable::new();
    let mut water_commands = Vec::new();
    // M29.6 — capacity matches main.rs's `App::new` construction:
    // (MAX_TOTAL_BONES / MBPM) - 1, with slot 0 reserved.
    let max_skinned = ((byroredux_renderer::vulkan::scene_buffer::MAX_TOTAL_BONES
        / byroredux_core::ecs::components::MAX_BONES_PER_MESH)
        - 1) as u32;
    let mut skin_slot_pool = byroredux_core::ecs::resources::SkinSlotPool::new(max_skinned);
    let _ = build_render_data(
        world,
        &mut draw_commands,
        &mut water_commands,
        &mut gpu_lights,
        &mut bone_world,
        &mut skin_offsets,
        &mut skin_slot_pool,
        &mut material_table,
        None,
    );
    (bone_world, skin_offsets)
}

#[test]
fn at_capacity_fills_palette_completely() {
    // M29.6 — slot pool capacity is `(MAX_TOTAL_BONES / MBPM) - 1`
    // (slot 0 reserved for global identity). Spawning exactly that
    // many entities fills every allocatable slot; the persistent
    // bone_world array reaches `(max_used_slot + 1) × MBPM =
    // MAX_TOTAL_BONES_rounded_down` entries.
    let max_skinned = (MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1;
    let world = make_skinned_world(max_skinned);
    let (palette, offsets) = run_build(&world);
    assert_eq!(
        offsets.len(),
        max_skinned,
        "all {} meshes must register a bone offset",
        max_skinned
    );
    // M29.6 sparse-slot layout: slot 0 (identity) + slots 1..=
    // max_skinned, each occupying MBPM bones → (max_skinned + 1)
    // × MBPM total entries. At the full ceiling that is
    // floor(196608/144) × 144 = 196560.
    let expected_slots = (max_skinned + 1) * MAX_BONES_PER_MESH;
    assert_eq!(palette.len(), expected_slots);
    assert!(
        expected_slots <= MAX_TOTAL_BONES,
        "expected_slots={expected_slots} must fit inside MAX_TOTAL_BONES={MAX_TOTAL_BONES}"
    );
}

#[test]
fn over_capacity_breaks_loop_and_truncates_offsets() {
    // M29.6 — requesting one more entity than the pool capacity
    // makes the pool's `allocate` return `None` for the offending
    // entity. That entity gets no slot, no `skin_offsets` entry,
    // and renders in bind pose (bone_offset = 0 = identity slot).
    // The palette stays bounded at the at-capacity ceiling.
    let max_skinned = (MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1;
    let world = make_skinned_world(max_skinned + 1);
    let (palette, offsets) = run_build(&world);
    assert!(
        offsets.len() < max_skinned + 1,
        "pool overflow must drop at least one mesh; got {} offsets for {} meshes",
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
