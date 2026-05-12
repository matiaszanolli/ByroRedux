//! Audio routing systems — reverb zones, footstep emitters.

use byroredux_core::ecs::{GlobalTransform, World};

use crate::components::CellLightingRes;

/// M44 Phase 6 — cell-acoustics → reverb send wiring (#846 / AUD-D5-NEW-05).
///
/// Watches [`CellLightingRes::is_interior`] and updates
/// [`byroredux_audio::AudioWorld::set_reverb_send_db`] so interior
/// cells get a subtle wet send (`-12 dB`) and exteriors stay dry
/// (`f32::NEG_INFINITY`). Pre-fix the setter existed but no caller
/// flipped it, so every cell sounded identically dry regardless of
/// interior/exterior. The audit's Phase 6 promise (interior reverb
/// detector) lands here.
///
/// Idempotent — only writes on transitions (the bit-equality check
/// handles `NEG_INFINITY` cleanly), so the system is cheap to leave
/// running every frame and only touches `AudioWorld` on actual cell
/// type changes.
///
/// **Kira semantics**: already-playing sounds keep their construction-
/// time send level — the change applies to sounds dispatched AFTER
/// the call. For cell-load handoffs that's by design (the new cell's
/// ambients & one-shots get the new reverb routing). A long-running
/// ambient that survives an interior→exterior transition keeps its
/// original send level until it ends naturally; that's a known
/// limitation tracked in AUD-D5-NEW-06 (per-cell acoustic data).
///
/// No-ops cleanly when:
///   - `CellLightingRes` resource isn't registered yet (engine boot
///     before any cell load — the default send is already
///     `NEG_INFINITY`, so dry is correct).
///   - `AudioWorld` resource isn't registered (engine started without
///     audio wiring).
///
/// Runs in `Stage::Late` alongside `audio_system` (registered first
/// in main.rs so the level is in place before any new spatial track
/// gets constructed this frame).
pub(crate) fn reverb_zone_system(world: &World, _dt: f32) {
    /// Subtle interior wet — matches `set_reverb_send_db` doc.
    /// `-6 dB` is more pronounced; `-12 dB` is the audit's call.
    const INTERIOR_REVERB_SEND_DB: f32 = -12.0;
    /// Exteriors stay dry — silent send (well below the `-60 dB`
    /// `with_send` cutoff in the audio crate).
    const EXTERIOR_REVERB_SEND_DB: f32 = f32::NEG_INFINITY;

    let is_interior = {
        let Some(cell_lit) = world.try_resource::<CellLightingRes>() else {
            return;
        };
        cell_lit.is_interior
    };
    let target_db = if is_interior {
        INTERIOR_REVERB_SEND_DB
    } else {
        EXTERIOR_REVERB_SEND_DB
    };

    let Some(mut audio_world) = world.try_resource_mut::<byroredux_audio::AudioWorld>() else {
        return;
    };
    // Bit-equality so `NEG_INFINITY → NEG_INFINITY` short-circuits
    // without touching the field. (`==` would also work — IEEE 754
    // says `inf == inf` — but `to_bits()` makes the no-op intent
    // explicit and dodges any future signaling-NaN edge case.)
    if audio_world.reverb_send_db().to_bits() == target_db.to_bits() {
        return;
    }
    audio_world.set_reverb_send_db(target_db);
    log::info!(
        "M44 Phase 6: reverb send → {:.1} dB (interior={})",
        target_db,
        is_interior,
    );
}

