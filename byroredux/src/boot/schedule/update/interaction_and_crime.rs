//! Vitals upkeep, canonical player interaction, activation flush, loot / jail / evidence, crime: the producer side of the activation contract.
//!
//! A contiguous run of `Stage::Update` registrations (#6359): registration order is the
//! contract, so [`super::register_update_systems`] calls the segments in file order.

use byroredux_core::ecs::Scheduler;

mod vitals_upkeep;
mod interaction;
mod activation;
mod container_loot;
mod jail_and_evidence;

/// Vitals upkeep, canonical player interaction, activation flush, loot / jail / evidence, crime: the producer side of the activation contract.
pub(super) fn register_interaction_and_crime(scheduler: &mut Scheduler) {
    vitals_upkeep::register_vitals_upkeep_systems(scheduler);
    interaction::register_interaction_systems(scheduler);
    activation::register_activation_systems(scheduler);
    container_loot::register_container_loot_systems(scheduler);
    jail_and_evidence::register_jail_and_evidence_systems(scheduler);
}
