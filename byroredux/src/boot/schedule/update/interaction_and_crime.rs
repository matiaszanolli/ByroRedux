//! Vitals upkeep, canonical player interaction, activation flush, loot / jail / evidence, crime: the producer side of the activation contract.
//!
//! A contiguous run of `Stage::Update` registrations (#6359): registration order is the
//! contract, so [`super::register_update_systems`] calls the segments in file order.

use byroredux_core::ecs::Scheduler;

mod detection;
mod vitals_upkeep;
mod interaction;
mod activation;
mod container_loot;
mod jail_and_evidence;

/// The detection tick (M65), vitals upkeep, canonical player interaction, activation flush, loot / jail /
/// evidence, crime: the producer side of the activation contract. The tick leads, so the witnesses here
/// and every later consumer read this frame's levels toward the player.
pub(super) fn register_interaction_and_crime(scheduler: &mut Scheduler) {
    register_detection_then_vitals(scheduler);
    interaction::register_interaction_systems(scheduler);
    activation::register_activation_systems(scheduler);
    container_loot::register_container_loot_systems(scheduler);
    jail_and_evidence::register_jail_and_evidence_systems(scheduler);
}

/// The detection tick first (M65: every consumer after it reads this frame's levels), then vitals upkeep.
fn register_detection_then_vitals(scheduler: &mut Scheduler) {
    detection::register_detection(scheduler);
    vitals_upkeep::register_vitals_upkeep_systems(scheduler);
}
