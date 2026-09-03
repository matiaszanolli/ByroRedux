//! Unit tests for the `AudioWorld` engine + listener + reverb send.
//! Extracted from `lib.rs` to keep the production code under
//! ~1300 lines; pulled in via `#[cfg(test)] mod tests;`.

use super::*;

/// AudioWorld must construct cleanly even when there's no audio
/// device — CI and headless servers have neither, and a panic
/// here would refuse to launch the engine.
#[test]
fn audio_world_constructs_without_panic_on_any_environment() {
    let _ = AudioWorld::new();
}

/// Regression for #845 / AUD-D4-NEW-04: `DEFAULT_UNLOAD_FADE_MS`
/// matches kira's `Tween::default()` duration (10 ms) so existing
/// call sites that don't override `AudioEmitter.unload_fade_ms`
/// stay on the pre-#845 stop-fade behaviour exactly.
#[test]
fn default_unload_fade_matches_kira_tween_default() {
    let kira_default = Tween::default();
    let expected_ms = kira_default.duration.as_secs_f32() * 1000.0;
    assert!(
        (DEFAULT_UNLOAD_FADE_MS - expected_ms).abs() < 1.0e-3,
        "DEFAULT_UNLOAD_FADE_MS ({DEFAULT_UNLOAD_FADE_MS} ms) must match \
         kira's Tween::default() duration ({expected_ms} ms) — pre-#845 \
         call sites that didn't author this field expected the kira default",
    );
}

/// Regression for #845: a long-tailed cell-ambient emitter MAY
/// declare a non-default `unload_fade_ms`, and that value
/// round-trips through `ActiveSound` so the prune sweep stops
/// the kira handle with the authored fade — not the 10 ms cutoff
/// that produced the audible click on long ambients.
///
/// This test pins the *capture* hop (emitter → ActiveSound)
/// since the prune sweep itself needs a real kira handle to call
/// `.stop(tween)` on. The capture invariant is the part this fix
/// adds; downstream `Tween` construction uses the captured
/// `f32` exactly.
#[test]
fn audio_emitter_authors_custom_unload_fade_for_long_ambients() {
    // Synthetic long-tail ambient: cathedral choir tail = 800 ms.
    let sound_data = StaticSoundData {
        sample_rate: 44_100,
        frames: Arc::new([Frame::ZERO; 64]),
        settings: kira::sound::static_sound::StaticSoundSettings::default(),
        slice: None,
    };
    let emitter = AudioEmitter {
        sound: Arc::new(sound_data),
        attenuation: Attenuation::default(),
        volume: 1.0,
        looping: true,
        unload_fade_ms: 800.0,
    };
    assert_eq!(emitter.unload_fade_ms, 800.0);

    // The Tween construction itself is what `prune_stopped_sounds`
    // does — pin the formula so a future refactor of
    // `fade_ms / 1000.0` doesn't silently round to zero on
    // sub-millisecond inputs or overflow on huge ones.
    let tween = Tween {
        start_time: kira::StartTime::Immediate,
        duration: Duration::from_secs_f32(emitter.unload_fade_ms.max(0.0) / 1000.0),
        easing: kira::Easing::Linear,
    };
    assert!(
        (tween.duration.as_secs_f32() - 0.8).abs() < 1.0e-6,
        "800 ms unload_fade_ms must produce a 0.8s Tween duration, got {:?}",
        tween.duration,
    );
}

/// A negative `unload_fade_ms` (authoring mistake or wraparound)
/// must clamp to 0 rather than panic at `Duration::from_secs_f32`.
/// The prune-stop site's `.max(0.0)` is the guard.
#[test]
fn unload_fade_clamps_negative_to_zero() {
    let fade_ms: f32 = -50.0;
    let clamped = fade_ms.max(0.0);
    let _ = Duration::from_secs_f32(clamped / 1000.0); // must not panic
    assert_eq!(clamped, 0.0);
}

/// Issue #842 regression gate: kira's default `Capacities` caps
/// `sub_track_capacity` at 128, which a populated Bethesda
/// interior cell can saturate (FO4 Diamond City Market ≈ 400
/// emitters in vanilla). Once we hit the cap, kira returns
/// `ResourceLimitReached` from `add_spatial_sub_track` and the
/// dispatch path silently drops the sound with only a `warn!`.
/// This test pins the override; a "simplify back to default"
/// refactor will trip it.
#[test]
fn manager_capacities_exceed_kira_defaults() {
    let defaults = Capacities::default();
    assert!(
        SUB_TRACK_CAPACITY > defaults.sub_track_capacity,
        "SUB_TRACK_CAPACITY={SUB_TRACK_CAPACITY} must exceed kira default \
         {} or populated cells will silently drop sounds (#842)",
        defaults.sub_track_capacity,
    );
    assert!(
        SEND_TRACK_CAPACITY > defaults.send_track_capacity,
        "SEND_TRACK_CAPACITY={SEND_TRACK_CAPACITY} must exceed kira default \
         {} (Phase 4 REGN ambients will need additional send tracks beyond \
         the single global reverb)",
        defaults.send_track_capacity,
    );
}

/// Default attenuation is in the "interior cell" range — a
/// regression that defaults to (0, 0) would silently mute every
/// sound at any distance. Pinned here so a future "simplify
/// defaults" refactor can't lose the range.
#[test]
fn default_attenuation_is_within_interior_range() {
    let a = Attenuation::default();
    assert!(a.min_distance > 0.0);
    assert!(a.max_distance > a.min_distance);
    assert!(a.max_distance >= 10.0, "interior cells need ≥10m falloff");
}

/// Verify the kira static-sound path is wired by synthesising a
/// short sine wave and decoding it through `StaticSoundData`. No
/// audio device required — the data path is independent of the
/// backend. If kira ever reorganises `StaticSoundData::from_*`,
/// this lights up before we ship.
#[test]
fn static_sound_data_constructs_from_decoded_frames() {
    use kira::sound::static_sound::StaticSoundSettings;

    // 0.1 s @ 22.05 kHz of a 440 Hz sine wave, mono. Use kira's
    // `Frame` (stereo float pair) — duplicate the mono sample
    // into both channels.
    let sample_rate: u32 = 22_050;
    let n: usize = (sample_rate as usize) / 10;
    let mut frames: Vec<kira::Frame> = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / sample_rate as f32;
        let s = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        frames.push(kira::Frame { left: s, right: s });
    }
    let sound = StaticSoundData {
        sample_rate,
        frames: Arc::from(frames.into_boxed_slice()),
        settings: StaticSoundSettings::default(),
        slice: None,
    };
    assert!(!sound.frames.is_empty());
    assert_eq!(sound.sample_rate, sample_rate);

    // Wrap in an Arc so the AudioEmitter can hold it. This pins
    // the Arc<StaticSoundData> shape Phase 1 commits to — Phase 2
    // (BSA-backed sounds) should re-use the same Arc handle so
    // a single decoded WAV backs every emitter playing it.
    let arc_sound = Arc::new(sound);
    let _emitter = AudioEmitter {
        sound: Arc::clone(&arc_sound),
        attenuation: Attenuation::default(),
        volume: 1.0,
        looping: false,
        unload_fade_ms: DEFAULT_UNLOAD_FADE_MS,
    };
    assert_eq!(Arc::strong_count(&arc_sound), 2);
}

