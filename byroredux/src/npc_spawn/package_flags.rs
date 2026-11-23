//! Ambient package flags (docs/prd/ai-packages-completion.md §5.7, AIP-FLG): the `PKDT` general flags the
//! ambient selection reads. The bit numbers are xEdit's `wbPackageFlags` (`wbDefinitionsCommon.pas`):
//! *Must Complete* 2, *Continue If PC Near* 9, *Once Per Day* 10; the meanings are the GECK *Packages* page's.
//!
//! * **Once Per Day** (AIP-FLG-001): "a 24 hour block beginning when the package was first selected". A package
//!   that is not the running one cannot win again until 24 game hours after it was first selected; the running
//!   one carries on while its window is open. The block is [`PackageDayBlocks`].
//! * **Must Complete** (AIP-FLG-002): the running package keeps winning past its schedule window until its
//!   procedure reports done — Travel arrived, Find's count met or target dealt with, UseWeapon's bursts fired.
//!   A procedure with no Done state (Sleep, Eat, Wander, Sandbox, Guard, …) never reports done, so it holds
//!   until a higher-priority package or `EvaluatePackage` ends it, as the GECK describes (#7224). An explicit
//!   `EvaluatePackage` drops the hold for that one selection (GECK *EvaluatePackage*: "EVP will still drop that
//!   package and force a reevaluation", #7243): the package keeps running only if its own window is open.
//! * **Continue If PC Near** (AIP-FLG-003): "will not terminate ... so long as the PC is nearby. Primarily used
//!   to keep shops open" — the running package keeps winning past its window while the player is in the
//!   actor's cell. The GECK gives no radius for "nearby"; the cell is the PRD's stand-in (Phase 0: verify).
//!
//! A kept package still loses to an earlier (higher-priority) one whose window is open, and still needs its
//! conditions to pass: only the *schedule* is extended.

use std::collections::HashMap;

use byroredux_core::ecs::components::{CellFormId, CellRoot, FindProgress, FormIdComponent, Traveled};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::form_id::FormId;
use byroredux_plugin::esm::records::{PackClock, PackRecord};

pub(super) const PKDT_MUST_COMPLETE: u32 = 0x0000_0004;
pub(super) const PKDT_CONTINUE_IF_PC_NEAR: u32 = 0x0000_0200;
pub(super) const PKDT_ONCE_PER_DAY: u32 = 0x0000_0400;

/// Game hours a Once Per Day block lasts.
const ONCE_PER_DAY_HOURS: f64 = 24.0;

/// When each `(actor, package)` Once Per Day package was first selected, in game hours since the runtime
/// began. `NOT_SAVED_BY_DESIGN`: a load restarts the 24-hour block, like the rest of the ambient runtime
/// state (`AmbientPackageRuntime`); it is cleared before the load's cell reload, whose spawns note their own (#7252).
///
/// #7317 — keyed by the actor's placed reference ([`BlockOwner`]), not its `EntityId`: streaming evicts and
/// respawns an actor under a fresh id (ids are never recycled), and a block keyed by the old id let the
/// package win again inside its 24 hours (AIP-FLG-001). A stream round trip inside one session keeps the block.
#[derive(Debug, Default, Clone)]
pub(crate) struct PackageDayBlocks(HashMap<(BlockOwner, u32), f64>);

/// Who a Once Per Day block belongs to: the actor's placed reference when it has one, which a stream
/// respawn keeps, else the entity. An NPC selects its first package inside its spawn job, before the
/// caller stamps the reference ([`adopt_entity_day_blocks`] moves those onto it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BlockOwner {
    Reference(FormId),
    Entity(EntityId),
}

/// The owner of `actor`'s blocks. Reads `FormIdComponent` alone; callers take [`PackageDayBlocks`] after.
fn block_owner(world: &World, actor: EntityId) -> BlockOwner {
    world
        .get::<FormIdComponent>(actor)
        .map(|form| BlockOwner::Reference(form.0))
        .unwrap_or(BlockOwner::Entity(actor))
}

