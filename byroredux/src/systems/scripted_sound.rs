//! M47.2 — `Sound.Play(akSource)` from a Papyrus script: the host half of
//! [`byroredux_scripting::PendingSoundPlays`].
//!
//! CK *Play - Sound*: "Plays the sound from the location of the specified
//! object reference", and its note that it "will return an error if the object
//! does not have any 3D loaded" — a source with no placement plays nothing. The
//! sound is a `SNDR` descriptor (Skyrim+) or a `SOUN`; both are `index.sounds`
//! records with a file, or a descriptor's several `ANAM` files, one of which
//! plays (taken in turn here). A looping descriptor is not played: a one-shot
//! cannot loop, and playing it once would be a different sound.
//!
//! Script-chosen paths are an open-ended keyspace, so they decode into the
//! byte-budgeted [`byroredux_audio::VoiceSoundCache`] rather than the
//! process-lifetime `SoundCache`, and only when the audio world has a device
//! (#6281). A `SOUN`'s authored `SNDD` attenuation distances and its `2D` /
//! `Menu Sound` flags shape the play; a record without them (a Skyrim `SNDR`,
//! whose attenuation lives in an undecoded output model) takes the generic
//! [`Attenuation::default`].

use std::sync::atomic::{AtomicUsize, Ordering};

use byroredux_audio::Attenuation;
use byroredux_core::ecs::components::GlobalTransform;
use byroredux_core::ecs::World;
use byroredux_core::lighting::BETHESDA_UNITS_PER_METER;
use byroredux_plugin::esm::records::SounRecord;

/// Which variant of a multi-file descriptor plays next.
static VARIANT_TURN: AtomicUsize = AtomicUsize::new(0);

/// The range a sound is heard over, in the metres [`Attenuation`] speaks: the
/// record's authored `SNDD` distances (game units) when it has them.
fn attenuation_for(record: &SounRecord) -> Attenuation {
    record
        .attenuation_units
        .map_or_else(Attenuation::default, |(min, max)| Attenuation {
            min_distance: min / BETHESDA_UNITS_PER_METER,
            max_distance: max / BETHESDA_UNITS_PER_METER,
        })
}

/// Where the sound is emitted: a `2D` / `Menu Sound` is not placed in the
/// world, so it plays at the listener (the player) when there is one;
/// everything else plays at its source. `None` when there is nowhere to play.
fn emitter_position(
    record: &SounRecord,
    source: Option<byroredux_core::math::Vec3>,
    player: Option<byroredux_core::math::Vec3>,
) -> Option<byroredux_core::math::Vec3> {
    if record.non_positional {
        player.or(source)
    } else {
        source
    }
}

