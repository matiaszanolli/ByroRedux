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
use std::collections::HashMap;
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
            // kira can refuse the track too: a silent voice is no playing track (#6921).
            Ok(sound) => audio.play_music_voice(voice, sound, level, fade, looping),
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

    fn set_level(&mut self, voice: u32, level: f32, fade: f32) {
        if let Some(audio) = self.audio.as_deref_mut() {
            audio.set_music_voice_level(voice, level, fade);
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
    /// The `LoadedCellIndex` it was built from. A `Weak`, compared by `Weak::ptr_eq`, so a later
    /// index cannot reuse the address of a freed one and keep a stale catalog (#7114).
    index: std::sync::Weak<byroredux_plugin::esm::records::EsmIndex>,
    catalog: Option<Arc<Catalog>>,
    /// Kept behind an `Arc`, so a frame in the same place reuses it rather than building the key
    /// again ([`current_place`], #6946).
    place: Option<Arc<Place>>,
    /// `CellMusicTypes::overrides` as of [`Self::cell_music_changes`]: copied when the count moves,
    /// not every frame (#6946).
    overrides: HashMap<u32, u32>,
    /// The New Vegas audio marker whose area the player is in (MUS-SRC-005).
    marker: Option<translate::Marker>,
    /// What that marker's controller counts around the player (#6904, #6906).
    around: translate::Surroundings,
    /// The roll a Random controller picks its set by, drawn on entering the marker's area (#6906).
    roll: u64,
    /// Whether the player was dead last frame: the death sting plays on the change (MUS-STG-001).
    player_dead: bool,
    /// Stings raised since the last frame (a level-up, [`raise_sting`]).
    stings: Vec<Sting>,
    /// The Script layer ends on the next frame (a cell change, a load).
    end_script: bool,
    /// A save load ended the outgoing session: its sting ends on the next frame too (AUD-4).
    end_sting: bool,
    /// The music volume last applied.
    volume: Option<f32>,
    /// `CellMusicTypes::changes` last seen: an OBSE `SetCellMusicType` re-resolves Location.
    cell_music_changes: u64,
}

impl byroredux_core::ecs::Resource for MusicCatalog {}

/// What a New Vegas controller counts around the player (#6906): the conditional faction's relation
/// to the player while a live actor of that faction is found, and whether a live actor that would
/// attack the player is (tension). New Vegas counts the player's compass targets, which follow
/// detection; until the Sneak and Detection PRD lands, an actor counts while inside the marker's area.
/// Tension is tested only under a controller that lists a Dungeon set, and the actors are not walked
/// at all when there is neither tension nor a faction to count (#6946).
fn surroundings(world: &World, marker: &translate::Marker, faction: Option<u32>, player: byroredux_core::ecs::EntityId) -> translate::Surroundings {
    use byroredux_core::ecs::components::{Dead, FactionRanks, GlobalTransform};
    let mut around = translate::Surroundings::default();
    if faction.is_none() && !marker.counts_tension {
        return around;
    }
    // The live actors inside the area, and whether one belongs to the faction; the guards drop before
    // the hostility test takes these locks again.
    let (inside, faction_found) = {
        let (Some(ranks), Some(globals)) = (world.query::<FactionRanks>(), world.query::<GlobalTransform>()) else {
            return around;
        };
        let dead = world.query::<Dead>();
        let mut inside = Vec::new();
        let mut faction_found = false;
        for (actor, actor_ranks) in ranks.iter() {
            if actor == player || dead.as_ref().is_some_and(|dead| dead.get(actor).is_some()) {
                continue;
            }
            let Some(at) = globals.get(actor).map(|t| byroredux_core::math::coord::yup_to_zup_pos(t.translation.to_array())) else {
                continue;
            };
            let d = [at[0] - marker.center[0], at[1] - marker.center[1], at[2] - marker.center[2]];
            if (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() <= marker.radius {
                faction_found |= faction.is_some_and(|faction| actor_ranks.0.iter().any(|(f, rank)| *f == faction && *rank >= 0));
                if marker.counts_tension {
                    inside.push(actor);
                }
            }
        }
        (inside, faction_found)
    };
    around.tension = inside.into_iter().any(|actor| crate::systems::faction_hostility::actor_would_attack(world, actor, player));
    if faction_found {
        around.relation = faction.and_then(|faction| byroredux_scripting::faction_reaction_toward(world, faction, player));
    }
    around
}

/// The player's place for music, from the session's cell contexts: `last` again while the player is
/// still there, so a frame in the same place builds no key (#6946).
fn current_place(world: &World, last: Option<Arc<Place>>) -> Option<Arc<Place>> {
    if let Some(cell) = world.try_resource::<crate::components::CurrentCellContext>() {
        let here = |last: &Arc<Place>| matches!(&**last, Place::Interior { cell: key } if key.eq_ignore_ascii_case(&cell.cell_editor_id));
        return Some(last.filter(here).unwrap_or_else(|| Arc::new(Place::Interior { cell: cell.cell_editor_id.to_ascii_lowercase() })));
    }
    let exterior = world.try_resource::<crate::components::CurrentExteriorContext>()?;
    let here = |last: &Arc<Place>| {
        matches!(&**last, Place::Exterior { worldspace, grid } if *grid == exterior.grid && worldspace.eq_ignore_ascii_case(&exterior.worldspace_key))
    };
    Some(last.filter(here).unwrap_or_else(|| {
        Arc::new(Place::Exterior { worldspace: exterior.worldspace_key.to_ascii_lowercase(), grid: exterior.grid })
    }))
}

/// Queue a sting for the next frame: a level-up awards it where the level is awarded, the path
/// `levelup xp <n>` reaches too (MUS-RPT-004).
pub(crate) fn raise_sting(world: &World, sting: Sting) {
    if let Some(mut catalog) = world.try_resource_mut::<MusicCatalog>() {
        catalog.stings.push(sting);
    }
}

/// A save load replaced the session: the sting the outgoing game was playing (a death sting, most
/// often, since the player loads after dying) ends with it, so it does not play on over the loaded
/// game and keep Script, Battle and Location suspended (AUD-4, `docs/engine/music.md` §9). A cell
/// change keeps a sting, so this is the load's own signal, beside [`end_script_layer`].
pub(crate) fn end_sting_layer(world: &World) {
    if let Some(mut catalog) = world.try_resource_mut::<MusicCatalog>() {
        catalog.end_sting = true;
    }
}

/// End the Script layer: the player left the cell it was played in, or a load began (MUS-SCR,
/// MUS-SAV-002).
pub(crate) fn end_script_layer(world: &World) {
    if let Some(mut catalog) = world.try_resource_mut::<MusicCatalog>() {
        catalog.end_script = true;
    }
}

/// The catalog for `index`, built on the first frame after a new index (a game load). Takes
/// `MusicCatalog`, then `SoundArchiveProvider` while it builds: the order [`music_system`] keeps.
fn current_catalog(world: &World, index: &Arc<byroredux_plugin::esm::records::EsmIndex>) -> Option<Arc<Catalog>> {
    let mut state = world.try_resource_mut::<MusicCatalog>()?;
    let built_for = Arc::downgrade(index);
    if !std::sync::Weak::ptr_eq(&state.index, &built_for) || state.catalog.is_none() {
        let keys = world
            .try_resource::<crate::asset_provider::SoundArchiveProvider>()
            .map(|provider| provider.music_keys())
            .unwrap_or_default();
        let files = directive::MusicFiles::new(keys);
        let ini = crate::game_ini::read_default_ini(&crate::cli_args::effective_args(), index.game);
        let catalog = Catalog::new(index, files, ini.as_deref());
        log::info!(
            "music: {:?}, {} music file(s), {} audio marker(s), crossfade {:.1}s, battle recovery {:.1}s",
            catalog.game,
            catalog.files.len(),
            catalog.marker_count(),
            catalog.fade_secs,
            catalog.recovery_secs
        );
        state.catalog = Some(Arc::new(catalog));
        state.index = built_for;
        state.place = None;
    }
    state.catalog.clone()
}

/// The music system (`docs/engine/music.md` §2): translate the place, scripts, combat, stings and
/// the movie hold into the controller's layers, then step it.
///
/// #6920 — every read of another resource or of a component happens before the music locks are
/// taken. The system then holds only `MusicCatalog` → `MusicController` → `AudioWorld` →
/// `SoundArchiveProvider`, the order [`status_lines`] keeps, and never holds `AudioWorld` across a
/// component query (the audio code's one-lock-at-a-time rule, #5556).
pub(crate) fn music_system(world: &World, dt: f32) {
    let Some(index) = world.try_resource::<crate::components::LoadedCellIndex>().map(|index| Arc::clone(&index.0)) else {
        return;
    };
    let Some(catalog) = current_catalog(world, &index) else {
        return;
    };

    let player = crate::components::PlayerEntity::get(world);
    let player_position = player
        .and_then(|p| world.get::<byroredux_core::ecs::components::Transform>(p).map(|t| t.translation))
        .map(|p| byroredux_core::math::coord::yup_to_zup_pos(p.to_array()));
    // Location: re-translated whenever the place or the audio-marker area changes (MUS-SEL-005 keeps
    // an equal directive playing).
    // What the last frame kept, read and released before the other resources (#6920).
    let (last_place, seen_changes) =
        world.try_resource::<MusicCatalog>().map_or((None, None), |state| (state.place.clone(), Some(state.cell_music_changes)));
    let place = current_place(world, last_place);
    let marker = place.as_deref().zip(player_position).and_then(|(place, at)| catalog.marker_at(place, at));
    // What the controller counts around the player, re-read each frame so a SetEnemy /
    // SetFactionReaction, or an actor arriving or dying, changes the music where the player stands
    // (#6904, #6906).
    let around = marker.zip(player).map_or_else(translate::Surroundings::default, |(marker, player)| {
        let faction = index.media_locations.get(&marker.controller).and_then(|aloc| aloc.conditional_faction);
        surroundings(world, &marker, faction, player)
    });
    // The overrides are copied only when `SetCellMusicType` changed them (#6946).
    let (cell_music_changes, overrides) = world
        .try_resource::<byroredux_scripting::obscript_more::CellMusicTypes>()
        .map_or((0, None), |types| (types.changes, (Some(types.changes) != seen_changes).then(|| types.overrides.clone())));
    let requests = world
        .try_resource_mut::<byroredux_scripting::obscript_more::PendingMusic>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    let streams = world
        .try_resource_mut::<byroredux_scripting::obscript_more::PendingStreamMusic>()
        .map(|mut pending| std::mem::take(&mut pending.0))
        .unwrap_or_default();
    let dead = player.is_some_and(|p| world.get::<byroredux_core::ecs::components::Dead>(p).is_some());
    let in_combat = player.is_some_and(|p| !dead && byroredux_scripting::combat::is_in_combat(world, p));
    // The music level (MUS-MIX-001).
    let volume = world
        .try_resource::<byroredux_core::settings::SettingsRegistry>()
        .and_then(|settings| match settings.get(MUSIC_VOLUME_SETTING_ID)?.value {
            byroredux_core::settings::SettingValue::Number(volume) => Some(volume),
            _ => None,
        })
        .unwrap_or(1.0);
    let hour = world.try_resource::<crate::components::GameTimeRes>().map_or(12.0, |time| time.hour);

    let cell_category = {
        let (Some(mut state), Some(mut music)) =
            (world.try_resource_mut::<MusicCatalog>(), world.try_resource_mut::<MusicController>())
        else {
            return;
        };
        let mut audio = world.try_resource_mut::<byroredux_audio::AudioWorld>();
        let provider = world.try_resource::<crate::asset_provider::SoundArchiveProvider>();
        let mut out = AudioOutput { audio: audio.as_deref_mut(), provider: provider.as_deref() };

        if marker != state.marker {
            state.roll = music.roll(u64::from(u32::MAX));
        }
        if let Some(overrides) = overrides {
            state.overrides = overrides;
        }
        let mut cell_category = None;
        if place != state.place
            || marker != state.marker
            || around != state.around
            || cell_music_changes != state.cell_music_changes
        {
            let directive = place.as_deref().map(|place| {
                marker
                    .and_then(|marker| catalog.marker_location(&index, &marker, around, state.roll))
                    .unwrap_or_else(|| catalog.location_with(&index, place, &state.overrides))
            });
            // OBSE `GetCellMusicType` answers the player's cell's own category (MUS-SCR-004),
            // written back once the music locks are released.
            cell_category = Some(place.as_deref().map_or(0, |place| Catalog::cell_category(&index, place, &state.overrides)));
            // Oblivion: a stream outlives a place change into its own category only (#6903).
            let place_category = directive.as_ref().and_then(|d| d.category).filter(|_| place != state.place);
            music.set(Layer::Location, directive, &mut out);
            if let Some(category) = place_category {
                music.place_category(category, &mut out);
            }
            state.place = place;
            state.marker = marker;
            state.around = around;
            state.cell_music_changes = cell_music_changes;
        }

        // Script: a cell change or a load ends it; `PlayMusic` replaces it (MUS-SCR-001). A load into
        // the same place changes no place, so the same signal ends a sting's silence (#6922).
        if std::mem::take(&mut state.end_script) {
            music.end_script(&mut out);
            music.end_sting_silence();
        }
        if std::mem::take(&mut state.end_sting) {
            music.set(Layer::Sting, None, &mut out);
        }
        if let Some(track) = requests.last() {
            let directive = catalog.play_music(&index, track.form_id);
            music.set(Layer::Script, Some(directive), &mut out);
        }
        if let Some(text) = streams.last() {
            let roll = music.roll(4);
            match catalog.stream_music(text, roll) {
                // `StreamMusic Battle`: Oblivion.exe's battle music follows combat (#6903).
                None => log::info!("music: StreamMusic {text}: battle music follows combat; ignored"),
                // A missing file leaves the playing track alone (PRD §6 row 5, MUS-SRC-012).
                Some(directive) if directive.outcome() == directive::Outcome::Missing => music.note(Layer::Script, &directive),
                Some(directive) => music.set(Layer::Script, Some(directive), &mut out),
            }
        }

        // Stings: the player's death (MUS-STG-001) and a level-up (MUS-STG-002).
        if dead && !state.player_dead {
            state.stings.push(Sting::Death);
        }
        state.player_dead = dead;
        for sting in std::mem::take(&mut state.stings) {
            if let Some(directive) = catalog.sting(&index, sting) {
                music.sting(directive, &mut out);
            }
        }

        if state.volume != Some(volume) {
            if let Some(audio) = out.audio.as_deref_mut() {
                audio.set_music_volume(volume);
            }
            state.volume = Some(volume);
        }

        let inputs = Inputs {
            dt,
            hold: out.audio.as_ref().is_some_and(|audio| audio.movie_hold().music_paused),
            in_combat,
            player: player_position,
            hour,
        };
        let (place, marker) = (state.place.clone(), state.marker);
        let (around, roll) = (state.around, state.roll);
        music.step(inputs, || catalog.battle(&index, place.as_deref(), marker.as_ref(), around, roll), &mut out);
        cell_category
    };
    if let Some(category) = cell_category
        && let Some(mut types) = world.try_resource_mut::<byroredux_scripting::obscript_more::CellMusicTypes>() {
            types.current = category;
        }
}

/// `music.status` (MUS-RPT-001).
pub(crate) fn status_lines(world: &World) -> Vec<String> {
    // `MusicCatalog` → `MusicController` → `AudioWorld`: the order `music_system` takes them (#6920).
    let state = world.try_resource::<MusicCatalog>();
    let Some(music) = world.try_resource::<MusicController>() else {
        return vec!["music: no controller".to_string()];
    };
    let audio = world.try_resource::<byroredux_audio::AudioWorld>();
    let out = ReadOnlyOutput(audio.as_deref());
    let mut lines = music.status(&out);
    if let Some(state) = state.as_deref() {
        let place = match state.place.as_deref() {
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
    fn set_level(&mut self, _: u32, _: f32, _: f32) {}
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

    /// #6920 — `music_system` and `music.status` take the music resources in one order,
    /// `MusicCatalog` → `MusicController` → `AudioWorld`. Under `BYRO_LOCK_ORDER_CHECK=1` (CI's
    /// ABBA job) the global order graph panics on an inverted pair, and this runs both.
    #[test]
    fn the_music_system_and_its_status_take_the_music_locks_in_one_order() {
        let mut world = World::new();
        install(&mut world);
        world.insert_resource(crate::components::LoadedCellIndex(Arc::new(Default::default())));
        world.insert_resource(byroredux_audio::AudioWorld::headless());
        music_system(&world, 0.016);
        let lines = status_lines(&world);
        assert!(lines.iter().any(|line| line.starts_with("Place:")), "the catalog was built: {lines:?}");
    }

    /// #6922 — the load / cell-leave signal (`end_script_layer`, which `unload_current_interior`
    /// and `clear_current_exterior_identity` raise on every save load) ends a sting's silence on the
    /// next music frame, even with no place change.
    #[test]
    fn a_load_ends_a_stings_silence_in_the_music_system() {
        let mut world = World::new();
        install(&mut world);
        world.insert_resource(crate::components::LoadedCellIndex(Arc::new(Default::default())));
        world.insert_resource(byroredux_audio::AudioWorld::headless());
        music_system(&world, 0.016);
        {
            let mut out = controller::tests::FakeOutput::default();
            let death = controller::tests::playlist("death", &["music\\special\\death.mp3"]);
            world.resource_mut::<MusicController>().sting(death, &mut out);
        }
        assert!(world.resource::<MusicController>().silenced());
        music_system(&world, 0.016);
        assert!(world.resource::<MusicController>().silenced(), "nothing ends it on its own");
        end_script_layer(&world);
        music_system(&world, 0.016);
        assert!(!world.resource::<MusicController>().silenced(), "a load ends it");
    }

    /// AUD-4 — a load ends the sting itself, not only its silence: a death sting must not play on
    /// over the loaded game. A cell change (`end_script_layer` alone) keeps it.
    #[test]
    fn a_load_ends_the_sting_layer() {
        let mut world = World::new();
        install(&mut world);
        world.insert_resource(crate::components::LoadedCellIndex(Arc::new(Default::default())));
        world.insert_resource(byroredux_audio::AudioWorld::headless());
        music_system(&world, 0.016);
        {
            let mut out = controller::tests::FakeOutput::default();
            let death = controller::tests::playlist("death", &["music\\special\\death.mp3"]);
            world.resource_mut::<MusicController>().sting(death, &mut out);
        }
        end_script_layer(&world);
        music_system(&world, 0.016);
        assert!(world.resource::<MusicController>().directive(Layer::Sting).is_some(), "a cell change keeps the sting");
        end_sting_layer(&world);
        music_system(&world, 0.016);
        assert!(world.resource::<MusicController>().directive(Layer::Sting).is_none(), "a load ends it");
    }
}
