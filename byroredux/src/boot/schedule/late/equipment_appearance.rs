//! Gear mesh-root reconcile after the Update-stage equipment emitters.
//!
//! Split out of `late.rs` (#6703); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage};

/// Gear mesh-root reconcile after the Update-stage equipment emitters.
pub(super) fn register_equipment_appearance_systems(scheduler: &mut Scheduler) {
    // P3 re-equip reconcile — runs after the Update-stage emitters
    // (container/corpse loot, native inventory actions) and beside the
    // other equipment-event readers, before the end-of-Late cleanup drains
    // the batch. Hides/reveals living actors' gear mesh roots; dead actors
    // are death-reconciliation territory.
    scheduler.add_exclusive_with_access(
        Stage::Late,
        crate::npc_spawn::loot_appearance::equipment_appearance_system,
        Access::new()
            .reads::<byroredux_scripting::EquipmentEventBatch>()
            // #5028 — leaves-inventory transfers queue gear-root release.
            .reads::<byroredux_scripting::ItemEventBatch>()
            .reads::<crate::npc_spawn::NpcEquipmentPart>()
            .reads::<byroredux_core::ecs::components::Dead>()
            .reads::<byroredux_core::ecs::components::Inventory>()
            .reads::<byroredux_core::ecs::CellRoot>()
            .writes::<crate::npc_spawn::loot_appearance::NpcAppearanceHidden>()
            .writes::<crate::npc_spawn::PendingGearRelease>()
            .reads::<byroredux_core::ecs::Children>()
            .reads::<byroredux_core::ecs::MeshHandle>()
            // #5069 — the mid-life gear-import half
            // (`queue_midlife_imports`, landed with 0182fc5e8 without a
            // schedule edit): PendingGearImport is read (the per-wearer
            // dedup gate) and written (the batched insert through
            // `query_mut`), ActorBodyClass read for the mesh pick, and the
            // loaded-cell index resolves item form IDs.
            .reads::<crate::npc_spawn::PendingGearImport>()
            .writes::<crate::npc_spawn::PendingGearImport>()
            .reads::<crate::npc_spawn::ActorBodyClass>()
            .reads_resource::<crate::cell_loader::LoadedCellIndex>(),
    );
}
