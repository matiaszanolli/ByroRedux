//! The per-frame `audio_system` and its sub-steps.

use super::*;

/// Per-frame audio update — synchronises listener pose, follows moving
/// emitters, plays new one-shots through per-emitter spatial sub-tracks,
/// prunes finished sounds. `Stage::Late` is the canonical home (after
/// transform propagation has produced final world poses for the listener
/// and every emitter).
///
/// The six passes, in body order:
///
/// 1. **Listener sync** ([`sync_listener_pose`]): locate the (single)
///    `AudioListener` entity. On first frame, lazily call
///    `manager.add_listener` with its `GlobalTransform`. On subsequent
///    frames, push pose updates through `ListenerHandle::set_position` /
///    `set_orientation`. Positions cross the BU→metre seam here
///    ([`bu_to_audio_space`]).
/// 2. **Emitter follow** ([`sync_emitter_positions`], #3086): for every
///    entity-backed active sound, push the source entity's current
///    `GlobalTransform` into its spatial sub-track so a sound emitted
///    from a moving actor stays anchored to it. Change-gated — a
///    stationary emitter pushes zero commands per tick (the counter is
///    [`AudioWorld::emitter_position_updates`]).
/// 3. **Underwater filters** ([`update_underwater_filters`]): drive every
///    live sub-track's low-pass cutoff and wet/dry mix to match
///    [`AudioWorld::underwater`], which `water_audio_system` sets from the
///    camera's `SubmersionState`.
/// 4. **Drain the pending queue** ([`AudioWorld::play_oneshot`]'s Phase 3.5
///    path): dispatch each queued entry through the spatial-sub-track path.
///    No entity allocation — this is the API for Systems, which cannot spawn.
/// 5. **Dispatch new one-shots**: for each entity carrying both
///    `OneShotSound` + `AudioEmitter`, create a spatial sub-track
///    anchored at the entity's `GlobalTransform`, play the sound on
///    that track, and remove `OneShotSound` so the dispatcher won't
///    re-trigger next frame. The `AudioEmitter` stays so callers
///    can query "is this entity still playing?" via the active list.
/// 6. **Prune stopped**: walk `active_sounds`, drop any whose handle
///    reports `PlaybackState::Stopped`. Removing the entity's
///    `AudioEmitter` lets a downstream cleanup system (or the cell
///    unloader) despawn it without coupling to audio state.
pub fn audio_system(world: &World, _dt: f32) {
    let Some(mut audio_world) = world.try_resource_mut::<AudioWorld>() else {
        return;
    };
    if !audio_world.is_active() {
        return;
    }

    sync_listener_pose(world, &mut audio_world);
    sync_emitter_positions(world, &mut audio_world);
    update_underwater_filters(&mut audio_world);
    drain_pending_oneshots(&mut audio_world);
    dispatch_new_oneshots(world, &mut audio_world);
    prune_stopped_sounds(world, &mut audio_world);
}

/// Push every entity-backed active sound's spatial sub-track to its
/// source entity's current `GlobalTransform` (#3086).
///
/// Pre-fix, an emitter's position was captured once at dispatch and
/// never refreshed — the `AudioEmitter` docstring promised a per-frame
/// update the system never performed, so footsteps, weapon fire and
/// looping ambients on moving actors (or vehicles) detached from their
/// source. The listener half was always updated per frame, which made
/// the failure directional and easy to misread as a listener-pose bug.
///
/// Queue-driven detached sounds are fire-and-forget by contract — there
/// is no entity to follow — and music is non-spatial, so neither is
/// touched here. `Anchored` queue plays (#5410, dialogue voice) follow
/// exactly like emitters. An entity that has been despawned or lost its
/// `GlobalTransform` mid-playback is skipped; the prune sweep stops both
/// `Emitter` sounds on emitter removal and `Anchored` sounds on transform
/// loss.
///
/// Per-tick cost: one `GlobalTransform` lookup per entity-backed
/// active sound plus one kira `set_position` command **only on ticks
/// where the source actually moved** — the dispatch position seeds
/// `ActiveSound::last_position`, so a stationary emitter (the vast
/// majority: torches, ambient loops, machinery) costs a comparison and
/// nothing else. The listener update above remains unconditional, as it
/// always was.
pub(crate) fn sync_emitter_positions(world: &World, audio_world: &mut AudioWorld) {
    let Some(gt_q) = world.query::<GlobalTransform>() else {
        return;
    };
    for active in &mut audio_world.active_sounds {
        let Some(entity) = active.source.followed_entity() else {
            continue;
        };
        let Some(gt) = gt_q.get(entity) else {
            continue;
        };
        let position = bu_to_audio_space(gt.translation);
        if active.last_position == Some(position) {
            continue;
        }
        active.track.set_position(position, Tween::default());
        active.last_position = Some(position);
        audio_world.emitter_position_updates += 1;
    }
}

