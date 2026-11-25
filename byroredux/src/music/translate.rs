//! Each game's music sources as directives: the only per-game code in the music path (PRD R5,
//! `docs/engine/music.md` §3, §5, §7).

use super::directive::{Area, AuthoredTrack, Content, Directive, MusicFiles, PlaylistPolicy, Source, Stem};
use byroredux_plugin::esm::cell::{CellData, WorldspaceRecord};
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::misc::{
    MUSC_ABRUPT_TRANSITION, MUSC_CYCLE_TRACKS, MUSC_DUCKS_CURRENT_TRACK,
    MUSC_MAINTAIN_TRACK_ORDER, MUSC_PLAYS_ONE_SELECTION,
};
use byroredux_sdk::relationships::CombatReaction;
use byroredux_plugin::esm::records::{
    select_active_region_sound, EsmIndex, MediaSetKind, MediaSetRecord, MuscRecord, MustKind, RegionDataPayload,
};
use std::collections::HashMap;

/// Where the player is, as far as music cares: an interior cell, or an exterior grid cell of a
/// worldspace. Keys are the lower-cased EditorIDs the cell index uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Place {
    Interior { cell: String },
    Exterior { worldspace: String, grid: (i32, i32) },
}

/// The crossfade where a game names none.
pub(crate) const FALLBACK_FADE_SECS: f32 = 3.0;
/// Oblivion.exe's music transition (`0x6acd10` with the update at `0x6ad030`, #6903): 1700 ms, the
/// old track fading out over the first half and the new one in over the second.
const OBLIVION_FADE_SECS: f32 = 1.7 / 2.0;
/// `iMusicTransitionInMS:Audio`, compiled into `Fallout3.exe` as 1000 (read 2026-11-13).
const FALLOUT_MUSIC_TRANSITION_MS: f32 = 1000.0;
/// GMST `fCombatMusicStopTime`, compiled into `Fallout3.exe` as 5.0. Oblivion has no recovery: its
/// battle music ends when the last actor drops the player as a combat target (`0x6136e0`, #6903).
const COMBAT_MUSIC_STOP_SECS: f32 = 5.0;

impl Place {
    /// The space a New Vegas audio marker belongs to: the interior cell, or the whole worldspace.
    fn space(&self) -> &str {
        match self {
            Place::Interior { cell } => cell,
            Place::Exterior { worldspace, .. } => worldspace,
        }
    }
}

/// A New Vegas audio marker: its reference, its area and its controller (MUS-SRC-005).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Marker {
    pub(crate) form: u32,
    /// Z-up world units, as authored.
    pub(crate) center: [f32; 3],
    pub(crate) radius: f32,
    pub(crate) controller: u32,
    /// The marker's own layer 2 / layer 3 triggers (`MNAM` / `NNAM`, fractions of the radius), when
    /// "Use Controller Values" is clear (#6902).
    pub(crate) triggers: Option<[f32; 2]>,
    /// Its controller lists a Dungeon set, the only set whose track tension picks
    /// ([`Catalog::location_set`]): without one the music system skips the tension test (#6946).
    pub(crate) counts_tension: bool,
}

/// What a controller counts around the player (#6906): its conditional faction's relation to the
/// player while an actor of that faction is found (`None`: none is), and whether an actor that would
/// attack the player is (the controller's tension). New Vegas counts the player's compass targets
/// (JohnnyGuitar NVSE's `MediaLocationController`: `uiFoundFactionActors`, `uiFoundHostileActors`);
/// until detection exists the marker's area stands in for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Surroundings {
    pub(crate) relation: Option<CombatReaction>,
    pub(crate) tension: bool,
}

/// The set a controller plays, or its "Don't use any Media Set" (`FactionNotFound::None`).
enum ActiveSet {
    Set(u32, &'static str),
    NoMusic,
}

/// Oblivion's music categories (`XCMT`, `SNAM`, `RDMD`; xEdit `wbDefinitionsCommon.pas:7976-7983`)
/// and the folder each plays (MUS-SRC-009; Default plays Explore, Q6).
fn category_folder(value: u32) -> Option<(&'static str, &'static str)> {
    match value {
        0 => Some(("Default", "explore\\")),
        1 => Some(("Public", "public\\")),
        2 => Some(("Dungeon", "dungeon\\")),
        _ => None,
    }
}

/// The death and level-up stings (MUS-STG).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sting {
    Death,
    LevelUp,
}

/// A load order's music, translated once per content load.
#[derive(Debug, Clone)]
pub(crate) struct Catalog {
    pub(crate) game: GameKind,
    pub(crate) files: MusicFiles,
    /// The crossfade between two directives (MUS-SEL-004).
    pub(crate) fade_secs: f32,
    /// The Battle layer's hold after combat (MUS-CMB-002).
    pub(crate) recovery_secs: f32,
    /// New Vegas audio markers by space ([`Place::space`]).
    markers: HashMap<String, Vec<Marker>>,
}

impl Catalog {
    /// `ini` is the install's default INI (`game_ini::read_default_ini`).
    pub(crate) fn new(index: &EsmIndex, files: MusicFiles, ini: Option<&str>) -> Self {
        let game = index.game;
        let (fade_secs, recovery_secs) = match game {
            GameKind::Fallout3NV => (
                ini.and_then(|text| crate::game_ini::ini_f32(text, "Audio", "iMusicTransitionInMS"))
                    .unwrap_or(FALLOUT_MUSIC_TRANSITION_MS)
                    / 1000.0,
                index.game_setting_float("fCombatMusicStopTime").unwrap_or(COMBAT_MUSIC_STOP_SECS),
            ),
            GameKind::Oblivion => (OBLIVION_FADE_SECS, 0.0),
            _ => (FALLBACK_FADE_SECS, COMBAT_MUSIC_STOP_SECS),
        };
        let mut markers: HashMap<String, Vec<Marker>> = HashMap::new();
        let cells = &index.cells;
        let spaces = cells
            .cells
            .iter()
            .chain(cells.exterior_cells.iter().flat_map(|(ws, grid)| grid.values().map(move |cell| (ws, cell))))
            .chain(cells.worldspace_persistent_cells.iter());
        for (space, cell) in spaces {
            for r in &cell.references {
                let (Some(marker), Some(radius)) = (r.audio_marker, r.radius_override.filter(|radius| *radius > 0.0)) else {
                    continue;
                };
                let Some(controller) = marker.controller else { continue };
                // GECK *Reference*, Audio Marker: "Use Controller Values: Check box to enable using
                // the 3 layer triggers of the selected Media Location Controller"; clear, the marker's
                // "Layer 2 trigger %" (middle) and "Layer 3 trigger %" (outer) apply. Vanilla authors
                // fractions (0.33 / 0.67); xEdit's default is 100.
                let fraction = |v: f32| if v > 1.0 { v / 100.0 } else { v };
                let triggers = (!marker.use_controller_values).then(|| marker.layer_triggers.map(fraction));
                let counts_tension = index.media_locations.get(&controller).is_some_and(|aloc| {
                    // The lists `Self::active_set` picks from; Battle sets play on the Battle layer.
                    [&aloc.neutral_sets, &aloc.ally_sets, &aloc.friend_sets, &aloc.enemy_sets, &aloc.location_sets]
                        .into_iter()
                        .flatten()
                        .any(|set| index.media_sets.get(set).is_some_and(|set| set.kind == MediaSetKind::Dungeon))
                });
                markers.entry(space.clone()).or_default().push(Marker { form: r.form_id, center: r.position, radius, controller, triggers, counts_tension });
            }
        }
        Self { game, files, fade_secs, recovery_secs, markers }
    }

    pub(crate) fn marker_count(&self) -> usize {
        self.markers.values().map(Vec::len).sum()
    }

    /// The audio marker whose area holds `player` at `place`: of several, the one the player is
    /// deepest into, by distance over radius; no source says which wins (MUS-SRC-005, #6906).
    pub(crate) fn marker_at(&self, place: &Place, player: [f32; 3]) -> Option<Marker> {
        let depth = |m: &Marker| {
            let d = [player[0] - m.center[0], player[1] - m.center[1], player[2] - m.center[2]];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() / m.radius
        };
        self.markers
            .get(place.space())?
            .iter()
            .filter(|m| depth(m) <= 1.0)
            .min_by(|a, b| depth(a).total_cmp(&depth(b)))
            .copied()
    }