/// `audio_system` must run cleanly against an empty World even
/// when no `AudioWorld` resource is present (smoke test for the
/// no-op posture under headless/CI).
#[test]
fn audio_system_runs_against_empty_world_without_panic() {
    use byroredux_core::ecs::World;
    let world = World::new();
    audio_system(&world, 0.016);
}

/// `SoundCache` returns `None` on miss and stable `Arc` clones
/// on hit. Lower-case key normalisation: same sound looked up
/// with different casings hits the same slot.
#[test]
fn sound_cache_hits_are_case_insensitive_and_share_arc() {
    use kira::sound::static_sound::StaticSoundSettings;
    let mut cache = SoundCache::new();
    assert!(cache.is_empty());
    assert!(cache.get(r"sound\fx\foo.wav").is_none());

    // Synthesise + insert.
    let sound = StaticSoundData {
        sample_rate: 22_050,
        frames: Arc::from(
            vec![
                kira::Frame {
                    left: 0.0,
                    right: 0.0
                };
                100
            ]
            .into_boxed_slice(),
        ),
        settings: StaticSoundSettings::default(),
        slice: None,
    };
    let inserted = cache.insert(r"sound\fx\Foo.wav", sound);
    assert_eq!(cache.len(), 1);

    // Different casing → same slot.
    let hit_lower = cache.get(r"sound\fx\foo.wav").expect("cache hit");
    let hit_upper = cache
        .get(r"SOUND\FX\FOO.WAV")
        .expect("case-insensitive hit");
    assert!(Arc::ptr_eq(&inserted, &hit_lower));
    assert!(Arc::ptr_eq(&inserted, &hit_upper));
}

/// `get_or_load` only invokes the loader on cache miss, and
/// short-circuits to a stable `Arc` clone on hit. Pinned because
/// a regression that re-decodes per call would silently 10×
/// the per-frame SFX cost without changing any visible behaviour.
#[test]
fn sound_cache_get_or_load_invokes_loader_only_on_miss() {
    use std::cell::Cell;
    let mut cache = SoundCache::new();
    let calls = Cell::new(0_usize);

    // Miss: loader fires, but our synthesised junk bytes won't
    // decode → returns None. The cache stays empty (we only
    // insert on successful decode).
    let result = cache.get_or_load(r"sound\fx\bar.wav", || {
        calls.set(calls.get() + 1);
        vec![0u8; 16] // not a valid audio file
    });
    assert!(result.is_none());
    assert_eq!(calls.get(), 1);
    assert!(cache.is_empty());

    // Insert a real synthetic sound at that path. Subsequent
    // get_or_load must hit the cache without invoking the loader.
    use kira::sound::static_sound::StaticSoundSettings;
    let sound = StaticSoundData {
        sample_rate: 22_050,
        frames: Arc::from(
            vec![
                kira::Frame {
                    left: 0.0,
                    right: 0.0
                };
                50
            ]
            .into_boxed_slice(),
        ),
        settings: StaticSoundSettings::default(),
        slice: None,
    };
    cache.insert(r"sound\fx\bar.wav", sound);

    let hit = cache.get_or_load(r"sound\fx\bar.wav", || {
        calls.set(calls.get() + 1);
        unreachable!("loader must not fire on cache hit");
    });
    assert!(hit.is_some());
    assert_eq!(
        calls.get(),
        1,
        "loader call count unchanged after cache hit"
    );
}

/// **#850 / AUD-D6-NEW-09**: `clear()` drops every cached entry
/// and `bytes_estimate()` reflects the live PCM footprint. Pinned
/// so a future LRU bolt-on can land without breaking the cell-
/// unload contract — the cell-unload path calls `clear()` when a
/// region exits scope; telemetry polls `bytes_estimate()` for
/// `stats` output. A regression that silently drops `clear()` or
/// makes `bytes_estimate()` constant would let mod-heavy
/// long-session memory growth slip past audit.
#[test]
fn sound_cache_clear_drops_entries_and_bytes_estimate_tracks_pcm_size() {
    use kira::sound::static_sound::StaticSoundSettings;
    let mut cache = SoundCache::new();
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.bytes_estimate(), 0);

    // Insert two sounds with known frame counts so the byte
    // estimate is deterministic.
    let frames_a = 100;
    let frames_b = 250;
    let make = |n: usize| StaticSoundData {
        sample_rate: 22_050,
        frames: Arc::from(
            vec![
                kira::Frame {
                    left: 0.0,
                    right: 0.0
                };
                n
            ]
            .into_boxed_slice(),
        ),
        settings: StaticSoundSettings::default(),
        slice: None,
    };
    let arc_a = cache.insert(r"sound\fx\a.wav", make(frames_a));
    cache.insert(r"sound\fx\b.wav", make(frames_b));
    assert_eq!(cache.len(), 2);

    let frame_size = std::mem::size_of::<kira::Frame>();
    let expected = (frames_a + frames_b) * frame_size;
    assert_eq!(
        cache.bytes_estimate(),
        expected,
        "bytes_estimate must sum frame storage across all entries",
    );

    // External `Arc` held by `arc_a` — clear() must drop the
    // cache's clone but the external clone keeps the sound alive.
    cache.clear();
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.bytes_estimate(), 0);
    assert!(cache.is_empty());
    assert!(cache.get(r"sound\fx\a.wav").is_none());
    // External Arc still valid — clear() only drops the cache's
    // own clone. Currently-playing kira handles that took their
    // own Arc clone via play_oneshot survive cell unload.
    assert_eq!(Arc::strong_count(&arc_a), 1);
}