/// Drive every live sub-track's low-pass to match the listener's submersion
/// state. Both the cutoff **and** the wet/dry mix are tweened: the mix is what
/// actually engages and bypasses the filter (#3179), the cutoff shapes the
/// crossfade in between.
pub(crate) fn update_underwater_filters(audio_world: &mut AudioWorld) {
    let underwater = audio_world.underwater;
    let cutoff = underwater_cutoff_hz(underwater);
    let mix = underwater_mix(underwater);
    for active in &mut audio_world.active_sounds {
        if active.underwater == underwater {
            continue;
        }
        active
            .underwater_filter
            .set_cutoff(cutoff, Tween::default());
        active.underwater_filter.set_mix(mix, Tween::default());
        active.underwater = underwater;
    }
}

/// Find the (first) `AudioListener` entity in the world, read its
/// `GlobalTransform`, and either lazy-create the kira listener or
/// push a pose update through the existing handle.
///
/// **Listener handle reuse contract (#849):** the kira listener
/// handle (`audio_world.listener`) is created lazily on the first
/// frame an `AudioListener` is observed and **never cleared**.
/// When the entity carrying the marker is despawned this function
/// early-returns at the first `iter.next()`; on the next respawn
/// (third-person camera transition, fly-cam swap, save-load cycle)
/// the existing handle's pose is updated rather than a fresh
/// `add_listener` call. This is intentional: kira's
/// `listener_capacity` is 8 (`kira-0.12.5`'s `ManagerSettings`
/// default), so a "clear on missing entity → re-add on respawn" simplification
/// would burn through that capacity on a bursty
/// debug-fly-cam-destroy-create loop and lock out future spawns.
/// Future maintainers must keep the `listener` field sticky across
/// entity churn.
pub(crate) fn sync_listener_pose(world: &World, audio_world: &mut AudioWorld) {
    let listener_entity = {
        let Some(q) = world.query::<AudioListener>() else {
            return;
        };
        // Diagnose multi-listener scenarios on the *first* frame the
        // count exceeds 1, then debounce so third-person camera
        // transitions / fly-cam swaps don't spam the log per-frame
        // for the brief window where two listener entities coexist.
        // The crate docstring on `AudioListener` documents the
        // "first wins" iteration policy; this surfaces it. See #843.
        let mut iter = q.iter();
        let Some((entity, _)) = iter.next() else {
            return;
        };
        if iter.next().is_some() && !audio_world.multi_listener_warned {
            // We've already pulled two; count remaining for an
            // accurate total in the warn message.
            let extra = iter.count();
            let total = 2 + extra;
            log::warn!(
                "M44: multiple AudioListener entities found ({total}); \
                 using whichever the query iteration produced first \
                 ({entity:?}). Cell-load / fly-cam swap usually leaves \
                 only the active camera tagged — check for a stale \
                 marker on a despawning entity."
            );
            audio_world.multi_listener_warned = true;
        }
        entity
    };
    let pose = {
        let Some(q) = world.query::<GlobalTransform>() else {
            return;
        };
        let Some(gt) = q.get(listener_entity) else {
            return;
        };
        (gt.translation, gt.rotation)
    };
    if audio_world.listener.is_none() {
        let Some(mgr) = audio_world.manager.as_mut() else {
            return;
        };
        match mgr.add_listener(bu_to_audio_space(pose.0), pose.1) {
            Ok(handle) => {
                log::info!(
                    "M44 Phase 3: kira listener created at ({:.1},{:.1},{:.1})",
                    pose.0.x,
                    pose.0.y,
                    pose.0.z,
                );
                audio_world.listener = Some(handle);
            }
            Err(e) => {
                log::warn!("M44 Phase 3: add_listener failed: {e}");
            }
        }
    } else if let Some(handle) = audio_world.listener.as_mut() {
        handle.set_position(bu_to_audio_space(pose.0), Tween::default());
        handle.set_orientation(pose.1, Tween::default());
    }
}