/// M44 Phase 3.5 — footstep gameplay loop.
///
/// Walks every entity with a `FootstepEmitter`, accumulates horizontal
/// (XZ-plane) movement from frame to frame against
/// `stride_threshold`, and queues a one-shot via
/// `AudioWorld::play_oneshot` each time the stride threshold is
/// crossed. Vertical movement (jumping, falling, elevators) does
/// NOT count toward stride.
///
/// No-ops cleanly when:
///   - `FootstepConfig` resource isn't registered (engine started
///     without audio wiring).
///   - `FootstepConfig.default_sound` is `None` (BSA-load failed
///     at startup; e.g. running without game data).
///   - `AudioWorld` is inactive (no audio device).
///   - The first tick on a fresh `FootstepEmitter` — the system seeds
///     `last_position` from the current pose without firing, so we
///     don't emit a "phantom footstep" against the default zero pose.
///
/// Spawn a `FootstepEmitter` on the player entity to opt in. The
/// fly-camera attach is wired in `main.rs::App::new`.
pub(crate) fn footstep_system(world: &World, _dt: f32) {
    use crate::components::{FootstepConfig, FootstepEmitter, FootstepScratch};

    let Some(config) = world.try_resource::<FootstepConfig>() else {
        return;
    };
    let Some(sound) = config.default_sound.clone() else {
        return;
    };
    let volume = config.volume;
    drop(config);

    // Phase 1: walk every emitter, accumulate stride, collect the
    // positions where a footstep should fire this tick. Holding
    // GlobalTransform read + FootstepEmitter write concurrently is
    // fine (separate storages), but we want to release both locks
    // before touching `AudioWorld` in Phase 2 — minimises contention
    // with `audio_system` running in the same stage.
    //
    // The triggers buffer is held in `FootstepScratch` (a Resource)
    // and `clear()`-reused across frames — pre-#932 a fresh
    // `Vec<Vec3>` was allocated every frame even when no NPCs were
    // walking. The buffer is sized 32 in `FootstepScratch::default`
    // to cover the typical 5–10 / peak ~50 walking-NPC range
    // without re-growing.
    let Some(mut scratch) = world.try_resource_mut::<FootstepScratch>() else {
        return;
    };
    scratch.triggers.clear();
    {
        let Some(gt_q) = world.query::<GlobalTransform>() else {
            return;
        };
        let Some(mut fs_q) = world.query_mut::<FootstepEmitter>() else {
            return;
        };
        for (entity, fs) in fs_q.iter_mut() {
            let Some(gt) = gt_q.get(entity) else {
                continue;
            };
            let pos = gt.translation;
            if !fs.initialised {
                fs.last_position = pos;
                fs.initialised = true;
                continue;
            }
            // XZ-plane delta only — vertical (Y) motion isn't a step.
            let dx = pos.x - fs.last_position.x;
            let dz = pos.z - fs.last_position.z;
            let horizontal = (dx * dx + dz * dz).sqrt();
            fs.accumulated_stride += horizontal;
            fs.last_position = pos;
            if fs.accumulated_stride >= fs.stride_threshold {
                fs.accumulated_stride = 0.0;
                scratch.triggers.push(pos);
            }
        }
    }

    // Phase 2: dispatch one-shots for every triggered stride.
    if scratch.triggers.is_empty() {
        return;
    }
    // Drop the scratch lock BEFORE acquiring AudioWorld — both are
    // resource-mut locks, holding both at once would force a strict
    // TypeId-sorted acquisition contract. Drain the scratch into a
    // local before releasing it (cheap — Vec move, no allocation).
    let triggers = std::mem::take(&mut scratch.triggers);
    drop(scratch);

    let Some(mut audio_world) = world.try_resource_mut::<byroredux_audio::AudioWorld>() else {
        // Audio gone — restore the scratch buffer for next frame
        // (preserves the heap allocation) and bail. Re-acquiring the
        // scratch lock here costs one resource_mut hop, but loses the
        // capacity otherwise.
        if let Some(mut scratch) = world.try_resource_mut::<FootstepScratch>() {
            scratch.triggers = triggers;
        }
        return;
    };
    for pos in &triggers {
        audio_world.play_oneshot(
            std::sync::Arc::clone(&sound),
            *pos,
            byroredux_audio::Attenuation {
                // Tighter attenuation than the default — footsteps
                // drop off fast in real environments. 0.5m → full
                // volume, 12m → inaudible.
                min_distance: 0.5,
                max_distance: 12.0,
            },
            volume,
        );
    }
    drop(audio_world);

    // Restore the scratch buffer (with its persisted capacity) so
    // next frame's `clear()` doesn't strand the allocation.
    if let Some(mut scratch) = world.try_resource_mut::<FootstepScratch>() {
        scratch.triggers = triggers;
    }
}

