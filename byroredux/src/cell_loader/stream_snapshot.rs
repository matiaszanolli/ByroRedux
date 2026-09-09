//! Actor state that must survive an ordinary stream-tile despawn/respawn
//! (EX-16 item 4, #3299).
//!
//! Design authority: `docs/engine/stream-boundary-state-continuity.md`
//! (§4 keep/drop table, §5 lifetime/bounding).
//!
//! `unload_cell_inner` despawns every entity in an evicted tile with no
//! AI-package-state awareness, and reload respawns fresh — placing each
//! actor back at its *authored REFR position* and re-initialising package
//! state from scratch. An actor that had made progress on a Travel package,
//! or was `Seated`, loses it the instant its spawn tile round-trips through
//! ordinary radius streaming. The most visible symptom is a completed
//! errand un-completing itself: an actor already carrying `Traveled` walks
//! the whole route again.
//!
//! ## Why this does not use a FormID→Entity index
//!
//! The design doc's §3 proposes a `CellRoot`-scoped `FormId → EntityId`
//! index, and #3299's sequencing note records one as having landed ahead of
//! its consumers. It did, and then it was **deleted** — #3884 removed the
//! whole single-root ref-index subsystem (three modules, 501 lines, two
//! resources) after EX-16 (#2372) and EX-14/15 (#2369) both closed without
//! ever wiring it. Rebuilding it here would resurrect code deleted three
//! days earlier for having zero call sites.
//!
//! The surviving `resolve_entity_by_global_form_id` is the right tool
//! instead. It is `O(n)` over `FormIdComponent`-tagged entities, which the
//! index existed to avoid — but the access pattern here is one lookup per
//! *restored actor per tile reload*, not per frame, and only for the small
//! minority of actors that accumulated state worth keeping. The index's own
//! doc justified it for a per-frame path that never materialised.
//!
//! ## Scope
//!
//! Keep (§4): live position, `AmbientPackageRuntime.active_package_form_id`,
//! `TravelState.destination` + `Traveled` presence, and `Seated.furniture`
//! **as a FormID**.
//!
//! Deliberately dropped: `WanderState`, because re-rolling it on respawn is
//! `WanderBehavior`'s *intended* behaviour — its `form_id` feeds a
//! deterministic desync hash, so restoring a phase would defeat the
//! spreading the hash exists to produce. Animation-phase state and
//! Papyrus VM locals are out of scope for the reasons §4 records (the
//! latter is already owned by the real save registry).

use byroredux_core::ecs::components::{FormIdComponent, Transform};
use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::World;
use byroredux_core::form_id::FormIdPool;
use byroredux_core::math::Vec3;
use std::collections::HashMap;

use crate::components::AmbientPackageRuntime;
use byroredux_core::ecs::components::sandbox::{Seated, SeatedAnimationRestore};
use byroredux_core::ecs::components::travel::{TravelState, Traveled};

/// How far an actor must have moved from its authored REFR placement before
/// the snapshot bothers to restore a position.
///
/// §4 wants "only if it has diverged", so an actor that never moved costs
/// nothing — the common case for the overwhelming majority of REFRs. The
/// comparison happens at *restore* time rather than capture time, because
/// that is where the authored placement is available for free: the entity
/// has just been spawned at it.
///
/// One Bethesda unit is roughly 1.4 cm, so 8 units is a few centimetres of
/// slack — well inside "did not meaningfully move" and well outside float
/// noise in the transform chain.
pub(crate) const POSITION_DIVERGENCE_EPSILON: f32 = 8.0;