/// Drain the `play_oneshot` queue and dispatch each entry through a
/// fresh spatial sub-track. Entity-less; queued items have no
/// associated `EntityId`. Logs at WARN if a single tick drains more
/// than 32 items — that's footstep-tempo gone wrong, audible signal
/// that something upstream is firing per-frame instead of per-stride.
/// #5738 — the shared spatial one-shot start both dispatch loops run:
/// the distance-windowed builder, the Phase-6 reverb send, the
/// underwater filter and the kira sub-track. Each new send/filter
/// (underwater #3086, reverb Phase 6) had to be added to both loops by
/// hand; now it lands here once. The play itself stays at the call
/// sites — the emitter path needs `loop_region(..)` before it. `Err`
/// is a pre-localised message the callers log with their own context.
pub(crate) fn build_spatial_track(
    mgr: &mut AudioManager,
    listener_id: kira::listener::ListenerId,
    position: byroredux_core::math::Vec3,
    attenuation: Attenuation,
    reverb: Option<&SendTrackHandle>,
    reverb_db: f32,
    underwater: bool,
) -> Result<(SpatialTrackHandle, FilterHandle), String> {
    // kira's `SpatialTrackBuilder::distances` accepts a
    // `RangeInclusive<f32>`; the values are min..=max **metres**
    // (#3178 — the position handed to kira alongside them is converted
    // from BU by `bu_to_audio_space`), falloff between is linear.
    let track_builder = SpatialTrackBuilder::new().distances(attenuation.distance_range());
    let track_builder = apply_reverb_send(track_builder, reverb, reverb_db);
    let mut track_builder = track_builder;
    let underwater_filter = apply_underwater_filter(&mut track_builder, underwater);
    let track = mgr
        .add_spatial_sub_track(listener_id, bu_to_audio_space(position), track_builder)
        .map_err(|e| format!("add_spatial_sub_track failed: {e}"))?;
    Ok((track, underwater_filter))
}