// ── M44 Phase 3.5 — footstep_system regression tests ──────────────
//
// Synthetic-only: walk an emitter through a known-distance path,
// verify the stride accumulator triggers exactly when expected and
// queues a one-shot for each trigger. Audio device not required —
// `AudioWorld` runs in the no-device branch and `play_oneshot`
// queues into the pending vec without touching kira.
#[cfg(test)]
mod footstep_tests {
    use super::*;
    use crate::components::{FootstepConfig, FootstepEmitter, FootstepScratch};
    use byroredux_audio::{Frame, Sound, SoundSettings};
    use byroredux_core::ecs::{Transform, World};
    use byroredux_core::math::{Quat, Vec3};
    use std::sync::Arc;

    fn synth_world(volume: f32) -> (World, Arc<Sound>) {
        let mut world = World::new();
        let sound = Arc::new(Sound {
            sample_rate: 22_050,
            frames: Arc::from(
                vec![
                    Frame {
                        left: 0.0,
                        right: 0.0
                    };
                    50
                ]
                .into_boxed_slice(),
            ),
            settings: SoundSettings::default(),
            slice: None,
        });
        world.insert_resource(FootstepConfig {
            default_sound: Some(Arc::clone(&sound)),
            volume,
        });
        // AudioWorld via `Default::default()` — picks up the
        // headless fallback path when the test host has no audio
        // device, otherwise creates a real manager. Either way
        // `play_oneshot` enqueues without immediately dispatching
        // (drain only fires inside `audio_system`, which we don't
        // call from the footstep tests).
        world.insert_resource(byroredux_audio::AudioWorld::default());
        world.insert_resource(FootstepScratch::default());
        (world, sound)
    }

    /// First tick on a fresh `FootstepEmitter` must seed
    /// `last_position` and NOT fire — otherwise the emitter would
    /// always emit one phantom footstep against the default zero
    /// pose at spawn time.
    #[test]
    fn first_tick_seeds_last_position_without_firing() {
        let (mut world, _sound) = synth_world(0.5);
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        world.insert(
            entity,
            GlobalTransform::new(Vec3::new(10.0, 0.0, 5.0), Quat::IDENTITY, 1.0),
        );
        world.insert(entity, FootstepEmitter::new());

        footstep_system(&world, 0.016);

        let aw = world.resource::<byroredux_audio::AudioWorld>();
        assert_eq!(
            aw.pending_oneshot_count(),
            0,
            "first tick must NOT fire — only seed last_position"
        );
        let q = world.query::<FootstepEmitter>().unwrap();
        let fs = q.get(entity).unwrap();
        assert!(fs.initialised, "first tick must mark emitter initialised");
        assert_eq!(fs.last_position, Vec3::new(10.0, 0.0, 5.0));
        assert_eq!(fs.accumulated_stride, 0.0);
    }

    /// Walking exactly one threshold distance fires exactly one
    /// footstep. Vertical motion is excluded — only XZ delta counts.
    #[test]
    fn stride_threshold_fires_exactly_one_footstep() {
        let (mut world, _sound) = synth_world(0.7);
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        world.insert(
            entity,
            GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
        );
        world.insert(entity, FootstepEmitter::new());

        // Tick 1: seed last_position at origin.
        footstep_system(&world, 0.016);

        // Move 1.5 game-units along +X (exactly the default threshold).
        // Also bump Y by 100 — vertical-only motion that must NOT
        // contribute to stride.
        {
            let mut q = world.query_mut::<GlobalTransform>().unwrap();
            let gt = q.get_mut(entity).unwrap();
            gt.translation = Vec3::new(1.5, 100.0, 0.0);
        }

        // Tick 2: stride accumulates 1.5 units, hits threshold, fires.
        footstep_system(&world, 0.016);

        let aw = world.resource::<byroredux_audio::AudioWorld>();
        assert_eq!(
            aw.pending_oneshot_count(),
            1,
            "1.5-unit horizontal stride must fire exactly one footstep"
        );
    }

