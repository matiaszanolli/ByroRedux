//! Component registration — creates ComponentDescriptor instances for
//! each inspectable component type.

use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::world::World;
use byroredux_debug_protocol::registry::{ComponentDescriptor, ComponentRegistry};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Register a component type with the debug registry.
///
/// The component must implement `Serialize + DeserializeOwned` (gated behind
/// the `inspect` feature on byroredux-core) so we can convert to/from JSON.
fn register_component<T>(
    registry: &mut ComponentRegistry,
    name: &'static str,
    field_names: Vec<&'static str>,
) where
    T: Component + Serialize + DeserializeOwned,
{
    let desc = ComponentDescriptor {
        name,
        field_names,
        get_json: Box::new(|world_any: &dyn std::any::Any, entity: u32| {
            let world = world_any.downcast_ref::<World>()?;
            let comp = world.get::<T>(entity)?;
            serde_json::to_value(&*comp).ok()
        }),
        set_json: Box::new(
            |world_any: &dyn std::any::Any, entity: u32, value: serde_json::Value| {
                // Note: set_json requires &mut World which we don't have in an exclusive system
                // that takes &World. For now, return an error — mutation will go through set_field.
                let _ = (world_any, entity, value);
                Err(
                    "whole-component replacement not yet supported; use field-level set"
                        .to_string(),
                )
            },
        ),
        list_entities: Box::new(|world_any: &dyn std::any::Any| {
            let world = match world_any.downcast_ref::<World>() {
                Some(w) => w,
                None => return Vec::new(),
            };
            match world.query::<T>() {
                Some(q) => q.iter().map(|(id, _)| id).collect(),
                None => Vec::new(),
            }
        }),
        get_field: Box::new(|world_any: &dyn std::any::Any, entity: u32, field: &str| {
            let world = world_any.downcast_ref::<World>()?;
            let comp = world.get::<T>(entity)?;
            let full = serde_json::to_value(&*comp).ok()?;
            // Try named field access
            if let Some(val) = full.get(field) {
                return Some(val.clone());
            }
            // For tuple structs serialized as single values, return the whole thing
            // when field is "0" or matches the type
            if field == "0" {
                return Some(full);
            }
            None
        }),
        set_field: Box::new(
            |world_any: &dyn std::any::Any, entity: u32, field: &str, value: serde_json::Value| {
                // For mutation we need to: read → modify JSON → deserialize back → write.
                // This requires &mut World, but the exclusive system has &World.
                // We'll need to use query_mut which takes &self on World (interior mutability).
                let world = world_any
                    .downcast_ref::<World>()
                    .ok_or_else(|| "internal error: world downcast failed".to_string())?;

                let mut q = world
                    .query_mut::<T>()
                    .ok_or_else(|| format!("no storage for component"))?;

                let comp = q
                    .get_mut(entity)
                    .ok_or_else(|| format!("entity {} has no component", entity))?;

                // Serialize current state, modify the field, deserialize back.
                let mut json =
                    serde_json::to_value(&*comp).map_err(|e| format!("serialize error: {}", e))?;

                // Handle named fields on objects
                if let serde_json::Value::Object(ref mut map) = json {
                    if map.contains_key(field) {
                        map.insert(field.to_string(), value);
                    } else {
                        return Err(format!("no field '{}' on component", field));
                    }
                } else if field == "0" {
                    // Tuple struct — replace the whole value
                    json = value;
                } else {
                    return Err(format!("component is not a struct with named fields"));
                }

                // Deserialize back and overwrite
                let new_comp: T = serde_json::from_value(json)
                    .map_err(|e| format!("deserialize error: {}", e))?;
                *comp = new_comp;

                Ok(())
            },
        ),
    };
    registry.insert(desc);
}

/// Register all inspectable components with the registry.
pub fn register_all(registry: &mut ComponentRegistry) {
    use byroredux_core::ecs::components::*;

    register_component::<Transform>(
        registry,
        "Transform",
        vec!["translation", "rotation", "scale"],
    );
    register_component::<GlobalTransform>(
        registry,
        "GlobalTransform",
        vec!["translation", "rotation", "scale"],
    );
    register_component::<Camera>(registry, "Camera", vec!["fov_y", "near", "far", "aspect"]);
    register_component::<LightSource>(registry, "LightSource", vec!["radius", "color", "flags"]);
    register_component::<Material>(
        registry,
        "Material",
        vec![
            "emissive_color",
            "emissive_mult",
            "specular_color",
            "specular_strength",
            "glossiness",
            "uv_offset",
            "uv_scale",
            "alpha",
            "env_map_scale",
            "normal_map",
            "texture_path",
            "glow_map",
            "detail_map",
            "gloss_map",
            "dark_map",
            "vertex_color_mode",
            "alpha_test",
            "alpha_threshold",
            "alpha_test_func",
        ],
    );
    register_component::<LocalBound>(registry, "LocalBound", vec!["center", "radius"]);
    register_component::<WorldBound>(registry, "WorldBound", vec!["center", "radius"]);
    register_component::<Billboard>(registry, "Billboard", vec!["mode"]);
    register_component::<MeshHandle>(registry, "MeshHandle", vec!["0"]);
    register_component::<TextureHandle>(registry, "TextureHandle", vec!["0"]);
    register_component::<BSXFlags>(registry, "BSXFlags", vec!["0"]);
    register_component::<BSBound>(registry, "BSBound", vec!["center", "half_extents"]);
    register_component::<AnimatedVisibility>(registry, "AnimatedVisibility", vec!["0"]);
    register_component::<AnimatedAlpha>(registry, "AnimatedAlpha", vec!["0"]);
    register_component::<AnimatedColor>(registry, "AnimatedColor", vec!["0"]);
}