/// **Phase 3**: `spawn_oneshot_at` lays down the canonical
/// component bundle so the audio system picks it up. Pinning
/// the bundle shape here means a future "simplify spawn helpers"
/// refactor can't quietly drop one component (e.g. `OneShotSound`)
/// and break dispatch silently.
#[test]
fn spawn_oneshot_at_creates_correct_component_bundle() {
    use kira::sound::static_sound::StaticSoundSettings;

    let mut world = byroredux_core::ecs::World::new();
    let sound = Arc::new(StaticSoundData {
        sample_rate: 22_050,
        frames: Arc::from(
            vec![
                kira::Frame {
                    left: 0.0,
                    right: 0.0
                };
                50
            ]
            .into_boxed_slice(),
        ),
        settings: StaticSoundSettings::default(),
        slice: None,
    });
    let pos = glam::Vec3::new(10.0, 0.0, 5.0);
    let entity = spawn_oneshot_at(&mut world, sound, pos, Attenuation::default(), 0.8);

    // Every component the audio system needs to dispatch this
    // entity must be present.
    assert!(world.has::<Transform>(entity));
    assert!(world.has::<GlobalTransform>(entity));
    assert!(world.has::<AudioEmitter>(entity));
    assert!(world.has::<OneShotSound>(entity));

    let q = world.query::<Transform>().unwrap();
    let t = q.get(entity).unwrap();
    assert_eq!(t.translation, pos);
    let q = world.query::<AudioEmitter>().unwrap();
    let e = q.get(entity).unwrap();
    assert_eq!(e.volume, 0.8);
    assert!(!e.looping);
}

/// **Phase 3**: when `AudioWorld` is inactive (no audio device,
/// CI / headless), `audio_system` must NOT dispatch — the
/// `OneShotSound` marker stays on the entity so a future tick
/// (or a future fix that brings audio back online) can pick it
/// up. Active-sound count stays at zero. This is the regression
/// gate against a "dispatch even without manager" refactor that
/// would crash on null-handle play.
#[test]
fn audio_system_no_op_when_audio_world_inactive() {
    use kira::sound::static_sound::StaticSoundSettings;
    let mut world = byroredux_core::ecs::World::new();

    // Force-construct an inactive AudioWorld. We can't reliably
    // hit the cpal-init failure path on a dev machine that has
    // a sound card, so build the variant by hand — same shape
    // `AudioWorld::new()` produces when init fails.
    let inactive = AudioWorld {
        active_sounds: Vec::new(),
        pending_oneshots: VecDeque::new(),
        music: None,
        reverb_send: None,
        reverb_send_db: f32::NEG_INFINITY,
        listener: None,
        manager: None,
        multi_listener_warned: false,
        underwater: false,
    };
    world.insert_resource(inactive);

    let sound = Arc::new(StaticSoundData {
        sample_rate: 22_050,
        frames: Arc::from(
            vec![
                kira::Frame {
                    left: 0.0,
                    right: 0.0
                };
                50
            ]
            .into_boxed_slice(),
        ),
        settings: StaticSoundSettings::default(),
        slice: None,
    });
    let pos = glam::Vec3::ZERO;
    let entity = spawn_oneshot_at(&mut world, sound, pos, Attenuation::default(), 1.0);

    audio_system(&world, 0.016);

    // Marker preserved — no dispatch.
    assert!(world.has::<OneShotSound>(entity));
    assert!(world.has::<AudioEmitter>(entity));
    let aw = world.resource::<AudioWorld>();
    assert_eq!(aw.active_sound_count(), 0);
}

/// **Phase 6**: reverb send level defaults to NEG_INFINITY
/// (silent / disabled). The dispatch path explicitly checks
/// `is_finite() && > -60.0` before calling `with_send`, so a
/// fresh AudioWorld never produces audible reverb regardless
/// of whether a send track was created. Pinned because the
/// alternative (default `0.0` = full wet) would produce shocking
/// reverb on every spatial sound the moment audio init succeeded.
#[test]
fn reverb_send_defaults_to_silent() {
    let world = AudioWorld::new();
    assert!(
        world.reverb_send_db().is_infinite() && world.reverb_send_db().is_sign_negative(),
        "reverb send must default to NEG_INFINITY (silent), got {}",
        world.reverb_send_db()
    );
}

/// **Phase 6**: `set_reverb_send_db` persists the new level.
/// New sub-tracks created after the call use the new level;
/// already-playing sounds keep their construction-time level
/// (kira's `with_send` API is build-time only). This test pins
/// only the resource-side state — actual audible reverb on a
/// running stream needs the real-data lifecycle test for that.
#[test]
fn set_reverb_send_db_persists() {
    let mut world = AudioWorld {
        active_sounds: Vec::new(),
        pending_oneshots: VecDeque::new(),
        music: None,
        reverb_send: None,
        reverb_send_db: f32::NEG_INFINITY,
        listener: None,
        manager: None,
        multi_listener_warned: false,
        underwater: false,
    };
    world.set_reverb_send_db(-12.0);
    assert!((world.reverb_send_db() - (-12.0)).abs() < 1e-6);
    world.set_reverb_send_db(f32::NEG_INFINITY);
    assert!(world.reverb_send_db().is_infinite());
}

/// Regression for #3780: `REVERB_FEEDBACK`/`REVERB_DAMPING`/
/// `REVERB_STEREO_WIDTH` were promoted from bare literals inline in the
/// `ReverbBuilder` call to named constants with no other behavior change
/// — this pins their exact values so an edit to the global reverb's sound
/// is a deliberate, reviewable diff to this test, not a silent drift in
/// an unnamed literal. Also pins the documented valid range for each
/// (kira's own doc comments: feedback/stereo_width `0.0..=1.0`, damping
/// unbounded-but-conceptually-normalized) so a future edit can't
/// accidentally land outside what kira's `ReverbBuilder` actually models.
#[test]
fn reverb_parameters_are_named_and_in_kiras_documented_range() {
    assert_eq!(REVERB_FEEDBACK, 0.85);
    assert_eq!(REVERB_DAMPING, 0.6);
    assert_eq!(REVERB_STEREO_WIDTH, 1.0);
    assert!(
        (0.0..=1.0).contains(&REVERB_FEEDBACK),
        "kira: 1.0 gives an infinitely reverberating room"
    );
    assert!(
        (0.0..=1.0).contains(&REVERB_STEREO_WIDTH),
        "kira: 0.0 fully mono, 1.0 fully stereo"
    );
}

#[test]
fn underwater_listener_state_persists() {
    let mut world = AudioWorld::new();
    assert!(!world.underwater());
    world.set_underwater(true);
    assert!(world.underwater());
    world.set_underwater(false);
    assert!(!world.underwater());
}

/// **Phase 5**: `play_music` and `stop_music` are no-ops when
/// the audio world is inactive (no audio device). Pinned because
/// a refactor that "asserts a manager exists" would crash on
/// every headless host on the very first cell-load music event.
#[test]
fn play_music_no_op_when_inactive() {
    // We can't construct a real StreamingSoundData without bytes
    // that decode, so this test only exercises the early-return
    // branch (manager.is_none() → silent return). is_music_active
    // and stop_music are also pinned along the inactive path.
    let mut audio_world = AudioWorld {
        active_sounds: Vec::new(),
        pending_oneshots: VecDeque::new(),
        music: None,
        reverb_send: None,
        reverb_send_db: f32::NEG_INFINITY,
        listener: None,
        manager: None,
        multi_listener_warned: false,
        underwater: false,
    };
    assert!(!audio_world.is_music_active());
    audio_world.stop_music(0.5); // No-op on no-music + no-manager.
    assert!(!audio_world.is_music_active());
}

