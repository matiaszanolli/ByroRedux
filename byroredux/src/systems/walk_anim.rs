//! NPC walk-cycle playback (M42.10) — the animation half of live ambient
//! locomotion. The six package procedures (and combat chase) move an
//! actor's root; this system watches each [`WalkAnimation`]-carrying
//! actor's per-tick XZ displacement and swaps its `AnimationPlayer`
//! between whatever it was playing and the game's authored walk clip,
//! restoring the previous playback state verbatim when the actor stops.
//!
//! The detector is deliberately *motion-based* rather than wired into the
//! six state machines: one system covers every mover that exists today
//! (Wander/Travel/Follow/Escort/Guard/Patrol + `npc_combat_ai_system`'s
//! chase) and every future one, with zero per-system plumbing, and a
//! blocked actor that isn't actually displacing gets its idle back
//! automatically.
//!
//! Take-over/restore protocol (per actor):
//!
//! - **Take** (first moving tick): snapshot the current `AnimationPlayer`
//!   (`WalkAnimSnapshot`) and install the walk clip at `local_time = 0`.
//!   If the actor has *no* player — the Skyrim+ shape, where ambient
//!   actors only gain one when an idle request plays — insert one bound to
//!   `HavokAnimationTarget::skeleton_root` and remember `captured = None`.
//! - **Restore** (first stationary tick): write the snapshot back verbatim,
//!   or remove the player we inserted when `captured` was `None`.
//! - **Yield** (an outside writer swapped the clip mid-walk): drop the
//!   walk state and leave the player untouched — from the instant the
//!   other writer swapped the clip it owns playback, and restoring *our*
//!   snapshot would stomp *theirs*. The captured pre-walk idle is
//!   deliberately lost along with it.
//! - **Abandon** (actor becomes seated/dead/cinematic mid-walk): drop the
//!   walk state *without touching the player* — the death/seat/cinematic
//!   path owns the pose from that instant, and a stale snapshot would
//!   clobber it on the next package handover.
//!
//! Movement here is still the locomotion systems' fixed-speed XZ step, so
//! the clip's authored stride can disagree slightly with 100 u/s — foot
//! sliding, not drift. Driving movement from the walk clip's
//! `RootMotionDelta` (produced every tick by the animation system for
//! accum-root clips) is the deferred polish pass.
//!
//! Registered `add_exclusive(Stage::PostUpdate, …)` **after** all six
//! locomotion procedures, so the position this system reads is already
//! this frame's final — a swap decided on stale positions would flicker
//! at leg boundaries. Actors are never both seated and walking
//! (a single winning package per NPC), so this never re-fights
//! `sandbox_seat_system`'s parked player.

use crate::components::{WalkAnimSnapshot, WalkAnimation};
use byroredux_core::animation::AnimationPlayer;
use byroredux_core::ecs::components::{Dead, Seated, Transform};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::Vec3;

/// Per-tick XZ displacement rate (world units/second) at or below which an
/// actor counts as stationary for clip purposes. The locomotion walk is
/// 100 u/s and one tick of it is far above this at any sane framerate;
/// the margin exists so a blocked-against-a-wall actor (KCC delivered
/// ~zero translation) reads as idle instead of grinding a walk cycle in
/// place.
const WALK_ANIM_MIN_SPEED: f32 = 8.0;

/// Hysteresis dwell windows (seconds). A take needs the actor to have been
/// moving for [`WALK_ANIM_TAKE_DWELL`]; a restore needs it continuously
/// stationary for [`WALK_ANIM_RELEASE_DWELL`]. The release window is the
/// longer one because the live-flicker mode is collision sliding *during*
/// a walk leg: single near-zero-displacement ticks must not cut the walk
/// clip (Camp McCarran Travel walkers, 2026-09-18 — 64 take/restore pairs
/// in ~70 s before the dwell windows landed).
const WALK_ANIM_TAKE_DWELL: f32 = 0.1;
const WALK_ANIM_RELEASE_DWELL: f32 = 0.4;

