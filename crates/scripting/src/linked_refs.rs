//! M47.2 — links scripts set: `ObjectReference.SetLinkedRef(apRef,
//! apKeyword = None)` ("Sets the reference linked to this one under the
//! specified keyword"), read back by `GetLinkedRef`.
//!
//! A reference's authored links are its `XLKR` pairs
//! ([`crate::scene::SceneAliasCandidate::linked_refs`]); a script's link
//! overrides the authored one under the same keyword, and a `None` target
//! clears it. Keyed by FormID so it outlives the cell that despawns the
//! entity. Saved.

use std::collections::HashMap;

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::world::World;

/// The authored encoding of the unkeyworded link (`XLKR` keyword 0).
const NO_KEYWORD: u32 = 0xFFFF_FFFF;

/// Every link a script set, by (source reference, keyword). The keyword of the
/// unkeyworded link is `0`; a target of `0` is a cleared link.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct LinkedRefOverrides {
    #[cfg_attr(feature = "save", serde(with = "crate::serde_pairs"))]
    links: HashMap<(u32, u32), u32>,
}

impl Resource for LinkedRefOverrides {}

impl LinkedRefOverrides {
    /// Link `source` to `target` under `keyword` (`0`: none); `target` `0`
    /// clears the link.
    pub fn set(&mut self, source: u32, keyword: u32, target: u32) {
        self.links.insert((source, keyword), target);
    }

    /// What a script made of `source`'s link under `keyword`: `Some(0)` when
    /// it cleared it, `None` when no script touched it.
    pub fn get(&self, source: u32, keyword: u32) -> Option<u32> {
        self.links.get(&(source, keyword)).copied()
    }
}

/// The link `candidate` (a placement's authored `XLKR` pairs) has under
/// `keyword`, `0` meaning the unkeyworded link.
fn authored_link(candidate: &crate::scene::SceneAliasCandidate, keyword: u32) -> Option<u32> {
    candidate
        .linked_refs
        .iter()
        .find(|(link, _)| *link == keyword || (keyword == 0 && *link == NO_KEYWORD))
        .map(|(_, target)| *target)
}

/// What a script made of `source`'s link under `keyword`, else `authored`.
fn effective_link(world: &World, source: u32, keyword: u32, authored: impl FnOnce() -> Option<u32>) -> Option<u32> {
    if let Some(target) = world
        .try_resource::<LinkedRefOverrides>()
        .and_then(|links| links.get(source, keyword))
    {
        return (target != 0).then_some(target);
    }
    authored()
}

/// The reference `source` is linked to under `keyword` (`0`: the unkeyworded
/// link): a script's link, else the authored one on the loaded `source` entity.
pub fn linked_ref(world: &World, source: u32, keyword: u32) -> Option<u32> {
    effective_link(world, source, keyword, || {
        let entity = crate::condition::resolve_entity_by_global_form_id(world, source)?;
        let candidate = world.get::<crate::scene::SceneAliasCandidate>(entity)?;
        authored_link(&candidate, keyword)
    })
}

/// The references linked to `target` under `keyword` (`0`: the unkeyworded link)
/// — CK *GetLinkedRefChildren*: "the references are directly linked as
/// children to this object using the specified keyword". Only loaded
/// references are searched; a script's link wins over the authored one, as in
/// [`linked_ref`].
pub fn linked_ref_children(world: &World, target: u32, keyword: u32) -> Vec<u32> {
    let mut sources: Vec<(u32, u32, Option<u32>)> = world
        .query::<crate::scene::SceneAliasCandidate>()
        .map(|candidates| {
            candidates
                .iter()
                .filter(|(_, candidate)| candidate.reference_form_id != 0)
                .map(|(entity, candidate)| (entity, candidate.reference_form_id, authored_link(candidate, keyword)))
                .collect()
        })
        .unwrap_or_default();
    sources.sort_unstable_by_key(|&(entity, ..)| entity);
    sources
        .into_iter()
        .filter(|&(_, source, authored)| effective_link(world, source, keyword, || authored) == Some(target))
        .map(|(_, source, _)| source)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The children of a reference are the loaded ones whose effective link
    /// (a script's over the authored one) under the keyword is that reference.
    #[test]
    fn children_are_the_loaded_references_linked_to_the_target() {
        use crate::scene::SceneAliasCandidate;
        const TARGET: u32 = 0x100;
        const KEYWORD: u32 = 0x200;
        let mut world = World::new();
        world.register::<SceneAliasCandidate>();
        world.insert_resource(LinkedRefOverrides::default());
        let spawn = |world: &mut World, reference: u32, links: Vec<(u32, u32)>| {
            let entity = world.spawn();
            world.insert(
                entity,
                SceneAliasCandidate { reference_form_id: reference, linked_refs: links, ..Default::default() },
            );
        };
        spawn(&mut world, 1, vec![(KEYWORD, TARGET)]); // authored child
        spawn(&mut world, 2, vec![(KEYWORD, 0x999)]); // linked elsewhere
        spawn(&mut world, 3, vec![]); // child by script
        spawn(&mut world, 4, vec![(KEYWORD, TARGET)]); // authored, then cleared by script
        {
            let mut links = world.resource_mut::<LinkedRefOverrides>();
            links.set(3, KEYWORD, TARGET);
            links.set(4, KEYWORD, 0);
        }
        assert_eq!(linked_ref_children(&world, TARGET, KEYWORD), vec![1, 3]);
        assert!(linked_ref_children(&world, TARGET, 0x300).is_empty(), "another keyword");
    }

    #[test]
    fn a_cleared_link_reads_as_none_and_an_untouched_one_as_unset() {
        let mut links = LinkedRefOverrides::default();
        assert_eq!(links.get(1, 0), None);
        links.set(1, 0, 7);
        assert_eq!(links.get(1, 0), Some(7));
        links.set(1, 0, 0);
        assert_eq!(links.get(1, 0), Some(0));
        assert_eq!(links.get(1, 5), None, "another keyword");
    }
}
