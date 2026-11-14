//! Background music (`docs/engine/music.md`, Game Music PRD): each game's sources translated into
//! directives ([`translate`]), played by one game-neutral controller ([`controller`]) on the audio
//! world's music voices.

#[cfg(test)]
mod census_tests;
mod controller;
mod directive;
mod translate;

pub(crate) use controller::{Layer, MusicController};
pub(crate) use translate::Sting;

use byroredux_core::ecs::world::World;
use controller::{Inputs, MusicOutput};
use std::sync::Arc;
use translate::{Catalog, Place};

/// The settings registry key of the music level (MUS-MIX-001).
pub(crate) const MUSIC_VOLUME_SETTING_ID: &str = byroredux_core::settings::builtin::MUSIC_VOLUME_SETTING_ID;

/// The music voices of the audio world, reading tracks from the sound provider.
struct AudioOutput<'a> {
    audio: Option<&'a mut byroredux_audio::AudioWorld>,
    provider: Option<&'a crate::asset_provider::SoundArchiveProvider>,
}

impl MusicOutput for AudioOutput<'_> {
    fn live(&self) -> bool {
        self.audio.as_ref().is_some_and(|audio| audio.is_active())
    }

    fn play(&mut self, voice: u32, key: &str, level: f32, fade: f32, looping: bool) -> bool {
        let Some(audio) = self.audio.as_deref_mut().filter(|audio| audio.is_active()) else {
            return true; // headless: the decision stands, nothing is heard (MUS-RPT-003)
        };
        let Some(bytes) = self.provider.and_then(|provider| provider.extract(key)) else {
            log::warn!("music: '{key}' could not be read");
            return false;
        };
        match byroredux_audio::load_streaming_sound_from_bytes(bytes) {
            Ok(sound) => {
                audio.play_music_voice(voice, sound, level, fade, looping);
                true
            }
            Err(error) => {
                log::warn!("music: '{key}' failed to decode: {error}");
                false
            }
        }
    }

    fn stop(&mut self, voice: u32, fade: f32) {
        if let Some(audio) = self.audio.as_deref_mut() {
            audio.stop_music_voice(voice, fade);
        }
    }

    fn pause(&mut self, voice: u32, fade: f32) {
        if let Some(audio) = self.audio.as_deref_mut() {
            audio.pause_music_voice(voice, fade);
        }
    }

    fn resume(&mut self, voice: u32, fade: f32) {
        if let Some(audio) = self.audio.as_deref_mut() {
            audio.resume_music_voice(voice, fade);
        }
    }

    fn active(&self, voice: u32) -> bool {
        self.audio.as_ref().is_some_and(|audio| audio.music_voice_active(voice))
    }

    fn position(&self, voice: u32) -> Option<f64> {
        self.audio.as_ref().filter(|audio| audio.is_active())?.music_voice_position(voice)
    }
}

/// What the music system derived from the loaded content, rebuilt when it changes.
/// `NOT_SAVED_BY_DESIGN`: a translation of the load order.
#[derive(Default)]
pub(crate) struct MusicCatalog {
    /// The `LoadedCellIndex` it was built from, by address.
    index: usize,
    catalog: Option<Arc<Catalog>>,
    place: Option<Place>,
    /// The New Vegas audio marker whose area the player is in (MUS-SRC-005).
    marker: Option<translate::Marker>,
    /// How that marker's controller's conditional faction treats the player (#6904).
    relation: Option<byroredux_sdk::relationships::CombatReaction>,
    /// Whether the player was dead last frame: the death sting plays on the change (MUS-STG-001).
    player_dead: bool,
    /// Stings raised since the last frame (a level-up, [`raise_sting`]).
    stings: Vec<Sting>,
    /// The Script layer ends on the next frame (a cell change, a load).
    end_script: bool,
    /// The music volume last applied.
    volume: Option<f32>,
    /// `CellMusicTypes::changes` last seen: an OBSE `SetCellMusicType` re-resolves Location.
    cell_music_changes: u64,
}

