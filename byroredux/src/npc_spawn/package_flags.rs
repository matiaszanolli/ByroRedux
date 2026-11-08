//! Ambient package flags (docs/prd/ai-packages-completion.md §5.7, AIP-FLG): the `PKDT` general flags the
//! ambient selection reads. The bit numbers are xEdit's `wbPackageFlags` (`wbDefinitionsCommon.pas`):
//! *Must Complete* 2, *Continue If PC Near* 9, *Once Per Day* 10; the meanings are the GECK *Packages* page's.
//!
//! * **Once Per Day** (AIP-FLG-001): "a 24 hour block beginning when the package was first selected". A package
//!   that is not the running one cannot win again until 24 game hours after it was first selected; the running
//!   one carries on while its window is open. The block is [`PackageDayBlocks`].
//! * **Must Complete** (AIP-FLG-002): the running package keeps winning past its schedule window until its
//!   procedure reports done — Travel arrived, Find's count met or target dealt with, UseWeapon's bursts fired.
//! * **Continue If PC Near** (AIP-FLG-003): "will not terminate ... so long as the PC is nearby. Primarily used
//!   to keep shops open" — the running package keeps winning past its window while the player is in the
//!   actor's cell. The GECK gives no radius for "nearby"; the cell is the PRD's stand-in (Phase 0: verify).
//!
//! A kept package still loses to an earlier (higher-priority) one whose window is open, and still needs its
//! conditions to pass: only the *schedule* is extended.

use std::collections::HashMap;

use byroredux_core::ecs::components::{CellFormId, CellRoot, FindProgress, Traveled};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_plugin::esm::records::{PackClock, PackRecord};

pub(super) const PKDT_MUST_COMPLETE: u32 = 0x0000_0004;
pub(super) const PKDT_CONTINUE_IF_PC_NEAR: u32 = 0x0000_0200;
pub(super) const PKDT_ONCE_PER_DAY: u32 = 0x0000_0400;

/// Game hours a Once Per Day block lasts.
const ONCE_PER_DAY_HOURS: f64 = 24.0;

/// When each `(actor, package)` Once Per Day package was first selected, in game hours since the runtime
/// began. `NOT_SAVED_BY_DESIGN`: a load restarts the 24-hour block, like the rest of the ambient runtime
/// state (`AmbientPackageRuntime`).
#[derive(Debug, Default, Clone)]
pub(crate) struct PackageDayBlocks(HashMap<(EntityId, u32), f64>);

impl Resource for PackageDayBlocks {}

/// Whether `package`'s procedure has reported done for `actor` (the Must Complete test).
fn procedure_done(world: &World, actor: EntityId, package: &PackRecord) -> bool {
    use byroredux_plugin::esm::records::misc::pack::PROCEDURE_TRAVEL;
    if package.is_find() {
        return world.get::<FindProgress>(actor).is_some_and(|progress| progress.done);
    }
    match package.procedure_type {
        PROCEDURE_TRAVEL => world.has::<Traveled>(actor),
        byroredux_plugin::esm::records::misc::pack::PROCEDURE_USE_WEAPON => {
            world.has::<byroredux_core::ecs::components::UseWeaponPackageStarted>(actor)
                && !world.has::<crate::systems::script_requests::UseWeaponRun>(actor)
        }
        // A procedure with no completion signal never reports done, so a Must Complete one stays until
        // something else ends it (the script-package slot's own rule).
        _ => false,
    }
}

/// Whether the player is in the cell `actor` stands in (both loaded, same known cell).
fn player_in_actors_cell(world: &World, actor: EntityId) -> bool {
    let Some(player) = crate::systems::PlayerEntity::get(world) else {
        return false;
    };
    let cell_of = |entity: EntityId| {
        let root = world.get::<CellRoot>(entity).map_or(entity, |root| root.0);
        world.get::<CellFormId>(root).map(|cell| cell.0)
    };
    matches!((cell_of(actor), cell_of(player)), (Some(a), Some(b)) if a == b)
}

/// Game hours since the runtime began, for the Once Per Day block.
pub(super) fn now_hours(world: &World) -> f64 {
    world
        .try_resource::<crate::components::GameTimeRes>()
        .map_or(0.0, |time| time.day as f64 * 24.0 + f64::from(time.hour))
}

