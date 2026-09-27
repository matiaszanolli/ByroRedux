//! P2 combat tail — combat feedback: one-shot clip takes (attack / hit /
//! death) for Draugr-family actors plus the spatial combat sound family.
//!
//! Asset family pinned by `docs/engine/p2-combat-anim-sound-fixture.md`;
//! the clips arrive pre-decoded in [`crate::components::DraugrCombatClips`]
//! (populated beside the walk clip), the sounds are decoded lazily from
//! `--sounds-bsa` archives on first use and cached in the shared `SoundCache`.
//!
//! Event edges consumed (all produced upstream in the same frame):
//!
//! * `HitEvent` (transient, scripting-Late cleanup) → impact one-shot at
//!   every marked target, independently of whether an animation take is
//!   active. A marked aggressor starts its attack take and swing sound at
//!   its own position.
//! * `Dead` → death take + death voice, latched once via
//!   `DraugrCombatAnim.death_played`. Death keeps no snapshot — the `Dead`
//!   marker owns the pose from there, and walk_anim's abandon rule already
//!   cedes playback to it. #4708: a death this system never saw happen (a
//!   corpse restored dead) latches silently, and a ragdolled corpse gets
//!   the voice only — never a take sampled over the physics pose.
//!
//! Take protocol mirrors `walk_anim`'s take/restore: capture the pre-take
//! `AnimationPlayer` (inserting one bound to `AnimationTarget::
//! skeleton_root` when absent), swap the clip, restore verbatim when the
//! take's duration expires. A mid-walk take relies on walk_anim's existing
//! yield rule (someone else swapped the clip → walk loses ownership
//! without stomping).

use byroredux_core::animation::AnimationPlayer;
use byroredux_core::ecs::components::{Dead, GlobalTransform};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::Vec3;

use crate::components::{
    AnimationTarget, CombatTake, DraugrCombatAnim, DraugrCombatClips, WalkAnimSnapshot,
};

/// Sound paths pinned by the fixture doc (plain PCM WAV — the M44
/// symphonia path decodes them as-is). The `.fuz` draugr dialogue family
/// is deliberately out: no decoder, and the gate doesn't need it.
const SWING_SOUND_PATH: &str = r"sound\fx\wpn\swing\blade2hand\fx_swing_blade2hand_03.wav";
const IMPACT_SOUND_PATH: &str =
    r"sound\fx\wpn\impact\blade\2hand\fleshdraugr\wpn_impact_blade2hand_fleshdraugr_01.wav";
const DEATH_VOICE_PATH: &str = r"sound\fx\npc\draugr\death\npc_draugr_death_02.wav";

/// What the write pass does for one actor.
enum TakeAction {
    /// Install a take: swap (or insert) the actor's `AnimationPlayer` to
    /// `handle` and record it on `DraugrCombatAnim`. `secs == 0.0` marks
    /// the death take — it never expires and latches `death_played`.
    Install {
        kind: CombatTake,
        handle: u32,
        secs: f32,
        death: bool,
    },
    /// The take expired: write the captured snapshot back (or remove a
    /// player this system inserted), clear the take.
    Restore,
    /// Keep the take, persist the decremented remaining time.
    Tick { remaining: f32 },
    /// #4708 — latch the death without installing the take: a corpse the
    /// system never saw alive, or one whose ragdoll already owns the pose.
    LatchDeath,
    /// #4708 — first observation of a live actor with nothing to play;
    /// records `seen_alive` so its later death is a real one.
    SeenAlive,
}

