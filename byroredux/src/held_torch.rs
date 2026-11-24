//! Hand Equipment (#7399, the owner's decision Q3): a torch held in a hand lights its surroundings, as a
//! carried light does (CK *Light*: "Time determines how long temporary lights, such as carried torches,
//! last"; the light is the LIGH's own radius, colour and falloff, translated as a placed light's are).
//!
//! One point light per actor holding a torch, owned through [`HeldTorchLight`] on the actor: spawned
//! when a torch comes into a hand, moved with the actor every frame, despawned when the torch leaves the
//! hand or the actor is gone. The light sits at the actor's left side at hand height (an engine rule:
//! the hand bone is not followed yet). The torch's burn time is not modelled.

use byroredux_core::ecs::components::EquipmentSlots;
use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::Component;
use byroredux_core::ecs::{EntityId, GlobalTransform, LightSource, Transform, World};
use byroredux_core::math::Vec3;
use byroredux_scripting::hands::{hand_item, Hand, HandItem};

/// Where the light sits in the actor's frame, in Bethesda units: to its left and at hand height (Z up).
const HAND_OFFSET: Vec3 = Vec3::new(-20.0, 10.0, 100.0);

/// The light of the torch an actor holds. Runtime only (`NOT_SAVED_BY_DESIGN`): re-derived from the
/// saved hands every frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HeldTorchLight {
    pub light: EntityId,
    /// The torch's base FormID: another torch brings its own light.
    pub torch: u32,
}

impl Component for HeldTorchLight {
    type Storage = SparseSetStorage<Self>;
}

/// Keep one light on each actor holding a torch, and none elsewhere.
pub(crate) fn held_torch_light_system(world: &mut World) {
    // Who holds a torch now (only an occupied left hand can).
    let candidates: Vec<EntityId> = world
        .query::<EquipmentSlots>()
        .map(|slots| slots.iter().filter(|(_, slots)| slots.left_hand.is_some()).map(|(entity, _)| entity).collect())
        .unwrap_or_default();
    let holders: Vec<(EntityId, u32)> = candidates
        .into_iter()
        .filter_map(|actor| match hand_item(world, actor, Hand::Left) {
            HandItem::Torch { base, .. } => Some((actor, base)),
            _ => None,
        })
        .collect();
    let lit: Vec<(EntityId, HeldTorchLight)> = world
        .query::<HeldTorchLight>()
        .map(|lights| lights.iter().map(|(actor, light)| (actor, *light)).collect())
        .unwrap_or_default();
    for &(actor, light) in &lit {
        if !holders.contains(&(actor, light.torch)) {
            world.despawn(light.light);
            let _ = world.remove::<HeldTorchLight>(actor);
        }
    }
    for &(actor, torch) in &holders {
        let current = world.get::<HeldTorchLight>(actor).map(|light| *light);
        let light = match current {
            Some(light) => light.light,
            None => {
                let Some(source) = torch_light(world, torch) else {
                    continue;
                };
                let light = world.spawn();
                world.insert(light, source);
                world.insert(actor, HeldTorchLight { light, torch });
                light
            }
        };
        let Some(at) = world.get::<GlobalTransform>(actor).map(|transform| transform.translation + transform.rotation * HAND_OFFSET)
        else {
            continue;
        };
        world.insert(light, Transform::new(at, byroredux_core::math::Quat::IDENTITY, 1.0));
        world.insert(light, GlobalTransform::new(at, byroredux_core::math::Quat::IDENTITY, 1.0));
    }
}

/// The point light of the carried LIGH `torch`, as a placed light of the same record is translated.
fn torch_light(world: &World, torch: u32) -> Option<LightSource> {
    let index = world.try_resource::<crate::components::LoadedCellIndex>()?;
    let data = index.0.cells.statics.get(&torch)?.light_data.clone()?;
    let falloff = crate::light_translate::canonical_light_falloff_exponent(index.0.game, data.falloff_exponent);
    Some(LightSource::from_legacy_world_units(
        data.radius,
        data.color,
        data.flags,
        falloff,
        byroredux_core::ecs::EmitterKind::Point,
        [0.0; 3],
        0.0,
        0,
    ))
}