/// One actor's carried-over state. Every reference is a FormID, never an
/// `EntityId` — see [`StreamStateSnapshots`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ActorStreamSnapshot {
    /// Where the actor actually was when its tile was evicted.
    pub(crate) position: Vec3,
    /// `AmbientPackageRuntime.active_package_form_id` — a FormID, so
    /// trivially safe to carry.
    pub(crate) active_package_form_id: Option<u32>,
    /// `TravelState.destination`, resolved once and frozen by
    /// `travel_system`, so restoring it verbatim resumes rather than
    /// re-picks.
    pub(crate) travel_destination: Option<Vec3>,
    /// Whether the actor had already reached its Travel destination.
    pub(crate) traveled: bool,
    /// `Seated.furniture` as the furniture's **global FormID**.
    ///
    /// `EntityId` allocation is monotonic and never recycled (#372), so a
    /// respawned furniture entity gets a new id and a raw `EntityId`
    /// snapshot would name a dead one — or, worse, a live but *different*
    /// entity. This is the concrete reason the store cannot be
    /// "serialize the component, deserialize it back".
    pub(crate) seated_furniture_form_id: Option<u32>,
    /// `Seated.animation_restore` — the five plain scalars `sandbox_seat_system`
    /// captured before parking the actor in its seat (#3333).
    ///
    /// Carried because a respawned actor comes back with **no** `Seated` at
    /// all, so the restore has to *reconstruct* the component rather than
    /// correct one. Rebuilding it with a default `animation_restore` would
    /// leave the actor's eventual un-seat restoring a pose it never had —
    /// the exact frozen-in-a-chair failure #3333 fixed. No `EntityId` in
    /// here, so it carries verbatim.
    pub(crate) seated_animation_restore: Option<SeatedAnimationRestore>,
}

impl ActorStreamSnapshot {
    /// Whether this snapshot carries anything worth restoring beyond the
    /// position. Used to skip storing rows for actors that merely exist.
    fn has_package_state(&self) -> bool {
        self.active_package_form_id.is_some()
            || self.travel_destination.is_some()
            || self.traveled
            || self.seated_furniture_form_id.is_some()
    }
}

/// FormID-keyed store of actor state captured at tile eviction.
///
/// Bounded per §5: cleared wholesale on `drain_streaming_state`, the single
/// choke point every exterior teardown funnels through. A snapshot is only
/// ever meaningful for "the player might walk back to this exact tile in
/// this exact worldspace session", and a drain is precisely the moment that
/// stops being true — so a TTL would be a more complicated way to express a
/// weaker rule.
#[derive(Debug, Default)]
pub(crate) struct StreamStateSnapshots {
    by_form_id: HashMap<u32, ActorStreamSnapshot>,
}

impl Resource for StreamStateSnapshots {}

impl StreamStateSnapshots {
    pub(crate) fn len(&self) -> usize {
        self.by_form_id.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.by_form_id.is_empty()
    }

    /// Non-consuming read. Test-only: production restores go through
    /// [`Self::take`], because a row that has been claimed must not be
    /// re-applied on a later visit to the same tile.
    #[cfg(test)]
    pub(crate) fn get(&self, form_id: u32) -> Option<&ActorStreamSnapshot> {
        self.by_form_id.get(&form_id)
    }

    pub(crate) fn insert(&mut self, form_id: u32, snapshot: ActorStreamSnapshot) {
        self.by_form_id.insert(form_id, snapshot);
    }

    /// Consume the row for `form_id`, if any. Restore is one-shot: a tile
    /// that comes back twice without the actor moving again should not
    /// re-apply a stale position on the second visit.
    pub(crate) fn take(&mut self, form_id: u32) -> Option<ActorStreamSnapshot> {
        self.by_form_id.remove(&form_id)
    }

    /// §5's bound. Called from `drain_streaming_state`.
    pub(crate) fn clear(&mut self) {
        self.by_form_id.clear();
    }
}

/// The global FormID an entity's `FormIdComponent` resolves to, matching
/// the key space `resolve_entity_by_global_form_id` searches.
fn global_form_id(world: &World, entity: EntityId) -> Option<u32> {
    let pool = world.try_resource::<FormIdPool>()?;
    let fid = world.get::<FormIdComponent>(entity)?;
    pool.resolve(fid.0).map(|pair| pair.local.0)
}