/// Move the blocks noted against `entity` itself onto its placed reference, once the spawn path has stamped
/// one (`stamp_quest_reference`). The earlier start wins where the reference already holds a block for the
/// same package (a respawn whose spawn job re-selected it), so the 24 hours run from the first selection.
///
/// #7340 — the spawn job could not see that earlier block (it had no reference to key it by), so a package
/// still inside its 24 hours may have won the spawn scan and been installed as running, which the block then
/// exempts. When the reference's earlier, live block names the package the spawn installed, that install is
/// undone and the actor reselects on its next pass, where the block applies.
pub(crate) fn adopt_entity_day_blocks(world: &World, entity: EntityId) {
    let BlockOwner::Reference(reference) = block_owner(world, entity) else {
        return;
    };
    let now = now_hours(world);
    let blocked: Vec<u32> = {
        let Some(mut blocks) = world.try_resource_mut::<PackageDayBlocks>() else {
            return;
        };
        let moved: Vec<(u32, f64)> = blocks
            .0
            .iter()
            .filter(|((owner, _), _)| *owner == BlockOwner::Entity(entity))
            .map(|((_, package), first)| (*package, *first))
            .collect();
        let mut blocked = Vec::new();
        for (package, first) in moved {
            blocks.0.remove(&(BlockOwner::Entity(entity), package));
            let kept = blocks.0.entry((BlockOwner::Reference(reference), package)).or_insert(first);
            if *kept < first && now - *kept < ONCE_PER_DAY_HOURS {
                blocked.push(package);
            }
            *kept = kept.min(first);
        }
        blocked
    };
    let running = world
        .get::<crate::components::AmbientPackageRuntime>(entity)
        .and_then(|runtime| runtime.active_package_form_id);
    if running.is_some_and(|package| blocked.contains(&package)) {
        super::ai_package::clear_ambient_behavior(world, entity);
        if let Some(mut runtimes) = world.query_mut::<crate::components::AmbientPackageRuntime>()
            && let Some(runtime) = runtimes.get_mut(entity)
        {
            runtime.active_package_form_id = None;
            runtime.last_evaluated_game_minute = None;
        }
    }
}

impl Resource for PackageDayBlocks {}


#[cfg(test)]
impl PackageDayBlocks {
    /// Note a block directly (tests outside this module, which cannot reach [`note_selected`]).
    /// `actor`'s own entity key: these tests' actors carry no placed reference.
    pub(crate) fn note(&mut self, actor: EntityId, package: u32, at_hours: f64) {
        self.0.insert((BlockOwner::Entity(actor), package), at_hours);
    }

    pub(crate) fn holds(&self, actor: EntityId, package: u32) -> bool {
        self.0.contains_key(&(BlockOwner::Entity(actor), package))
    }
}

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
        // A procedure with no Done state never reports done, so a Must Complete one holds past its window
        // until something else ends it (a higher-priority package, `EvaluatePackage`, a script package).
        // That is the GECK's own account, not a stand-in (#7224): *OnPackageDone* says a package completes
        // when "the duration has expired, unless the Must Reach Location or Must Complete flags are
        // checked", and that "wanders, sleeps and eats, cannot 'complete', as they have no Done state";
        // *Guard Package* says one marked Must Complete "can never complete, therefore the package will
        // never reevaluate". The pages that call the flag "Invalid" (Sandbox, Accompany, Guard) warn
        // against setting it; they do not say the engine ignores it.
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
/// `release_hold` is an explicit `EvaluatePackage` this pass (`ReleasePackageHold`): it ends the Must Complete
/// hold, which is all the GECK says EVP drops; Continue If PC Near is left as it is.
fn kept_past_window(world: &World, actor: EntityId, current: Option<u32>, release_hold: bool, package: &PackRecord) -> bool {
    if current != Some(package.form_id) {
        return false;
    }
    (!release_hold && package.package_flags & PKDT_MUST_COMPLETE != 0 && !procedure_done(world, actor, package))
        || (package.package_flags & PKDT_CONTINUE_IF_PC_NEAR != 0 && player_in_actors_cell(world, actor))
}

/// Whether a Once Per Day `package` is inside its 24-hour block and not the running package.
fn once_per_day_blocked(world: &World, owner: BlockOwner, current: Option<u32>, package: &PackRecord, now: f64) -> bool {
    package.package_flags & PKDT_ONCE_PER_DAY != 0
        && current != Some(package.form_id)
        && world
            .try_resource::<PackageDayBlocks>()
            .and_then(|blocks| blocks.0.get(&(owner, package.form_id)).copied())
            .is_some_and(|first| now - first < ONCE_PER_DAY_HOURS)
}