/// One actor's computed playback decision for this tick, applied in Pass 2
/// after all Pass-1 reads have dropped (the shared two-pass convention of
/// every exclusive system in this crate).
struct WalkAnimDecision {
    entity: EntityId,
    walk_handle: u32,
    walking: bool,
    last_pos: Vec3,
    captured: Option<WalkAnimSnapshot>,
    /// Current player state read in Pass 1 (`None` when the actor has no
    /// `AnimationPlayer` — the Skyrim+ standing shape).
    player: Option<WalkAnimSnapshot>,
    /// `HavokAnimationTarget::skeleton_root`, for inserting a player where
    /// none existed. `None` when there is no skeleton to bind — an actor
    /// with a walk handle but no skeleton can't be animated at all.
    skeleton_root: Option<EntityId>,
    /// Updated hysteresis accumulator to persist into the component.
    transition_secs: f32,
    /// Set in Pass 1 when a state change needs a player write in Pass 2.
    action: WalkAnimAction,
}

#[derive(Default)]
enum WalkAnimAction {
    #[default]
    None,
    /// Install the walk clip (capturing, or inserting a player).
    Take,
    /// Write the captured snapshot back into the player.
    Restore(WalkAnimSnapshot),
    /// Remove the player this system inserted for the actor.
    RemovePlayer,
}

/// Reusable per-frame scratch (mirrors `WanderScratch`'s #2033 shape —
/// the decisions backing allocation survives across frames).
#[derive(Default)]
struct WalkAnimScratch {
    decisions: Vec<WalkAnimDecision>,
}