impl byroredux_core::ecs::Resource for MusicCatalog {}

/// The player's place for music, from the session's cell contexts.
fn current_place(world: &World) -> Option<Place> {
    if let Some(cell) = world.try_resource::<crate::components::CurrentCellContext>() {
        return Some(Place::Interior { cell: cell.cell_editor_id.to_ascii_lowercase() });
    }
    let exterior = world.try_resource::<crate::components::CurrentExteriorContext>()?;
    Some(Place::Exterior { worldspace: exterior.worldspace_key.to_ascii_lowercase(), grid: exterior.grid })
}

/// Queue a sting for the next frame: a level-up awards it where the level is awarded, the path
/// `levelup xp <n>` reaches too (MUS-RPT-004).
pub(crate) fn raise_sting(world: &World, sting: Sting) {
    if let Some(mut catalog) = world.try_resource_mut::<MusicCatalog>() {
        catalog.stings.push(sting);
    }
}

/// End the Script layer: the player left the cell it was played in, or a load began (MUS-SCR,
/// MUS-SAV-002).
pub(crate) fn end_script_layer(world: &World) {
    if let Some(mut catalog) = world.try_resource_mut::<MusicCatalog>() {
        catalog.end_script = true;
    }
}

/// The music system (`docs/engine/music.md` §2): translate the place, scripts, combat, stings and
/// the movie hold into the controller's layers, then step it.
pub(crate) fn music_system(world: &World, dt: f32) {
    let Some(index) = world.try_resource::<crate::components::LoadedCellIndex>().map(|index| Arc::clone(&index.0)) else {
        return;
    };
    let (Some(mut state), Some(mut music)) =
        (world.try_resource_mut::<MusicCatalog>(), world.try_resource_mut::<MusicController>())
    else {
        return;
    };
    let mut audio = world.try_resource_mut::<byroredux_audio::AudioWorld>();
    let provider = world.try_resource::<crate::asset_provider::SoundArchiveProvider>();
    let mut out = AudioOutput { audio: audio.as_deref_mut(), provider: provider.as_deref() };

    let address = Arc::as_ptr(&index) as usize;
    if state.index != address || state.catalog.is_none() {
        let files = directive::MusicFiles::new(provider.as_deref().map(|p| p.music_keys()).unwrap_or_default());
        let ini = crate::game_ini::read_default_ini(&crate::cli_args::effective_args(), index.game);
        let catalog = Catalog::new(&index, files, ini.as_deref());
        log::info!(
            "music: {:?}, {} music file(s), {} audio marker(s), crossfade {:.1}s, battle recovery {:.1}s",
            catalog.game,
            catalog.files.len(),
            catalog.marker_count(),
            catalog.fade_secs,
            catalog.recovery_secs
        );
        state.catalog = Some(Arc::new(catalog));
        state.index = address;
        state.place = None;
    }
    let catalog = Arc::clone(state.catalog.as_ref().expect("built above"));

    let player = crate::components::PlayerEntity::get(world);
    let player_position = player
        .and_then(|p| world.get::<byroredux_core::ecs::components::Transform>(p).map(|t| t.translation))
        .map(|p| byroredux_core::math::coord::yup_to_zup_pos(p.to_array()));

    // Location: re-translated whenever the place or the audio-marker area changes (MUS-SEL-005 keeps
    // an equal directive playing).
    let place = current_place(world);
    let marker = place.as_ref().zip(player_position).and_then(|(place, at)| catalog.marker_at(place, at));
    // The conditional faction's reaction picks a relation list; re-read each frame so a SetEnemy /
    // SetFactionReaction changes the music where the player stands (#6904).
    let relation = marker.map(|marker| {
        index
            .media_locations
            .get(&marker.controller)
            .and_then(|aloc| aloc.conditional_faction)
            .zip(player)
            .and_then(|(faction, player)| byroredux_scripting::faction_reaction_toward(world, faction, player))
            .unwrap_or(byroredux_sdk::relationships::CombatReaction::Neutral)
    });
    let mut cell_music = world.try_resource_mut::<byroredux_scripting::obscript_more::CellMusicTypes>();
    let cell_music_changes = cell_music.as_ref().map_or(0, |types| types.changes);
    if place != state.place
        || marker != state.marker
        || relation != state.relation
        || cell_music_changes != state.cell_music_changes
    {
        let empty = std::collections::HashMap::new();
        let overrides = cell_music.as_ref().map_or(&empty, |types| &types.overrides);
        let directive = place.as_ref().map(|place| {
            marker
                .zip(relation)
                .and_then(|(marker, relation)| catalog.marker_location(&index, &marker, relation))
                .unwrap_or_else(|| catalog.location_with(&index, place, overrides))
        });
        // OBSE `GetCellMusicType` answers the player's cell's own category (MUS-SCR-004).
        let category = place.as_ref().map_or(0, |place| Catalog::cell_category(&index, place, overrides));
        if let Some(types) = cell_music.as_mut() {
            types.current = category;
        }
        music.set(Layer::Location, directive, &mut out);
        state.place = place;
        state.marker = marker;
        state.relation = relation;
        state.cell_music_changes = cell_music_changes;
    }
    drop(cell_music);

    // Script: a cell change or a load ends it; `PlayMusic` replaces it (MUS-SCR-001).
    if std::mem::take(&mut state.end_script) {
        music.end_script(&mut out);
    }
    let requests = world
        .try_resource_mut::<byroredux_scripting::obscript_more::PendingMusic>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    if let Some(track) = requests.last() {
        let directive = catalog.music_type(&index, track.form_id, "PlayMusic");
        music.set(Layer::Script, Some(directive), &mut out);
    }
    let streams = world
        .try_resource_mut::<byroredux_scripting::obscript_more::PendingStreamMusic>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    if let Some(text) = streams.last() {
        let directive = catalog.stream_music(text);
        // A missing file leaves the playing track alone (PRD §6 row 5, MUS-SRC-012).
        if directive.outcome() == directive::Outcome::Missing {
            music.note(Layer::Script, &directive);
        } else {
            music.set(Layer::Script, Some(directive), &mut out);
        }
    }

    // Stings: the player's death (MUS-STG-001) and a level-up (MUS-STG-002).
    let dead = player.is_some_and(|p| world.get::<byroredux_core::ecs::components::Dead>(p).is_some());
    if dead && !state.player_dead {
        state.stings.push(Sting::Death);
    }
    state.player_dead = dead;
    for sting in std::mem::take(&mut state.stings) {
        if let Some(directive) = catalog.sting(&index, sting) {
            music.sting(directive, &mut out);
        }
    }

    // The music level (MUS-MIX-001).
    let volume = world
        .try_resource::<byroredux_core::settings::SettingsRegistry>()
        .and_then(|settings| match settings.get(MUSIC_VOLUME_SETTING_ID)?.value {
            byroredux_core::settings::SettingValue::Number(volume) => Some(volume),
            _ => None,
        })
        .unwrap_or(1.0);
    if state.volume != Some(volume) {
        if let Some(audio) = out.audio.as_deref_mut() {
            audio.set_music_volume(volume);
        }
        state.volume = Some(volume);
    }

    let inputs = Inputs {
        dt,
        hold: out.audio.as_ref().is_some_and(|audio| audio.movie_hold().music_paused),
        in_combat: player.is_some_and(|p| !dead && byroredux_scripting::combat::is_in_combat(world, p)),
        player: player_position,
        hour: world.try_resource::<crate::components::GameTimeRes>().map_or(12.0, |time| time.hour),
    };
    let (place, marker) = (state.place.clone(), state.marker);
    music.step(inputs, || catalog.battle(&index, place.as_ref(), marker.as_ref()), &mut out);
}

