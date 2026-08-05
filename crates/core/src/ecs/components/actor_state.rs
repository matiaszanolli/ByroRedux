//! Sparse actor lifecycle markers.

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// Marks an actor as dead.
///
/// Absence means alive. Keeping death as a sparse marker makes it cheap for
/// the common live-actor case and gives combat, scripts, resurrection, and
/// condition evaluation one shared source of truth.
///
/// # Save registry — deliberately NOT registered yet (#2293 / SAVE-D1-10)
///
/// `Dead` is intentionally absent from `byroredux::save_io::build_save_registry`.
/// This is not an oversight: nothing in the live codebase inserts `Dead`
/// today outside of `condition.rs`'s own unit test (there is no
/// death-resolution/combat-kill system yet — `crates/core/src/combat.rs` is
/// pure damage-formula helpers), so there is no state a save/load could lose.
///
/// **Forward-latent tripwire**: the moment a real system starts inserting
/// `Dead` during gameplay, register it in `build_save_registry` in that SAME
/// commit — a dead NPC silently reviving on every load is a much worse
/// variant of the exact bug class SAVE-D1-08/09 demonstrated. Do not let this
/// comment go stale; if you're adding a kill/death system and reading this,
/// this is your reminder.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Dead;

impl Component for Dead {
    type Storage = SparseSetStorage<Self>;
}
