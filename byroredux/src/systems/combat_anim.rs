//! P2 combat tail — combat feedback: one-shot clip takes (attack / hit /
//! death) for Draugr-family actors plus the spatial combat sound family.
//!
//! Asset family pinned by `docs/engine/p2-combat-anim-sound-fixture.md`;
//! the clips arrive pre-decoded in [`crate::components::DraugrCombatClips`]
//! (populated beside the walk clip), the sounds are decoded lazily from
//! `--sounds-bsa` archives on first use and cached in the system's closure
//! scratch.
//!
//! Event edges consumed (all produced upstream in the same frame):
//!
//! * `ActionState` attack edge → `combat_input_system` arms
//!   `CombatState.attacks_started`; the counter DELTA — not the raw input
//!   edge, because the cooldown gate has already armed by PostUpdate, so
//!   re-deriving `was_pressed` here would both miss gated swings and
//!   double-count nothing — fires the swing one-shot at the player's
//!   position.
//! * `HitEvent` (transient, scripting-Late cleanup) → impact one-shot at
//!   the target; on a Draugr family member, an attack take on the
//!   AGGRESSOR (NPC strikes carry the aggressor, #4324) and a hit take on
//!   the target.
//! * `Dead` → death take + death voice, latched once via
//!   `DraugrCombatAnim.death_played`. Death keeps no snapshot — the `Dead`
//!   marker owns the pose from there, and walk_anim's abandon rule already
//!   cedes playback to it.
//!
//! Take protocol mirrors `walk_anim`'s take/restore: capture the pre-take
//! `AnimationPlayer` (inserting one bound to `AnimationTarget::
//! skeleton_root` when absent), swap the clip, restore verbatim when the
//! take's duration expires. A mid-walk take relies on walk_anim's existing
//! yield rule (someone else swapped the clip → walk loses ownership
//! without stomping).

use std::collections::VecDeque;

use byroredux_core::animation::AnimationPlayer;
use byroredux_core::ecs::components::{Dead, GlobalTransform};
use byroredux_core::ecs::{EntityId, World};
use byroredux_core::math::Vec3;
use rustc_hash::FxHashMap;

use crate::combat::CombatState;
use crate::components::{
    CombatTake, DraugrCombatAnim, DraugrCombatClips, AnimationTarget, WalkAnimSnapshot,
};
use crate::systems::{PlayerEntity, PlayerMode};

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
}

struct FeedbackDecision {
    actor: EntityId,
    action: TakeAction,
    /// Pre-take player snapshot + skeleton root, read before any write.
    snapshot: Option<WalkAnimSnapshot>,
    skeleton_root: Option<EntityId>,
    sound: Option<FeedbackSound>,
}

#[derive(Clone, Copy)]
enum FeedbackSound {
    Impact,
    DeathVoice,
}

/// Closure-persistent scratch: the lazily-decoded sound cache, the swing
/// counter from last frame, and this tick's decision buffer.
#[derive(Default)]
struct FeedbackScratch {
    sounds: FxHashMap<&'static str, Option<std::sync::Arc<byroredux_audio::Sound>>>,
    last_attacks_started: u64,
    swings: VecDeque<Vec3>,
    decisions: Vec<FeedbackDecision>,
}