/// `music.status` (MUS-RPT-001).
pub(crate) fn status_lines(world: &World) -> Vec<String> {
    let Some(music) = world.try_resource::<MusicController>() else {
        return vec!["music: no controller".to_string()];
    };
    let audio = world.try_resource::<byroredux_audio::AudioWorld>();
    let out = ReadOnlyOutput(audio.as_deref());
    let mut lines = music.status(&out);
    if let Some(state) = world.try_resource::<MusicCatalog>() {
        let place = match &state.place {
            Some(Place::Interior { cell }) => format!("interior {cell}"),
            Some(Place::Exterior { worldspace, grid }) => format!("exterior {worldspace} {grid:?}"),
            None => "none".to_string(),
        };
        let marker = state.marker.map_or("none".to_string(), |m| format!("{:#010x} (radius {:.0})", m.form, m.radius));
        lines.insert(0, format!("Place: {place}; audio marker {marker}"));
    }
    if !out.live() {
        lines.push("(no audio device: positions are the controller's clock)".to_string());
    }
    lines
}

/// The audio world, read only, for reporting.
struct ReadOnlyOutput<'a>(Option<&'a byroredux_audio::AudioWorld>);

impl MusicOutput for ReadOnlyOutput<'_> {
    fn live(&self) -> bool {
        self.0.is_some_and(|audio| audio.is_active())
    }
    fn play(&mut self, _: u32, _: &str, _: f32, _: f32, _: bool) -> bool {
        false
    }
    fn stop(&mut self, _: u32, _: f32) {}
    fn pause(&mut self, _: u32, _: f32) {}
    fn resume(&mut self, _: u32, _: f32) {}
    fn active(&self, voice: u32) -> bool {
        self.0.is_some_and(|audio| audio.music_voice_active(voice))
    }
    fn position(&self, voice: u32) -> Option<f64> {
        self.0.filter(|audio| audio.is_active())?.music_voice_position(voice)
    }
}

