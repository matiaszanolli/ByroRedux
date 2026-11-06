//! Activate parents (M47.2): a reference passes its activation on to its
//! children.
//!
//! CK *Reference*, Activate Parents: "A reference can be set up to have one or
//! more Activate Parents. These are references which will 'pass on' an activation
//! event to this reference when they are activated." *Delay*: "the number of
//! seconds to wait after the parent's activation before activating this
//! reference". *Parent Activate Only*: "this reference can ONLY be activated by an
//! Activate Parent - direct activation (e.g. by the player) will not be possible.
//! Scripts can always activate the object". Notes: "for scripting purposes, this
//! reference will be activated by the reference which acted as the activate
//! parent, not the originally activating NPC". The CK *Traps and Prefabs*
//! tutorial: "when the Parent object is activated, any 'children' will also
//! receive an Activation" - a pull chain opens its portcullis, a tripwire springs
//! its trap.
//!
//! The data is the REFRs' `XAPR` / `XAPD` sub-records, kept by form id over the
//! whole load order ([`ActivateParentTable`]), so a relay does not depend on which
//! cell holds the parent. [`activate_parent_relay_system`] reads the
//! `ActivateEvent` markers of the frame: each activated parent queues an
//! activation of every loaded child, immediately or after its delay, through the
//! shared [`PendingFragmentActivations`] so every consumer sees it exactly once.
//! A child that is not loaded when its turn comes is skipped. A relay that would
//! close a cycle (the child is already an ancestor of the parent) is skipped.

use std::collections::{HashMap, HashSet};

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::{EntityId, World};
use byroredux_plugin::esm::records::EsmIndex;

/// Parent → its children (with delays), and the children that take no direct
/// activation. `NOT_SAVED_BY_DESIGN`: rebuilt from the records every boot.
#[derive(Debug, Default, Clone)]
pub struct ActivateParentTable {
    children: HashMap<u32, Vec<(u32, f32)>>,
    parents: HashMap<u32, Vec<u32>>,
    parent_only: HashSet<u32>,
}

impl Resource for ActivateParentTable {}

impl ActivateParentTable {
    pub fn from_index(index: &EsmIndex) -> Self {
        let mut table = Self::default();
        let mut visit = |cell: &byroredux_plugin::esm::cell::CellData| {
            for placed in &cell.references {
                if placed.parent_activate_only {
                    table.parent_only.insert(placed.form_id);
                }
                for parent in &placed.activate_parents {
                    table.add(parent.reference, placed.form_id, parent.delay);
                }
            }
        };
        for cell in index.cells.cells.values() {
            visit(cell);
        }
        for grids in index.cells.exterior_cells.values() {
            for cell in grids.values() {
                visit(cell);
            }
        }
        for cell in index.cells.worldspace_persistent_cells.values() {
            visit(cell);
        }
        table
    }

    fn add(&mut self, parent: u32, child: u32, delay: f32) {
        let children = self.children.entry(parent).or_default();
        if !children.iter().any(|(existing, _)| *existing == child) {
            children.push((child, if delay.is_finite() { delay.max(0.0) } else { 0.0 }));
            self.parents.entry(child).or_default().push(parent);
        }
    }

    pub fn edge_count(&self) -> usize {
        self.children.values().map(Vec::len).sum()
    }

    /// The children `parent` passes its activation to.
    pub fn children_of(&self, parent: u32) -> &[(u32, f32)] {
        self.children.get(&parent).map_or(&[], Vec::as_slice)
    }

    /// Whether `reference` takes activations only from its activate parents.
    pub fn takes_parent_activation_only(&self, reference: u32) -> bool {
        self.parent_only.contains(&reference)
    }

    /// Whether `candidate` is `reference` or one of its ancestors through activate
    /// parents (bounded: a malformed chain cannot hang it).
    fn is_ancestor(&self, candidate: u32, reference: u32) -> bool {
        let mut seen = HashSet::new();
        let mut frontier = vec![reference];
        while let Some(at) = frontier.pop() {
            if at == candidate {
                return true;
            }
            if !seen.insert(at) || seen.len() > 256 {
                continue;
            }
            frontier.extend(self.parents.get(&at).into_iter().flatten().copied());
        }
        false
    }
}

/// Activations waiting out their delay: `(seconds left, child, parent)`.
/// Transient.
#[derive(Debug, Default)]
pub struct PendingParentActivations(Vec<(f32, u32, u32)>);

impl Resource for PendingParentActivations {}

/// Install the load order's [`ActivateParentTable`]; returns its edge count.
pub fn install_activate_parent_table(world: &mut World, index: &EsmIndex) -> usize {
    let table = ActivateParentTable::from_index(index);
    let edges = table.edge_count();
    world.insert_resource(table);
    edges
}

fn form_of(world: &World, entity: EntityId) -> Option<u32> {
    crate::condition::entity_global_form_id(world, entity)
}

/// Pass this frame's activations on to the activated references' children.
pub fn activate_parent_relay_system(world: &World, dt: f32) {
    // The activated parents.
    let activated: Vec<EntityId> = match world.query::<crate::ActivateEvent>() {
        Some(events) => events.iter().map(|(entity, _)| entity).collect(),
        None => Vec::new(),
    };
    let have_table = world.try_resource::<ActivateParentTable>().is_some_and(|t| t.edge_count() > 0);
    if !have_table {
        return;
    }
    let mut due: Vec<(u32, u32)> = Vec::new();
    let mut waiting: Vec<(f32, u32, u32)> = Vec::new();
    {
        let table = world.resource::<ActivateParentTable>();
        for entity in activated {
            let Some(parent) = form_of(world, entity) else { continue };
            for &(child, delay) in table.children_of(parent) {
                if table.is_ancestor(child, parent) {
                    continue;
                }
                if delay > 0.0 {
                    waiting.push((delay, child, parent));
                } else {
                    due.push((child, parent));
                }
            }
        }
    }
    if let Some(mut pending) = world.try_resource_mut::<PendingParentActivations>() {
        pending.0.extend(waiting);
        let mut still = Vec::with_capacity(pending.0.len());
        for (left, child, parent) in std::mem::take(&mut pending.0) {
            let left = left - dt.max(0.0);
            if left <= 0.0 {
                due.push((child, parent));
            } else {
                still.push((left, child, parent));
            }
        }
        pending.0 = still;
    }
    if due.is_empty() {
        return;
    }
    let resolved: Vec<(EntityId, EntityId)> = due
        .into_iter()
        .filter_map(|(child, parent)| {
            let child = crate::condition::resolve_entity_by_global_form_id(world, child)?;
            let parent = crate::condition::resolve_entity_by_global_form_id(world, parent)?;
            Some((child, parent))
        })
        .collect();
    if let Some(mut queue) = world.try_resource_mut::<crate::fragment::PendingFragmentActivations>() {
        for (child, parent) in resolved {
            log::debug!("activate parent: entity {parent} passes its activation to entity {child}");
            queue.push(child, parent);
        }
    }
}

/// #6706 — this subsystem's world setup, owned here instead of the crate's `register` hub.
pub fn install(world: &mut byroredux_core::ecs::World) {
    // M47.2 — Sound.Play requests awaiting the host (transient).
    world.init_resource::<PendingParentActivations>();
}

#[cfg(test)]
mod tests;