/// Capture the keep-set state of every victim that has any, keyed by FormID.
///
/// Called from `unload_cell_inner` before the despawn. Actors with nothing
/// worth carrying — which is nearly all of them — produce no row at all, so
/// the store stays proportional to actors that actually accumulated state
/// rather than to resident population.
pub(crate) fn capture_actor_snapshots(world: &mut World, victims: &[EntityId]) {
    if victims.is_empty() || world.try_resource::<StreamStateSnapshots>().is_none() {
        return;
    }
    let mut captured: Vec<(u32, ActorStreamSnapshot)> = Vec::new();
    for &victim in victims {
        let Some(form_id) = global_form_id(world, victim) else {
            continue;
        };
        let Some(position) = world.get::<Transform>(victim).map(|t| t.translation) else {
            continue;
        };
        let seated = world.get::<Seated>(victim).map(|seated| *seated);
        let seated_furniture_form_id =
            seated.and_then(|seated| global_form_id(world, seated.furniture));
        let seated_animation_restore = seated_furniture_form_id
            .and(seated)
            .map(|seated| seated.animation_restore);
        let snapshot = ActorStreamSnapshot {
            position,
            active_package_form_id: world
                .get::<AmbientPackageRuntime>(victim)
                .and_then(|runtime| runtime.active_package_form_id),
            travel_destination: world
                .get::<TravelState>(victim)
                .map(|state| state.destination),
            traveled: world.get::<Traveled>(victim).is_some(),
            seated_furniture_form_id,
            seated_animation_restore,
        };
        if snapshot.has_package_state() {
            captured.push((form_id, snapshot));
        }
    }
    if captured.is_empty() {
        return;
    }
    let carried = captured.len();
    let mut store = world.resource_mut::<StreamStateSnapshots>();
    for (form_id, snapshot) in captured {
        store.insert(form_id, snapshot);
    }
    if !store.is_empty() {
        log::debug!(
            "stream-snapshot: carried {carried} actor state row(s) across a tile eviction \
             ({} parked in total)",
            store.len()
        );
    }
}