struct FeedbackDecision {
    actor: EntityId,
    action: TakeAction,
    /// Pre-take player snapshot + skeleton root, read before any write.
    snapshot: Option<WalkAnimSnapshot>,
    skeleton_root: Option<EntityId>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FeedbackSound {
    Swing,
    Impact,
    DeathVoice,
}

/// Closure-persistent buffers reused by the feedback system.
#[derive(Default)]
struct FeedbackScratch {
    decisions: Vec<FeedbackDecision>,
    sound_events: Vec<(EntityId, FeedbackSound)>,
    /// This tick's `(target, aggressor)` HitEvent snapshot (#4613).
    hit_events: Vec<(EntityId, EntityId)>,
}

fn combat_feedback_system_inner(world: &World, dt: f32, scratch: &mut FeedbackScratch) {
    // #4605 — take the Copy in a scoped expression so the resource guard
    // dies at the copy (the #3444 rule): holding it through the read,
    // write and sound passes below recorded a spurious
    // DraugrCombatClips-first edge against every storage this system
    // touches.
    let Some(clips) = world
        .try_resource::<DraugrCombatClips>()
        .map(|clips| *clips)
    else {
        return;
    };

    // ── Read pass: events + per-actor state → buffered decisions. Every
    // query here is read-only, so all borrows drop before the write pass.
    scratch.decisions.clear();
    scratch.sound_events.clear();
    {
        // #4613 — refilled in place; a fresh `collect` allocated every frame
        // of an active fight.
        scratch.hit_events.clear();
        if let Some(events) = world.query::<byroredux_scripting::HitEvent>() {
            scratch.hit_events.extend(
                events
                    .iter()
                    .map(|(entity, event)| (entity, event.aggressor)),
            );
        }
        let hit_events = &scratch.hit_events;

        if let Some(anim_q) = world.query::<DraugrCombatAnim>() {
            for (actor, state) in anim_q.iter() {
                // Sound follows transient gameplay events, not animation-slot
                // availability. Keep each event, including repeated hits on one
                // target, even if the actor is already in a take or dies below.
                for _ in hit_events.iter().filter(|&&(target, _)| target == actor) {
                    scratch.sound_events.push((actor, FeedbackSound::Impact));
                }
                let dead = world.get::<Dead>(actor).is_some();

                // 1. Death — terminal, latched, checked first so the killing
                //    blow produces the death take, never a hit take.
                if dead {
                    if !state.death_played {
                        let skeleton_root = read_skeleton_root(world, actor);
                        // #4708 — (b) a corpse restored dead (revisit, save
                        // load) died in some earlier session: latch, replay
                        // nothing. (a) The killing blow's `reconcile_dead_actor`
                        // already removed the actor's players and handed the
                        // skeleton to physics; a take re-inserted now would
                        // sample over the ragdoll — the voice alone plays.
                        let replayed = !state.seen_alive;
                        let ragdolled = skeleton_root.is_some_and(|root| {
                            world.get::<crate::ragdoll::RagdollActive>(root).is_some()
                        });
                        if !replayed {
                            scratch
                                .sound_events
                                .push((actor, FeedbackSound::DeathVoice));
                        }
                        scratch.decisions.push(FeedbackDecision {
                            actor,
                            action: if replayed || ragdolled {
                                TakeAction::LatchDeath
                            } else {
                                TakeAction::Install {
                                    kind: CombatTake::Hit,
                                    handle: clips.death,
                                    secs: 0.0,
                                    death: true,
                                }
                            },
                            snapshot: read_player_snapshot(world, actor),
                            skeleton_root,
                        });
                    }
                    continue;
                }

                // 2. An active take ticks down (hit reactions only — death
                //    never reaches here with an active take because death
                //    latches and takes aren't persisted into it).
                if let Some(kind) = state.take {
                    let remaining = state.take_remaining - dt;
                    if remaining <= 0.0 {
                        scratch.decisions.push(FeedbackDecision {
                            actor,
                            action: TakeAction::Restore,
                            snapshot: state.captured,
                            skeleton_root: None,
                        });
                    } else {
                        scratch.decisions.push(FeedbackDecision {
                            actor,
                            action: TakeAction::Tick { remaining },
                            snapshot: None,
                            skeleton_root: None,
                        });
                    }
                    let _ = kind;
                    continue;
                }

                // 3. Hit reaction — animation takes still avoid replacing an
                //    active take; impact audio was already queued above.
                if let Some(&(_, aggressor)) =
                    hit_events.iter().find(|&&(target, _)| target == actor)
                {
                    let _ = aggressor;
                    scratch.decisions.push(FeedbackDecision {
                        actor,
                        action: TakeAction::Install {
                            kind: CombatTake::Hit,
                            handle: clips.hit,
                            secs: clips.hit_secs,
                            death: false,
                        },
                        snapshot: read_player_snapshot(world, actor),
                        skeleton_root: read_skeleton_root(world, actor),
                    });
                    continue;
                }

                // 4. Attack take — this marked actor struck someone. The enemy
                //    owns the attack sound and its world-space position.
                if hit_events.iter().any(|&(_, aggressor)| aggressor == actor) {
                    scratch.sound_events.push((actor, FeedbackSound::Swing));
                    scratch.decisions.push(FeedbackDecision {
                        actor,
                        action: TakeAction::Install {
                            kind: CombatTake::Attack,
                            handle: clips.attack,
                            secs: clips.attack_secs,
                            death: false,
                        },
                        snapshot: read_player_snapshot(world, actor),
                        skeleton_root: read_skeleton_root(world, actor),
                    });
                    continue;
                }

                // 5. Alive with nothing to play: record the first sighting.
                if !state.seen_alive {
                    scratch.decisions.push(FeedbackDecision {
                        actor,
                        action: TakeAction::SeenAlive,
                        snapshot: None,
                        skeleton_root: None,
                    });
                }
            }
        }
    }

    // ── Write pass: player swaps + component state. One storage write at
    // a time, mirroring walk_anim's apply pass.
    if scratch.decisions.is_empty() && scratch.sound_events.is_empty() {
        return;
    }
    if let Some(mut pq) = world.query_mut::<AnimationPlayer>() {
        for decision in &scratch.decisions {
            match &decision.action {
                TakeAction::Install { handle, .. } => match pq.get_mut(decision.actor) {
                    Some(player) => {
                        player.clip_handle = *handle;
                        player.local_time = 0.0;
                        player.prev_time = 0.0;
                        player.speed = 1.0;
                        player.playing = true;
                    }
                    None => {
                        // No player yet: only insert when the actor has a
                        // skeleton to animate. The component pass below
                        // still records the take/latch, so a death on a
                        // skeleton-less actor never retries every frame.
                        if let Some(root) = decision.skeleton_root {
                            pq.insert(
                                decision.actor,
                                AnimationPlayer::new(*handle).with_root(root),
                            );
                        }
                    }
                },
                TakeAction::Restore => match decision.snapshot {
                    Some(snapshot) => {
                        if let Some(player) = pq.get_mut(decision.actor) {
                            player.clip_handle = snapshot.clip_handle;
                            player.local_time = snapshot.local_time;
                            player.prev_time = snapshot.prev_time;
                            player.speed = snapshot.speed;
                            player.playing = snapshot.playing;
                        }
                    }
                    None => {
                        pq.remove(decision.actor);
                    }
                },
                TakeAction::Tick { .. } | TakeAction::LatchDeath | TakeAction::SeenAlive => {}
            }
        }
    }
    if let Some(mut aq) = world.query_mut::<DraugrCombatAnim>() {
        for decision in &scratch.decisions {
            let Some(state) = aq.get_mut(decision.actor) else {
                continue;
            };
            match &decision.action {
                TakeAction::Install {
                    kind, secs, death, ..
                } => {
                    if *death {
                        state.death_played = true;
                        state.take = None;
                        state.take_remaining = 0.0;
                        state.captured = None;
                    } else {
                        state.take = Some(*kind);
                        state.take_remaining = *secs;
                        state.captured = decision.snapshot;
                        state.inserted_player = decision.snapshot.is_none();
                    }
                }
                TakeAction::Restore => {
                    state.take = None;
                    state.take_remaining = 0.0;
                    state.captured = None;
                }
                TakeAction::Tick { remaining } => {
                    state.take_remaining = *remaining;
                }
                TakeAction::LatchDeath => {
                    state.death_played = true;
                    state.take = None;
                    state.take_remaining = 0.0;
                    state.captured = None;
                }
                TakeAction::SeenAlive => {}
            }
            // Every decision but a death is made about a live actor.
            if !matches!(
                decision.action,
                TakeAction::LatchDeath | TakeAction::Install { death: true, .. }
            ) {
                state.seen_alive = true;
            }
        }
    }

    // ── Sound dispatch — after every component write, locks dropped.
    let sound_events = std::mem::take(&mut scratch.sound_events);
    for &(actor, sound) in &sound_events {
        let Some(position) = world
            .query::<GlobalTransform>()
            .and_then(|gt| gt.get(actor).map(|gt| gt.translation))
        else {
            continue;
        };
        let path = match sound {
            FeedbackSound::Swing => SWING_SOUND_PATH,
            FeedbackSound::Impact => IMPACT_SOUND_PATH,
            FeedbackSound::DeathVoice => DEATH_VOICE_PATH,
        };
        play_oneshot_cached(world, path, position);
    }
    scratch.sound_events = sound_events;
}

fn read_player_snapshot(world: &World, actor: EntityId) -> Option<WalkAnimSnapshot> {
    world.query::<AnimationPlayer>().and_then(|q| {
        q.get(actor).map(|p| WalkAnimSnapshot {
            clip_handle: p.clip_handle,
            local_time: p.local_time,
            prev_time: p.prev_time,
            speed: p.speed,
            playing: p.playing,
        })
    })
}

fn read_skeleton_root(world: &World, actor: EntityId) -> Option<EntityId> {
    world
        .query::<AnimationTarget>()
        .and_then(|q| q.get(actor).map(|t| t.skeleton_root))
}

/// Lazily decode (or reuse) a one-shot and enqueue it. Headless (no
/// `AudioWorld`) and archive-less (no provider / decode failure) paths
/// both collapse to a silent skip — sound is never a hard dependency.
fn play_oneshot_cached(world: &World, path: &'static str, position: Vec3) {
    let data = world
        .try_resource_mut::<byroredux_audio::SoundCache>()
        .and_then(|mut cache| {
            cache.get_or_load(path, || {
                let provider =
                    world.try_resource::<crate::asset_provider::SoundArchiveProvider>()?;
                if provider.is_empty() {
                    return None;
                }
                provider.extract(path).or_else(|| {
                    log::warn!("combat sound: '{path}' not found in any --sounds-bsa archive");
                    None
                })
            })
        });
    let Some(data) = data else {
        return;
    };
    if let Some(mut audio) = world.try_resource_mut::<byroredux_audio::AudioWorld>() {
        audio.play_oneshot(
            data,
            position,
            // Weapon/voice one-shots carry a little further than
            // footsteps (footsteps: 12 m) — a swing should read across
            // a small interior.
            byroredux_audio::Attenuation {
                min_distance: 0.5,
                max_distance: 24.0,
            },
            1.0,
        );
    }
}

/// System factory — returns a closure with persistent scratch, mirroring
/// [`crate::systems::make_npc_walk_animation_system`]. Wire with
/// `add_exclusive(Stage::PostUpdate, …)` AFTER the walk-animation system:
/// a death take must be installed after walk_anim's abandon pass so the
/// abandoned player isn't restored over it, and a mid-walk stagger relies
/// on walk_anim's yield rule having already given up ownership this frame.
pub(crate) fn make_combat_feedback_system() -> impl FnMut(&World, f32) + Send + Sync {
    let mut scratch = FeedbackScratch::default();
    move |world: &World, dt: f32| {
        combat_feedback_system_inner(world, dt, &mut scratch);
    }
}

/// Kept for test ergonomics, mirroring `npc_walk_animation_system`'s
/// `#[cfg(test)]` twin.
#[cfg(test)]
pub(crate) fn combat_feedback_system(world: &World, dt: f32) {
    combat_feedback_system_inner(world, dt, &mut FeedbackScratch::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::animation::AnimationClipRegistry;
    use byroredux_core::ecs::World;

    const ATTACK: u32 = 101;
    const HIT: u32 = 102;
    const DEATH: u32 = 103;
    const HIT_SECS: f32 = 2.0;
    const ATTACK_SECS: f32 = 2.5;

    fn clips_resource() -> DraugrCombatClips {
        DraugrCombatClips {
            attack: ATTACK,
            hit: HIT,
            death: DEATH,
            attack_secs: ATTACK_SECS,
            hit_secs: HIT_SECS,
        }
    }

    /// A skeleton-bearing draugr at `world_to_screen`: placement entity with
    /// Transform + GlobalTransform, a skeleton-root child, the combat-anim
    /// marker, and (optionally) a pre-existing AnimationPlayer mid-clip.
    struct Fixture {
        world: World,
        actor: EntityId,
        skeleton: EntityId,
    }

    fn spawn_actor(with_player: bool) -> Fixture {
        let mut world = World::new();
        world.register::<DraugrCombatAnim>();
        world.register::<AnimationPlayer>();
        world.register::<AnimationTarget>();
        world.register::<GlobalTransform>();
        world.register::<byroredux_scripting::HitEvent>();
        world.register::<Dead>();
        let actor = world.spawn();
        let skeleton = world.spawn();
        world.insert(actor, byroredux_core::ecs::components::Transform::default());
        world.insert(actor, GlobalTransform::default());
        world.insert(
            actor,
            AnimationTarget {
                skeleton_root: skeleton,
                consumed_idle_serial: 0,
            },
        );
        world.insert(actor, DraugrCombatAnim::default());
        if with_player {
            let pre = AnimationPlayer::new(999).with_root(skeleton);
            world.insert(actor, pre);
        }
        Fixture {
            world,
            actor,
            skeleton,
        }
    }

    fn clip_handle(world: &World, actor: EntityId) -> Option<u32> {
        world
            .query::<AnimationPlayer>()
            .and_then(|q| q.get(actor).map(|p| p.clip_handle))
    }

    fn player_root(world: &World, actor: EntityId) -> Option<Option<EntityId>> {
        world
            .query::<AnimationPlayer>()
            .and_then(|q| q.get(actor).map(|p| p.root_entity))
    }

    fn install_clips(world: &mut World) {
        world.insert_resource(clips_resource());
    }

    fn hit_event(world: &mut World, target: EntityId, aggressor: EntityId) {
        if let Some(mut events) = world.query_mut::<byroredux_scripting::HitEvent>() {
            events.insert(
                target,
                byroredux_scripting::HitEvent {
                    aggressor,
                    source: aggressor,
                    projectile: 0,
                    damage: 8.0,
                    power_attack: false,
                    sneak_attack: false,
                    bash_attack: false,
                    blocked: false,
                },
            );
        }
    }

    fn combat_anim_state(world: &World, actor: EntityId) -> DraugrCombatAnim {
        world
            .query::<DraugrCombatAnim>()
            .and_then(|q| q.get(actor).map(|state| *state))
            .expect("fixture actor carries the combat-anim marker")
    }

    /// The core hit-reaction loop: a HitEvent takes playback onto the
    /// stagger clip, the take runs for the clip's duration, then the
    /// pre-take player state is restored verbatim.
    #[test]
    fn hit_take_swaps_clip_then_restores_the_captured_player() {
        let Fixture { world, actor, .. } = spawn_actor(true);
        let mut world = world;
        install_clips(&mut world);

        hit_event(&mut world, actor, 7);
        combat_feedback_system(&world, 1.0 / 60.0);
        assert_eq!(
            clip_handle(&world, actor),
            Some(HIT),
            "stagger take installed"
        );
        let state = combat_anim_state(&world, actor);
        assert_eq!(state.take, Some(CombatTake::Hit));
        assert!((state.take_remaining - HIT_SECS).abs() < 1e-4);
        let captured = state.captured.expect("pre-take player captured");
        assert_eq!(captured.clip_handle, 999);

        // Tick past the duration in 0.5 s steps: still HIT at the boundary…
        for _ in 0..3 {
            combat_feedback_system(&world, 0.5);
            assert_eq!(clip_handle(&world, actor), Some(HIT));
        }
        // …and restored verbatim once the take expires.
        combat_feedback_system(&world, 0.5);
        assert_eq!(
            clip_handle(&world, actor),
            Some(999),
            "the captured player state must come back after the stagger"
        );
        let state = combat_anim_state(&world, actor);
        assert_eq!(state.take, None);
        assert_eq!(state.captured.map(|c| c.clip_handle), None);
    }

    /// #4613 — the sound tail takes `decisions` for its loop and must put it
    /// back: an unrestored `mem::take` left the closure-persistent scratch at
    /// zero capacity, so every frame of an active take regrew it. The
    /// HitEvent snapshot is persistent scratch too.
    #[test]
    fn feedback_scratch_keeps_its_buffers_across_frames() {
        let Fixture { world, actor, .. } = spawn_actor(true);
        let mut world = world;
        install_clips(&mut world);
        let mut scratch = FeedbackScratch::default();

        hit_event(&mut world, actor, 7);
        combat_feedback_system_inner(&world, 1.0 / 60.0, &mut scratch);
        assert_eq!(clip_handle(&world, actor), Some(HIT), "the take fired");
        assert!(
            scratch
                .sound_events
                .contains(&(actor, FeedbackSound::Impact))
        );
        assert!(
            scratch.decisions.capacity() > 0,
            "decisions must survive the sound tail's take"
        );
        assert_eq!(scratch.hit_events, vec![(actor, 7)]);
        let buffers = (scratch.decisions.as_ptr(), scratch.hit_events.as_ptr());

        hit_event(&mut world, actor, 7);
        combat_feedback_system_inner(&world, 1.0 / 60.0, &mut scratch);
        assert!(
            scratch
                .sound_events
                .contains(&(actor, FeedbackSound::Impact)),
            "a second hit during the active take still dispatches an impact"
        );
        assert_eq!(
            (scratch.decisions.as_ptr(), scratch.hit_events.as_ptr()),
            buffers,
            "a second frame reuses both buffers"
        );
    }

    /// A draugr without a pre-existing player gets one inserted (bound to
    /// its skeleton root), and the restore removes it again — walk_anim's
    /// insert/RemovePlayer contract.
    #[test]
    fn hit_take_inserts_a_player_and_removes_it_on_restore() {
        let Fixture {
            world,
            actor,
            skeleton,
        } = spawn_actor(false);
        let mut world = world;
        install_clips(&mut world);

        hit_event(&mut world, actor, 7);
        combat_feedback_system(&world, 1.0 / 60.0);
        assert_eq!(clip_handle(&world, actor), Some(HIT));
        let state = combat_anim_state(&world, actor);
        assert!(state.inserted_player, "the take inserted the player");
        assert_eq!(
            player_root(&world, actor),
            Some(Some(skeleton)),
            "the inserted player must bind the actor's skeleton root"
        );

        for _ in 0..4 {
            combat_feedback_system(&world, 0.5);
        }
        assert_eq!(clip_handle(&world, actor), None, "restored by removal");
        assert!(
            clip_handle(&world, actor).is_none() && player_root(&world, actor).is_none(),
            "the inserted player must be removed on restore"
        );
    }

    /// Death is terminal: the take latches, survives every later tick, and
    /// never restores — the `Dead` marker owns the pose.
    #[test]
    fn death_take_plays_once_and_never_restores() {
        let Fixture { world, actor, .. } = spawn_actor(true);
        let mut world = world;
        install_clips(&mut world);
        let mut scratch = FeedbackScratch::default();
        // Seen alive first: only a death the system watched happen plays.
        combat_feedback_system_inner(&world, 1.0 / 60.0, &mut scratch);
        world.insert(actor, Dead);

        combat_feedback_system_inner(&world, 1.0 / 60.0, &mut scratch);
        assert_eq!(clip_handle(&world, actor), Some(DEATH));
        assert!(combat_anim_state(&world, actor).death_played);
        assert!(
            scratch
                .sound_events
                .contains(&(actor, FeedbackSound::DeathVoice))
        );

        // Repeat ticks must not re-fire (no restart) nor restore.
        for _ in 0..6 {
            combat_feedback_system(&world, 0.5);
            assert_eq!(clip_handle(&world, actor), Some(DEATH));
        }
        assert!(combat_anim_state(&world, actor).death_played);
    }

    /// A killing blow produces the DEATH take, not a hit take — the Dead
    /// check outranks the HitEvent check on the same frame.
    #[test]
    fn a_killing_hit_prefers_the_death_take() {
        let Fixture { world, actor, .. } = spawn_actor(true);
        let mut world = world;
        install_clips(&mut world);
        combat_feedback_system(&world, 1.0 / 60.0);
        world.insert(actor, Dead);
        hit_event(&mut world, actor, 7);

        let mut scratch = FeedbackScratch::default();
        combat_feedback_system_inner(&world, 1.0 / 60.0, &mut scratch);
        assert_eq!(clip_handle(&world, actor), Some(DEATH));
        assert!(combat_anim_state(&world, actor).death_played);
        assert_eq!(combat_anim_state(&world, actor).take, None);
        assert!(
            scratch
                .sound_events
                .contains(&(actor, FeedbackSound::Impact))
        );
        assert!(
            scratch
                .sound_events
                .contains(&(actor, FeedbackSound::DeathVoice))
        );
    }

    /// #4708 (b) — a corpse respawned or reloaded dead (`Dead` from its
    /// first observed frame, a fresh un-latched marker) replays nothing:
    /// the latch is derived from `Dead`, the player is left alone.
    #[test]
    fn a_corpse_restored_dead_latches_without_replaying_the_death() {
        let Fixture { world, actor, .. } = spawn_actor(true);
        let mut world = world;
        install_clips(&mut world);
        world.insert(actor, Dead);

        combat_feedback_system(&world, 1.0 / 60.0);
        assert!(combat_anim_state(&world, actor).death_played);
        assert_eq!(clip_handle(&world, actor), Some(999), "no death take");
        combat_feedback_system(&world, 1.0 / 60.0);
        assert_eq!(clip_handle(&world, actor), Some(999));
    }

    /// #4708 (a) — the killing blow's `reconcile_dead_actor` removed the
    /// players and activated the ragdoll before this system runs; the
    /// death must not re-insert a player over the physics pose.
    #[test]
    fn a_ragdolled_death_latches_without_inserting_a_player() {
        let Fixture {
            world,
            actor,
            skeleton,
        } = spawn_actor(false);
        let mut world = world;
        install_clips(&mut world);
        world.register::<crate::ragdoll::RagdollActive>();
        combat_feedback_system(&world, 1.0 / 60.0);
        assert!(combat_anim_state(&world, actor).seen_alive);

        world.insert(actor, Dead);
        world.insert(skeleton, crate::ragdoll::RagdollActive);
        combat_feedback_system(&world, 1.0 / 60.0);

        assert!(combat_anim_state(&world, actor).death_played);
        assert_eq!(
            clip_handle(&world, actor),
            None,
            "the ragdoll owns the pose"
        );
    }

    /// The aggressor half of the family: an NPC strike takes the ATTACK
    /// clip on the attacker. The player (no marker) never takes — only the
    /// marker component opts an actor in.
    #[test]
    fn npc_aggressor_takes_the_attack_clip() {
        let Fixture { world, actor, .. } = spawn_actor(false);
        let mut world = world;
        install_clips(&mut world);
        let human = world.spawn();
        world.insert(human, GlobalTransform::default());

        // The draugr struck the human: draugr takes ATTACK.
        hit_event(&mut world, human, actor);
        let mut scratch = FeedbackScratch::default();
        combat_feedback_system_inner(&world, 1.0 / 60.0, &mut scratch);
        assert_eq!(
            clip_handle(&world, actor),
            Some(ATTACK),
            "the attacking draugr takes the attack clip"
        );
        assert!(
            scratch
                .sound_events
                .contains(&(actor, FeedbackSound::Swing))
        );
        assert!(
            clip_handle(&world, human).is_none(),
            "the human target has no combat-anim marker, so no take"
        );
    }

    /// The marker IS the gate: an actor without `DraugrCombatAnim` is
    /// invisible to the feedback system even with clips installed.
    #[test]
    fn actors_without_the_marker_never_take() {
        let mut world = World::new();
        world.register::<AnimationPlayer>();
        world.register::<byroredux_scripting::HitEvent>();
        let actor = world.spawn();
        world.insert(actor, AnimationPlayer::new(55));
        world.insert_resource(clips_resource());
        hit_event(&mut world, actor, 8);

        combat_feedback_system(&world, 1.0 / 60.0);
        assert_eq!(clip_handle(&world, actor), Some(55), "untouched");
    }

    /// Without the decoded-clip resource (non-Skyrim, missing archives,
    /// decode failure) the system is a total no-op — the documented
    /// silent-downgrade contract.
    #[test]
    fn missing_clips_resource_is_a_no_op() {
        let Fixture { world, actor, .. } = spawn_actor(true);
        let mut world = world;
        world.insert(actor, Dead);
        hit_event(&mut world, actor, 7);

        combat_feedback_system(&world, 1.0 / 60.0);
        assert_eq!(clip_handle(&world, actor), Some(999), "player untouched");
        assert!(!combat_anim_state(&world, actor).death_played);
    }

    /// Player input without a marked Draugr does not create a generic
    /// two-handed-blade sound or leave feedback work queued across ticks.
    #[test]
    fn player_swing_without_marked_draugr_does_not_queue_feedback() {
        let mut world = World::new();
        world.register::<GlobalTransform>();
        world.insert_resource(clips_resource());
        world.insert_resource(byroredux_audio::AudioWorld::headless());
        world.insert_resource(byroredux_audio::SoundCache::new());
        let player = world.spawn();
        world.insert(player, GlobalTransform::default());
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        world.insert_resource(crate::systems::PlayerMode::Character);
        world.insert_resource(crate::combat::CombatState {
            attacks_started: 3,
            hits_landed: 0,
            kills: 0,
            last: None,
        });

        let mut scratch = FeedbackScratch::default();
        combat_feedback_system_inner(&world, 1.0 / 60.0, &mut scratch);
        assert!(scratch.sound_events.is_empty());
        combat_feedback_system_inner(&world, 1.0 / 60.0, &mut scratch);
        assert!(scratch.sound_events.is_empty());
    }

    /// The registry path pins what the real-data catalog test asserts
    /// against — source-level so the resource/paths can't silently drift
    /// apart from `asset_provider::populate_draugr_combat_clips`.
    #[test]
    fn fixture_paths_and_resource_contract_stay_aligned() {
        let anim = include_str!("../asset_provider/animation.rs");
        for path in [
            r"meshes\actors\draugr\character assets\skeletonf.hkx",
            r"meshes\actors\draugr\animations\2hmattackforwardb.hkx",
            r"meshes\actors\draugr\animations\mtstaggermedium.hkx",
            r"meshes\actors\draugr\animations\special_deathbackward.hkx",
        ] {
            assert!(anim.contains(path), "catalog lost the pinned path {path}");
        }
        let _ = AnimationClipRegistry::default();
    }

    #[test]
    #[ignore = "needs Skyrim - Sounds.bsa on disk"]
    fn draugr_combat_sound_assets_extract_and_decode_when_available() {
        let data_dir = std::env::var_os("BYROREDUX_SKYRIM_DATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(
                    "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
                )
            });
        let archive = data_dir.join("Skyrim - Sounds.bsa");
        if !archive.is_file() {
            panic!("missing real-data fixture {}", archive.display());
        }
        let args = vec![
            "--sounds-bsa".to_owned(),
            archive.to_string_lossy().into_owned(),
        ];
        let provider = crate::asset_provider::build_sound_archive_provider(&args);
        for path in [SWING_SOUND_PATH, IMPACT_SOUND_PATH, DEATH_VOICE_PATH] {
            let bytes = provider
                .extract(path)
                .unwrap_or_else(|| panic!("{path} missing from {}", archive.display()));
            byroredux_audio::load_sound_from_bytes(bytes)
                .unwrap_or_else(|error| panic!("{path} failed to decode: {error}"));
        }
    }
}