/// Whether the schedule of `package` is extended past its window for `actor` right now: it is the running
/// package and Must Complete is unmet, or Continue If PC Near holds.
fn kept_past_window(world: &World, actor: EntityId, current: Option<u32>, package: &PackRecord) -> bool {
    if current != Some(package.form_id) {
        return false;
    }
    (package.package_flags & PKDT_MUST_COMPLETE != 0 && !procedure_done(world, actor, package))
        || (package.package_flags & PKDT_CONTINUE_IF_PC_NEAR != 0 && player_in_actors_cell(world, actor))
}

/// Whether a Once Per Day `package` is inside its 24-hour block and not the running package.
fn once_per_day_blocked(world: &World, actor: EntityId, current: Option<u32>, package: &PackRecord, now: f64) -> bool {
    package.package_flags & PKDT_ONCE_PER_DAY != 0
        && current != Some(package.form_id)
        && world
            .try_resource::<PackageDayBlocks>()
            .and_then(|blocks| blocks.0.get(&(actor, package.form_id)).copied())
            .is_some_and(|first| now - first < ONCE_PER_DAY_HOURS)
}

/// The first package, in priority order, whose (possibly extended) window is open, that is not blocked for
/// the day and whose conditions pass — `active_package` with the three flags.
pub(super) fn scan_with_flags<'a>(
    world: &World,
    actor: EntityId,
    clock: PackClock,
    current: Option<u32>,
    packages: impl IntoIterator<Item = &'a PackRecord>,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> Option<&'a PackRecord> {
    let now = now_hours(world);
    packages.into_iter().find(|package| {
        (package.scheduled_active_at(clock) || kept_past_window(world, actor, current, package))
            && !once_per_day_blocked(world, actor, current, package, now)
            && condition_met(package)
    })
}