/// **Phase 5 real-data integration**: open FNV `Fallout - Sound.bsa`,
/// extract the longest OGG (proxy for "music" — vanilla FNV
/// ships music as separate `Fallout - Music.bsa` which we don't
/// require for the test), play it through `play_music`, verify
/// `is_music_active() == true` after dispatch, then `stop_music`
/// and verify it goes inactive.
///
/// `#[ignore]` — needs working audio device + vanilla FNV data.
#[test]
#[ignore = "needs a working audio device and FNV game data"]
fn play_music_drives_streaming_playback_on_real_ogg() {
    use byroredux_bsa::BsaArchive;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    const FNV_DEFAULT: &str = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
    let dir = std::env::var("BYROREDUX_FNV_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(FNV_DEFAULT));
    if !dir.is_dir() {
        return;
    }
    let bsa = match BsaArchive::open(dir.join("Fallout - Sound.bsa")) {
        Ok(b) => b,
        Err(_) => return,
    };
    let bytes = bsa
        .extract(
            r"sound\fx\amb\~regions\goodsprings\oneshots\creak_low\amb_gsinterioroneshots_04.ogg",
        )
        .expect("vanilla creak OGG");

    let streaming = load_streaming_sound_from_bytes(bytes).expect("decode");

    let mut audio_world = AudioWorld::new();
    if !audio_world.is_active() {
        return;
    }

    // play_music with 0.05s fade-in (short so the test is fast).
    audio_world.play_music(streaming, 0.5, 0.05);
    assert!(
        audio_world.is_music_active(),
        "play_music must produce a Playing handle on first dispatch"
    );

    // Let it play briefly, then stop with a fade.
    std::thread::sleep(Duration::from_millis(100));
    audio_world.stop_music(0.05);

    // Poll until inactive (the stop fade completes).
    let deadline = Instant::now() + Duration::from_secs(2);
    while audio_world.is_music_active() {
        if Instant::now() > deadline {
            panic!("stop_music never produced an inactive state");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// **Phase 4**: an `AudioEmitter` with `looping = true` plays
/// past its natural duration. The real-data lifecycle test
/// (Phase 3) confirmed a one-shot stops at ~580ms; this test
/// runs the same sound for a wall-clock duration well past that
/// and asserts the active count stays at 1. Then removes the
/// emitter component (simulating cell unload) and verifies the
/// prune sweep stops the handle.
///
/// `#[ignore]` — needs working audio device + vanilla FNV data.
#[test]
#[ignore = "needs a working audio device and FNV game data"]
fn looping_emitter_survives_natural_duration_and_stops_on_emitter_remove() {
    use byroredux_bsa::BsaArchive;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    const FNV_DEFAULT: &str = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
    let dir = std::env::var("BYROREDUX_FNV_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(FNV_DEFAULT));
    if !dir.is_dir() {
        return;
    }
    let bsa = match BsaArchive::open(dir.join("Fallout - Sound.bsa")) {
        Ok(b) => b,
        Err(_) => return,
    };
    let bytes = bsa
        .extract(r"sound\fx\npc\robotsecuritron\armswing\npc_securitron_armswing_02.wav")
        .expect("vanilla securitron arm-swing");
    let sound = Arc::new(load_sound_from_bytes(bytes).expect("decode WAV"));

    let mut world = byroredux_core::ecs::World::new();
    let aw = AudioWorld::new();
    if !aw.is_active() {
        return;
    }
    world.insert_resource(aw);

    let listener = world.spawn();
    world.insert(listener, Transform::IDENTITY);
    world.insert(listener, GlobalTransform::IDENTITY);
    world.insert(listener, AudioListener);

    // Spawn a looping emitter manually (spawn_oneshot_at sets
    // looping=false; need this branch).
    let emitter = world.spawn();
    let pos = glam::Vec3::new(0.0, 0.0, 5.0);
    world.insert(emitter, Transform::new(pos, glam::Quat::IDENTITY, 1.0));
    world.insert(
        emitter,
        GlobalTransform::new(pos, glam::Quat::IDENTITY, 1.0),
    );
    world.insert(
        emitter,
        AudioEmitter {
            sound: Arc::clone(&sound),
            attenuation: Attenuation::default(),
            volume: 0.5, // half-volume so the test isn't loud
            looping: true,
            unload_fade_ms: DEFAULT_UNLOAD_FADE_MS,
        },
    );
    world.insert(emitter, OneShotSound);

    // Tick — listener creates, emitter dispatches with loop_region.
    audio_system(&world, 0.016);
    assert_eq!(world.resource::<AudioWorld>().active_sound_count(), 1);

    // Wait past the sound's natural duration (~580 ms) plus a
    // safety margin. If looping isn't actually applied, the
    // handle would report Stopped here and the prune would drop it.
    std::thread::sleep(Duration::from_millis(900));
    audio_system(&world, 0.016);
    assert_eq!(
        world.resource::<AudioWorld>().active_sound_count(),
        1,
        "looping emitter must survive past natural duration — \
         loop_region(..) wasn't applied?"
    );

    // Remove the AudioEmitter component to simulate cell unload.
    // The next prune sweep should call .stop(), and a subsequent
    // tick should observe Stopped and drop the entry.
    {
        let mut q = world.query_mut::<AudioEmitter>().unwrap();
        q.remove(emitter);
    }

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        audio_system(&world, 0.016);
        if world.resource::<AudioWorld>().active_sound_count() == 0 {
            break;
        }
        if Instant::now() > deadline {
            panic!(
                "looping sound never reported Stopped after AudioEmitter removal — \
                 prune sweep missed the despawn signal"
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// **#858 / SAFE-23**: a non-looping `AudioEmitter` whose source
/// entity loses its emitter component mid-playback (cell unload,
/// scripted teardown) must be truncated at the kira layer through
/// the same fade-out path as a looping despawned emitter. Pre-fix
/// `prune_stopped_sounds` skipped non-looping sounds entirely and
/// they kept playing at the stale despawn pose until natural
/// termination — audible as faint cross-cell SFX bleed on fast
/// interior↔interior fast-travel.
///
/// Companion to `looping_emitter_survives_natural_duration_and_stops_on_emitter_remove`
/// (Phase 4). Same setup, `looping: false`, racing the remove
/// against the ~580 ms natural duration of the securitron arm-
/// swing so the prune-stop is the observable cause of termination.
///
/// `#[ignore]` — needs working audio device + vanilla FNV data.
#[test]
#[ignore = "needs a working audio device and FNV game data"]
fn non_looping_emitter_stops_on_emitter_remove_regression_858() {
    use byroredux_bsa::BsaArchive;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    const FNV_DEFAULT: &str = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
    let dir = std::env::var("BYROREDUX_FNV_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(FNV_DEFAULT));
    if !dir.is_dir() {
        return;
    }
    let bsa = match BsaArchive::open(dir.join("Fallout - Sound.bsa")) {
        Ok(b) => b,
        Err(_) => return,
    };
    let bytes = bsa
        .extract(r"sound\fx\npc\robotsecuritron\armswing\npc_securitron_armswing_02.wav")
        .expect("vanilla securitron arm-swing");
    let sound = Arc::new(load_sound_from_bytes(bytes).expect("decode WAV"));

    let mut world = byroredux_core::ecs::World::new();
    let aw = AudioWorld::new();
    if !aw.is_active() {
        return;
    }
    world.insert_resource(aw);

    let listener = world.spawn();
    world.insert(listener, Transform::IDENTITY);
    world.insert(listener, GlobalTransform::IDENTITY);
    world.insert(listener, AudioListener);

    // Non-looping emitter — runs to natural termination ~580 ms
    // unless the prune sweep truncates it first.
    let emitter = world.spawn();
    let pos = glam::Vec3::new(0.0, 0.0, 5.0);
    world.insert(emitter, Transform::new(pos, glam::Quat::IDENTITY, 1.0));
    world.insert(
        emitter,
        GlobalTransform::new(pos, glam::Quat::IDENTITY, 1.0),
    );
    world.insert(
        emitter,
        AudioEmitter {
            sound: Arc::clone(&sound),
            attenuation: Attenuation::default(),
            volume: 0.5,
            looping: false,
            unload_fade_ms: DEFAULT_UNLOAD_FADE_MS,
        },
    );
    world.insert(emitter, OneShotSound);

    audio_system(&world, 0.016);
    assert_eq!(world.resource::<AudioWorld>().active_sound_count(), 1);

    // Remove the AudioEmitter long before the ~580 ms natural
    // termination so the truncation, not natural end, is what the
    // prune sweep observes.
    std::thread::sleep(Duration::from_millis(50));
    {
        let mut q = world.query_mut::<AudioEmitter>().unwrap();
        q.remove(emitter);
    }

    // Pre-#858 the active count would stay at 1 for the remaining
    // ~530 ms; post-fix the prune sweep issues `.stop(tween)`
    // within one tick and the next ticks drop the entry once kira
    // reports `Stopped`. Generous deadline absorbs the 10 ms
    // default fade plus kira's async settle.
    let deadline = Instant::now() + Duration::from_millis(400);
    loop {
        audio_system(&world, 0.016);
        if world.resource::<AudioWorld>().active_sound_count() == 0 {
            break;
        }
        if Instant::now() > deadline {
            panic!(
                "non-looping sound was not truncated after AudioEmitter removal — \
                 prune sweep didn't extend to non-looping despawned emitters (#858)"
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// **#853 / C4-NEW-01**: `play_oneshot` drops on the floor when
/// the manager is inactive — the queue never fills on a no-
/// device host. Pre-#853 the queue filled to its 256 cap and
/// pinned ~12 KB + one `Arc<StaticSoundData>` strong-count per
/// cached sound. Pinned here so a refactor that re-enables
/// "queue while inactive for future replay" semantics has to
/// flip this test deliberately.
#[test]
fn play_oneshot_drops_when_manager_inactive() {
    use kira::sound::static_sound::StaticSoundSettings;
    let mut audio_world = AudioWorld {
        active_sounds: Vec::new(),
        pending_oneshots: VecDeque::new(),
        music: None,
        reverb_send: None,
        reverb_send_db: f32::NEG_INFINITY,
        listener: None,
        manager: None,
        multi_listener_warned: false,
        underwater: false,
    };
    let sound = Arc::new(StaticSoundData {
        sample_rate: 22_050,
        frames: Arc::from(
            vec![
                kira::Frame {
                    left: 0.0,
                    right: 0.0
                };
                50
            ]
            .into_boxed_slice(),
        ),
        settings: StaticSoundSettings::default(),
        slice: None,
    });

    // Hammer the API on an inactive world.
    for i in 0..300 {
        audio_world.play_oneshot(
            Arc::clone(&sound),
            glam::Vec3::new(i as f32, 0.0, 0.0),
            Attenuation::default(),
            1.0,
        );
    }
    // Queue must stay empty — Arc strong-count drops back to 1
    // (just our local `sound` binding) so no stale sound data
    // is pinned across the engine lifetime.
    assert_eq!(
        audio_world.pending_oneshot_count(),
        0,
        "inactive audio must drop one-shots, not queue them; got {}",
        audio_world.pending_oneshot_count()
    );
    assert_eq!(
        Arc::strong_count(&sound),
        1,
        "dropped one-shots must release their Arc<StaticSoundData> clone",
    );
}

/// **Phase 3.5 cap pin**: when the manager IS active and the
/// drain pump is somehow not running, the queue still caps at
/// 256 via FIFO drop-oldest. Pinned so a refactor that removes
/// the cap is caught.
#[test]
fn play_oneshot_queue_caps_at_max_pending_when_active() {
    use kira::sound::static_sound::StaticSoundSettings;
    // Acquire a real manager. Skip the test on hosts without a
    // working audio device — the inactive path is covered by
    // `play_oneshot_drops_when_manager_inactive` above.
    let manager = match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("skipping cap-pin test — no audio device: {e}");
            return;
        }
    };
    let mut audio_world = AudioWorld {
        active_sounds: Vec::new(),
        pending_oneshots: VecDeque::new(),
        music: None,
        reverb_send: None,
        reverb_send_db: f32::NEG_INFINITY,
        listener: None,
        manager: Some(manager),
        multi_listener_warned: false,
        underwater: false,
    };
    let sound = Arc::new(StaticSoundData {
        sample_rate: 22_050,
        frames: Arc::from(
            vec![
                kira::Frame {
                    left: 0.0,
                    right: 0.0
                };
                50
            ]
            .into_boxed_slice(),
        ),
        settings: StaticSoundSettings::default(),
        slice: None,
    });

    for i in 0..300 {
        audio_world.play_oneshot(
            Arc::clone(&sound),
            glam::Vec3::new(i as f32, 0.0, 0.0),
            Attenuation::default(),
            1.0,
        );
    }
    assert_eq!(
        audio_world.pending_oneshot_count(),
        256,
        "active queue must cap at 256; got {}",
        audio_world.pending_oneshot_count()
    );
}

/// **Phase 3.5 real-data integration**: queue API drives playback
/// end-to-end. Mirrors the entity-based lifecycle test but uses
/// `play_oneshot` (no entity allocation) — the path a System
/// without `&mut World` would take.
///
/// `#[ignore]` — needs working audio device + vanilla FNV data.
#[test]
#[ignore = "needs a working audio device and FNV game data"]
fn play_oneshot_queue_drives_real_playback() {
    use byroredux_bsa::BsaArchive;
    use std::path::PathBuf;
    use std::time::Instant;

    const FNV_DEFAULT: &str = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
    let dir = std::env::var("BYROREDUX_FNV_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(FNV_DEFAULT));
    if !dir.is_dir() {
        eprintln!("skipping: FNV data dir {:?} not found", dir);
        return;
    }
    let bsa = match BsaArchive::open(dir.join("Fallout - Sound.bsa")) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("skipping: open FNV Sound.bsa: {e}");
            return;
        }
    };
    let bytes = bsa
        .extract(r"sound\fx\npc\robotsecuritron\armswing\npc_securitron_armswing_02.wav")
        .expect("vanilla FNV Sound.bsa must contain securitron arm-swing");
    let sound = Arc::new(load_sound_from_bytes(bytes).expect("decode WAV"));

    let mut world = byroredux_core::ecs::World::new();
    let aw = AudioWorld::new();
    if !aw.is_active() {
        eprintln!("skipping: no audio device on this host");
        return;
    }
    world.insert_resource(aw);

    // Listener at origin.
    let listener = world.spawn();
    world.insert(listener, Transform::IDENTITY);
    world.insert(listener, GlobalTransform::IDENTITY);
    world.insert(listener, AudioListener);

    // Queue a one-shot via the new API. No entity allocation.
    {
        let mut aw = world.resource_mut::<AudioWorld>();
        aw.play_oneshot(
            Arc::clone(&sound),
            glam::Vec3::new(0.0, 0.0, 5.0),
            Attenuation::default(),
            1.0,
        );
        assert_eq!(aw.pending_oneshot_count(), 1);
    }

    // Tick: listener creates, queue drains, dispatch fires.
    audio_system(&world, 0.016);
    {
        let aw = world.resource::<AudioWorld>();
        assert_eq!(
            aw.pending_oneshot_count(),
            0,
            "queue must drain on first tick"
        );
        assert_eq!(
            aw.active_sound_count(),
            1,
            "drained queue items become active sounds"
        );
    }

    // Poll until Stopped (3s timeout).
    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    loop {
        audio_system(&world, 0.016);
        if world.resource::<AudioWorld>().active_sound_count() == 0 {
            break;
        }
        if Instant::now() > deadline {
            panic!("queue-driven sound never reported Stopped within 3s");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// **Phase 3 real-data integration**: open FNV `Fallout - Sound.bsa`,
/// decode a real WAV, spawn it as a one-shot at world origin
/// with a listener nearby, run `audio_system` to dispatch, then
/// poll until kira reports `Stopped` (or a max-iteration cap
/// fires). Verifies the full lifecycle end-to-end on the real
/// cpal backend.
///
/// `#[ignore]` — needs a working audio device AND vanilla FNV
/// game data. Run with:
/// ```sh
/// BYROREDUX_FNV_DATA=<path> cargo test -p byroredux-audio
///   audio_system_full_lifecycle -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs a working audio device and FNV game data"]
fn audio_system_full_lifecycle_on_real_fnv_sound() {
    use byroredux_bsa::BsaArchive;
    use std::path::PathBuf;
    use std::time::Instant;

    const FNV_DEFAULT: &str = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
    let dir = std::env::var("BYROREDUX_FNV_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(FNV_DEFAULT));
    if !dir.is_dir() {
        eprintln!("skipping: FNV data dir {:?} not found", dir);
        return;
    }
    let bsa = match BsaArchive::open(dir.join("Fallout - Sound.bsa")) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("skipping: open FNV Sound.bsa: {e}");
            return;
        }
    };
    let bytes = bsa
        .extract(r"sound\fx\npc\robotsecuritron\armswing\npc_securitron_armswing_02.wav")
        .expect("vanilla FNV Sound.bsa must contain securitron arm-swing");
    let sound = Arc::new(load_sound_from_bytes(bytes).expect("decode real WAV"));

    let mut world = byroredux_core::ecs::World::new();
    let aw = AudioWorld::new();
    if !aw.is_active() {
        eprintln!("skipping: no audio device on this host");
        return;
    }
    world.insert_resource(aw);

    // Listener at origin (default orientation), emitter 5m in
    // front. Inside the (2, 30) attenuation envelope, so we get
    // audible playback rather than a silent test.
    let listener = world.spawn();
    world.insert(listener, Transform::IDENTITY);
    world.insert(listener, GlobalTransform::IDENTITY);
    world.insert(listener, AudioListener);

    let emitter = spawn_oneshot_at(
        &mut world,
        Arc::clone(&sound),
        glam::Vec3::new(0.0, 0.0, 5.0),
        Attenuation::default(),
        1.0,
    );

    // First tick: `sync_listener_pose` creates the listener,
    // then `dispatch_new_oneshots` (running in the same tick)
    // sees the just-created handle and dispatches. Both the
    // OneShotSound removal and the active_sounds insert happen
    // on tick 1.
    audio_system(&world, 0.016);
    assert!(
        !world.has::<OneShotSound>(emitter),
        "tick 1 must dispatch the one-shot — listener creation \
         and dispatch both run inside `audio_system`"
    );
    {
        let aw = world.resource::<AudioWorld>();
        assert_eq!(
            aw.active_sound_count(),
            1,
            "exactly one sound dispatched and tracked"
        );
    }

    // Poll for Stopped. The arm-swing is ~580 ms; cap at 3s
    // wall-clock so a stuck test fails loud rather than hanging.
    let deadline = Instant::now() + std::time::Duration::from_secs(3);
    loop {
        audio_system(&world, 0.016);
        let aw = world.resource::<AudioWorld>();
        if aw.active_sound_count() == 0 {
            break;
        }
        drop(aw);
        if Instant::now() > deadline {
            panic!(
                "audio_system did not prune the active sound within 3s — kira's \
                 PlaybackState::Stopped never reported, or prune logic has a bug"
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // After completion, the emitter entity has no audio
    // components — a downstream cleanup system can despawn it.
    assert!(
        !world.has::<AudioEmitter>(emitter),
        "AudioEmitter must be removed after Stopped"
    );
}

/// **Real-data integration**: extract one WAV and one OGG from
/// vanilla FNV `Fallout - Sound.bsa` and decode each through
/// `load_sound_from_bytes`. Pins the kira ↔ symphonia ↔ BSA
/// path end-to-end against actual game content.
///
/// `#[ignore]` because it needs vanilla FNV game data; run with:
/// ```sh
/// BYROREDUX_FNV_DATA=<path> cargo test -p byroredux-audio
///   real_fnv_sounds_decode -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs FNV game data on disk"]
fn real_fnv_sounds_decode_through_kira() {
    use byroredux_bsa::BsaArchive;
    use std::path::PathBuf;

    const FNV_DEFAULT: &str = "/mnt/data/SteamLibrary/steamapps/common/Fallout New Vegas/Data";
    let dir = std::env::var("BYROREDUX_FNV_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(FNV_DEFAULT));
    if !dir.is_dir() {
        eprintln!("skipping: FNV data dir {:?} not found", dir);
        return;
    }
    let bsa_path = dir.join("Fallout - Sound.bsa");
    let bsa = match BsaArchive::open(&bsa_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("skipping: open {bsa_path:?}: {e}");
            return;
        }
    };

    // Two canonical sample paths verified via probe_extensions
    // 2026-05-05: a securitron arm-swing WAV and a Goodsprings
    // ambient creak OGG. If either disappears from the vanilla
    // archive in a future patch, the test fails loud and the
    // sample list above gets refreshed.
    let cases: &[(&str, &str)] = &[
        (
            "wav",
            r"sound\fx\npc\robotsecuritron\armswing\npc_securitron_armswing_02.wav",
        ),
        (
            "ogg",
            r"sound\fx\amb\~regions\goodsprings\oneshots\creak_low\amb_gsinterioroneshots_04.ogg",
        ),
    ];

    for (label, path) in cases {
        let bytes = bsa
            .extract(path)
            .unwrap_or_else(|e| panic!("vanilla FNV BSA must contain {path}: {e}"));
        assert!(!bytes.is_empty(), "{label}: empty extract");
        let sound = load_sound_from_bytes(bytes).unwrap_or_else(|e| {
            panic!("{label} decode failed for {path}: {e}");
        });
        eprintln!(
            "[M44 P2] {label} '{path}' → {} frames @ {} Hz",
            sound.frames.len(),
            sound.sample_rate
        );
        assert!(
            sound.frames.len() > 100,
            "{label}: short decode ({} frames) — symphonia may have aborted",
            sound.frames.len()
        );
        assert!(
            sound.sample_rate >= 11_025 && sound.sample_rate <= 48_000,
            "{label}: unexpected sample rate {} Hz",
            sound.sample_rate
        );
    }
}

/// #1612 — a reversed Attenuation (min > max) must normalize to a sane
/// range, never hand kira a `min > max` that panics `f32::clamp` on the
/// audio render thread at playback.
#[test]
fn reversed_attenuation_normalizes_instead_of_panicking() {
    let att = Attenuation {
        min_distance: 30.0,
        max_distance: 2.0,
    };
    let range = att.distance_range();
    assert!(range.start() <= range.end());
    assert_eq!(*range.start(), 2.0);
    assert_eq!(*range.end(), 30.0);

    // A well-ordered range passes through unchanged.
    let ok = Attenuation {
        min_distance: 0.5,
        max_distance: 12.0,
    };
    assert_eq!(ok.distance_range(), 0.5..=12.0);
}

/// AUD-2026-08-07-D5-01 — the reverb-send gate that both
/// `drain_pending_oneshots` and `dispatch_new_oneshots` now share via
/// `apply_reverb_send` must key off `SILENCE_DB`, not a re-typed
/// literal. Pins the boundary at exactly `SILENCE_DB` (exclusive) and
/// the two documented callers' sentinel/typical values.
#[test]
fn reverb_send_gate_matches_silence_db_boundary() {
    // Below or exactly at the floor: gate stays closed.
    assert!(!reverb_send_gate_open(SILENCE_DB));
    assert!(!reverb_send_gate_open(SILENCE_DB - 0.01));
    // f32::NEG_INFINITY is the documented "reverb off" construction
    // default (`AudioWorld::new`) — must never open the gate.
    assert!(!reverb_send_gate_open(f32::NEG_INFINITY));
    // NaN must not slip through via a `>` comparison quirk.
    assert!(!reverb_send_gate_open(f32::NAN));

    // Above the floor: gate opens. -12.0 dB is the interior send level
    // `reverb_zone_system` applies (byroredux/src/systems/audio.rs).
    assert!(reverb_send_gate_open(SILENCE_DB + 0.01));
    assert!(reverb_send_gate_open(-12.0));
    assert!(reverb_send_gate_open(0.0));
}

/// #2394 / ECS-D7-2026-08-07-01 — the one-shot dispatch loop must
/// consume the `OneShotSound` marker on its two failure arms, not just
/// on success. Leaving the marker re-collects the entity into `pending`
/// every subsequent frame: a `warn!` per entity per frame at 60 Hz, and
/// — when only `track.play` fails — a spatial sub-track allocated and
/// immediately dropped every frame for the rest of the session.
/// `prune_stopped_sounds` cannot clean up after it, because a failed
/// dispatch never pushed an `ActiveSound`.
///
/// Both arms are kira failures (sub-track resource exhaustion, an
/// unplayable `StaticSoundData`) that no headless test can force — with
/// no audio device `audio_system` returns at the `manager.is_none()`
/// gate long before this loop, which is exactly what
/// `audio_system_no_op_when_audio_world_inactive` above pins. So this
/// pins the fix at the source level instead, matching the convention
/// used for device-only renderer paths.
#[test]
fn oneshot_marker_is_consumed_on_both_dispatch_failure_arms() {
    const LIB_RS: &str = include_str!("lib.rs");

    for arm in [
        "M44 Phase 3: add_spatial_sub_track failed for entity",
        "M44 Phase 3: track.play failed for entity",
    ] {
        let arm_pos = LIB_RS
            .find(arm)
            .unwrap_or_else(|| panic!("the `{arm}` failure arm must still exist"));
        // Slice from the log call to the `continue` that ends the arm.
        let continue_pos = LIB_RS[arm_pos..]
            .find("continue;")
            .map(|i| arm_pos + i)
            .unwrap_or_else(|| panic!("the `{arm}` arm must still end in `continue`"));
        let body = &LIB_RS[arm_pos..continue_pos];
        assert!(
            body.contains("consumed.push(p.entity);"),
            "the `{arm}` arm must push onto `consumed` before continuing — a \
             failed one-shot is still a consumed one-shot, or the entity \
             re-dispatches (and re-warns) every frame forever (#2394)",
        );
    }

    // The success path must consume too — three pushes total, one per
    // loop exit. A future arm added without a push would leave the
    // count at 3 only if it also skipped the marker, so pin the shape.
    assert_eq!(
        LIB_RS.matches("consumed.push(p.entity);").count(),
        3,
        "expected exactly 3 `consumed.push` sites in the one-shot dispatch \
         loop (two failure arms + the success path) — a new exit path that \
         doesn't consume the marker reintroduces #2394",
    );
}

// ---------------------------------------------------------------------------
// #3178 — the BU -> metre seam at the audio boundary.
// ---------------------------------------------------------------------------

/// The conversion the whole spatial contract rests on: an emitter parked at
/// `Attenuation::default().max_distance` metres of world distance must land
/// exactly on that attenuation bound in kira space.
#[test]
fn emitter_at_max_distance_in_bu_lands_on_max_distance_in_audio_space() {
    let max_m = Attenuation::default().max_distance;
    // Listener at the origin, emitter `max_distance` metres away in BU.
    let emitter_bu = Vec3::new(0.0, 0.0, max_m * BETHESDA_UNITS_PER_METER);
    let converted = bu_to_audio_space(emitter_bu);

    assert!(
        (converted.length() - max_m).abs() < 1.0e-3,
        "emitter {max_m} m away resolved to {} in kira space — the BU->metre \
         seam is wrong, which silently shrinks every audible radius by ~70x \
         (#3178)",
        converted.length(),
    );
    // The pre-fix behaviour, pinned so it cannot come back: raw BU would put
    // this emitter 70x past `max_distance` and therefore at Decibels::SILENCE.
    assert!(
        emitter_bu.length() > max_m * 10.0,
        "sanity: raw BU must be far outside the metre-authored bound"
    );
}

/// The default attenuation band, stated in the units it is authored in.
#[test]
fn default_attenuation_band_is_metres_not_bethesda_units() {
    let a = Attenuation::default();
    assert_eq!((a.min_distance, a.max_distance), (2.0, 30.0));
    // 2 m .. 30 m of world is 140 BU .. 2100 BU. If someone "fixes" this by
    // pre-scaling the constants instead of converting at the boundary, kira's
    // metre-scaled ear model (EAR_DISTANCE = 0.1) ends up in the wrong space.
    assert!(
        a.max_distance < BETHESDA_UNITS_PER_METER,
        "Attenuation constants look pre-scaled into Bethesda units — they must \
         stay in metres; the conversion belongs in bu_to_audio_space (#3178)"
    );
}

/// SIBLING guard for #3178: **every** position handed to kira crosses the
/// seam. One missed site is a silent half-fix, and none of them are
/// observable from a unit test of behaviour.
#[test]
fn every_kira_position_site_goes_through_the_unit_seam() {
    // Whitespace-normalized so rustfmt's line wrapping can't break the needles.
    let lib_rs: String = squeeze_whitespace(include_str!("lib.rs"));
    let lib_rs = lib_rs.as_str();
    for needle in [
        "mgr.add_listener(bu_to_audio_space(pose.0), pose.1)",
        "handle.set_position(bu_to_audio_space(pose.0), Tween::default())",
    ] {
        assert!(
            lib_rs.contains(needle),
            "listener position site no longer converts BU -> metres: \
             expected `{needle}` (#3178)"
        );
    }
    // Both dispatch paths (queue + entity).
    assert_eq!(
        lib_rs
            .matches("add_spatial_sub_track( listener_id, bu_to_audio_space(p.position)")
            .count(),
        2,
        "expected both spatial dispatch paths to convert BU -> metres (#3178)"
    );
    assert!(
        !lib_rs.contains("add_spatial_sub_track(listener_id, p.position"),
        "a spatial dispatch path still passes raw Bethesda units to kira (#3178)"
    );
}

// ---------------------------------------------------------------------------
// #3179 — the above-water filter state must be a real bypass.
// ---------------------------------------------------------------------------

/// `FilterBuilder::default()` is `Mix::WET`, so "above water" is only a bypass
/// if the dry mix is set explicitly. kira blends
/// `output * sqrt(mix) + input * sqrt(1 - mix)`, so `Mix::DRY` is bit-exact
/// passthrough at every device sample rate.
#[test]
fn above_water_filter_state_is_a_dry_bypass() {
    assert_eq!(underwater_mix(false), Mix::DRY);
    assert_eq!(underwater_mix(true), Mix::WET);
    // The default this overrides — if kira ever changes it, the override is
    // still correct, but the reason in the docs would need revisiting.
    assert_eq!(
        kira::effect::filter::FilterBuilder::default().mix,
        kira::Value::Fixed(Mix::WET),
        "kira's FilterBuilder default is no longer Mix::WET — re-read the \
         bypass rationale on apply_underwater_filter (#3179)"
    );
}

#[test]
fn underwater_cutoff_matches_the_submersion_state() {
    assert_eq!(underwater_cutoff_hz(true), UNDERWATER_CUTOFF_HZ);
    assert_eq!(underwater_cutoff_hz(false), ABOVE_WATER_CUTOFF_HZ);
}

/// #3179 — the above-water cutoff must stay strictly below half of the lowest
/// device rate we expect. kira computes
/// `g = tan(PI * clamp(f_c / f_s, 0.0001, 0.5))`, so a cutoff at or above
/// Nyquist pins the clamp at 0.5 and yields `tan(PI/2)` ~ 1.6e16 with
/// `a1 = 1/(1 + g*(g + k))` ~ 3.7e-33 — a numerically degenerate filter, not a
/// transparent one. This is why transparency is bought with `Mix::DRY` rather
/// than by parking the cutoff beyond Nyquist.
#[test]
fn above_water_cutoff_never_reaches_the_kira_clamp_ceiling() {
    const LOWEST_PLAUSIBLE_DEVICE_RATE_HZ: f64 = 44_100.0;
    let nyquist = LOWEST_PLAUSIBLE_DEVICE_RATE_HZ / 2.0;
    assert!(
        ABOVE_WATER_CUTOFF_HZ < nyquist,
        "ABOVE_WATER_CUTOFF_HZ ({ABOVE_WATER_CUTOFF_HZ} Hz) is at or above \
         Nyquist for a {LOWEST_PLAUSIBLE_DEVICE_RATE_HZ} Hz device \
         ({nyquist} Hz) — that pins kira's clamp at 0.5 and makes the filter \
         numerically degenerate (#3179)"
    );
}

/// SIBLING guard for #3179: the filter is constructed in exactly one place, so
/// a future change cannot land on one dispatch path only — the same failure
/// #2405 was filed for on the reverb-send gate three lines above.
#[test]
fn both_dispatch_paths_build_the_filter_through_one_call_site() {
    let lib_rs = squeeze_whitespace(include_str!("lib.rs"));
    assert_eq!(
        lib_rs.matches("FilterBuilder::new()").count(),
        1,
        "the underwater filter is constructed in more than one place — route \
         both dispatch paths through apply_underwater_filter (#3179 / #2405)"
    );
    assert_eq!(
        lib_rs
            .matches("apply_underwater_filter(&mut track_builder, underwater)")
            .count(),
        2,
        "expected both dispatch paths to call the shared filter helper (#3179)"
    );
}

/// Collapse every run of whitespace to a single space so source assertions
/// survive rustfmt re-wrapping a call across lines.
fn squeeze_whitespace(src: &str) -> String {
    src.split_whitespace().collect::<Vec<_>>().join(" ")
}