/// Re-apply a captured snapshot to a freshly respawned actor.
///
/// Called immediately after the spawn path installs fresh package state, so
/// this deliberately *overwrites* what `apply_ai_package_behavior` just
/// chose. That order matters: the fresh attach establishes the components
/// (and their storages), and the restore corrects the ones that had
/// progressed.
///
/// `authored_position` is where the REFR placement just put the actor.
/// Position is restored only when the snapshot diverges from it by more
/// than [`POSITION_DIVERGENCE_EPSILON`], so an actor that never moved is
/// left exactly where the authored data says — no float drift accumulating
/// across repeated tile round trips.
///
/// Returns whether anything was restored, for the caller's telemetry and
/// for tests.
pub(crate) fn restore_actor_snapshot(
    world: &mut World,
    entity: EntityId,
    form_id: u32,
    authored_position: Vec3,
) -> bool {
    let Some(snapshot) = world
        .try_resource_mut::<StreamStateSnapshots>()
        .and_then(|mut store| store.take(form_id))
    else {
        return false;
    };

    let mut restored = false;

    if snapshot.position.distance(authored_position) > POSITION_DIVERGENCE_EPSILON {
        if let Some(mut transforms) = world.query_mut::<Transform>() {
            if let Some(transform) = transforms.get_mut(entity) {
                transform.translation = snapshot.position;
                restored = true;
            }
        }
    }

    if let Some(active) = snapshot.active_package_form_id {
        if let Some(mut runtimes) = world.query_mut::<AmbientPackageRuntime>() {
            if let Some(runtime) = runtimes.get_mut(entity) {
                runtime.active_package_form_id = Some(active);
                restored = true;
            }
        }
    }

    if let Some(destination) = snapshot.travel_destination {
        if let Some(mut travels) = world.query_mut::<TravelState>() {
            travels.insert(entity, TravelState { destination });
            restored = true;
        }
    }
    if snapshot.traveled {
        if let Some(mut done) = world.query_mut::<Traveled>() {
            done.insert(entity, Traveled);
            restored = true;
        }
    }

    if let Some(furniture_form_id) = snapshot.seated_furniture_form_id {
        // Re-resolve by FormID, never by the stored `EntityId` — the
        // furniture's respawned entity has a *different* id, and ids are
        // never recycled, so a verbatim restore would name a dead or wrong
        // entity (#372).
        //
        // An unresolvable furniture means the seat's own tile is not
        // resident. Dropping the `Seated` restore is the correct outcome
        // there: re-seating is what `sandbox_seat_system` does when the
        // furniture comes back, and a `Seated` pointing at nothing would be
        // a one-shot terminal marker gating an actor forever.
        if let Some(furniture) = byroredux_scripting::condition::resolve_entity_by_global_form_id(
            world,
            furniture_form_id,
        ) {
            // Reconstructed, not corrected: the respawn path re-seats
            // nothing, so the actor arrives with no `Seated` component at
            // all and there is nothing here to merely fix up.
            let animation_restore = snapshot.seated_animation_restore.unwrap_or_default();
            if let Some(mut seats) = world.query_mut::<Seated>() {
                seats.insert(
                    entity,
                    Seated {
                        furniture,
                        animation_restore,
                    },
                );
                restored = true;
            }
        }
    }

    restored
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::components::sandbox::SeatedAnimationRestore;
    use byroredux_core::ecs::components::wander::{WanderPhase, WanderState};
    use byroredux_core::form_id::{FormIdPair, LocalFormId, PluginId};
    use byroredux_core::math::Quat;

    fn spawn_with_form_id(world: &mut World, raw: u32) -> EntityId {
        world.register::<FormIdComponent>();
        let mut pool = world.remove_resource::<FormIdPool>().unwrap_or_default();
        let fid = pool.intern(FormIdPair {
            plugin: PluginId::from_filename("Skyrim.esm"),
            local: LocalFormId(raw),
        });
        world.insert_resource(pool);
        let entity = world.spawn();
        world.insert(entity, FormIdComponent(fid));
        entity
    }

    fn fixture() -> World {
        let mut world = World::new();
        world.insert_resource(StreamStateSnapshots::default());
        world.register::<Transform>();
        world.register::<AmbientPackageRuntime>();
        world.register::<TravelState>();
        world.register::<Traveled>();
        world.register::<Seated>();
        world.register::<WanderState>();
        world
    }

    fn at(world: &mut World, entity: EntityId, position: Vec3) {
        world.insert(
            entity,
            Transform {
                translation: position,
                rotation: Quat::IDENTITY,
                scale: 1.0,
            },
        );
    }

    /// Acceptance 1 — a Travel package in progress survives an ordinary
    /// radius-streaming eviction.
    ///
    /// The `Traveled` half is the visibly-wrong symptom the mechanism
    /// exists for: an actor that already finished its errand walks the
    /// whole route again on every tile round trip, because respawn puts it
    /// back at the authored REFR position with no marker.
    #[test]
    fn travel_progress_survives_an_eviction_and_respawn_cycle() {
        const ACTOR: u32 = 0x0001_9001;
        let destination = Vec3::new(4000.0, 0.0, 250.0);
        let walked_to = Vec3::new(3900.0, 0.0, 250.0);
        let authored = Vec3::new(100.0, 0.0, 250.0);

        let mut world = fixture();
        let actor = spawn_with_form_id(&mut world, ACTOR);
        at(&mut world, actor, walked_to);
        world.insert(actor, TravelState { destination });
        world.insert(actor, Traveled);

        capture_actor_snapshots(&mut world, &[actor]);
        assert_eq!(world.resource::<StreamStateSnapshots>().len(), 1);

        // The despawn/respawn: a NEW entity, as a real reload produces —
        // entity ids are monotonic and never recycled (#372).
        world.despawn(actor);
        let respawned = spawn_with_form_id(&mut world, ACTOR);
        at(&mut world, respawned, authored);
        assert_ne!(respawned, actor);

        assert!(restore_actor_snapshot(
            &mut world, respawned, ACTOR, authored
        ));

        assert_eq!(
            world.get::<TravelState>(respawned).map(|s| s.destination),
            Some(destination),
            "the frozen destination must resume, not be re-picked"
        );
        assert!(
            world.get::<Traveled>(respawned).is_some(),
            "a completed errand must not un-complete itself"
        );
        assert_eq!(
            world.get::<Transform>(respawned).map(|t| t.translation),
            Some(walked_to),
            "the actor must stay where it walked to, not snap back to the \
             authored REFR placement"
        );
    }

    /// Acceptance 2 — `Seated` survives, with the furniture re-resolved by
    /// FormID.
    ///
    /// The stored `EntityId` is deliberately never reused: this test gives
    /// the respawned furniture a different id and asserts the restore lands
    /// on it anyway. Restoring the raw id would name a dead entity — or a
    /// live but wrong one, since ids are monotonic.
    #[test]
    fn seated_is_restored_through_the_furniture_form_id_not_the_stale_entity_id() {
        const ACTOR: u32 = 0x0001_9002;
        const CHAIR: u32 = 0x0001_9003;

        let mut world = fixture();
        let chair = spawn_with_form_id(&mut world, CHAIR);
        let actor = spawn_with_form_id(&mut world, ACTOR);
        at(&mut world, actor, Vec3::new(10.0, 0.0, 0.0));
        world.insert(
            actor,
            Seated {
                furniture: chair,
                animation_restore: SeatedAnimationRestore {
                    clip_handle: 7,
                    local_time: 0.5,
                    prev_time: 0.4,
                    playing: false,
                    speed: 1.0,
                },
            },
        );

        capture_actor_snapshots(&mut world, &[actor]);

        // The eviction itself. Despawning matters here, not just for
        // realism: `resolve_entity_by_global_form_id` returns the FIRST
        // entity matching a FormID, so a fixture that left the pre-eviction
        // pair alive would resolve back to the stale chair and this test
        // would "pass" against exactly the behaviour it exists to reject.
        world.despawn(actor);
        world.despawn(chair);

        // Respawn both, in an order that guarantees fresh ids for each.
        let respawned_chair = spawn_with_form_id(&mut world, CHAIR);
        let respawned_actor = spawn_with_form_id(&mut world, ACTOR);
        at(&mut world, respawned_actor, Vec3::new(10.0, 0.0, 0.0));
        // The spawn path re-seats nothing, so the actor comes back with NO
        // `Seated` at all. Pre-seating it here would let a restore that
        // merely kept whatever the component already named pass this test —
        // which is exactly the bug the FormID round trip exists to prevent.
        assert!(world.get::<Seated>(respawned_actor).is_none());
        assert_ne!(respawned_chair, chair);

        assert!(restore_actor_snapshot(
            &mut world,
            respawned_actor,
            ACTOR,
            Vec3::new(10.0, 0.0, 0.0)
        ));

        assert_eq!(
            world.get::<Seated>(respawned_actor).map(|s| s.furniture),
            Some(respawned_chair),
            "the seat must re-resolve to the furniture's NEW entity id"
        );
        assert_ne!(
            world.get::<Seated>(respawned_actor).map(|s| s.furniture),
            Some(chair),
            "the stale pre-eviction entity id must never be restored verbatim"
        );
        // #3333's pre-seat pose snapshot has to come back with the seat. A
        // reconstructed `Seated` carrying a default `animation_restore`
        // would leave the actor's eventual un-seat restoring a pose it never
        // had — frozen in a chair, which is the failure that field exists
        // to prevent.
        assert_eq!(
            world
                .get::<Seated>(respawned_actor)
                .map(|s| s.animation_restore),
            Some(SeatedAnimationRestore {
                clip_handle: 7,
                local_time: 0.5,
                prev_time: 0.4,
                playing: false,
                speed: 1.0,
            }),
            "the pre-seat animation snapshot must survive the round trip"
        );
    }

    /// Acceptance 3 — `WanderState` is deliberately NOT carried.
    ///
    /// `WanderBehavior`'s `form_id` feeds a deterministic desync hash, so
    /// re-rolling the phase on respawn is the *intended* behaviour — it is
    /// what keeps a crowd of respawned wanderers from stepping in lockstep.
    /// Restoring it would defeat the spreading the hash exists to produce.
    #[test]
    fn wander_state_is_deliberately_not_carried_across_the_boundary() {
        const ACTOR: u32 = 0x0001_9004;
        let mut world = fixture();
        let actor = spawn_with_form_id(&mut world, ACTOR);
        at(&mut world, actor, Vec3::new(500.0, 0.0, 0.0));
        world.insert(
            actor,
            WanderState {
                home: Vec3::ZERO,
                target: Vec3::new(700.0, 0.0, 0.0),
                phase: WanderPhase::Paused { remaining: 2.5 },
                pick_count: 4,
            },
        );
        // Wander alone is not "state worth keeping" — no row at all.
        capture_actor_snapshots(&mut world, &[actor]);
        assert!(
            world.resource::<StreamStateSnapshots>().is_empty(),
            "an actor carrying only WanderState must produce no snapshot row"
        );

        // ...and even alongside state that IS kept, wander is not in the row.
        world.insert(actor, Traveled);
        capture_actor_snapshots(&mut world, &[actor]);
        let store = world.resource::<StreamStateSnapshots>();
        let row = store.get(ACTOR).expect("Traveled must produce a row");
        assert!(row.traveled);
        // ...and the wander state itself is untouched on the way through:
        // capture reads it nowhere, so nothing can smuggle it into the row.
        assert_eq!(
            world.get::<WanderState>(actor).map(|w| w.pick_count),
            Some(4),
            "capture must not mutate wander state either"
        );
    }

    /// §4 — an actor that never moved costs nothing, and its authored
    /// placement is left exactly as the ESM says rather than overwritten
    /// with a float-drifted copy of itself on every tile round trip.
    #[test]
    fn a_position_within_the_divergence_epsilon_is_not_restored() {
        const ACTOR: u32 = 0x0001_9005;
        let authored = Vec3::new(1000.0, 0.0, 0.0);
        let nudged = authored + Vec3::new(POSITION_DIVERGENCE_EPSILON * 0.5, 0.0, 0.0);

        let mut world = fixture();
        let actor = spawn_with_form_id(&mut world, ACTOR);
        at(&mut world, actor, nudged);
        world.insert(actor, Traveled);
        capture_actor_snapshots(&mut world, &[actor]);

        let respawned = spawn_with_form_id(&mut world, ACTOR);
        at(&mut world, respawned, authored);
        restore_actor_snapshot(&mut world, respawned, ACTOR, authored);

        assert_eq!(
            world.get::<Transform>(respawned).map(|t| t.translation),
            Some(authored),
            "a sub-epsilon difference must leave the authored placement alone"
        );
        assert!(
            world.get::<Traveled>(respawned).is_some(),
            "...while the package state it came with is still restored"
        );
    }

    /// §5 — the store is bounded, and a restore is one-shot.
    #[test]
    fn the_store_is_cleared_and_a_row_is_consumed_by_its_restore() {
        const ACTOR: u32 = 0x0001_9006;
        let mut world = fixture();
        let actor = spawn_with_form_id(&mut world, ACTOR);
        at(&mut world, actor, Vec3::new(900.0, 0.0, 0.0));
        world.insert(actor, Traveled);
        capture_actor_snapshots(&mut world, &[actor]);
        assert_eq!(world.resource::<StreamStateSnapshots>().len(), 1);

        let respawned = spawn_with_form_id(&mut world, ACTOR);
        at(&mut world, respawned, Vec3::ZERO);
        restore_actor_snapshot(&mut world, respawned, ACTOR, Vec3::ZERO);
        assert!(
            world.resource::<StreamStateSnapshots>().is_empty(),
            "a restored row must be consumed — a second visit to the same \
             tile must not re-apply a stale position"
        );

        capture_actor_snapshots(&mut world, &[respawned]);
        assert_eq!(world.resource::<StreamStateSnapshots>().len(), 1);
        world.resource_mut::<StreamStateSnapshots>().clear();
        assert!(
            world.resource::<StreamStateSnapshots>().is_empty(),
            "drain_streaming_state's clear must empty the store"
        );
    }

    /// An actor with nothing accumulated produces no row, so the store
    /// stays proportional to actors that actually progressed rather than to
    /// resident population.
    #[test]
    fn an_actor_with_no_accumulated_state_produces_no_row() {
        const ACTOR: u32 = 0x0001_9007;
        let mut world = fixture();
        let actor = spawn_with_form_id(&mut world, ACTOR);
        at(&mut world, actor, Vec3::new(5.0, 0.0, 0.0));
        capture_actor_snapshots(&mut world, &[actor]);
        assert!(world.resource::<StreamStateSnapshots>().is_empty());
    }
}