    /// Walking 4× the threshold distance in one tick must fire 1
    /// footstep (stride resets when the threshold is crossed; a
    /// catastrophic teleport doesn't multiply footsteps). This pins
    /// the "reset to zero on fire" semantic — a "subtract threshold,
    /// keep remainder" refactor would fire 4 footsteps and feel
    /// machine-gun-like at high speeds.
    #[test]
    fn single_large_jump_fires_one_footstep_only() {
        let (mut world, _sound) = synth_world(1.0);
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        world.insert(
            entity,
            GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
        );
        world.insert(entity, FootstepEmitter::new());

        footstep_system(&world, 0.016); // seed

        // 6.0 horizontal units in one frame — 4× threshold.
        {
            let mut q = world.query_mut::<GlobalTransform>().unwrap();
            let gt = q.get_mut(entity).unwrap();
            gt.translation = Vec3::new(6.0, 0.0, 0.0);
        }

        footstep_system(&world, 0.016);

        let aw = world.resource::<byroredux_audio::AudioWorld>();
        assert_eq!(
            aw.pending_oneshot_count(),
            1,
            "single-tick teleport must fire exactly one footstep, not multiple"
        );
    }

    /// A standing-still emitter (zero stride) never fires. Pinned
    /// because a regression that "fires on every tick when stride
    /// >= 0" would silently spam audio when the player isn't moving.
    #[test]
    fn standing_still_never_fires() {
        let (mut world, _sound) = synth_world(0.5);
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        world.insert(
            entity,
            GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
        );
        world.insert(entity, FootstepEmitter::new());

        for _ in 0..30 {
            footstep_system(&world, 0.016);
        }

        let aw = world.resource::<byroredux_audio::AudioWorld>();
        assert_eq!(aw.pending_oneshot_count(), 0);
    }

    /// Footsteps no-op cleanly when no `default_sound` is loaded
    /// (i.e. user didn't pass --sounds-bsa). The emitter should still
    /// update its last_position so a future runtime reload of the
    /// sound picks up cleanly without a phantom step.
    #[test]
    fn no_default_sound_is_silent_noop() {
        let (mut world, _sound) = synth_world(0.5);
        // Drop the sound reference, leaving the config but with
        // default_sound: None.
        {
            let mut config = world.resource_mut::<FootstepConfig>();
            config.default_sound = None;
        }
        let entity = world.spawn();
        world.insert(entity, Transform::IDENTITY);
        world.insert(
            entity,
            GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0),
        );
        world.insert(entity, FootstepEmitter::new());

        footstep_system(&world, 0.016);
        {
            let mut q = world.query_mut::<GlobalTransform>().unwrap();
            let gt = q.get_mut(entity).unwrap();
            gt.translation = Vec3::new(5.0, 0.0, 0.0);
        }
        footstep_system(&world, 0.016);

        let aw = world.resource::<byroredux_audio::AudioWorld>();
        assert_eq!(aw.pending_oneshot_count(), 0);
    }
}

// ── M44 Phase 6 — reverb_zone_system regression tests (#846) ──────
#[cfg(test)]
mod reverb_tests {
    use super::*;
    use crate::components::CellLightingRes;
    use byroredux_core::ecs::World;

    /// Build a synthetic CellLightingRes with the specified
    /// interior/exterior flag. All extended-XCLL fields stay `None`
    /// — the system only reads `is_interior`, so the rest is
    /// irrelevant.
    fn cell_lit(is_interior: bool) -> CellLightingRes {
        CellLightingRes {
            ambient: [0.1, 0.1, 0.1],
            directional_color: [1.0, 1.0, 1.0],
            directional_dir: [0.0, 1.0, 0.0],
            is_interior,
            fog_color: [0.5, 0.5, 0.5],
            fog_near: 100.0,
            fog_far: 1000.0,
            directional_fade: None,
            fog_clip: None,
            fog_power: None,
            fog_far_color: None,
            fog_max: None,
            light_fade_begin: None,
            light_fade_end: None,
            directional_ambient: None,
            specular_color: None,
            specular_alpha: None,
            fresnel_power: None,
        }
    }