fn npc_walk_animation_system_inner(world: &World, dt: f32, scratch: &mut WalkAnimScratch) {
    // ── Pass 1: read-only gather + decide. ──
    scratch.decisions.clear();
    {
        let Some(walk_q) = world.query::<WalkAnimation>() else {
            return;
        };
        let Some(transform_q) = world.query::<Transform>() else {
            return;
        };
        let player_q = world.query::<AnimationPlayer>();
        let havok_q = world.query::<crate::components::HavokAnimationTarget>();
        let seated_q = world.query::<Seated>();
        let dead_q = world.query::<Dead>();
        let cinematic_q = world.query::<byroredux_scripting::ActorCinematicState>();

        for (entity, walk) in walk_q.iter() {
            let Some(transform) = transform_q.get(entity) else {
                continue;
            };
            let player = player_q.as_ref().and_then(|q| q.get(entity)).map(|p| {
                WalkAnimSnapshot {
                    clip_handle: p.clip_handle,
                    local_time: p.local_time,
                    prev_time: p.prev_time,
                    speed: p.speed,
                    playing: p.playing,
                }
            });
            let skeleton_root =
                havok_q
                    .as_ref()
                    .and_then(|q| q.get(entity))
                    .map(|t| t.skeleton_root);
            let pose_owned_elsewhere = seated_q.as_ref().is_some_and(|q| q.contains(entity))
                || dead_q.as_ref().is_some_and(|q| q.contains(entity))
                || cinematic_q.as_ref().is_some_and(|q| q.contains(entity));

            // Motion classification. Guard the division: a first tick after
            // a long hitch must not read as teleport-speed walking.
            let moved = Vec3::new(
                transform.translation.x - walk.last_pos.x,
                0.0,
                transform.translation.z - walk.last_pos.z,
            );
            let speed = if dt > f32::MIN_POSITIVE && dt < 0.25 {
                moved.length() / dt
            } else {
                0.0
            };
            let moving = speed > WALK_ANIM_MIN_SPEED;

            let mut decision = WalkAnimDecision {
                entity,
                walk_handle: walk.walk_handle,
                walking: walk.walking,
                last_pos: transform.translation,
                captured: walk.captured,
                player,
                skeleton_root,
                transition_secs: walk.transition_secs,
                action: WalkAnimAction::None,
            };

            if walk.walking {
                if pose_owned_elsewhere {
                    // Abandon: seat/death/cinematic owns the pose from now
                    // on. Drop the snapshot — restoring it would clobber
                    // whatever that path parked on the player.
                    decision.walking = false;
                    decision.captured = None;
                    decision.transition_secs = 0.0;
                } else if !moving {
                    // Release hysteresis: hold the walk clip through brief
                    // stationary ticks (blocked slides, waypoint turns);
                    // restore only after a sustained stop.
                    let secs = walk.transition_secs + dt;
                    if secs >= WALK_ANIM_RELEASE_DWELL {
                        match walk.captured {
                            Some(snapshot) => decision.action = WalkAnimAction::Restore(snapshot),
                            None => decision.action = WalkAnimAction::RemovePlayer,
                        }
                        decision.walking = false;
                        decision.transition_secs = 0.0;
                    } else {
                        decision.transition_secs = secs;
                    }
                } else if player.as_ref().is_some_and(|p| p.clip_handle != walk.walk_handle) {
                    // Yield: someone else took playback over mid-walk
                    // (sandbox re-seat, a cinematic idle request). From the
                    // instant they swapped the clip they own playback —
                    // restoring *our* snapshot here would stomp *their*
                    // clip (the exact bug the yield test pins), so we only
                    // drop our state and leave the player untouched. The
                    // pre-walk idle that `captured` held is lost; the
                    // outside writer is responsible for what plays next.
                    decision.action = WalkAnimAction::None;
                    decision.walking = false;
                    decision.captured = None;
                    decision.transition_secs = 0.0;
                } else {
                    // Still walking with the walk clip installed — reset
                    // the release accumulator.
                    decision.transition_secs = 0.0;
                }
            } else if moving && !pose_owned_elsewhere {
                if player.as_ref().is_some_and(|p| p.clip_handle == walk.walk_handle) {
                    // Playback is already on the walk clip (a save restored
                    // mid-walk, or a previous take lost its component
                    // state): adopt it without touching the player, and
                    // with no capture — there is no pre-walk pose to go
                    // back to beyond "whatever is playing now", which is
                    // the walk itself.
                    decision.walking = true;
                    decision.captured = None;
                    decision.transition_secs = 0.0;
                } else {
                    // Take hysteresis: a single fast tick (a slide after
                    // being stuck) must not snap the clip on. Require a
                    // short sustained movement first; the accumulator
                    // persists across ticks in the component.
                    let secs = walk.transition_secs + dt;
                    if secs >= WALK_ANIM_TAKE_DWELL {
                        decision.action = WalkAnimAction::Take;
                        decision.walking = true;
                        decision.captured = player;
                        decision.transition_secs = 0.0;
                    } else {
                        decision.transition_secs = secs;
                    }
                }
            } else if !pose_owned_elsewhere {
                // Stationary and not walking — decay the take accumulator
                // so two fast ticks 10 s apart don't add up.
                decision.transition_secs = 0.0;
            }

            scratch.decisions.push(decision);
        }
    }
    if scratch.decisions.is_empty() {
        return;
    }

    // ── Pass 2: apply. Each write acquires its storage alone. ──
    if let Some(mut pq) = world.query_mut::<AnimationPlayer>() {
        for d in &scratch.decisions {
            match d.action {
                WalkAnimAction::None => {}
                WalkAnimAction::Take => log::debug!(
                    "walk anim: entity {} take clip {} (captured {:?})",
                    d.entity,
                    d.walk_handle,
                    d.captured.map(|c| c.clip_handle),
                ),
                WalkAnimAction::Restore(snapshot) => log::debug!(
                    "walk anim: entity {} restore clip {}",
                    d.entity, snapshot.clip_handle
                ),
                WalkAnimAction::RemovePlayer => log::debug!(
                    "walk anim: entity {} remove inserted player",
                    d.entity
                ),
            }
            match d.action {
                WalkAnimAction::None => {}
                WalkAnimAction::Take => {
                    let Some(current) = d.player else {
                        // No player before the walk: insert one bound to the
                        // actor's skeleton (Skyrim+ ambient shape). An actor
                        // with neither player nor skeleton can't animate;
                        // the component's walking flag still flips, and the
                        // next stationary tick restores to `RemovePlayer` —
                        // a no-op removal on a player that never existed.
                        if let Some(root) = d.skeleton_root {
                            pq.insert(d.entity, AnimationPlayer::new(d.walk_handle).with_root(root));
                        }
                        continue;
                    };
                    let _ = current; // capture already recorded in Pass 1
                    if let Some(p) = pq.get_mut(d.entity) {
                        p.clip_handle = d.walk_handle;
                        p.local_time = 0.0;
                        p.prev_time = 0.0;
                        p.speed = 1.0;
                        p.playing = true;
                    }
                }
                WalkAnimAction::Restore(snapshot) => {
                    if let Some(p) = pq.get_mut(d.entity) {
                        p.clip_handle = snapshot.clip_handle;
                        p.local_time = snapshot.local_time;
                        p.prev_time = snapshot.prev_time;
                        p.speed = snapshot.speed;
                        p.playing = snapshot.playing;
                    }
                }
                WalkAnimAction::RemovePlayer => {
                    pq.remove(d.entity);
                }
            }
        }
    }
    if let Some(mut wq) = world.query_mut::<WalkAnimation>() {
        for d in &scratch.decisions {
            // `captured` for a Take decided this tick must persist into the
            // component — Pass 1 recorded it from the pre-walk player read.
            if let Some(existing) = wq.get_mut(d.entity) {
                existing.walking = d.walking;
                existing.last_pos = d.last_pos;
                existing.captured = d.captured;
                existing.transition_secs = d.transition_secs;
            }
        }
    }
}

