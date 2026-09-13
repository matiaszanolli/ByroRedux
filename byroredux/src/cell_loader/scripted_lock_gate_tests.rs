//! Regression coverage for #4136 (SAVE-D1-2026-09-11-02) — the runtime
//! consumer for a Papyrus `Lock()` / `Unlock()` / `SetLockLevel()`.
//!
//! #3159 made `Locked` runtime-mutable but left the change living only on
//! the component, which loses it twice over: across a save/load (it is not
//! a registered column) and across an ordinary in-session cell revisit
//! (cell unload despawns the placement root, and `spawn_placement_root`
//! re-stamps the plugin's authored `XLOC` onto a brand-new entity).
//!
//! The second is why the fix is a FormID-keyed ledger rather than a saved
//! component: a component cannot outlive its own entity, so registering
//! `Locked` would have left a player who picks a lock and walks back
//! through the door facing it locked again.
//!
//! `spawn_placement_root` needs a live `VulkanContext`, so — following the
//! `reference_enable_gate_tests` precedent for exactly this problem — the
//! decision lives in the Vulkan-free `scripted_lock_override` accessor and
//! the contract is pinned here.
use super::spawn::scripted_lock_override;
use byroredux_core::ecs::World;
use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};
use byroredux_scripting::{LockOverride, ReferenceLockState};

const DOOR: u32 = 0x0001_2345;
const NEIGHBOUR: u32 = 0x0001_9999;

fn pair(local: u32) -> FormIdPair {
    FormIdPair {
        plugin: PluginId::from_filename("Skyrim.esm"),
        local: LocalFormId(local),
    }
}

fn world_with(state: ReferenceLockState) -> World {
    let mut world = World::new();
    world.insert_resource(state);
    world
}

/// The core of the bug: a scripted unlock has to be visible to the *next*
/// cell load, on an entity that does not exist yet.
#[test]
fn a_scripted_unlock_overrides_the_authored_lock_on_reload() {
    let mut state = ReferenceLockState::default();
    state.set_unlocked(DOOR);
    let world = world_with(state);

    assert_eq!(
        scripted_lock_override(&world, Some(pair(DOOR))),
        Some(LockOverride::Unlocked),
        "#4136: a door the player unlocked must not be re-locked by the \
         authored XLOC when the cell is revisited"
    );
    assert_eq!(
        scripted_lock_override(&world, Some(pair(NEIGHBOUR))),
        None,
        "#4136: the override must be per-REFR — an untouched neighbour keeps \
         its authored lock"
    );
}

/// A scripted *lock* has to carry its level and key across the same edge,
/// or a door a quest locked reopens on revisit.
#[test]
fn a_scripted_lock_overrides_with_its_recorded_level_and_key() {
    let mut state = ReferenceLockState::default();
    state.set_locked(DOOR, 90, Some(0xDEAD));
    let world = world_with(state);

    assert_eq!(
        scripted_lock_override(&world, Some(pair(DOOR))),
        Some(LockOverride::Locked {
            lock_level: 90,
            key_form_id: Some(0xDEAD)
        })
    );
}

/// Absent must mean "fall back to the authored XLOC", which is what keeps
/// every untouched placement in every cell behaving exactly as before.
#[test]
fn an_untouched_reference_falls_back_to_the_authored_lock() {
    let world = world_with(ReferenceLockState::default());
    assert_eq!(scripted_lock_override(&world, Some(pair(DOOR))), None);
}

/// Loose-NIF spawns pass no placement identity, and a world without the
/// ledger (a bare test world, or the loose-NIF path) must not panic — same
/// posture as `placement_is_disabled`'s own `None` arms.
#[test]
fn a_placement_without_identity_or_ledger_is_never_overridden() {
    let world = world_with(ReferenceLockState::default());
    assert_eq!(scripted_lock_override(&world, None), None);

    let bare = World::new();
    assert_eq!(
        scripted_lock_override(&bare, Some(pair(DOOR))),
        None,
        "a world with no ReferenceLockState resource must fall back cleanly"
    );
}

/// The predicate tests above prove `scripted_lock_override` decides
/// correctly; this pins that `spawn_placement_root` actually *asks* it.
///
/// Needed because the accessor and the stamp are testable at different
/// levels: the stamp needs a live `VulkanContext` and cannot run here, so
/// without this a change that bypassed the override — restoring the old
/// unconditional `if let Some(l) = lock` — would leave every assertion
/// above green while reinstating the entire defect. Verified by injection:
/// replacing the call with a constant `None` passes the predicate tests
/// and fails this one.
#[test]
fn the_spawn_stamp_consults_the_ledger_before_applying_the_authored_lock() {
    const SRC: &str = include_str!("spawn.rs");

    let stamp_at = SRC
        .find("Locked {")
        .expect("the XLOC stamp must still exist");
    let consult_at = SRC
        .find("match scripted_lock_override(world, placement_form_id_pair)")
        .expect(
            "spawn_placement_root must consult ReferenceLockState before stamping the \
             authored XLOC (#4136) — an unconditional stamp re-locks every door a \
             script opened, on every cell revisit",
        );
    assert!(
        consult_at < stamp_at,
        "the ledger must be consulted before the authored lock is applied \
         (consult {consult_at}, stamp {stamp_at})"
    );
}