pub(crate) fn combat_feedback_system_inner(
    world: &World,
    dt: f32,
    scratch: &mut FeedbackScratch,
) {
    let Some(clips) = world.try_resource::<DraugrCombatClips>() else {
        return;
    };
    let clips = *clips;

    // ── Read pass: events + per-actor state → buffered decisions. Every
    // query here is read-only, so all borrows drop before the write pass.
    scratch.decisions.clear();
    {
        let hit_events: Vec<(EntityId, EntityId)> = world
            .query::<byroredux_scripting::HitEvent>()
            .map(|events| {
                events
                    .iter()
                    .map(|(entity, event)| (entity, event.aggressor))
                    .collect()
            })
            .unwrap_or_default();

        // Swing sound — `attacks_started` advanced this frame. Position is
        // the player's; a swing without a resolvable player pose (fly-cam,
        // no player yet) is skipped rather than fired at the world origin.
        let attacks_started = world
            .try_resource::<CombatState>()
            .map(|state| state.attacks_started)
            .unwrap_or(0);
        if attacks_started != scratch.last_attacks_started {
            scratch.last_attacks_started = attacks_started;
            if let Some(position) = world
                .try_resource::<PlayerEntity>()
                .and_then(|player| player.0)
                .and_then(|player| {
                    world
                        .query::<GlobalTransform>()
                        .and_then(|gt| gt.get(player).map(|gt| gt.translation))
                })
                .filter(|_| {
                    world
                        .try_resource::<PlayerMode>()
                        .is_some_and(|mode| *mode == PlayerMode::Character)
                })
            {
                scratch.swings.push_back(position);
            }
        }

        let Some(anim_q) = world.query::<DraugrCombatAnim>() else {
            return;
        };
        for (actor, state) in anim_q.iter() {
            let dead = world.get::<Dead>(actor).is_some();

            // 1. Death — terminal, latched, checked first so the killing
            //    blow produces the death take, never a hit take.
            if dead {
                if !state.death_played {
                    scratch.decisions.push(FeedbackDecision {
                        actor,
                        action: TakeAction::Install {
                            kind: CombatTake::Hit,
                            handle: clips.death,
                            secs: 0.0,
                            death: true,
                        },
                        snapshot: read_player_snapshot(world, actor),
                        skeleton_root: read_skeleton_root(world, actor),
                        sound: Some(FeedbackSound::DeathVoice),
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
                        sound: None,
                    });
                } else {
                    scratch.decisions.push(FeedbackDecision {
                        actor,
                        action: TakeAction::Tick { remaining },
                        snapshot: None,
                        skeleton_root: None,
                        sound: None,
                    });
                }
                let _ = kind;
                continue;
            }

            // 3. Hit reaction — this actor was struck (deduped by the
            //    take.is_none() gate above: the transient event can be
            //    visible for more than one read).
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
                    sound: Some(FeedbackSound::Impact),
                });
                continue;
            }

            // 4. Attack take — this actor struck someone (their swing).
            //    The player aggressor carries no marker and never reaches
            //    this loop, so the player's own swings get only the sound.
            if hit_events.iter().any(|&(_, aggressor)| aggressor == actor) {
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
                    sound: None,
                });
            }
        }
    }

    // ── Write pass: player swaps + component state. One storage write at
    // a time, mirroring walk_anim's apply pass.
    if scratch.decisions.is_empty() && scratch.swings.is_empty() {
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
                TakeAction::Restore => {
                    match decision.snapshot {
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
                    }
                }
                TakeAction::Tick { .. } => {}
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
                    kind,
                    secs,
                    death,
                    ..
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
            }
        }
    }

    // ── Sound dispatch — after every component write, locks dropped.
    while let Some(position) = scratch.swings.pop_front() {
        play_oneshot_cached(world, scratch, SWING_SOUND_PATH, position);
    }
    let decisions = std::mem::take(&mut scratch.decisions);
    for decision in &decisions {
        let Some(sound) = decision.sound else {
            continue;
        };
        let Some(position) = world
            .query::<GlobalTransform>()
            .and_then(|gt| gt.get(decision.actor).map(|gt| gt.translation))
        else {
            continue;
        };
        match sound {
            FeedbackSound::Impact => {
                play_oneshot_cached(world, scratch, IMPACT_SOUND_PATH, position)
            }
            FeedbackSound::DeathVoice => {
                play_oneshot_cached(world, scratch, DEATH_VOICE_PATH, position)
            }
        }
    }
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
fn play_oneshot_cached(
    world: &World,
    scratch: &mut FeedbackScratch,
    path: &'static str,
    position: Vec3,
) {
    let data = match scratch.sounds.get(path) {
        Some(cached) => cached.clone(),
        None => {
            let decoded = world
                .try_resource::<crate::asset_provider::SoundArchiveProvider>()
                .filter(|provider| !provider.is_empty())
                .and_then(|provider| provider.extract(path))
                .and_then(|bytes| match byroredux_audio::load_sound_from_bytes(bytes) {
                    Ok(data) => Some(std::sync::Arc::new(data)),
                    Err(e) => {
                        log::warn!("combat sound: decode '{path}' failed: {e}");
                        None
                    }
                });
            scratch.sounds.insert(path, decoded.clone());
            decoded
        }
    };
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

/// System factory — returns a closure with persistent scratch (the sound
/// cache + swing counter), mirroring
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
        world.insert(
            actor,
            byroredux_core::ecs::components::Transform::default(),
        );
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
        Fixture { world, actor, skeleton }
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
        assert_eq!(clip_handle(&world, actor), Some(HIT), "stagger take installed");
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

    /// A draugr without a pre-existing player gets one inserted (bound to
    /// its skeleton root), and the restore removes it again — walk_anim's
    /// insert/RemovePlayer contract.
    #[test]
    fn hit_take_inserts_a_player_and_removes_it_on_restore() {
        let Fixture { world, actor, skeleton } = spawn_actor(false);
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
        world.insert(actor, Dead);

        combat_feedback_system(&world, 1.0 / 60.0);
        assert_eq!(clip_handle(&world, actor), Some(DEATH));
        assert!(combat_anim_state(&world, actor).death_played);

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
        world.insert(actor, Dead);
        hit_event(&mut world, actor, 7);

        combat_feedback_system(&world, 1.0 / 60.0);
        assert_eq!(clip_handle(&world, actor), Some(DEATH));
        assert!(combat_anim_state(&world, actor).death_played);
        assert_eq!(combat_anim_state(&world, actor).take, None);
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
        combat_feedback_system(&world, 1.0 / 60.0);
        assert_eq!(
            clip_handle(&world, actor),
            Some(ATTACK),
            "the attacking draugr takes the attack clip"
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

    /// A headless swing (attacks_started advanced, audio manager absent)
    /// must not panic and must not disturb actor state — the sound half
    /// degrades silently, the animation half stays independent.
    #[test]
    fn headless_swing_delta_is_a_safe_no_op() {
        let Fixture { world, actor, .. } = spawn_actor(true);
        let mut world = world;
        install_clips(&mut world);
        world.insert_resource(crate::combat::CombatState {
            attacks_started: 3,
            hits_landed: 0,
            kills: 0,
            last: None,
        });

        combat_feedback_system(&world, 1.0 / 60.0);
        combat_feedback_system(&world, 1.0 / 60.0);
        assert_eq!(clip_handle(&world, actor), Some(999), "no spurious takes");
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
        let here = include_str!("combat_anim.rs");
        for path in [
            SWING_SOUND_PATH,
            IMPACT_SOUND_PATH,
            DEATH_VOICE_PATH,
        ] {
            assert!(here.contains(path));
        }
        let _ = AnimationClipRegistry::default();
    }
}