    /// A marker's Location directive: the set its controller plays (see [`Self::active_set`]), as
    /// stems by day and night and by depth (MUS-SRC-005 to -007). `None` when the controller names no
    /// set to play, so the place's own music plays.
    pub(crate) fn marker_location(&self, index: &EsmIndex, marker: &Marker, around: Surroundings, roll: u64) -> Option<Directive> {
        let aloc = index.media_locations.get(&marker.controller)?;
        let mut directive = match Self::active_set(index, marker, around.relation, roll)? {
            ActiveSet::NoMusic => Directive {
                notes: vec!["no actor of the conditional faction found: Faction Not Found is None".to_string()],
                ..self.directive(Source::record(aloc.editor_id.clone(), aloc.form_id, "audio marker"), Content::Silent)
            },
            ActiveSet::Set(set_form, via) => match index.media_sets.get(&set_form) {
                Some(set) => self.location_set(set, marker, aloc.day_start, aloc.night_start, via, around.tension),
                None => Directive::unresolved(
                    Source { label: format!("{set_form:#010x}"), form: Some(set_form), via },
                    "not a media set in the load order",
                ),
            },
        };
        directive.beside_script = true;
        directive.notes.push(format!("controller {} ({:#010x}), marker {:#010x}", aloc.editor_id, aloc.form_id, marker.form));
        if let Some(faction) = aloc.conditional_faction {
            match around.relation {
                Some(relation) => directive.notes.push(format!("{relation:?} to the player: faction {faction:#010x}")),
                None => directive.notes.push(format!("no actor of faction {faction:#010x} found")),
            }
        }
        Some(directive)
    }

    /// The set a marker's controller plays (#6904, #6906). With a conditional faction, while an actor
    /// of it is found its relation to the player picks the Neutral / Ally / Friend / Enemy list;
    /// while none is, the "Faction Not Found" list (`NAM1`) plays — "if no actors are detected, MLC
    /// uses the 'Dead Reputation' value to pick the mediaset, overriding the actual faction relation"
    /// (JohnnyGuitar NVSE, `MediaLocationControllerTweaks`). Without one the Location list plays.
    /// An empty list falls back to the Location list, then the Neutral list. Of the list, Random
    /// (`NAM1`'s looping option) takes the set `roll` picks; the others take the first. `None` for a
    /// controller with Battle sets only.
    fn active_set(index: &EsmIndex, marker: &Marker, relation: Option<CombatReaction>, roll: u64) -> Option<ActiveSet> {
        use byroredux_plugin::esm::records::{FactionNotFound, LoopingOption};
        let aloc = index.media_locations.get(&marker.controller)?;
        let neutral = (&aloc.neutral_sets, "audio marker Neutral set");
        let enemy = (&aloc.enemy_sets, "audio marker Enemy set");
        let ally = (&aloc.ally_sets, "audio marker Ally set");
        let friend = (&aloc.friend_sets, "audio marker Friend set");
        let location = (&aloc.location_sets, "audio marker");
        let (list, via) = match (aloc.conditional_faction, relation) {
            (None, _) => location,
            (Some(_), Some(CombatReaction::Neutral)) => neutral,
            (Some(_), Some(CombatReaction::Enemy)) => enemy,
            (Some(_), Some(CombatReaction::Ally)) => ally,
            (Some(_), Some(CombatReaction::Friend)) => friend,
            (Some(_), None) => match aloc.faction_not_found() {
                FactionNotFound::Neutral => neutral,
                FactionNotFound::Enemy => enemy,
                FactionNotFound::Ally => ally,
                FactionNotFound::Friend => friend,
                FactionNotFound::Location => location,
                FactionNotFound::None => return Some(ActiveSet::NoMusic),
            },
        };
        let (list, via) = [(list, via), location, neutral].into_iter().find(|(list, _)| !list.is_empty())?;
        let pick = match aloc.looping() {
            LoopingOption::Random => (roll % list.len() as u64) as usize,
            LoopingOption::Loop | LoopingOption::Retrigger | LoopingOption::None => 0,
        };
        Some(ActiveSet::Set(list[pick], via))
    }

    fn location_set(&self, set: &MediaSetRecord, marker: &Marker, day_start: u32, night_start: u32, via: &'static str, tension: bool) -> Directive {
        let source = Source::record(set.editor_id.clone(), set.form_id, via);
        let fade = Some(set.timings[2]).filter(|t| *t > 0.0).unwrap_or(self.fade_secs);
        // 40 of New Vegas's controllers list a Dungeon set. Its slots are Battle / Explore / Suspense
        // (xEdit's `(D)` labels), not layers. GECK *Media Set*: Explore plays "while player is
        // exploring", Battle "while the player is engaged in combat" (`Self::battle`), Suspense
        // "while enemies are on alert (unverified)" — the controller's tension, an actor that would
        // attack the player found (#6902, #6906). Every vanilla Dungeon set names one file for all
        // three.
        if set.kind == MediaSetKind::Dungeon {
            let (slot, note) = if tension { (2, "a Dungeon set: its Suspense track (tension)") } else { (1, "a Dungeon set: its Explore track") };
            let directive = Directive { fade_secs: fade, looping: true, level: 10f32.powf(set.track_decibels[slot] / 20.0), ..Directive::new(source, self.files.resolve(&set.tracks[slot])) };
            return Directive { notes: vec![note.to_string()], ..directive };
        }
        if set.kind != MediaSetKind::Location {
            return Directive::unresolved(source, format!("a {:?} set as a Location set", set.kind));
        }
        let mut stems = Vec::new();
        let mut missing = None;
        for (i, track) in set.tracks.iter().enumerate() {
            if set.enable_flags & (1 << i) == 0 || track.is_empty() {
                continue;
            }
            match self.files.resolve(track) {
                Content::Playlist(keys) if keys.len() == 1 => stems.push(Stem {
                    key: keys[0].clone(),
                    level: 10f32.powf(set.track_decibels[i] / 20.0),
                    night: i >= 3,
                    reach: Self::reach(set, marker, i),
                }),
                Content::Missing(path) => missing = missing.or(Some(path)),
                _ => missing = missing.or_else(|| Some(format!("Data\\Music\\{track}"))),
            }
        }
        let content = match (stems.is_empty(), missing) {
            (false, _) => Content::Stems {
                stems,
                area: Area { center: marker.center, radius: marker.radius, day_start, night_start },
            },
            (true, Some(path)) => Content::Missing(path),
            (true, None) => Content::Silent,
        };
        // The set's layer crossfade time (`FNAM`), else the game's crossfade.
        Directive { fade_secs: fade, looping: true, ..Directive::new(source, content) }
    }

    /// How far out a Location set's slot `i` (Outer / Middle / Inner, day then night) plays, as a
    /// fraction of the radius: the set's boundary, or with "Use Controller Values" clear the
    /// marker's triggers, each where the next ring out begins — Inner to layer 2's, Middle to layer
    /// 3's, Outer to the edge (#6902). Every vanilla marker's triggers equal the sets' boundaries.
    fn reach(set: &MediaSetRecord, marker: &Marker, i: usize) -> f32 {
        match (marker.triggers, i % 3) {
            (Some(_), 0) => 1.0,
            (Some([_, layer3]), 1) => layer3,
            (Some([layer2, _]), _) => layer2,
            (None, _) => set.boundaries[i] / 100.0,
        }
    }

    /// A New Vegas Battle set: its loop track, intro and outro, and recovery time (MUS-CMB-001 to -003);
    /// or a Dungeon set's Battle track, intro and outro (GECK *Media Set*, #6902), which carries no
    /// recovery time of its own.
    pub(crate) fn battle_set(&self, index: &EsmIndex, set: &MediaSetRecord, via: &'static str) -> Directive {
        let source = Source::record(set.editor_id.clone(), set.form_id, via);
        if !matches!(set.kind, MediaSetKind::Battle | MediaSetKind::Dungeon) {
            return Directive::unresolved(source, format!("a {:?} set, not a battle set", set.kind));
        }
        let dungeon = set.kind == MediaSetKind::Dungeon;
        let sound = |form: Option<u32>| {
            let form = form?;
            (!crate::asset_provider::sound_is_folder(&index.sounds, form))
                .then(|| crate::asset_provider::resolve_sound_path(&index.sounds, form))
                .flatten()
                .map(crate::asset_provider::sound_archive_path)
        };
        Directive {
            looping: true,
            level: 10f32.powf(set.track_decibels[0] / 20.0),
            // `ENAM` "Loop Fade Out", `FNAM` "Recovery Time" (xEdit's (B) labels); a Dungeon set's
            // `FNAM` is its layer crossfade.
            fade_secs: Some(set.timings[if dungeon { 2 } else { 1 }]).filter(|t| *t > 0.0).unwrap_or(self.fade_secs),
            recovery_secs: Some(set.timings[2]).filter(|t| *t > 0.0 && !dungeon).or(Some(self.recovery_secs)),
            intro: sound(set.sound_a),
            outro: sound(set.sound_b),
            ..Directive::new(source, self.files.resolve(&set.tracks[0]))
        }
    }

    fn directive(&self, source: Source, content: Content) -> Directive {
        Directive { fade_secs: self.fade_secs, ..Directive::new(source, content) }
    }

