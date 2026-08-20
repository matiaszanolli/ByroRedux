//! FO4+ precombined-mesh ownership marker (EX-15 / #2369).
//!
//! Precombined architecture (`meshes\precombined\*_oc.nif`, see the
//! `byroredux::cell_loader::precombined` binary module) is spawned instead
//! of — never alongside — the individual REFRs it absorbs;
//! `absorbed_refs_or_empty` is the gate that keeps the two exclusive within
//! one cell's own load. What that gate cannot see is whether a live
//! streaming soak (repeated load/unload across a real traversal) ever
//! leaves a precombine mesh resident without its owning cell, or lets the
//! count drift instead of returning to baseline after an out-and-back
//! cycle.
//!
//! [`PrecombinedMesh`] exists purely so [`OwnershipSnapshot`](crate::ecs::OwnershipSnapshot)
//! can count precombine-spawned entities as their own reclaim class —
//! `world.owners` already asserts an `Exact` return-to-baseline for
//! `cell_root_rows`; splitting this subset out means a leak specific to
//! precombine geometry (as opposed to ordinary per-REFR architecture)
//! shows up as its own named finding instead of hiding inside the
//! aggregate.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// Marks an entity as spawned from a baked `_oc.nif` precombine object,
/// as opposed to an ordinary per-REFR placement. Stamped once, at spawn,
/// by the binary's cell-loader alongside the entity's `CellRoot` — never
/// removed individually; it goes away with the rest of the cell on
/// `unload_cell`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PrecombinedMesh;

impl Component for PrecombinedMesh {
    type Storage = SparseSetStorage<Self>;
}