pub(crate) fn drain_pending_oneshots(audio_world: &mut AudioWorld) {
    let Some(listener_id) = audio_world.listener.as_ref().map(|l| l.id()) else {
        return;
    };
    if audio_world.pending_oneshots.is_empty() {
        return;
    }
    // Manager-active gate moves *before* the drain below (#851).
    // Pre-fix, an unconditional drain ran first, so on a hypothetical
    // `manager = None` re-entry the drained items would be silently
    // dropped — the `// Inactive — queue cleared` branch below was
    // reachable in theory and would have lost the queued one-shots
    // forever. In practice the parent `audio_system` early-returns at
    // `is_active()` before calling this helper, so the manager is
    // always `Some` here, but the defensive ordering keeps the
    // contract local: we only consume the queue once we know we can
    // dispatch its contents.
    let Some(mgr) = audio_world.manager.as_mut() else {
        return;
    };
    if audio_world.pending_oneshots.len() > 32 {
        log::warn!(
            "M44 Phase 3.5: drained {} pending one-shots in one tick — \
             upstream system is firing too fast (footstep stride, weapon \
             rate-of-fire, dialogue queue?)",
            audio_world.pending_oneshots.len()
        );
    }
    // #3521 (AUD-2026-08-27-D1-01) — `VecDeque::drain(..)` in place,
    // not the old swap-in-a-fresh-default-then-consume-by-value shape.
    // That prior approach replaced the live queue with a fresh,
    // zero-capacity default and dropped the old (capacity-holding) one
    // at the end of this function, so every tick that drained anything
    // paid an allocate+free pair on the next `play_oneshot` push.
    // `drain(..)` empties `pending_oneshots` in place and keeps its
    // allocated capacity for the next fill — same class of fix as
    // `#932` / `#3059` / `#3257`. `mgr` (borrowed from
    // `audio_world.manager`) and the field accesses below
    // (`reverb_send`, `active_sounds`, `underwater`) are all disjoint
    // fields from `pending_oneshots`, so the borrow checker accepts
    // draining it in place across the loop.
    //
    // NOTE for future editors: this comment deliberately never spells
    // out the old approach's own method-path text — the sibling
    // regression test in `tests.rs` scans this function's source for
    // exactly that substring, and writing it here would make the test
    // match its own describing comment instead of real code.
    for p in audio_world.pending_oneshots.drain(..) {
        // #5738 — the shared track build + sends, then the queue path's
        // straight play (no loop region on this branch).
        let (mut track, underwater_filter) = match build_spatial_track(
            mgr,
            listener_id,
            p.position,
            p.attenuation,
            audio_world.reverb_send.as_ref(),
            audio_world.reverb_send_db,
            audio_world.underwater,
        ) {
            Ok(built) => built,
            Err(e) => {
                log::warn!("M44 Phase 3.5: {e}");
                continue;
            }
        };
        // kira reasons about gain in decibels; gameplay reasons in linear
        // amplitude. `linear_volume_to_db` does the conversion + silence
        // clamp; `volume()` returns a fresh settings value, not new audio.
        let db = linear_volume_to_db(p.volume);
        let sound = (*p.sound).clone().volume(db);
        let handle = match track.play(sound) {
            Ok(h) => h,
            Err(e) => {
                log::warn!("M44 Phase 3.5: track.play (queue) failed: {e}");
                continue;
            }
        };
        audio_world.active_sounds.push(ActiveSound {
            source: p.source,
            handle,
            track,
            // `Detached`: the track sits at the queued position for its
            // whole life (nothing to follow), so the seeded value is
            // final (#3086). `Anchored` (#5410): the dispatch position
            // anchors the track and the follow pass corrects it on the
            // first tick the speaker has moved.
            last_position: Some(bu_to_audio_space(p.position)),
            underwater_filter,
            underwater: audio_world.underwater,
            // Queue-driven sounds carry no emitter lifecycle — `Detached`
            // runs to natural termination as `play_oneshot`'s documented
            // contract requires, and `Anchored` stops only through
            // `stop_sounds_for` (#5410). The prune sweep's
            // emitter-presence check skips both, and
            // `unload_fade_ms` is never consulted on this branch.
            unload_fade_ms: DEFAULT_UNLOAD_FADE_MS,
            // Queue-driven sounds never re-enter the prune sweep's
            // stop branch, so this flag stays `false` until an explicit
            // `stop_sounds_for` flips it. See #844 / #858 / #5410.
            stop_issued: false,
        });
    }
}

