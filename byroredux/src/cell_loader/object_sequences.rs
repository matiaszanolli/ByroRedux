//! The named animation sequences of placed objects (M47.3).
//!
//! A model with several `NiControllerSequence`s plays the first on spawn; the rest are what an
//! ObScript `PlayGroup Forward` chooses among. Each unique model's sequences are converted and
//! registered once per process, here, and every placement of it carries the same
//! [`NamedSequences`] handle table. The clip registry never recycles a slot, so the handles stay
//! valid whatever the NIF cache evicts.

use std::collections::HashMap;
use std::sync::Arc;

use byroredux_core::animation::{AnimationClipRegistry, NamedSequences};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;
use byroredux_core::string::StringPool;

type SequenceTable = Arc<[(Box<str>, u32)]>;

/// Model cache key to the clip handles of its sequences. `NOT_SAVED_BY_DESIGN`: rebuilt from the
/// models as they spawn.
#[derive(Default)]
pub(crate) struct ObjectSequenceStore(HashMap<String, SequenceTable>);

impl Resource for ObjectSequenceStore {}

/// The [`NamedSequences`] for the model under `key`: converted and registered on first use,
/// shared afterwards. `None` for a model with no sequences.
pub(crate) fn named_sequences_for(
    world: &mut World,
    key: &str,
    sequences: &[byroredux_nif::anim::AnimationClip],
) -> Option<NamedSequences> {
    if sequences.is_empty() {
        return None;
    }
    if let Some(table) = world.try_resource::<ObjectSequenceStore>().and_then(|store| store.0.get(key).cloned()) {
        return Some(NamedSequences(table));
    }
    let table: SequenceTable = {
        let converted: Vec<_> = {
            let mut pool = world.resource_mut::<StringPool>();
            sequences
                .iter()
                .map(|clip| (clip.name.to_ascii_lowercase().into_boxed_str(), crate::anim_convert::convert_nif_clip(clip, &mut pool)))
                .collect()
        };
        let mut registry = world.resource_mut::<AnimationClipRegistry>();
        converted.into_iter().map(|(name, clip)| (name, registry.add(clip))).collect()
    };
    world.init_resource::<ObjectSequenceStore>();
    world.resource_mut::<ObjectSequenceStore>().0.insert(key.to_owned(), Arc::clone(&table));
    Some(NamedSequences(table))
}