/// Start (or restart, once the last has lapsed) the 24-hour block of a Once Per Day package that has just
/// been selected.
pub(super) fn note_selected(world: &World, actor: EntityId, package_flags: u32, package: u32) {
    if package_flags & PKDT_ONCE_PER_DAY == 0 {
        return;
    }
    let now = now_hours(world);
    if let Some(mut blocks) = world.try_resource_mut::<PackageDayBlocks>() {
        let first = blocks.0.entry((actor, package)).or_insert(now);
        if now - *first >= ONCE_PER_DAY_HOURS {
            *first = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::GameTimeRes;
    use byroredux_core::ecs::components::UseWeaponPackageStarted;
    use byroredux_plugin::esm::records::misc::pack::{PackSchedule, PROCEDURE_TRAVEL};

    /// A package open 08:00-12:00 with `flags`.
    fn morning(form_id: u32, flags: u32) -> PackRecord {
        PackRecord {
            form_id,
            package_flags: flags,
            procedure_type: PROCEDURE_TRAVEL,
            schedule: Some(PackSchedule { start_hour: Some(8), duration_minutes: 4 * 60, ..Default::default() }),
            ..Default::default()
        }
    }

    fn always(form_id: u32) -> PackRecord {
        PackRecord { form_id, ..Default::default() }
    }

    fn world_at(day: u64, hour: f32) -> (World, EntityId) {
        let mut world = World::new();
        world.register::<Traveled>();
        world.register::<FindProgress>();
        world.register::<UseWeaponPackageStarted>();
        world.register::<CellRoot>();
        world.register::<CellFormId>();
        let mut time = GameTimeRes::frozen_at(hour);
        time.day = day;
        world.insert_resource(time);
        world.insert_resource(PackageDayBlocks::default());
        let actor = world.spawn();
        (world, actor)
    }

    fn winner(world: &World, actor: EntityId, hour: f32, current: Option<u32>, packages: &[PackRecord]) -> Option<u32> {
        scan_with_flags(world, actor, PackClock::from(hour), current, packages.iter(), |_| true).map(|p| p.form_id)
    }

    /// AIP-FLG-002 -- a Must Complete package keeps winning past its window until its procedure is done.
    #[test]
    fn a_must_complete_package_outlasts_its_window_until_it_is_done() {
        let (world, actor) = world_at(0, 14.0);
        let packages = [morning(1, PKDT_MUST_COMPLETE), always(2)];
        assert_eq!(winner(&world, actor, 14.0, None, &packages), Some(2), "closed window, not running: the fallback wins");
        assert_eq!(winner(&world, actor, 14.0, Some(1), &packages), Some(1), "running and not done: it stays");
        world.query_mut::<Traveled>().unwrap().insert(actor, Traveled);
        assert_eq!(winner(&world, actor, 14.0, Some(1), &packages), Some(2), "Travel arrived: it ends");
        // Without the flag the window ends it.
        let plain = [morning(1, 0), always(2)];
        assert_eq!(winner(&world, actor, 14.0, Some(1), &plain), Some(2));
    }

    /// A kept package still yields to an earlier package whose window is open.
    #[test]
    fn a_kept_package_still_yields_to_a_higher_priority_open_one() {
        let (world, actor) = world_at(0, 14.0);
        let packages = [always(9), morning(1, PKDT_MUST_COMPLETE)];
        assert_eq!(winner(&world, actor, 14.0, Some(1), &packages), Some(9));
    }

    /// AIP-FLG-003 -- Continue If PC Near holds a package open while the player is in the actor's cell.
    #[test]
    fn continue_if_pc_near_holds_while_the_player_is_in_the_actors_cell() {
        let (mut world, actor) = world_at(0, 14.0);
        let player = world.spawn();
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        let (cell_a, cell_b) = (world.spawn(), world.spawn());
        world.insert(cell_a, CellFormId(0xA));
        world.insert(cell_b, CellFormId(0xB));
        world.insert(actor, CellRoot(cell_a));
        world.insert(player, CellRoot(cell_b));
        let packages = [morning(1, PKDT_CONTINUE_IF_PC_NEAR), always(2)];
        assert_eq!(winner(&world, actor, 14.0, Some(1), &packages), Some(2), "the player is elsewhere");
        world.insert(player, CellRoot(cell_a));
        assert_eq!(winner(&world, actor, 14.0, Some(1), &packages), Some(1), "the player is in the cell");
    }

    /// AIP-FLG-001 -- a Once Per Day package cannot win again within 24 game hours of its first selection, but
    /// the running one carries on; after the block it may win again.
    #[test]
    fn a_once_per_day_package_cannot_win_again_within_24_hours_of_its_first_selection() {
        let (mut world, actor) = world_at(0, 9.0);
        let packages = [morning(1, PKDT_ONCE_PER_DAY), always(2)];
        assert_eq!(winner(&world, actor, 9.0, None, &packages), Some(1), "first selection");
        note_selected(&world, actor, PKDT_ONCE_PER_DAY, 1);
        assert_eq!(winner(&world, actor, 9.0, Some(1), &packages), Some(1), "the running one carries on");
        // The window closes and reopens the next morning, 24 hours after the first selection at 09:00 day 0:
        // 08:00 on day 1 is 23 hours later, still blocked; 09:00 is 24 and free.
        let mut time = GameTimeRes::frozen_at(8.5);
        time.day = 1;
        world.insert_resource(time);
        assert_eq!(winner(&world, actor, 8.5, Some(2), &packages), Some(2), "23.5 hours in: blocked");
        let mut time = GameTimeRes::frozen_at(9.0);
        time.day = 1;
        world.insert_resource(time);
        assert_eq!(winner(&world, actor, 9.0, Some(2), &packages), Some(1), "24 hours in: free");
        // Re-selection restarts the block.
        note_selected(&world, actor, PKDT_ONCE_PER_DAY, 1);
        assert_eq!(winner(&world, actor, 9.0, Some(2), &packages), Some(2), "blocked again");
    }

    /// A package with none of the flags is the old `active_package` scan.
    #[test]
    fn unflagged_packages_scan_as_before() {
        let (world, actor) = world_at(0, 9.0);
        let packages = [morning(1, 0), always(2)];
        assert_eq!(winner(&world, actor, 9.0, None, &packages), Some(1));
        assert_eq!(winner(&world, actor, 13.0, None, &packages), Some(2));
    }
}