/// Walk-animation system factory — returns a closure with a persistent
/// [`WalkAnimScratch`] (mirrors [`super::wander::make_wander_system`]'s
/// #2033 shape). Wire with `add_exclusive(Stage::PostUpdate, …)` after the
/// locomotion procedures.
pub(crate) fn make_npc_walk_animation_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = WalkAnimScratch::default();
    move |world: &World, dt: f32| {
        npc_walk_animation_system_inner(world, dt, &mut scratch);
    }
}

/// Kept for test ergonomics, mirroring `wander_system`'s `#[cfg(test)]`
/// twin. Production code uses [`make_npc_walk_animation_system`].
#[cfg(test)]
pub(crate) fn npc_walk_animation_system(world: &World, dt: f32) {
    npc_walk_animation_system_inner(world, dt, &mut WalkAnimScratch::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::HavokAnimationTarget;

    fn walk_component(handle: u32, last_pos: Vec3) -> WalkAnimation {
        WalkAnimation {
            walk_handle: handle,
            walking: false,
            last_pos,
            captured: None,
            transition_secs: 0.0,
        }
    }

    /// Drive the system `n` ticks at 60 Hz. When `velocity_dir` is `Some`,
    /// the actor advances 3 units per tick along it (180 u/s — well above
    /// the take threshold, mirroring a locomotion walk); otherwise the
    /// actor stays put.
    fn run_ticks(world: &World, entity: EntityId, n: usize, velocity_dir: Option<Vec3>) {
        let mut pos = {
            let tq = world.query::<Transform>().unwrap();
            tq.get(entity).unwrap().translation
        };
        for _ in 0..n {
            if let Some(dir) = velocity_dir {
                pos += dir;
                if let Some(mut tq) = world.query_mut::<Transform>() {
                    if let Some(t) = tq.get_mut(entity) {
                        t.translation = pos;
                    }
                }
            }
            npc_walk_animation_system(world, 1.0 / 60.0);
        }
    }

    fn register(world: &mut World) {
        world.register::<WalkAnimation>();
        world.register::<Transform>();
        world.register::<AnimationPlayer>();
        world.register::<HavokAnimationTarget>();
        world.register::<Seated>();
        world.register::<Dead>();
        world.register::<byroredux_scripting::ActorCinematicState>();
    }

    fn player_state(world: &World, entity: EntityId) -> Option<(u32, f32, f32, f32, bool)> {
        world.query::<AnimationPlayer>().and_then(|q| {
            q.get(entity)
                .map(|p| (p.clip_handle, p.local_time, p.prev_time, p.speed, p.playing))
        })
    }

    #[test]
    fn moving_actor_swaps_to_walk_clip_and_back() {
        let mut world = World::new();
        register(&mut world);

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)));
        let mut player = AnimationPlayer::new(7); // the spawn idle
        player.local_time = 1.25;
        player.prev_time = 1.0;
        player.speed = 1.05;
        world.insert(entity, player);
        world.insert(entity, walk_component(9, Vec3::new(0.0, 0.0, 0.0)));

        // Walk 3 units per tick (well above WALK_ANIM_MIN_SPEED at dt) for
        // long enough to cross the take dwell window.
        run_ticks(&world, entity, 8, Some(Vec3::new(1.0, 0.0, 0.0)));

        let (clip, local, _prev, speed, playing) =
            player_state(&world, entity).expect("player still present");
        assert_eq!(clip, 9, "the walk clip must be installed over the idle");
        assert_eq!(local, 0.0);
        assert!(playing);
        assert_eq!(speed, 1.0);

        // The component recorded the capture + walking flag.
        {
            let wq = world.query::<WalkAnimation>().unwrap();
            let walk = wq.get(entity).unwrap();
            assert!(walk.walking);
            let captured = walk.captured.expect("pre-walk idle captured");
            assert_eq!(captured.clip_handle, 7);
            assert_eq!(captured.local_time, 1.25);
            assert_eq!(captured.speed, 1.05);
        }

        // Stop: after the release dwell window, restore the idle verbatim.
        run_ticks(&world, entity, 30, None);
        let (clip, local, prev, speed, playing) =
            player_state(&world, entity).expect("player restored, not removed");
        assert_eq!(clip, 7);
        assert_eq!(local, 1.25);
        assert_eq!(prev, 1.0);
        assert_eq!(speed, 1.05);
        assert!(playing);
        let wq = world.query::<WalkAnimation>().unwrap();
        assert!(!wq.get(entity).unwrap().walking);
        drop(wq);
    }

    #[test]
    fn skyrim_style_actor_gains_then_loses_a_player() {
        let mut world = World::new();
        register(&mut world);

        let skeleton = world.spawn();
        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::ZERO));
        world.insert(
            entity,
            HavokAnimationTarget {
                skeleton_root: skeleton,
                consumed_idle_serial: 0,
            },
        );
        world.insert(entity, walk_component(9, Vec3::ZERO));

        // Move → a player is created on the walk clip.
        run_ticks(&world, entity, 8, Some(Vec3::new(1.0, 0.0, 0.0)));
        let (clip, _, _, _, _) = player_state(&world, entity).expect("player inserted");
        assert_eq!(clip, 9);
        {
            let pq = world.query::<AnimationPlayer>().unwrap();
            assert_eq!(pq.get(entity).unwrap().root_entity, Some(skeleton));
        }

        // Stop → the inserted player is removed again (pre-walk shape).
        run_ticks(&world, entity, 30, None);
        assert!(
            player_state(&world, entity).is_none(),
            "a player this system inserted must be removed on stop"
        );
    }

    #[test]
    fn seated_actor_is_never_taken_over_and_abandons_mid_walk() {
        let mut world = World::new();
        register(&mut world);

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::ZERO));
        let mut player = AnimationPlayer::new(7);
        player.playing = false; // the sandbox "parked" shape
        world.insert(entity, player);
        world.insert(
            entity,
            Seated {
                furniture: entity,
                animation_restore: byroredux_core::ecs::components::SeatedAnimationRestore {
                    clip_handle: 7,
                    local_time: 0.0,
                    prev_time: 0.0,
                    playing: false,
                    speed: 1.0,
                },
            },
        );
        world.insert(entity, walk_component(9, Vec3::ZERO));

        // Parked actor isn't moving; system must not touch the player.
        npc_walk_animation_system(&world, 1.0 / 60.0);
        let (clip, _, _, _, playing) = player_state(&world, entity).unwrap();
        assert_eq!(clip, 7);
        assert!(!playing);

        // Actor moving while seated → take must stay blocked by Seated.
        run_ticks(&world, entity, 8, Some(Vec3::new(1.0, 0.0, 0.0)));
        let wq = world.query::<WalkAnimation>().unwrap();
        assert!(
            !wq.get(entity).unwrap().walking,
            "Seated must block taking playback"
        );
        drop(wq);
    }

    #[test]
    fn yielding_when_another_writer_swapped_the_clip_mid_walk() {
        let mut world = World::new();
        register(&mut world);

        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::new(0.0, 0.0, 0.0)));
        let mut player = AnimationPlayer::new(7);
        player.local_time = 0.5;
        world.insert(entity, player);
        world.insert(entity, walk_component(9, Vec3::new(0.0, 0.0, 0.0)));

        // Take the walk.
        run_ticks(&world, entity, 8, Some(Vec3::new(1.0, 0.0, 0.0)));
        assert_eq!(player_state(&world, entity).unwrap().0, 9);

        // An outside writer (e.g. a one-shot script idle) replaces the
        // player wholesale while the actor keeps moving.
        if let Some(mut pq) = world.query_mut::<AnimationPlayer>() {
            if let Some(p) = pq.get_mut(entity) {
                *p = AnimationPlayer::new(11);
            }
        }
        // The very next tick must yield: the foreign write survives
        // untouched, and the walk state (including its stale capture) is
        // dropped rather than restored over it.
        if let Some(mut tq) = world.query_mut::<Transform>() {
            if let Some(t) = tq.get_mut(entity) {
                t.translation += Vec3::new(1.0, 0.0, 0.0);
            }
        }
        npc_walk_animation_system(&world, 1.0 / 60.0);
        let (clip, local, _, _, _) = player_state(&world, entity).unwrap();
        assert_eq!(clip, 11, "the outside writer's clip must survive the yield");
        assert_eq!(local, 0.0);
        {
            let wq = world.query::<WalkAnimation>().unwrap();
            assert!(!wq.get(entity).unwrap().walking);
            assert!(
                wq.get(entity).unwrap().captured.is_none(),
                "abandoned capture must not be reused after a yield"
            );
        }
        // If the actor genuinely keeps moving afterwards, the walk clip
        // legitimately resumes once the take dwell elapses — the yield is
        // a one-tick courtesy, not a permanent handover.
        run_ticks(&world, entity, 8, Some(Vec3::new(1.0, 0.0, 0.0)));
        let (clip, _, _, _, _) = player_state(&world, entity).unwrap();
        assert_eq!(clip, 9, "walking must resume after the yield once the actor keeps moving");
    }

    #[test]
    fn scratch_reuses_allocation_across_frames() {
        let mut world = World::new();
        register(&mut world);
        let entity = world.spawn();
        world.insert(entity, Transform::from_translation(Vec3::ZERO));
        world.insert(entity, walk_component(9, Vec3::ZERO));

        let mut scratch = WalkAnimScratch::default();
        npc_walk_animation_system_inner(&world, 1.0 / 60.0, &mut scratch);
        let cap = scratch.decisions.capacity();
        assert!(cap > 0);
        npc_walk_animation_system_inner(&world, 1.0 / 60.0, &mut scratch);
        assert_eq!(scratch.decisions.capacity(), cap);
    }
}
