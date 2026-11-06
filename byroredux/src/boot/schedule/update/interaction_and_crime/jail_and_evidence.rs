//! Jail beds and evidence deposits.
//!
//! Split out of `interaction_and_crime.rs` (#6704); the rows, their order and their comments are unchanged.

use byroredux_core::ecs::{Access, Scheduler, Stage};

/// Jail beds and evidence deposits.
pub(super) fn register_jail_and_evidence_systems(scheduler: &mut Scheduler) {
    // Resting in a jail cell's bed ends the sentence; after the activation flush like every consumer.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::crime::jail_bed_system,
        Access::new()
            .reads_resource::<crate::systems::PlayerEntity>()
            .reads::<byroredux_scripting::ActivateEvent>()
            .reads::<byroredux_core::ecs::components::Furniture>()
            .writes_resource::<byroredux_scripting::obscript_ledger::ObScriptLedger>()
            .writes_resource::<byroredux_scripting::obscript_more::PendingSleep>()
            .writes_resource::<byroredux_scripting::PlayerMoveRequest>()
            .writes_resource::<crate::notifications::PlayerNotifications>(),
    );
    // Confiscated goods reach the jail's `StolenGoods` chest, loaded or parked.
    scheduler.add_exclusive_with_access(
        Stage::Update,
        crate::crime::evidence_deposit_system,
        Access::new()
            .writes_resource::<byroredux_scripting::jail::PendingEvidenceDeposits>()
            .writes_resource::<byroredux_scripting::obscript_ledger::ObScriptLedger>()
            .writes_resource::<crate::cell_loader::reference_state::PersistentReferenceStates>()
            .reads_resource::<crate::cell_loader::load_order::GlobalFormIdResolver>()
            .reads::<byroredux_core::ecs::components::FormIdComponent>()
            .reads_resource::<byroredux_core::form_id::FormIdPool>()
            .writes::<byroredux_core::ecs::components::Inventory>(),
    );
}