/// The music level's default for the game `args` load (Q4): `fDefaultMusicVolume` from its shipped
/// default INI, 0 when Oblivion's `bMusicEnabled` is off; 1.0 (the authored level) without one.
pub(crate) fn default_music_volume(args: &[String]) -> f32 {
    let Some(esm) = crate::cli_args::esm_discovery_root(args).ok().map(|(esm, _)| esm) else {
        return 1.0;
    };
    let Some(game) = crate::game_ini::plugin_game(std::path::Path::new(&esm)) else {
        return 1.0;
    };
    let Some(ini) = crate::game_ini::read_default_ini(args, game) else {
        return 1.0;
    };
    music_volume_from_ini(&ini)
}

fn music_volume_from_ini(ini: &str) -> f32 {
    if crate::game_ini::ini_value(ini, "Audio", "bMusicEnabled").is_some_and(|v| v.trim() == "0") {
        return 0.0;
    }
    crate::game_ini::ini_f32(ini, "Audio", "fDefaultMusicVolume").map_or(1.0, |v| v.clamp(0.0, 1.0))
}

/// Install the music resources (`boot/world.rs`).
pub(crate) fn install(world: &mut World) {
    world.insert_resource(MusicController::default());
    world.insert_resource(MusicCatalog::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_music_level_is_the_games_default_and_music_off_is_zero() {
        assert_eq!(music_volume_from_ini("[Audio]\nfDefaultMusicVolume=0.3000\nbMusicEnabled=1\n"), 0.3);
        assert_eq!(music_volume_from_ini("[Audio]\nfDefaultMusicVolume=0.6000\n"), 0.6);
        assert_eq!(music_volume_from_ini("[Audio]\nfDefaultMusicVolume=0.3\nbMusicEnabled=0\n"), 0.0);
        assert_eq!(music_volume_from_ini("[General]\n"), 1.0);
    }
}