/// Iterate `OneShotSound + AudioEmitter` entities; for each, create
/// a spatial sub-track anchored at the entity's world position, play
/// the sound on that track, and remove `OneShotSound` so the entity
/// isn't re-dispatched next frame. The track + handle land in
/// `active_sounds` so they outlive the helper-function scope.
pub(crate) fn dispatch_new_oneshots(world: &World, audio_world: &mut AudioWorld) {
    let Some(listener_id) = audio_world.listener.as_ref().map(|l| l.id()) else {
        // No listener yet — defer dispatch. The next frame's
        // `sync_listener_pose` will create it; one-shots queued this
        // frame will dispatch then.
        return;
    };

    // Snapshot the (entity, sound, attenuation, volume, position) tuple
    // for every new one-shot before mutating storages. Locks held
    // across `manager.add_spatial_sub_track` would otherwise
    // collide with the per-emitter component reads.
    struct Pending {
        entity: EntityId,
        sound: Arc<StaticSoundData>,
        attenuation: Attenuation,
        volume: f32,
        position: Vec3,
        looping: bool,
        unload_fade_ms: f32,
    }
    let mut pending: Vec<Pending> = Vec::new();
    {
        let Some(oneshot_q) = world.query::<OneShotSound>() else {
            return;
        };
        let Some(emitter_q) = world.query::<AudioEmitter>() else {
            return;
        };
        let Some(gt_q) = world.query::<GlobalTransform>() else {
            return;
        };
        for (entity, _) in oneshot_q.iter() {
            let Some(emitter) = emitter_q.get(entity) else {
                continue;
            };
            let Some(gt) = gt_q.get(entity) else {
                continue;
            };
            pending.push(Pending {
                entity,
                sound: Arc::clone(&emitter.sound),
                attenuation: emitter.attenuation,
                volume: emitter.volume,
                position: gt.translation,
                looping: emitter.looping,
                unload_fade_ms: emitter.unload_fade_ms,
            });
        }
    }

    if pending.is_empty() {
        return;
    }

    let Some(mgr) = audio_world.manager.as_mut() else {
        return;
    };
    // Entities whose `OneShotSound` marker is spent this frame —
    // successes *and* failures alike (#2394 / ECS-D7-2026-08-07-01). A
    // failed one-shot is still a consumed one-shot: leaving the marker
    // on a dispatch failure re-collects the entity into `pending` every
    // subsequent frame, which at 60 Hz is one `warn!` per entity per
    // frame plus (when only `track.play` fails) a spatial sub-track
    // allocated and dropped per frame, forever. `prune_stopped_sounds`
    // can't clean up after it either: it walks `active_sounds`, and a
    // failed dispatch never pushed an `ActiveSound`.
    let mut consumed: Vec<EntityId> = Vec::with_capacity(pending.len());
    for p in pending {
        // #5738 — the shared track build + sends; the play follows the
        // loop-region decision below.
        let (mut track, underwater_filter) = match build_spatial_track(
            mgr,
            listener_id,
            p.position,
            p.attenuation,
            audio_world.reverb_send.as_ref(),
            audio_world.reverb_send_db,
            audio_world.underwater,
        ) {
            Ok(built) => built,
            Err(e) => {
                log::warn!(
                    "M44 Phase 3: add_spatial_sub_track failed for entity {:?}: {e}",
                    p.entity
                );
                // Consume the marker anyway — see `consumed`'s note.
                consumed.push(p.entity);
                continue;
            }
        };
        // kira reasons about gain in decibels; gameplay reasons in linear
        // amplitude. `linear_volume_to_db` does the conversion + silence
        // clamp. The underlying `Arc<[Frame]>` is reused — `volume()` returns
        // a fresh `StaticSoundData` value with new settings, not new audio.
        let db = linear_volume_to_db(p.volume);
        let mut sound = (*p.sound).clone().volume(db);
        if p.looping {
            // Phase 4: kira's `loop_region(..)` enables full-region
            // looping. When the source entity is despawned externally
            // (cell unload), the cleanup-looping sweep notices the
            // missing entity and stops the handle.
            sound = sound.loop_region(..);
        }
        let handle = match track.play(sound) {
            Ok(h) => h,
            Err(e) => {
                log::warn!(
                    "M44 Phase 3: track.play failed for entity {:?}: {e}",
                    p.entity
                );
                // Consume the marker anyway — see `consumed`'s note.
                consumed.push(p.entity);
                continue;
            }
        };
        audio_world.active_sounds.push(ActiveSound {
            source: SoundSource::Emitter(p.entity),
            handle,
            track,
            // The sub-track was created AT the dispatch position, so that
            // is the seeded `last_position` — a stationary emitter's
            // follow pass is a comparison per tick, not a command (#3086).
            last_position: Some(bu_to_audio_space(p.position)),
            underwater_filter,
            underwater: audio_world.underwater,
            unload_fade_ms: p.unload_fade_ms,
            stop_issued: false,
        });
        consumed.push(p.entity);
    }

    // Clear the OneShotSound marker on every entity whose dispatch was
    // attempted — started or failed — so we don't re-dispatch next
    // frame. AudioEmitter stays — callers can observe "is this entity
    // still playing?" through the active list.
    if !consumed.is_empty() {
        if let Some(mut oneshot_q) = world.query_mut::<OneShotSound>() {
            for entity in consumed {
                oneshot_q.remove(entity);
            }
        }
    }
}