    /// Why a game's music is not translated.
    fn untranslated(&self) -> Option<&'static str> {
        match self.game {
            GameKind::Fallout3NV | GameKind::Oblivion | GameKind::Skyrim | GameKind::Fallout4 => None,
            _ => Some("Wwise music is out of scope (MUS-NG-005)"),
        }
    }

    fn authored_music_types(&self) -> bool {
        matches!(self.game, GameKind::Skyrim | GameKind::Fallout4)
    }

    /// Translate a music type into a playable directive.
    pub(crate) fn music_type(&self, index: &EsmIndex, form: u32, via: &'static str) -> Directive {
        let Some(musc) = index.music_types.get(&form) else {
            return Directive::unresolved(
                Source { label: format!("{form:#010x}"), form: Some(form), via },
                "not a music type in the load order",
            );
        };
        let source = Source::record(musc.editor_id.clone(), form, via);
        if let Some(reason) = self.untranslated() {
            return Directive::unresolved(source, reason);
        }
        if self.authored_music_types() {
            return self.authored_directive(index, musc, source);
        }
        // Fallout 3 / New Vegas: one file or folder, looping with a positive ANAM at -|ANAM| dB.
        Directive {
            looping: musc.looping(),
            level: 10f32.powf(-musc.decibels.abs() / 20.0),
            ..self.directive(source, self.files.resolve(&musc.file))
        }
    }

    fn authored_directive(&self, index: &EsmIndex, musc: &MuscRecord, source: Source) -> Directive {
        let mut tracks = Vec::new();
        let mut first_issue = None;
        let mut budget = 4096;
        let mut ancestry = Vec::new();
        for &form in &musc.tracks {
            match self.authored_track(index, form, &mut ancestry, &mut budget) {
                Ok(track) => tracks.push(track),
                Err(issue) => { first_issue.get_or_insert(issue); }
            }
            if budget == 0 {
                break;
            }
        }
        let has = |flag| musc.flags & flag != 0;
        let content = if tracks.is_empty() {
            first_issue.unwrap_or(Content::Silent)
        } else {
            let policy = PlaylistPolicy {
                one_selection: has(MUSC_PLAYS_ONE_SELECTION),
                cycle: has(MUSC_CYCLE_TRACKS),
                ordered: has(MUSC_MAINTAIN_TRACK_ORDER),
            };
            Content::AuthoredPlaylist { tracks, policy }
        };
        let fade_secs = if musc.fade_secs.is_finite() { musc.fade_secs.max(0.0) } else { self.fade_secs };
        Directive {
            priority: Some(musc.priority),
            ducking_db: has(MUSC_DUCKS_CURRENT_TRACK).then_some(musc.ducking_db),
            fade_secs,
            change_fade_secs: has(MUSC_ABRUPT_TRANSITION).then_some(0.0),
            ..Directive::new(source, content)
        }
    }

    fn authored_track(
        &self,
        index: &EsmIndex,
        form: u32,
        ancestry: &mut Vec<u32>,
        budget: &mut usize,
    ) -> Result<AuthoredTrack, Content> {
        let unresolved = |reason| Content::Unresolved(format!("music track {form:#010x}: {reason}"));
        if *budget == 0 || ancestry.len() >= 4 || ancestry.contains(&form) {
            return Err(unresolved("palette references exceed the traversal bounds or form a cycle"));
        }
        *budget -= 1;
        let track = index.music_tracks.get(&form).ok_or_else(|| unresolved("absent from the load order"))?;
        let label = || if track.editor_id.is_empty() { format!("{form:#010x}") } else { track.editor_id.clone() };
        if track.kind == MustKind::SingleTrack && !track.file.is_empty() {
            return match self.files.resolve_track(&track.file) {
                Content::Playlist(mut files) if files.len() == 1 => Ok(AuthoredTrack::File(files.remove(0))),
                issue @ (Content::Missing(_) | Content::Unresolved(_)) => Err(issue),
                _ => Err(unresolved("single track needs one file")),
            };
        }
        let duration = track.duration;
        if !duration.is_finite() || duration < 0.0 {
            return Err(unresolved("invalid duration"));
        }
        match track.kind {
            MustKind::SilentTrack => Ok(AuthoredTrack::Silence { label: label(), duration }),
            MustKind::Palette => {
                if !track.fade_out.is_finite() || track.fade_out < 0.0 {
                    return Err(unresolved("invalid fade out"));
                }
                ancestry.push(form);
                let mut layers = Vec::new();
                for members in track.sub_tracks.split(|member| *member == 0) {
                    let mut choices = Vec::new();
                    for &member in members {
                        if let Ok(choice) = self.authored_track(index, member, ancestry, budget) {
                            choices.push(choice);
                        }
                        if *budget == 0 {
                            break;
                        }
                    }
                    if !choices.is_empty() {
                        layers.push(choices.into());
                    }
                    if *budget == 0 {
                        break;
                    }
                }
                ancestry.pop();
                if layers.is_empty() {
                    return Err(unresolved("no resolvable palette layers"));
                }
                Ok(AuthoredTrack::Palette { label: label(), duration, fade_out: track.fade_out, layers })
            }
            _ => Err(unresolved("unsupported or incomplete track")),
        }
    }

    /// Fallout 3 / New Vegas `PlayMusic <music type>` (MUS-SCR-005, MUS-CMB-004). `Fallout3.exe`
    /// plays it on the one music stream the cell's music uses (`0x79f860` → `0x8e10b0` →
    /// `0x8e2680`), so it replaces that music at once; combat then replaces it with the battle music
    /// (the update `0x8e2fe0` fades to Battle over any track but a death / success sting or another
    /// battle, `0x8e35ea`), a request while Battle plays is dropped (`0x8e2680` refuses it), and the
    /// cell's music follows combat. So Battle ends the directive (#6946). New Vegas keeps the shared
    /// music code; its executable is encrypted.
    pub(crate) fn play_music(&self, index: &EsmIndex, form: u32) -> Directive {
        Directive { ends_on_battle: !self.authored_music_types(), ..self.music_type(index, form, "PlayMusic") }
    }

    fn cell<'a>(index: &'a EsmIndex, place: &Place) -> Option<&'a CellData> {
        match place {
            Place::Interior { cell } => index.cells.cells.get(cell),
            Place::Exterior { worldspace, grid } => index.cells.exterior_cells.get(worldspace)?.get(grid),
        }
    }

    /// The Location directive for `place` (MUS-SRC-001, -002, -004, -008, -009).
    /// [`Self::location_with`] without OBSE overrides: the authored sources alone (the census, tests).
    #[cfg(test)]
    pub(crate) fn location(&self, index: &EsmIndex, place: &Place) -> Directive {
        self.location_with(index, place, &HashMap::new())
    }

    /// The cell's own Oblivion category, as OBSE `GetCellMusicType` reads it: a `SetCellMusicType`
    /// override (by CELL FormID), else its `XCMT`, else Default (MUS-SCR-004).
    pub(crate) fn cell_category(index: &EsmIndex, place: &Place, overrides: &HashMap<u32, u32>) -> u32 {
        let Some(cell) = Self::cell(index, place) else { return 0 };
        overrides.get(&cell.form_id).copied().or(cell.music_type_enum.map(u32::from)).unwrap_or(0)
    }

    /// [`Self::location`] with OBSE `SetCellMusicType` overrides, which win over the cell's `XCMT`.
    pub(crate) fn location_with(&self, index: &EsmIndex, place: &Place, overrides: &HashMap<u32, u32>) -> Directive {
        if let Some(reason) = self.untranslated() {
            return Directive::unresolved(Source::keyword("location", "game"), reason);
        }
        let cell = Self::cell(index, place);
        let worldspace = match place {
            Place::Exterior { worldspace, .. } => index.cells.worldspaces.get(worldspace),
            Place::Interior { .. } => None,
        };
        if self.game == GameKind::Oblivion {
            // Oblivion.exe's `TESObjectCELL::GetMusicType` (`0x4cad00`, #6903): the cell's
            // `ExtraCellMusicType` (XCMT, or an OBSE SetCellMusicType override, MUS-SCR-004), then
            // for an exterior the region sound data at the player (RDMD), then the worldspace's SNAM.
            // Each is read as a value, so a Default one falls through to the next (MUS-SRC-008).
            let set = |v: u32| (v != 0).then_some(v);
            let region_category = || {
                let entry = select_active_region_sound(&cell?.regions, &index.regions)?;
                let RegionDataPayload::Sound { music_type, .. } = &entry.payload else { return None };
                music_type.and_then(set)
            };
            let own = cell.and_then(|c| match overrides.get(&c.form_id) {
                Some(v) => set(*v).map(|v| (v, "SetCellMusicType")),
                None => c.music_type_enum.map(u32::from).and_then(set).map(|v| (v, "cell XCMT")),
            });
            let (value, via) = own
                .or_else(|| worldspace.and(region_category()).map(|v| (v, "region RDMD")))
                .or_else(|| worldspace.and_then(|w| w.music_category).and_then(set).map(|v| (v, "worldspace SNAM")))
                .unwrap_or((0, "no category"));
            // A cell change starts another category's music at once (`0x6ab160` stops the old track).
            return Directive { category: Some(value), change_fade_secs: Some(0.0), ..self.category(value, via) };
        }
        if self.authored_music_types() {
            let source = Self::authored_location_type(index, cell, worldspace);
            return match source {
                Some((form, via)) => self.music_type(index, form, via),
                None => self.directive(Source::keyword("no music source", "none"), Content::Silent),
            };
        }
        // Fallout 3 / New Vegas: cell XCMO → worldspace ZNAM (an exterior's) → DOBJ Default Music
        // (#6900). Fallout3.exe's `TESObjectCELL::GetMusicType` (0x745460) reads the cell's
        // `ExtraCellMusicType`, else an exterior's worldspace music; a null result plays
        // `GetDefaultObject(DefaultMusic)` (0x8e10b0). No region is consulted: the region loader
        // keeps `RDMO` but no music path reads it, and drops `RDMD` (as the cell loader drops `XCMT`).
        if let Some(form) = cell.and_then(|c| c.music_type_form) {
            return self.music_type(index, form, "cell XCMO");
        }
        if let Some(form) = worldspace.and_then(|w| w.default_music) {
            return self.music_type(index, form, "worldspace ZNAM");
        }
        match index.default_music.and_then(|music| music.default) {
            Some(form) => self.music_type(index, form, "DOBJ Default Music"),
            None => self.directive(Source::keyword("no music source", "none"), Content::Silent),
        }
    }

    fn authored_location_type(
        index: &EsmIndex,
        cell: Option<&CellData>,
        worldspace: Option<&WorldspaceRecord>,
    ) -> Option<(u32, &'static str)> {
        // V5 verifies the source fields. Cross-source ordering is provisional.
        let location_music = |location| index.locations.get(&location).and_then(|record| record.music);
        cell.and_then(|cell| cell.music_type_form).map(|form| (form, "cell XCMO"))
            .or_else(|| cell?.location_form.and_then(location_music).map(|form| (form, "cell location NAM1")))
            .or_else(|| worldspace?.location_form.and_then(location_music).map(|form| (form, "worldspace location NAM1")))
            .or_else(|| {
                worldspace?;
                let entry = select_active_region_sound(&cell?.regions, &index.regions)?;
                let RegionDataPayload::Sound { music: Some(form), .. } = entry.payload else { return None };
                Some((form, "region RDMO"))
            })
            .or_else(|| worldspace?.default_music.map(|form| (form, "worldspace ZNAM")))
            .or_else(|| index.default_music?.default.map(|form| (form, "DOBJ Default Music")))
    }

    /// An Oblivion category's folder (MUS-SRC-009).
    pub(crate) fn category(&self, value: u32, via: &'static str) -> Directive {
        match category_folder(value) {
            Some((name, folder)) => Directive { looping: false, ..self.directive(Source::keyword(name, via), self.files.resolve(folder)) },
            None => Directive::unresolved(Source::keyword(format!("category {value}"), via), "not a music category"),
        }
    }

    /// The Battle directive while the player is in combat at `place` (MUS-CMB-001): on New Vegas the
    /// marker's controller's Battle sets, then the Battle track of the Dungeon set the marker plays,
    /// then the region's `RDSB` set, then the Battle Music object (that order between
    /// them is the engine's, #6906); Oblivion's `Battle` folder.
    pub(crate) fn battle(&self, index: &EsmIndex, place: Option<&Place>, marker: Option<&Marker>, around: Surroundings, roll: u64) -> Directive {
        if let Some(reason) = self.untranslated() {
            return Directive::unresolved(Source::keyword("Battle", "game"), reason);
        }
        if self.game == GameKind::Oblivion {
            let folder = self.files.resolve("battle\\");
            return Directive { recovery_secs: Some(self.recovery_secs), ..self.directive(Source::keyword("Battle", "Data\\Music\\Battle"), folder) };
        }
        let controller_set = marker
            .and_then(|m| index.media_locations.get(&m.controller))
            .and_then(|aloc| aloc.battle_sets.first())
            .and_then(|form| index.media_sets.get(form))
            .map(|set| (set, "audio marker Battle set"));
        let dungeon_set = || {
            let ActiveSet::Set(form, _) = Self::active_set(index, marker?, around.relation, roll)? else { return None };
            index.media_sets.get(&form).filter(|set| set.kind == MediaSetKind::Dungeon).map(|set| (set, "audio marker Dungeon set"))
        };
        let region_set = || {
            let cell = Self::cell(index, place?)?;
            let entry = select_active_region_sound(&cell.regions, &index.regions)?;
            let RegionDataPayload::Sound { music: Some(form), .. } = &entry.payload else { return None };
            index.media_sets.get(form).map(|set| (set, "region RDSB"))
        };
        if self.game == GameKind::Fallout3NV
            && let Some((set, via)) = controller_set.or_else(dungeon_set).or_else(region_set) {
            return self.battle_set(index, set, via);
        }
        let directive = match index.default_music.and_then(|music| music.battle) {
            Some(form) => self.music_type(index, form, "DOBJ Battle Music"),
            None => self.directive(Source::keyword("no battle music", "none"), Content::Silent),
        };
        Directive { recovery_secs: Some(self.recovery_secs), ..directive }
    }

    /// Oblivion `StreamMusic` (MUS-SCR-003), as Oblivion.exe runs it (`0x509610`, #6903):
    /// - a keyword makes its category the music's: `Explore`, `Public`, `Dungeon`, or `Random` (a
    ///   roll of four: Explore, Public, Dungeon, Explore), playing on from its folder after a track;
    /// - a quoted path relative to the game folder plays once, at once, as Default music, then the
    ///   Default music plays on;
    /// - `Battle` is `None`: battle music follows combat, which drops it at once.
    ///
    /// Either outlives a cell change into its own category (`category`) and ends when Battle plays.
    pub(crate) fn stream_music(&self, text: &str, roll: u64) -> Option<Directive> {
        let text = text.trim().trim_matches('"').trim();
        let source = Source::keyword(text, "StreamMusic");
        let keyword = |source: Source, category: u32, name: &str| Directive {
            category: Some(category),
            persists: true,
            ends_on_battle: true,
            ..self.directive(source, self.files.resolve(&format!("{name}\\")))
        };
        let directive = match text.to_ascii_lowercase().as_str() {
            "explore" => keyword(source, 0, "explore"),
            "public" => keyword(source, 1, "public"),
            "dungeon" => keyword(source, 2, "dungeon"),
            "random" => {
                let (category, name) = [(0, "explore"), (1, "public"), (2, "dungeon"), (3, "explore")][(roll % 4) as usize];
                let mut directive = keyword(source, category, name);
                directive.notes.push(format!("rolled {name}"));
                directive
            }
            "battle" => return None,
            _ => {
                let path = text.replace('/', "\\");
                let lower = path.to_ascii_lowercase();
                let lower = lower.trim_start_matches(".\\");
                let content = match lower.strip_prefix("data\\music\\") {
                    Some(rest) => self.files.resolve(&path[path.len() - rest.len()..]),
                    None => Content::Unresolved(format!("{text} is outside Data\\Music")),
                };
                let then = keyword(Source::keyword("Default", "StreamMusic"), 0, "explore");
                Directive {
                    category: Some(0),
                    change_fade_secs: Some(0.0),
                    ends_on_battle: true,
                    then: Some(Box::new(then)),
                    ..self.directive(source, content)
                }
            }
        };
        Some(directive)
    }

    /// A sting's directive (MUS-STG-001, -002; #6901). Oblivion.exe plays its two files on the
    /// music graph (`0x600b7d` on the player's death, `0x5ad005` on a level-up), in the music's
    /// place. Fallout3.exe plays sounds: `MUSDeath` when the player dies, once it has stopped the
    /// music (`0x7e5d7d`), and `MUSSuccess` when the level-up menu opens (`LevelUpMenu::Update`,
    /// `0x93cdd0`), over the music. It never reads the Death, Success or Level Up Music objects
    /// (no `GetDefaultObject` call names slot 11, 12 or 13); New Vegas carries the same records
    /// and names.
    pub(crate) fn sting(&self, index: &EsmIndex, sting: Sting) -> Option<Directive> {
        // No sting where the music is not translated: it would also silence the music (#6931).
        // `sting()` scanned every `SOUN` for `MUSDeath` on Skyrim+ to find none.
        if self.untranslated().is_some() || self.authored_music_types() {
            return None;
        }
        if self.game == GameKind::Oblivion {
            // `Oblivion.exe` names both: `.\Data\Music\Special\death.mp3`, `success.mp3`.
            let (label, file) = match sting {
                Sting::Death => ("death", "special\\death.mp3"),
                Sting::LevelUp => ("success", "special\\success.mp3"),
            };
            return Some(self.directive(Source::keyword(label, "Data\\Music\\Special"), self.files.resolve(file)));
        }
        let (editor_id, over_music) = match sting {
            Sting::Death => ("MUSDeath", false),
            Sting::LevelUp => ("MUSSuccess", true),
        };
        let sound = index.sounds.values().find(|s| s.editor_id.eq_ignore_ascii_case(editor_id) && !s.is_folder())?;
        let path = crate::asset_provider::resolve_sound_path(&index.sounds, sound.form_id)?;
        let content = Content::Playlist(vec![crate::asset_provider::sound_archive_path(path)]);
        Some(Directive { over_music, ..self.directive(Source::record(sound.editor_id.clone(), sound.form_id, "SOUN"), content) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::{DefaultMusic, LctnRecord, MustRecord};

    #[test]
    fn authored_music_sources_resolve_tracks_and_combat_uses_the_default_object() {
        for game in [GameKind::Skyrim, GameKind::Fallout4] {
            let mut index = EsmIndex { game, ..Default::default() };
            index.music_types.insert(1, MuscRecord {
                editor_id: "Explore".into(), priority: 90, fade_secs: 12.0, tracks: vec![11], ..Default::default()
            });
            index.music_types.insert(2, MuscRecord {
                editor_id: "Combat".into(), priority: 8, tracks: vec![12], ..Default::default()
            });
            index.music_tracks.insert(11, MustRecord {
                kind: MustKind::SingleTrack, file: r"\Data\Music\Ambient\Explore.wav".into(), ..Default::default()
            });
            index.music_tracks.insert(12, MustRecord {
                kind: MustKind::SingleTrack, file: "data/Music/Combat.wav".into(), ..Default::default()
            });
            index.default_music = Some(DefaultMusic { default: Some(1), battle: Some(2), ..Default::default() });
            index.locations.insert(20, LctnRecord { music: Some(1), ..Default::default() });
            index.cells.worldspaces.insert("world".into(), WorldspaceRecord {
                location_form: Some(20), default_music: Some(2), ..Default::default()
            });
            index.cells.exterior_cells.entry("world".into()).or_default().insert((0, 0), CellData {
                music_type_form: Some(2), location_form: Some(20), ..Default::default()
            });
            let catalog = Catalog::new(&index, MusicFiles::new([
                "music\\ambient\\explore.xwm".to_string(),
                "music\\combat.xwm".to_string(),
                "music\\combat.wav".to_string(),
            ]), None);
            let exterior = |grid| Place::Exterior { worldspace: "world".into(), grid };
            let cell = catalog.location(&index, &exterior((0, 0)));
            assert_eq!(cell.source.via, "cell XCMO");
            assert!(matches!(cell.content, Content::AuthoredPlaylist { ref tracks, .. }
                if tracks == &[AuthoredTrack::File("music\\combat.wav".into())]));
            let location = catalog.location(&index, &exterior((1, 1)));
            assert_eq!(location.source.via, "worldspace location NAM1");
            assert!(matches!(location.content, Content::AuthoredPlaylist { ref tracks, .. }
                if tracks == &[AuthoredTrack::File("music\\ambient\\explore.xwm".into())]));
            assert_eq!((location.priority, location.fade_secs), (Some(90), 12.0));
            let battle = catalog.battle(&index, None, None, Surroundings::default(), 0);
            assert_eq!((battle.source.via, battle.priority), ("DOBJ Battle Music", Some(8)));
        }
    }

    fn musc(form: u32, edid: &str, file: &str) -> MuscRecord {
        MuscRecord { form_id: form, editor_id: edid.into(), file: file.into(), ..Default::default() }
    }

    /// A Fallout 3 load order: the default music objects, three music types, a worldspace with
    /// none, a cell with its own.
    fn fallout3() -> (EsmIndex, Catalog) {
        let mut index = EsmIndex { game: GameKind::Fallout3NV, ..Default::default() };
        for m in [musc(1, "DefaultExplore", "explore\\"), musc(2, "DefaultPublic", "public\\"), musc(3, "1NoMusic", ""), musc(4, "DefaultBattle", "battle\\"), musc(5, "DefaultDeath", "special\\death.mp3")] {
            index.music_types.insert(m.form_id, m);
        }
        index.default_music = Some(DefaultMusic { default: Some(1), battle: Some(4), death: Some(5), success: None, level_up: None });
        index.cells.cells.insert("megatoncratersidesupply".into(), CellData { music_type_form: Some(2), ..Default::default() });
        index.cells.cells.insert("quiet".into(), CellData { music_type_form: Some(3), ..Default::default() });
        index.cells.worldspaces.insert("wasteland".into(), WorldspaceRecord::default());
        let mut grids = std::collections::HashMap::new();
        grids.insert((0, 0), CellData::default());
        index.cells.exterior_cells.insert("wasteland".into(), grids);
        let files = MusicFiles::new(
            ["music\\explore\\a.mp3", "music\\explore\\b.mp3", "music\\public\\p.mp3", "music\\battle\\x.mp3", "music\\special\\death.mp3"].map(String::from),
        );
        let catalog = Catalog::new(&index, files, Some("[Audio]\nfDefaultMusicVolume=0.3000\n"));
        (index, catalog)
    }

    #[test]
    fn a_cell_music_type_wins_and_a_silent_one_is_silent() {
        let (index, catalog) = fallout3();
        let d = catalog.location(&index, &Place::Interior { cell: "megatoncratersidesupply".into() });
        assert_eq!(d.source.label, "DefaultPublic");
        assert_eq!(d.source.via, "cell XCMO");
        assert_eq!(d.content, Content::Playlist(vec!["music\\public\\p.mp3".into()]));
        let quiet = catalog.location(&index, &Place::Interior { cell: "quiet".into() });
        assert_eq!(quiet.content, Content::Silent, "1NoMusic (MUS-SRC-004)");
    }

    #[test]
    fn an_exterior_with_no_cell_or_worldspace_music_takes_the_default_music_object() {
        let (index, catalog) = fallout3();
        let d = catalog.location(&index, &Place::Exterior { worldspace: "wasteland".into(), grid: (0, 0) });
        assert_eq!((d.source.label.as_str(), d.source.via), ("DefaultExplore", "DOBJ Default Music"));
        assert!(matches!(&d.content, Content::Playlist(files) if files.len() == 2));
        assert_eq!(d.fade_secs, 1.0, "iMusicTransitionInMS's compiled-in 1000 ms");
    }

    /// #6901 — battle from the Battle Music object; the stings from the `MUSDeath` / `MUSSuccess`
    /// sounds Fallout3.exe plays, not the Death Music object the fixture also sets.
    #[test]
    fn battle_comes_from_the_battle_music_object_and_the_stings_from_their_sounds() {
        use byroredux_plugin::esm::records::SounRecord;
        let (mut index, catalog) = fallout3();
        let battle = catalog.battle(&index, None, None, Surroundings::default(), 0);
        assert_eq!(battle.source.label, "DefaultBattle");
        assert_eq!(battle.recovery_secs, Some(5.0), "fCombatMusicStopTime's compiled-in 5.0");
        assert_eq!(catalog.sting(&index, Sting::Death), None, "no MUSDeath sound, no sting");
        for (form, edid, path) in [(0x78595, "MUSDeath", "fx\\mus\\mus_death.wav"), (0x78594, "MUSSuccess", "fx\\mus\\mus_success.wav")] {
            index.sounds.insert(form, SounRecord { form_id: form, editor_id: edid.into(), sound_path: path.into(), ..Default::default() });
        }
        let death = catalog.sting(&index, Sting::Death).unwrap();
        assert_eq!((death.source.label.as_str(), death.source.via), ("MUSDeath", "SOUN"));
        assert_eq!(death.content, Content::Playlist(vec!["sound\\fx\\mus\\mus_death.wav".into()]));
        assert!(!death.looping && !death.over_music, "death stops the music");
        let level_up = catalog.sting(&index, Sting::LevelUp).unwrap();
        assert_eq!(level_up.content, Content::Playlist(vec!["sound\\fx\\mus\\mus_success.wav".into()]));
        assert!(level_up.over_music, "the level-up sting plays over the music");
    }

    #[test]
    fn a_positive_anam_loops_and_its_magnitude_attenuates() {
        let (mut index, _) = fallout3();
        index.music_types.insert(9, MuscRecord { decibels: 6.0, ..musc(9, "musDocMitchellHouse", "loc\\a.mp3") });
        let catalog = Catalog::new(&index, MusicFiles::new(["music\\loc\\a.mp3".to_string()]), None);
        let d = catalog.music_type(&index, 9, "cell XCMO");
        assert!(d.looping);
        assert!((d.level - 10f32.powf(-6.0 / 20.0)).abs() < 1e-6);
    }

    /// A New Vegas worldspace with one audio marker whose controller names a Location set and a
    /// Battle set, and a region whose RDSB names another battle set.
    fn new_vegas() -> (EsmIndex, Catalog) {
        use byroredux_plugin::esm::cell::{AudioMarker, PlacedRef};
        use byroredux_plugin::esm::records::{MediaLocationRecord, RegionDataEntry, RegionDataKind, RegnRecord};
        let mut index = EsmIndex { game: GameKind::Fallout3NV, ..Default::default() };
        let mut tracks: [String; 6] = Default::default();
        tracks[0] = "loc\\gs\\day_outer.mp3".into();
        tracks[1] = "loc\\gs\\day_middle.mp3".into();
        tracks[3] = "loc\\gs\\night_outer.mp3".into();
        index.media_sets.insert(
            0x50,
            MediaSetRecord {
                form_id: 0x50,
                editor_id: "musSetGoodsprings".into(),
                kind: MediaSetKind::Location,
                tracks,
                boundaries: [100.0, 66.6, 33.3, 100.0, 66.6, 33.3],
                enable_flags: 0b001011,
                timings: [6.0, 15.0, 9.0, 0.0],
                ..Default::default()
            },
        );
        let mut battle_tracks: [String; 6] = Default::default();
        battle_tracks[0] = "battle\\loop.mp3".into();
        index.media_sets.insert(
            0x60,
            MediaSetRecord { form_id: 0x60, editor_id: "musSetBattleGS".into(), kind: MediaSetKind::Battle, tracks: battle_tracks.clone(), timings: [1.0, 3.0, 10.0, 0.0], ..Default::default() },
        );
        index.media_sets.insert(
            0x61,
            MediaSetRecord { form_id: 0x61, editor_id: "musSetBattleRegion".into(), kind: MediaSetKind::Battle, tracks: battle_tracks, timings: [1.0, 3.0, 8.0, 0.0], ..Default::default() },
        );
        index.media_locations.insert(
            0x70,
            MediaLocationRecord { form_id: 0x70, editor_id: "musCtrlGoodsprings".into(), day_start: 64, night_start: 255, location_sets: vec![0x50], battle_sets: vec![0x60], enemy_sets: vec![0x51], ..Default::default() },
        );
        index.regions.insert(
            0x80,
            RegnRecord {
                entries: vec![RegionDataEntry {
                    kind: RegionDataKind::Sound,
                    flags: 0,
                    priority: 50,
                    payload: RegionDataPayload::Sound { music: Some(0x61), music_type: None, incidental: None, sounds: Vec::new() },
                }],
                ..Default::default()
            },
        );
        let mut marker = PlacedRef::at(0x152433, 0x23, [1000.0, 0.0, 0.0], [0.0; 3]);
        marker.radius_override = Some(5000.0);
        marker.audio_marker = Some(AudioMarker { controller: Some(0x70), use_controller_values: true, layer_triggers: [0.33, 0.67] });
        let mut grids = HashMap::new();
        grids.insert((0, 0), CellData { regions: vec![0x80], ..Default::default() });
        index.cells.exterior_cells.insert("wastelandnv".into(), grids);
        index.cells.worldspace_persistent_cells.insert("wastelandnv".into(), CellData { references: vec![marker], ..Default::default() });
        let files = MusicFiles::new(
            ["music\\loc\\gs\\day_outer.mp3", "music\\loc\\gs\\day_middle.mp3", "music\\loc\\gs\\night_outer.mp3", "music\\battle\\loop.mp3"].map(String::from),
        );
        let catalog = Catalog::new(&index, files, None);
        (index, catalog)
    }

    #[test]
    fn inside_an_audio_markers_area_its_location_set_plays_as_stems() {
        let (index, catalog) = new_vegas();
        let place = Place::Exterior { worldspace: "wastelandnv".into(), grid: (0, 0) };
        assert_eq!(catalog.marker_at(&place, [7000.0, 0.0, 0.0]), None, "outside the 5000-unit area");
        let marker = catalog.marker_at(&place, [2000.0, 0.0, 0.0]).expect("inside it");
        assert_eq!(marker.form, 0x152433);
        let d = catalog.marker_location(&index, &marker, Surroundings::default(), 0).unwrap();
        assert_eq!(d.source.label, "musSetGoodsprings");
        assert_eq!(d.fade_secs, 9.0, "the set's layer crossfade time");
        let Content::Stems { stems, area } = &d.content else { panic!("{d:?}") };
        assert_eq!(stems.iter().map(|s| (s.key.as_str(), s.night, s.reach)).collect::<Vec<_>>(), [
            ("music\\loc\\gs\\day_outer.mp3", false, 1.0),
            ("music\\loc\\gs\\day_middle.mp3", false, 0.666),
            ("music\\loc\\gs\\night_outer.mp3", true, 1.0),
        ]);
        assert_eq!((area.radius, area.day_start, area.night_start), (5000.0, 64, 255));
        assert!(d.notes.iter().any(|n| n.contains("controller musCtrlGoodsprings")), "{:?}", d.notes);
    }

    /// #6904 — the conditional faction's reaction to the player picks the relation list ahead of the
    /// Location list; a missing list falls back to Location, then Neutral; Battle sets only play none.
    #[test]
    fn the_conditional_factions_relation_picks_the_list() {
        let (mut index, _) = new_vegas();
        let mut set = |form: u32, edid: &str| {
            let mut tracks: [String; 6] = Default::default();
            tracks[0] = "loc\\gs\\day_outer.mp3".into();
            index.media_sets.insert(form, MediaSetRecord { form_id: form, editor_id: edid.into(), kind: MediaSetKind::Location, tracks, enable_flags: 1, boundaries: [100.0; 6], ..Default::default() });
        };
        set(0x52, "musSetGoodspringsNeutral");
        set(0x51, "musSetGoodspringsEnemy");
        let aloc = index.media_locations.get_mut(&0x70).unwrap();
        aloc.conditional_faction = Some(0x104C6E);
        aloc.neutral_sets = vec![0x52];
        aloc.location_sets.clear();
        let catalog = Catalog::new(&index, MusicFiles::new(["music\\loc\\gs\\day_outer.mp3".to_string()]), None);
        let place = Place::Exterior { worldspace: "wastelandnv".into(), grid: (0, 0) };
        let marker = catalog.marker_at(&place, [1000.0, 0.0, 0.0]).unwrap();
        let pick = |index: &EsmIndex, relation| {
            let around = Surroundings { relation: Some(relation), tension: false };
            catalog.marker_location(index, &marker, around, 0).map(|d| (d.source.label, d.source.via))
        };
        assert_eq!(pick(&index, CombatReaction::Neutral), Some(("musSetGoodspringsNeutral".into(), "audio marker Neutral set")));
        assert_eq!(pick(&index, CombatReaction::Enemy), Some(("musSetGoodspringsEnemy".into(), "audio marker Enemy set")));
        assert_eq!(pick(&index, CombatReaction::Friend), Some(("musSetGoodspringsNeutral".into(), "audio marker Neutral set")), "no Friend list, no Location list: Neutral");
        index.media_locations.get_mut(&0x70).unwrap().location_sets = vec![0x50];
        assert_eq!(pick(&index, CombatReaction::Friend), Some(("musSetGoodsprings".into(), "audio marker")), "no Friend list: Location");
        index.media_locations.get_mut(&0x70).unwrap().conditional_faction = None;
        assert_eq!(pick(&index, CombatReaction::Enemy).unwrap().0, "musSetGoodsprings", "no conditional faction: the Location list");
        let aloc = index.media_locations.get_mut(&0x70).unwrap();
        (aloc.location_sets, aloc.neutral_sets, aloc.enemy_sets) = (Vec::new(), Vec::new(), Vec::new());
        assert_eq!(pick(&index, CombatReaction::Neutral), None, "battle sets only: the place's own music");
    }

    /// #6906 — `NAM1`: with no actor of the conditional faction found the "Faction Not Found" list
    /// plays (None: no music); Random picks the rolled set.
    #[test]
    fn faction_not_found_and_random_come_from_nam1() {
        let (mut index, _) = new_vegas();
        let mut tracks: [String; 6] = Default::default();
        tracks[0] = "loc\\gs\\day_outer.mp3".into();
        for (form, edid) in [(0x51, "musSetEnemy"), (0x52, "musSetNeutral"), (0x53, "musSetLocationB")] {
            index.media_sets.insert(form, MediaSetRecord { form_id: form, editor_id: edid.into(), kind: MediaSetKind::Location, tracks: tracks.clone(), enable_flags: 1, boundaries: [100.0; 6], ..Default::default() });
        }
        let aloc = index.media_locations.get_mut(&0x70).unwrap();
        (aloc.conditional_faction, aloc.neutral_sets, aloc.location_sets, aloc.flags) = (Some(0x104C6E), vec![0x52], vec![0x50, 0x53], 0x40);
        let catalog = Catalog::new(&index, MusicFiles::new(["music\\loc\\gs\\day_outer.mp3".to_string()]), None);
        let place = Place::Exterior { worldspace: "wastelandnv".into(), grid: (0, 0) };
        let marker = catalog.marker_at(&place, [1000.0, 0.0, 0.0]).unwrap();
        let play = |index: &EsmIndex, roll| catalog.marker_location(index, &marker, Surroundings::default(), roll).map(|d| (d.source.label, d.content.clone()));
        assert_eq!(play(&index, 0).unwrap().0, "musSetNeutral", "0x40: Faction Not Found is Neutral (musCtrlGoodsprings)");
        index.media_locations.get_mut(&0x70).unwrap().flags = 0x44;
        assert_eq!(play(&index, 0).unwrap().0, "musSetGoodsprings", "0x44: the Location list, its first set (Loop)");
        index.media_locations.get_mut(&0x70).unwrap().flags = 0x54;
        assert_eq!(play(&index, 1).unwrap().0, "musSetLocationB", "0x54: Random takes the rolled set");
        index.media_locations.get_mut(&0x70).unwrap().flags = 0x45;
        assert_eq!(play(&index, 0).unwrap().1, Content::Silent, "0x45: None, no music (musCtrlSmallRV)");
        let found = Surroundings { relation: Some(CombatReaction::Enemy), tension: false };
        assert_eq!(catalog.marker_location(&index, &marker, found, 0).unwrap().source.label, "musSetEnemy", "a found faction actor's relation wins");
    }

    /// #6946 — `PlayMusic` is a music type that Battle ends (Fallout3.exe `0x8e35ea`, MUS-CMB-004).
    #[test]
    fn play_music_is_a_music_type_battle_ends() {
        let (mut index, catalog) = new_vegas();
        index.music_types.insert(0x90, MuscRecord { form_id: 0x90, editor_id: "MUSDefaultBattle".into(), file: "battle\\loop.mp3".into(), ..Default::default() });
        let played = catalog.play_music(&index, 0x90);
        assert!(played.ends_on_battle && played.source.via == "PlayMusic", "{played:?}");
        assert_eq!(played.content, Content::Playlist(vec!["music\\battle\\loop.mp3".into()]));
        assert!(!catalog.music_type(&index, 0x90, "cell").ends_on_battle, "a place's music type is not a script directive");
    }

    /// #6906 — a Dungeon set plays its Suspense track under the controller's tension; #6946 — only a
    /// controller listing one makes the music system test tension.
    #[test]
    fn a_dungeon_set_plays_suspense_under_tension() {
        let (mut index, location_only) = new_vegas();
        let place = Place::Exterior { worldspace: "wastelandnv".into(), grid: (0, 0) };
        assert!(!location_only.marker_at(&place, [1000.0, 0.0, 0.0]).unwrap().counts_tension, "a Location set ignores tension");
        let mut tracks: [String; 6] = Default::default();
        (tracks[0], tracks[1], tracks[2]) = ("d\\battle.mp3".into(), "d\\explore.mp3".into(), "d\\suspense.mp3".into());
        index.media_sets.insert(0x53, MediaSetRecord { form_id: 0x53, editor_id: "musSetDungeon".into(), kind: MediaSetKind::Dungeon, tracks, ..Default::default() });
        index.media_locations.get_mut(&0x70).unwrap().location_sets = vec![0x53];
        let catalog = Catalog::new(&index, MusicFiles::new(["music\\d\\explore.mp3", "music\\d\\suspense.mp3"].map(String::from)), None);
        let marker = catalog.marker_at(&place, [1000.0, 0.0, 0.0]).unwrap();
        assert!(marker.counts_tension);
        let content = |tension| catalog.marker_location(&index, &marker, Surroundings { relation: None, tension }, 0).unwrap().content;
        assert_eq!(content(false), Content::Playlist(vec!["music\\d\\explore.mp3".into()]));
        assert_eq!(content(true), Content::Playlist(vec!["music\\d\\suspense.mp3".into()]));
    }

    /// #6902 — GECK *Reference*: with "Use Controller Values" clear the marker's own triggers place
    /// the rings — Inner to layer 2's, Middle to layer 3's, Outer to the edge.
    #[test]
    fn a_markers_own_layer_triggers_place_the_rings_when_controller_values_are_off() {
        use byroredux_plugin::esm::cell::AudioMarker;
        let (mut index, _) = new_vegas();
        let marker = &mut index.cells.worldspace_persistent_cells.get_mut("wastelandnv").unwrap().references[0];
        marker.audio_marker = Some(AudioMarker { controller: Some(0x70), use_controller_values: false, layer_triggers: [0.2, 0.5] });
        let files = ["music\\loc\\gs\\day_outer.mp3", "music\\loc\\gs\\day_middle.mp3", "music\\loc\\gs\\night_outer.mp3"].map(String::from);
        let catalog = Catalog::new(&index, MusicFiles::new(files), None);
        let place = Place::Exterior { worldspace: "wastelandnv".into(), grid: (0, 0) };
        let marker = catalog.marker_at(&place, [1000.0, 0.0, 0.0]).unwrap();
        assert_eq!(marker.triggers, Some([0.2, 0.5]));
        let d = catalog.marker_location(&index, &marker, Surroundings::default(), 0).unwrap();
        let Content::Stems { stems, .. } = &d.content else { panic!("{d:?}") };
        assert_eq!(stems.iter().map(|s| s.reach).collect::<Vec<_>>(), [1.0, 0.5, 1.0], "outer to the edge, middle to layer 3's trigger");
    }

    /// #6902 — GECK *Media Set*: a Dungeon set's Battle track plays "while the player is engaged in
    /// combat", after the controller's Battle sets and before the region's.
    #[test]
    fn a_dungeon_sets_battle_track_plays_in_combat() {
        let (mut index, _) = new_vegas();
        let mut tracks: [String; 6] = Default::default();
        tracks[0] = "old\\fo1\\battle.mp3".into();
        tracks[1] = "old\\fo1\\explore.mp3".into();
        tracks[2] = tracks[1].clone();
        index.media_sets.insert(0x53, MediaSetRecord { form_id: 0x53, editor_id: "musSetOldFO1".into(), kind: MediaSetKind::Dungeon, tracks, timings: [0.0, 0.0, 4.0, 0.0], ..Default::default() });
        let aloc = index.media_locations.get_mut(&0x70).unwrap();
        aloc.location_sets = vec![0x53];
        let files = ["music\\old\\fo1\\battle.mp3", "music\\old\\fo1\\explore.mp3", "music\\battle\\loop.mp3"].map(String::from);
        let catalog = Catalog::new(&index, MusicFiles::new(files.clone()), None);
        let place = Place::Exterior { worldspace: "wastelandnv".into(), grid: (0, 0) };
        let marker = catalog.marker_at(&place, [1000.0, 0.0, 0.0]);
        let explore = catalog.marker_location(&index, marker.as_ref().unwrap(), Surroundings::default(), 0).unwrap();
        assert_eq!(explore.content, Content::Playlist(vec!["music\\old\\fo1\\explore.mp3".into()]));
        let battle = catalog.battle(&index, Some(&place), marker.as_ref(), Surroundings::default(), 0);
        assert_eq!(battle.source.label, "musSetBattleGS", "the controller's Battle sets first");
        index.media_locations.get_mut(&0x70).unwrap().battle_sets.clear();
        let catalog = Catalog::new(&index, MusicFiles::new(files), None);
        let battle = catalog.battle(&index, Some(&place), marker.as_ref(), Surroundings::default(), 0);
        assert_eq!((battle.source.label.as_str(), battle.source.via), ("musSetOldFO1", "audio marker Dungeon set"));
        assert_eq!(battle.content, Content::Playlist(vec!["music\\old\\fo1\\battle.mp3".into()]));
        assert_eq!((battle.fade_secs, battle.recovery_secs), (4.0, Some(catalog.recovery_secs)), "its layer crossfade, the game's recovery");
    }

    #[test]
    fn new_vegas_battle_prefers_the_markers_set_then_the_regions() {
        let (index, catalog) = new_vegas();
        let place = Place::Exterior { worldspace: "wastelandnv".into(), grid: (0, 0) };
        let marker = catalog.marker_at(&place, [1000.0, 0.0, 0.0]);
        let battle = catalog.battle(&index, Some(&place), marker.as_ref(), Surroundings::default(), 0);
        assert_eq!((battle.source.label.as_str(), battle.recovery_secs), ("musSetBattleGS", Some(10.0)));
        assert_eq!(battle.content, Content::Playlist(vec!["music\\battle\\loop.mp3".into()]));
        assert!(battle.looping);
        let regional = catalog.battle(&index, Some(&place), None, Surroundings::default(), 0);
        assert_eq!((regional.source.label.as_str(), regional.source.via), ("musSetBattleRegion", "region RDSB"));
        let location = catalog.location(&index, &place);
        assert_ne!(location.source.via, "region RDSB", "an RDSB battle set is no Location source");
    }

    /// #6900 — Fallout3.exe reads no region for location music: a region `RDMO` naming a music type
    /// loses to the Default Music object, and an interior never takes a worldspace's music.
    #[test]
    fn a_region_music_type_is_no_location_source_on_fallout_3_and_new_vegas() {
        use byroredux_plugin::esm::records::{RegionDataEntry, RegionDataKind, RegnRecord};
        let (mut index, _) = fallout3();
        let payload = RegionDataPayload::Sound { music: Some(2), music_type: Some(1), incidental: None, sounds: Vec::new() };
        let entry = RegionDataEntry { kind: RegionDataKind::Sound, flags: 0, priority: 50, payload };
        index.regions.insert(0x80, RegnRecord { entries: vec![entry], ..Default::default() });
        index.cells.exterior_cells.get_mut("wasteland").unwrap().get_mut(&(0, 0)).unwrap().regions = vec![0x80];
        index.cells.cells.insert("vault19a".into(), CellData { regions: vec![0x80], ..Default::default() });
        index.cells.worldspaces.get_mut("wasteland").unwrap().default_music = Some(2);
        let catalog = Catalog::new(&index, MusicFiles::new(["music\\explore\\a.mp3".to_string()]), None);
        let exterior = catalog.location(&index, &Place::Exterior { worldspace: "wasteland".into(), grid: (0, 0) });
        assert_eq!((exterior.source.label.as_str(), exterior.source.via), ("DefaultPublic", "worldspace ZNAM"));
        let interior = catalog.location(&index, &Place::Interior { cell: "vault19a".into() });
        assert_eq!((interior.source.label.as_str(), interior.source.via), ("DefaultExplore", "DOBJ Default Music"));
    }

    fn oblivion() -> (EsmIndex, Catalog) {
        use byroredux_plugin::esm::records::{RegionDataEntry, RegionDataKind, RegnRecord};
        let mut index = EsmIndex { game: GameKind::Oblivion, ..Default::default() };
        index.cells.cells.insert("brumajghastashouse".into(), CellData { music_type_enum: Some(1), ..Default::default() });
        index.cells.cells.insert("sageglenhollow".into(), CellData { music_type_enum: Some(2), ..Default::default() });
        index.cells.worldspaces.insert("tamriel".into(), WorldspaceRecord::default());
        index.regions.insert(
            0x80,
            RegnRecord {
                entries: vec![RegionDataEntry {
                    kind: RegionDataKind::Sound,
                    flags: 0,
                    priority: 50,
                    payload: RegionDataPayload::Sound { music: None, music_type: Some(1), incidental: None, sounds: Vec::new() },
                }],
                ..Default::default()
            },
        );
        let mut grids = HashMap::new();
        grids.insert((0, 0), CellData { regions: vec![0x80], ..Default::default() });
        grids.insert((1, 0), CellData::default());
        index.cells.exterior_cells.insert("tamriel".into(), grids);
        let files = MusicFiles::new(
            ["music\\public\\town_01.mp3", "music\\dungeon\\dungeon_01.mp3", "music\\explore\\atmosphere_01.mp3", "music\\battle\\battle_01.mp3", "music\\special\\death.mp3", "music\\special\\success.mp3"]
                .map(String::from),
        );
        let catalog = Catalog::new(&index, files, None);
        (index, catalog)
    }

    /// #6903 — Oblivion.exe's `GetMusicType` (`0x4cad00`): the cell, then an exterior's region,
    /// then its worldspace; a Default value at any step falls through to the next.
    #[test]
    fn oblivion_categories_pick_their_folders_cell_then_region_then_worldspace() {
        let (mut index, _) = oblivion();
        index.cells.worldspaces.get_mut("tamriel").unwrap().music_category = Some(2);
        index.cells.exterior_cells.get_mut("tamriel").unwrap().insert((2, 0), CellData { music_type_enum: Some(0), regions: vec![0x80], ..Default::default() });
        index.cells.cells.insert("defaultinterior".into(), CellData { music_type_enum: Some(0), regions: vec![0x80], ..Default::default() });
        let catalog = Catalog::new(&index, oblivion().1.files, None);
        let public = catalog.location(&index, &Place::Interior { cell: "brumajghastashouse".into() });
        assert_eq!((public.source.label.as_str(), public.source.via), ("Public", "cell XCMT"));
        assert_eq!(public.content, Content::Playlist(vec!["music\\public\\town_01.mp3".into()]));
        let dungeon = catalog.location(&index, &Place::Interior { cell: "sageglenhollow".into() });
        assert_eq!(dungeon.content, Content::Playlist(vec!["music\\dungeon\\dungeon_01.mp3".into()]));
        let exterior = |grid| catalog.location(&index, &Place::Exterior { worldspace: "tamriel".into(), grid });
        let region = exterior((0, 0));
        assert_eq!((region.source.label.as_str(), region.source.via), ("Public", "region RDMD"), "the region before the worldspace");
        assert_eq!((exterior((2, 0)).source.label.as_str(), exterior((2, 0)).source.via), ("Public", "region RDMD"), "a Default XCMT falls through");
        assert_eq!((exterior((1, 0)).source.label.as_str(), exterior((1, 0)).source.via), ("Dungeon", "worldspace SNAM"));
        let interior = catalog.location(&index, &Place::Interior { cell: "defaultinterior".into() });
        assert_eq!((interior.source.label.as_str(), interior.source.via), ("Default", "no category"), "an interior reads no region");
        assert_eq!(interior.content, Content::Playlist(vec!["music\\explore\\atmosphere_01.mp3".into()]), "Q6: Default plays Explore");
        assert_eq!((interior.category, interior.change_fade_secs, interior.fade_secs), (Some(0), Some(0.0), 0.85), "a cell change switches at once");
        assert_eq!(catalog.recovery_secs, 0.0, "battle music ends with combat");
    }

    /// MUS-SCR-004 — a SetCellMusicType override wins over the cell's XCMT, and GetCellMusicType reads it.
    #[test]
    fn an_obse_override_changes_the_cells_category() {
        let (mut index, catalog) = oblivion();
        index.cells.cells.get_mut("brumajghastashouse").unwrap().form_id = 0x42;
        let place = Place::Interior { cell: "brumajghastashouse".into() };
        let mut overrides = HashMap::new();
        assert_eq!(Catalog::cell_category(&index, &place, &overrides), 1);
        overrides.insert(0x42, 2);
        let d = catalog.location_with(&index, &place, &overrides);
        assert_eq!((d.source.label.as_str(), d.source.via), ("Dungeon", "SetCellMusicType"));
        assert_eq!(Catalog::cell_category(&index, &place, &overrides), 2);
    }

    /// #6903 — `StreamMusic` as Oblivion.exe's `0x509610` runs it.
    #[test]
    fn oblivion_battle_stings_and_stream_music() {
        let (index, catalog) = oblivion();
        assert_eq!(catalog.battle(&index, None, None, Surroundings::default(), 0).content, Content::Playlist(vec!["music\\battle\\battle_01.mp3".into()]));
        assert_eq!(catalog.sting(&index, Sting::LevelUp).unwrap().content, Content::Playlist(vec!["music\\special\\success.mp3".into()]));
        let stream = |text: &str, roll| catalog.stream_music(text, roll).expect(text);
        let dungeon = stream("Dungeon", 0);
        assert_eq!((dungeon.category, dungeon.persists, dungeon.ends_on_battle), (Some(2), true, true), "MUS-CMB-004");
        assert_eq!(dungeon.content, Content::Playlist(vec!["music\\dungeon\\dungeon_01.mp3".into()]));
        let rolled: Vec<_> = (0..4).map(|roll| (stream("Random", roll).category, stream("Random", roll).content)).collect();
        let folder = |file: &str| Content::Playlist(vec![file.to_string()]);
        assert_eq!(rolled, [
            (Some(0), folder("music\\explore\\atmosphere_01.mp3")),
            (Some(1), folder("music\\public\\town_01.mp3")),
            (Some(2), folder("music\\dungeon\\dungeon_01.mp3")),
            (Some(3), folder("music\\explore\\atmosphere_01.mp3")),
        ]);
        assert_eq!(stream("Random", 1).source.label, "Random");
        assert_eq!(catalog.stream_music("battle", 0), None, "battle music follows combat");
        let file = stream("\"data\\music\\special\\success.mp3\"", 0);
        assert_eq!(file.content, Content::Playlist(vec!["music\\special\\success.mp3".into()]));
        assert_eq!((file.category, file.change_fade_secs, file.persists), (Some(0), Some(0.0), false), "a file plays once, at once, as Default");
        assert_eq!(file.then.as_ref().map(|then| (then.category, then.persists)), Some((Some(0), true)), "then the Default music");
        assert_eq!(stream("\"Data\\Music\\Special\\nope.mp3\"", 0).outcome(), crate::music::directive::Outcome::Missing);
        assert_eq!(stream("\"C:\\WINDOWS\\Media\\tada.wav\"", 0).outcome(), crate::music::directive::Outcome::Unresolved);
    }

    /// #6931 — combat and the stings decline where the music is not translated, as the place does:
    /// Starfield's Battle is an explicit Unresolved, and there is no sting (which would silence it).
    #[test]
    fn battle_and_stings_decline_where_the_music_is_not_translated() {
        // The index carries the sting SOUNs, so only the decline can make `sting()` return `None`
        // (INC2 missing test: without them both assertions held before #6931 too).
        let mut index = EsmIndex { game: GameKind::Starfield, ..Default::default() };
        for (form, edid) in [(0x10, "MUSDeath"), (0x11, "MUSSuccess")] {
            index.sounds.insert(form, byroredux_plugin::esm::records::SounRecord { form_id: form, editor_id: edid.into(), sound_path: "music\\special\\sting.wav".into(), ..Default::default() });
        }
        let catalog = Catalog::new(&index, MusicFiles::new(Vec::<String>::new()), None);
        let battle = catalog.battle(&index, None, None, Surroundings::default(), 0);
        assert_eq!(battle.outcome(), crate::music::directive::Outcome::Unresolved, "{battle:?}");
        assert!(catalog.sting(&index, Sting::Death).is_none());
        assert!(catalog.sting(&index, Sting::LevelUp).is_none());
        // The same records on a translated game do resolve: the fixture is not what declines.
        index.game = GameKind::Fallout3NV;
        let fallout = Catalog::new(&index, MusicFiles::new(Vec::<String>::new()), None);
        assert!(fallout.sting(&index, Sting::Death).is_some());
    }
}