/// Play this frame's `Sound.Play` requests.
pub(crate) fn scripted_sound_system(world: &World, _dt: f32) {
    let plays = world
        .try_resource_mut::<byroredux_scripting::PendingSoundPlays>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    if plays.is_empty() {
        return;
    }
    let Some(index) = world
        .try_resource::<crate::cell_loader::LoadedCellIndex>()
        .map(|index| index.0.clone())
    else {
        return;
    };
    let player = byroredux_scripting::papyrus_demo::PapyrusPlayerEntity::get(world);
    for play in plays {
        let Some(record) = index.sounds.get(&play.sound) else {
            log::debug!("Sound.Play {:08X}: not a sound descriptor", play.sound);
            continue;
        };
        if record.looping {
            log::debug!("Sound.Play {:08X}: a looping sound is not played as a one-shot", play.sound);
            continue;
        }
        let file = match record.variants.len() {
            0 => record.sound_path.as_str(),
            n => record.variants[VARIANT_TURN.fetch_add(1, Ordering::Relaxed) % n].as_str(),
        };
        if file.is_empty() || file.ends_with('\\') || file.ends_with('/') {
            log::debug!("Sound.Play {:08X}: no file to play", play.sound);
            continue;
        }
        let source = if play.source == byroredux_scripting::PLAYER_REFERENCE {
            player
        } else {
            byroredux_scripting::condition::resolve_entity_by_global_form_id(world, play.source)
        };
        let translation = |entity: Option<byroredux_core::ecs::EntityId>| {
            entity
                .and_then(|entity| world.get::<GlobalTransform>(entity))
                .map(|transform| transform.translation)
        };
        let Some(position) = emitter_position(record, translation(source), translation(player))
        else {
            log::debug!("Sound.Play {:08X}: the source has no 3D", play.sound);
            continue;
        };
        let path = crate::asset_provider::sound_archive_path(file);
        if crate::systems::play_oneshot_bounded(
            world,
            &path,
            position,
            attenuation_for(record),
            "script sound",
        ) {
            log::debug!("Sound.Play {:08X}: {path}", play.sound);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::{EsmIndex, SounRecord};
    use byroredux_scripting::{PendingSoundPlays, SoundPlay};

    fn record(form_id: u32, looping: bool, variants: &[&str]) -> SounRecord {
        SounRecord {
            form_id,
            editor_id: String::new(),
            sound_path: variants.first().map(|v| (*v).to_owned()).unwrap_or_default(),
            looping,
            variants: variants.iter().map(|v| (*v).to_owned()).collect(),
            ..Default::default()
        }
    }

    /// Every request is consumed; one that cannot play (an unknown sound, a
    /// looping one, a source with no 3D) is dropped without reaching the audio
    /// world, which a headless world would not notice anyway.
    #[test]
    fn requests_drain_and_unplayable_ones_are_dropped() {
        let mut world = World::new();
        byroredux_scripting::register(&mut world);
        world.insert_resource(byroredux_audio::SoundCache::new());
        world.insert_resource(byroredux_audio::VoiceSoundCache::new());
        world.insert_resource(byroredux_audio::AudioWorld::headless());
        let mut index = EsmIndex::default();
        index.sounds.insert(0x10, record(0x10, true, &["fx\\loop.wav"]));
        index.sounds.insert(0x11, record(0x11, false, &["fx\\a.wav", "fx\\b.wav"]));
        world.insert_resource(crate::cell_loader::LoadedCellIndex(std::sync::Arc::new(index)));
        let mover = world.spawn();
        world.insert(mover, GlobalTransform::default());
        world.resource_mut::<PendingSoundPlays>().0.extend([
            SoundPlay { sound: 0x99, source: 0x14 },
            SoundPlay { sound: 0x10, source: 0x14 },
            SoundPlay { sound: 0x11, source: 0x0005_0000 },
        ]);
        scripted_sound_system(&world, 0.0);
        assert!(world.resource::<PendingSoundPlays>().0.is_empty());
        assert_eq!(world.resource::<byroredux_audio::AudioWorld>().oneshots_requested(), 0);
    }

    /// #6281 — authored `SNDD` distances (game units) become the metre range
    /// the audio boundary expects; a record without them gets the generic
    /// default, not the combat window.
    #[test]
    fn authored_attenuation_is_converted_to_metres() {
        let mut authored = record(1, false, &["fx\\a.wav"]);
        authored.attenuation_units = Some((35.0, 7000.0));
        let range = attenuation_for(&authored);
        assert_eq!(range.min_distance, 35.0 / BETHESDA_UNITS_PER_METER);
        assert_eq!(range.max_distance, 7000.0 / BETHESDA_UNITS_PER_METER);
        assert_eq!(attenuation_for(&record(2, false, &[])), Attenuation::default());
    }

    /// #6281 — a `2D` / `Menu Sound` plays at the listener, not the source.
    #[test]
    fn a_non_positional_sound_plays_at_the_player() {
        use byroredux_core::math::Vec3;
        let source = Some(Vec3::new(100.0, 0.0, 0.0));
        let player = Some(Vec3::new(0.0, 5.0, 0.0));
        let mut menu = record(1, false, &["ui\\a.wav"]);
        assert_eq!(emitter_position(&menu, source, player), source);
        menu.non_positional = true;
        assert_eq!(emitter_position(&menu, source, player), player);
        assert_eq!(emitter_position(&menu, source, None), source);
        assert_eq!(emitter_position(&menu, None, None), None);
    }

    /// #6281 — without a device a play decodes nothing: the bounded cache
    /// stays empty even for a path the provider could serve.
    #[test]
    fn a_headless_audio_world_decodes_nothing() {
        let mut world = World::new();
        world.insert_resource(byroredux_audio::VoiceSoundCache::new());
        world.insert_resource(byroredux_audio::AudioWorld::headless());
        let played = crate::systems::play_oneshot_bounded(
            &world,
            "sound\\fx\\a.wav",
            byroredux_core::math::Vec3::ZERO,
            Attenuation::default(),
            "script sound",
        );
        assert!(!played);
        let cache = world.resource::<byroredux_audio::VoiceSoundCache>();
        assert_eq!((cache.len(), cache.negative_len()), (0, 0));
    }
}