/// The first package, in priority order, whose (possibly extended) window is open, that is not blocked for
/// the day and whose conditions pass — `active_package` with the three flags. `release_hold`: see
/// [`kept_past_window`].
pub(super) fn scan_with_flags<'a>(
    world: &World,
    actor: EntityId,
    clock: PackClock,
    current: Option<u32>,
    release_hold: bool,
    packages: impl IntoIterator<Item = &'a PackRecord>,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> Option<&'a PackRecord> {
    let now = now_hours(world);
    let owner = block_owner(world, actor);
    packages.into_iter().find(|package| {
        (package.scheduled_active_at(clock) || kept_past_window(world, actor, current, release_hold, package))
            && !once_per_day_blocked(world, owner, current, package, now)
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
    let owner = block_owner(world, actor);
    if let Some(mut blocks) = world.try_resource_mut::<PackageDayBlocks>() {
        // An entry past its 24 hours blocks nothing (`once_per_day_blocked`), so it is dropped here
        // rather than kept for an actor whose cell unloaded long ago (INC7-6: the map only grew).
        blocks.0.retain(|_, first| now - *first < ONCE_PER_DAY_HOURS);
        let first = blocks.0.entry((owner, package)).or_insert(now);
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
        scan_with_flags(world, actor, PackClock::from(hour), current, false, packages.iter(), |_| true).map(|p| p.form_id)
    }

    /// The scan with an explicit `EvaluatePackage` releasing the running package's hold (#7243).
    fn winner_after_evp(world: &World, actor: EntityId, hour: f32, current: Option<u32>, packages: &[PackRecord]) -> Option<u32> {
        scan_with_flags(world, actor, PackClock::from(hour), current, true, packages.iter(), |_| true).map(|p| p.form_id)
    }

    /// INC7-6 -- an entry past its 24 hours blocks nothing, so the next selection drops it: the map does
    /// not keep every Once Per Day actor the session ever saw.
    #[test]
    fn expired_day_blocks_are_dropped_when_a_new_one_is_noted() {
        let (mut world, actor) = world_at(1, 9.0);
        let other = world.spawn();
        note_selected(&world, other, PKDT_ONCE_PER_DAY, 0x10);
        assert_eq!(world.resource::<PackageDayBlocks>().0.len(), 1);
        let mut later = GameTimeRes::frozen_at(10.0);
        later.day = 2;
        world.insert_resource(later);
        note_selected(&world, actor, PKDT_ONCE_PER_DAY, 0x20);
        let blocks = &world.resource::<PackageDayBlocks>().0;
        assert_eq!(blocks.len(), 1, "the other actor's expired block is gone");
        assert!(blocks.contains_key(&(BlockOwner::Entity(actor), 0x20)));
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

    /// #7224 -- a Must Complete package whose procedure has no Done state (a Sleep here) holds past its window
    /// for good: the GECK's *OnPackageDone* and *Guard Package* pages say the flag stops a package ending on
    /// its duration and that such a package never reevaluates. It still yields to a higher-priority open one.
    #[test]
    fn a_must_complete_package_with_no_done_state_holds_past_its_window() {
        let (world, actor) = world_at(0, 14.0);
        let sleep = PackRecord { procedure_type: byroredux_plugin::esm::records::misc::pack::PROCEDURE_SLEEP, ..morning(1, PKDT_MUST_COMPLETE) };
        let packages = [sleep.clone(), always(2)];
        assert_eq!(winner(&world, actor, 14.0, Some(1), &packages), Some(1), "past its window, still running");
        assert_eq!(winner(&world, actor, 14.0, None, &packages), Some(2), "not running: the closed window keeps it out");
        let preempted = [always(9), sleep];
        assert_eq!(winner(&world, actor, 14.0, Some(1), &preempted), Some(9), "a higher-priority open package ends it");
    }

    /// #7243 -- an explicit `EvaluatePackage` drops the Must Complete hold for that selection (GECK
    /// *EvaluatePackage*): past its window the package loses; inside it, it wins on its own schedule. Continue If
    /// PC Near, which the GECK does not name there, still holds.
    #[test]
    fn an_explicit_evaluate_package_releases_only_the_must_complete_hold() {
        let (mut world, actor) = world_at(0, 14.0);
        let sleep = PackRecord { procedure_type: byroredux_plugin::esm::records::misc::pack::PROCEDURE_SLEEP, ..morning(1, PKDT_MUST_COMPLETE) };
        let packages = [sleep, always(2)];
        assert_eq!(winner_after_evp(&world, actor, 14.0, Some(1), &packages), Some(2), "past its window: EVP ends it");
        assert_eq!(winner_after_evp(&world, actor, 9.0, Some(1), &packages), Some(1), "inside its window it still wins");

        let player = world.spawn();
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        let cell = world.spawn();
        world.insert(cell, CellFormId(0xA));
        world.insert(actor, CellRoot(cell));
        world.insert(player, CellRoot(cell));
        let shop = [morning(1, PKDT_CONTINUE_IF_PC_NEAR), always(2)];
        assert_eq!(winner_after_evp(&world, actor, 14.0, Some(1), &shop), Some(1), "Continue If PC Near is not released");
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

    /// The placed reference an actor's `FormIdComponent` names.
    fn reference(world: &mut World, local: u32) -> FormId {
        use byroredux_core::form_id::{FormIdPair, FormIdPool, LocalFormId, PluginId};
        world.init_resource::<FormIdPool>();
        world
            .resource_mut::<FormIdPool>()
            .intern(FormIdPair { plugin: PluginId::from_filename("FalloutNV.esm"), local: LocalFormId(local) })
    }

    /// #7317 -- a stream round trip despawns the actor and respawns it under a new `EntityId` (ids are never
    /// recycled); the Once Per Day block follows its placed reference, so the package does not win again
    /// inside its 24 hours (AIP-FLG-001).
    #[test]
    fn a_once_per_day_block_survives_the_actor_respawning() {
        let (mut world, actor) = world_at(1, 9.0);
        let placed = reference(&mut world, 0x1234);
        world.insert(actor, FormIdComponent(placed));
        let packages = [morning(1, PKDT_ONCE_PER_DAY), always(2)];
        assert_eq!(winner(&world, actor, 9.0, None, &packages), Some(1));
        note_selected(&world, actor, PKDT_ONCE_PER_DAY, 1);
        world.despawn(actor);
        let respawned = world.spawn();
        world.insert(respawned, FormIdComponent(placed));
        assert_eq!(winner(&world, respawned, 10.0, None, &packages), Some(2), "one hour in: still blocked");
        // An actor without a reference keeps the entity key.
        let unplaced = world.spawn();
        assert_eq!(winner(&world, unplaced, 10.0, None, &packages), Some(1));
    }

    /// #7317 -- an NPC selects its first package inside its spawn job, before `stamp_quest_reference` gives
    /// it a `FormIdComponent`, so that block is noted under the entity. The stamp moves it onto the
    /// reference, and where the reference already holds one (a respawn), the earlier start is kept.
    #[test]
    fn a_block_noted_before_the_reference_stamp_moves_onto_it_keeping_the_earlier_start() {
        let (mut world, actor) = world_at(1, 9.0);
        let placed = reference(&mut world, 0x1234);
        let earlier = now_hours(&world) - 5.0;
        world.resource_mut::<PackageDayBlocks>().0.insert((BlockOwner::Reference(placed), 1), earlier);
        note_selected(&world, actor, PKDT_ONCE_PER_DAY, 1);
        note_selected(&world, actor, PKDT_ONCE_PER_DAY, 3);
        world.insert(actor, FormIdComponent(placed));
        adopt_entity_day_blocks(&world, actor);
        let now = now_hours(&world);
        let blocks = &world.resource::<PackageDayBlocks>().0;
        assert!(blocks.keys().all(|(owner, _)| *owner != BlockOwner::Entity(actor)), "{blocks:?}");
        assert_eq!(blocks.get(&(BlockOwner::Reference(placed), 1)), Some(&earlier), "the earlier start wins");
        assert_eq!(blocks.get(&(BlockOwner::Reference(placed), 3)), Some(&now));
    }

    /// #7340 -- the spawn job picked package 1 before the reference was stamped, so it could not see the
    /// reference's live block for it and installed 1 as running (which the block exempts). The adoption undoes
    /// that install so the next pass reselects under the block; a pick the reference does not block is kept.
    #[test]
    fn a_spawn_pick_the_references_block_forbids_is_undone_at_the_stamp() {
        use crate::components::AmbientPackageRuntime;
        let runtime = |active| AmbientPackageRuntime {
            package_candidates: vec![1, 2],
            active_package_form_id: active,
            actor_form_id: 0,
            last_evaluated_game_minute: Some(5),
        };
        for (blocked_earlier, expect) in [(true, None), (false, Some(1))] {
            let (mut world, actor) = world_at(1, 9.0);
            world.register::<AmbientPackageRuntime>();
            let placed = reference(&mut world, 0x1234);
            if blocked_earlier {
                let earlier = now_hours(&world) - 5.0;
                world.resource_mut::<PackageDayBlocks>().0.insert((BlockOwner::Reference(placed), 1), earlier);
            }
            note_selected(&world, actor, PKDT_ONCE_PER_DAY, 1);
            world.insert(actor, runtime(Some(1)));
            world.insert(actor, FormIdComponent(placed));
            adopt_entity_day_blocks(&world, actor);
            let after = world.get::<AmbientPackageRuntime>(actor).unwrap().clone();
            assert_eq!(after.active_package_form_id, expect, "blocked earlier: {blocked_earlier}");
            assert_eq!(after.last_evaluated_game_minute.is_none(), expect.is_none(), "a reselect is forced only when undone");
        }
    }
}