    /// Interior cell flips the reverb send to a subtle wet level.
    /// Pre-fix this was `NEG_INFINITY` regardless of cell type — the
    /// audit's "every cell sounds dry" complaint.
    #[test]
    fn interior_cell_sets_subtle_reverb_send() {
        let mut world = World::new();
        world.insert_resource(byroredux_audio::AudioWorld::default());
        world.insert_resource(cell_lit(true));

        // Pre-condition: default AudioWorld boots with NEG_INFINITY.
        assert!(
            world
                .resource::<byroredux_audio::AudioWorld>()
                .reverb_send_db()
                .is_infinite(),
            "default AudioWorld must boot with NEG_INFINITY reverb send"
        );

        reverb_zone_system(&world, 0.016);

        let aw = world.resource::<byroredux_audio::AudioWorld>();
        assert_eq!(
            aw.reverb_send_db(),
            -12.0,
            "interior cell must set the subtle-wet reverb send level"
        );
    }

    /// Exterior cell keeps the send dry (NEG_INFINITY). Default
    /// already is, but verify the system doesn't accidentally trip
    /// to a finite value on exterior.
    #[test]
    fn exterior_cell_keeps_dry_send() {
        let mut world = World::new();
        world.insert_resource(byroredux_audio::AudioWorld::default());
        world.insert_resource(cell_lit(false));

        reverb_zone_system(&world, 0.016);

        let db = world
            .resource::<byroredux_audio::AudioWorld>()
            .reverb_send_db();
        assert!(
            db.is_infinite() && db.is_sign_negative(),
            "exterior cell must leave reverb send at NEG_INFINITY (got {db})"
        );
    }

    /// Interior → exterior transition flips the send back to dry.
    /// Pin the round trip so a future regression that breaks the
    /// exterior branch (e.g. wrong sign, wrong constant) shows up.
    #[test]
    fn interior_to_exterior_transition_resets_send_to_dry() {
        let mut world = World::new();
        world.insert_resource(byroredux_audio::AudioWorld::default());

        // Tick 1 — interior: send = -12 dB.
        world.insert_resource(cell_lit(true));
        reverb_zone_system(&world, 0.016);
        assert_eq!(
            world
                .resource::<byroredux_audio::AudioWorld>()
                .reverb_send_db(),
            -12.0,
        );

        // Tick 2 — exterior cell load: send must drop back to dry.
        world.insert_resource(cell_lit(false));
        reverb_zone_system(&world, 0.016);
        let db = world
            .resource::<byroredux_audio::AudioWorld>()
            .reverb_send_db();
        assert!(
            db.is_infinite() && db.is_sign_negative(),
            "interior → exterior transition must reset send to NEG_INFINITY (got {db})"
        );
    }

    /// No `CellLightingRes` (engine boot before any cell load) → the
    /// system must no-op without panic. Default AudioWorld send stays
    /// at NEG_INFINITY (= dry, which is the correct safe default).
    #[test]
    fn no_cell_lighting_resource_is_safe_noop() {
        let mut world = World::new();
        world.insert_resource(byroredux_audio::AudioWorld::default());
        // Deliberately omit CellLightingRes.

        reverb_zone_system(&world, 0.016);

        let db = world
            .resource::<byroredux_audio::AudioWorld>()
            .reverb_send_db();
        assert!(
            db.is_infinite() && db.is_sign_negative(),
            "no-CellLightingRes path must leave default send untouched"
        );
    }

    /// No `AudioWorld` (engine started without audio wiring) → the
    /// system must no-op without panic when the resource is absent.
    #[test]
    fn no_audio_world_is_safe_noop() {
        let mut world = World::new();
        world.insert_resource(cell_lit(true));
        // Deliberately omit AudioWorld.

        reverb_zone_system(&world, 0.016);
        // Survival is the assertion — no panic, no aborted run.
    }
}