/// Walk `active_sounds`, drop any whose `StaticSoundHandle::state()`
/// reports `Stopped`, and remove the `AudioEmitter` component from
/// the source entity so a downstream cleanup system can despawn it
/// without coupling to audio state.
pub(crate) fn prune_stopped_sounds(world: &World, audio_world: &mut AudioWorld) {
    // Phase 4 / #858 / SAFE-23: any active sound whose source entity
    // has lost its `AudioEmitter` (despawn-by-cell-unload, explicit
    // remove) should be stopped at the kira layer. Pre-#858 only
    // looping sounds were truncated here — non-looping SFX kept
    // playing past the despawn at the stale entity transform until
    // natural termination (50 ms – 3 s typical), surfacing as faint
    // cross-cell SFX bleed on fast interior↔interior fast-travel.
    // #5410 — only `SoundSource::Emitter` entries carry emitter-removal
    // coupling. Anchored dialogue survives SFX emitter churn, but cannot
    // keep playing after its followed speaker loses `GlobalTransform`.
    let has_anchored_sounds = audio_world
        .active_sounds
        .iter()
        .any(|sound| !sound.stop_issued && matches!(&sound.source, SoundSource::Anchored(_)));
    let orphaned_anchored: std::collections::HashSet<EntityId> = if has_anchored_sounds {
        world
            .query::<GlobalTransform>()
            .map(|gt_q| {
                audio_world
                    .active_sounds
                    .iter()
                    .filter_map(|sound| {
                        if sound.stop_issued || !matches!(&sound.source, SoundSource::Anchored(_)) {
                            return None;
                        }
                        let has_global_transform = sound
                            .source
                            .followed_entity()
                            .is_some_and(|entity| gt_q.get(entity).is_some());
                        orphaned_anchored_entity(&sound.source, has_global_transform)
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        std::collections::HashSet::new()
    };
    let emitter_q = world.query::<AudioEmitter>();
    let mut to_stop_indices: Vec<usize> = Vec::new();
    for (idx, s) in audio_world.active_sounds.iter().enumerate() {
        // Don't re-mark entries whose stop has already been issued —
        // the handle is fading out asynchronously and won't report
        // `Stopped` until the tween completes. Pre-fix every prune
        // tick during the fade window re-walked + re-pushed the
        // ringbuf `stop` command (idempotent in effect, wasted CPU
        // on the active-list walk). See #844.
        if s.stop_issued {
            continue;
        }
        if matches!(&s.source, SoundSource::Anchored(entity) if orphaned_anchored.contains(entity))
        {
            to_stop_indices.push(idx);
            continue;
        }
        let Some(entity) = s.source.emitter_coupled() else {
            continue;
        };
        let still_has_emitter = emitter_q
            .as_ref()
            .map(|q| q.get(entity).is_some())
            .unwrap_or(false);
        if !still_has_emitter {
            to_stop_indices.push(idx);
        }
    }
    drop(emitter_q);
    for idx in &to_stop_indices {
        // Per-emitter fade-out (#845). Captured at dispatch time from
        // `AudioEmitter.unload_fade_ms` because the source emitter
        // component is already gone by the time we're stopping. The
        // 10 ms default matches `Tween::default()` exactly so authors
        // who don't override stay on the pre-#845 behaviour.
        let fade_ms = audio_world.active_sounds[*idx].unload_fade_ms.max(0.0);
        let tween = Tween {
            start_time: kira::StartTime::Immediate,
            duration: Duration::from_secs_f32(fade_ms / 1000.0),
            easing: kira::Easing::Linear,
        };
        audio_world.active_sounds[*idx].handle.stop(tween);
        // Mark so subsequent prune ticks skip the re-stop until the
        // handle actually transitions to `Stopped` and `retain`
        // drops the entry. See #844.
        audio_world.active_sounds[*idx].stop_issued = true;
    }

    let mut finished: Vec<EntityId> = Vec::new();
    audio_world.active_sounds.retain(|s| {
        if matches!(s.handle.state(), PlaybackState::Stopped) {
            // Queue-driven plays (Detached, and Anchored since #5410)
            // have no emitter component to clean up. Emitter-dispatched
            // plays surface their `EntityId` so the prune pass can
            // remove the `AudioEmitter` component.
            if let Some(e) = s.source.emitter_coupled() {
                finished.push(e);
            }
            false
        } else {
            true
        }
    });
    if !finished.is_empty() {
        if let Some(mut emitter_q) = world.query_mut::<AudioEmitter>() {
            for entity in finished {
                emitter_q.remove(entity);
            }
        }
    }
}
