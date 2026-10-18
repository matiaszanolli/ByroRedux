//! The surface glow of a playing magic shader (M47.3 `PlayMagicShaderVisuals`).
//!
//! `byroredux_scripting::magic_shaders` keeps which shader plays on which reference and how strong its
//! fill layer is this frame; this system puts that on the reference's meshes: each material's emissive is
//! raised by the fill colour weighted by the alpha, and put back when the shader ends. The originals are
//! remembered per mesh, not recomputed, so a shader that ends leaves the material exactly as it was.

use std::collections::HashMap;

use byroredux_core::ecs::components::{Children, Material};
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{Resource, World};
use byroredux_scripting::magic_shaders::{current_tint, PlayingShaders};

/// The emissive of every mesh a shader is tinting, as it was before: `owner -> [(mesh, color, mult)]`.
/// `NOT_SAVED_BY_DESIGN`: the tint is presentation and ends with the session.
#[derive(Debug, Default)]
pub(crate) struct ShaderTintRestore(HashMap<EntityId, Vec<(EntityId, [f32; 3], f32)>>);

impl Resource for ShaderTintRestore {}

/// The meshes under `root` (itself included) that carry a `Material`.
fn mesh_entities(world: &World, root: EntityId) -> Vec<EntityId> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if world.has::<Material>(entity) {
            found.push(entity);
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.0.iter().copied());
        }
    }
    found
}

fn restore(world: &World, originals: &[(EntityId, [f32; 3], f32)]) {
    let Some(mut materials) = world.query_mut::<Material>() else { return };
    for &(mesh, color, mult) in originals {
        if let Some(material) = materials.get_mut(mesh) {
            material.emissive_color = color;
            material.emissive_mult = mult;
        }
    }
}

pub(crate) fn magic_shader_tint_system(world: &World, _dt: f32) {
    let playing: Vec<EntityId> = world.query::<PlayingShaders>().map(|q| q.iter().map(|(entity, _)| entity).collect()).unwrap_or_default();
    let tinted: Vec<EntityId> = world.try_resource::<ShaderTintRestore>().map(|restore| restore.0.keys().copied().collect()).unwrap_or_default();
    if playing.is_empty() && tinted.is_empty() {
        return;
    }
    // Shaders that ended: give the materials back.
    for owner in tinted.iter().filter(|owner| !playing.contains(owner)) {
        let originals = world.try_resource_mut::<ShaderTintRestore>().and_then(|mut restore| restore.0.remove(owner));
        if let Some(originals) = originals {
            restore(world, &originals);
        }
    }
    for owner in playing {
        let Some(tint) = current_tint(world, owner) else { continue };
        // First frame of a play: remember what the meshes look like.
        let known = world.try_resource::<ShaderTintRestore>().is_some_and(|restore| restore.0.contains_key(&owner));
        if !known {
            let originals: Vec<(EntityId, [f32; 3], f32)> = {
                let Some(materials) = world.query::<Material>() else { continue };
                mesh_entities(world, owner)
                    .into_iter()
                    .filter_map(|mesh| materials.get(mesh).map(|m| (mesh, m.emissive_color, m.emissive_mult)))
                    .collect()
            };
            if let Some(mut store) = world.try_resource_mut::<ShaderTintRestore>() {
                store.0.insert(owner, originals);
            }
        }
        let originals = world.try_resource::<ShaderTintRestore>().and_then(|restore| restore.0.get(&owner).cloned()).unwrap_or_default();
        let Some(mut materials) = world.query_mut::<Material>() else { continue };
        for (mesh, color, mult) in originals {
            let Some(material) = materials.get_mut(mesh) else { continue };
            // emissive = colour × mult: add the tint to that product, keeping a positive multiplier.
            let base = [color[0] * mult, color[1] * mult, color[2] * mult];
            let glow = [base[0] + tint[0], base[1] + tint[1], base[2] + tint[2]];
            let new_mult = glow.iter().cloned().fold(0.0f32, f32::max).max(mult).max(f32::EPSILON);
            material.emissive_mult = new_mult;
            material.emissive_color = [glow[0] / new_mult, glow[1] / new_mult, glow[2] / new_mult];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::{EfshFill, EfshRecord, EsmIndex};
    use byroredux_scripting::magic_shaders::{magic_shader_clock_system, play_shader, ShaderCatalog};

    #[test]
    fn a_playing_shader_lights_the_meshes_and_gives_them_back() {
        let mut world = World::new();
        world.register::<PlayingShaders>();
        world.register::<Material>();
        world.register::<Children>();
        world.insert_resource(ShaderTintRestore::default());
        let mut index = EsmIndex::default();
        index.effect_shaders.insert(
            0x77,
            EfshRecord {
                form_id: 0x77,
                fill: Some(EfshFill { color: [255, 0, 0], fade_in: 0.0, full_alpha_time: 10.0, fade_out: 1.0, persistent_alpha_ratio: 1.0, pulse_amplitude: 0.0, pulse_frequency: 0.0, no_membrane: false }),
                ..Default::default()
            },
        );
        world.insert_resource(ShaderCatalog::from_index(&index));
        let actor = world.spawn();
        let mesh = world.spawn();
        world.insert(actor, Children(vec![mesh]));
        let material = Material { emissive_color: [0.0, 0.0, 0.0], emissive_mult: 1.0, ..Default::default() };
        world.insert(mesh, material.clone());

        assert!(play_shader(&world, actor, 0x77, Some(2.0)));
        magic_shader_tint_system(&world, 0.0);
        let lit = world.get::<Material>(mesh).unwrap().emissive_color;
        assert!(lit[0] > 0.9 && lit[1] == 0.0, "red glow: {lit:?}");
        // The shader's time is up: the meshes are exactly as they were.
        magic_shader_clock_system(&world, 30.0);
        magic_shader_tint_system(&world, 0.0);
        let back = world.get::<Material>(mesh).unwrap();
        assert_eq!((back.emissive_color, back.emissive_mult), (material.emissive_color, material.emissive_mult));
    }
}
