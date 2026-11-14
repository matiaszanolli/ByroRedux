//! Each game's music sources as directives: the only per-game code in the music path (PRD R5,
//! `docs/engine/music.md` §3, §5, §7).

use super::directive::{Area, Content, Directive, MusicFiles, Source, Stem};
use byroredux_plugin::esm::cell::CellData;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::records::{select_active_region_sound, EsmIndex, MediaSetKind, MediaSetRecord, RegionDataPayload};
use std::collections::HashMap;

/// Where the player is, as far as music cares: an interior cell, or an exterior grid cell of a
/// worldspace. Keys are the lower-cased EditorIDs the cell index uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Place {
    Interior { cell: String },
    Exterior { worldspace: String, grid: (i32, i32) },
}

/// The crossfade where a game names none: the interim region-music fade at HEAD (Oblivion, #6903).
pub(crate) const FALLBACK_FADE_SECS: f32 = 3.0;
/// `iMusicTransitionInMS:Audio`, compiled into `Fallout3.exe` as 1000 (read 2026-11-13).
const FALLOUT_MUSIC_TRANSITION_MS: f32 = 1000.0;
/// GMST `fCombatMusicStopTime`, compiled into `Fallout3.exe` as 5.0; also Oblivion's interim (#6903).
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
            _ => (FALLBACK_FADE_SECS, COMBAT_MUSIC_STOP_SECS),
        };
        let mut markers: HashMap<String, Vec<Marker>> = HashMap::new();
        let cells = &index.cells;
        let spaces = cells
            .cells
            .iter()
            .map(|(key, cell)| (key, cell))
            .chain(cells.exterior_cells.iter().flat_map(|(ws, grid)| grid.values().map(move |cell| (ws, cell))))
            .chain(cells.worldspace_persistent_cells.iter());
        for (space, cell) in spaces {
            for r in &cell.references {
                let (Some(marker), Some(radius)) = (r.audio_marker, r.radius_override.filter(|radius| *radius > 0.0)) else {
                    continue;
                };
                let Some(controller) = marker.controller else { continue };
                markers.entry(space.clone()).or_default().push(Marker { form: r.form_id, center: r.position, radius, controller });
            }
        }
        Self { game, files, fade_secs, recovery_secs, markers }
    }

    pub(crate) fn marker_count(&self) -> usize {
        self.markers.values().map(Vec::len).sum()
    }

    /// The audio marker whose area holds `player` at `place`: of several, the one the player is
    /// deepest into, by distance over radius (MUS-SRC-005, #6902).
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

    /// A marker's Location directive: its controller's first Location set, as stems by day and night
    /// and by depth (MUS-SRC-005 to -007). `None` when the controller names no Location set, so the
    /// place's own music plays.
    pub(crate) fn marker_location(&self, index: &EsmIndex, marker: &Marker) -> Option<Directive> {
        let aloc = index.media_locations.get(&marker.controller)?;
        let note = format!("controller {} ({:#010x}), marker {:#010x}", aloc.editor_id, aloc.form_id, marker.form);
        let Some(&set_form) = aloc.location_sets.first() else {
            // Battle sets only: the place's own music plays. Relation sets only (Goodsprings): what
            // they play waits on the faction relation (Q3), so the area reports Unresolved rather
            // than falling back to music the data does not ask for.
            if !aloc.has_relation_sets() {
                return None;
            }
            let source = Source::record(aloc.editor_id.clone(), aloc.form_id, "audio marker");
            let reason = "faction-relation sets only (Neutral / Ally / Friend / Enemy): not played until the relation is modelled (Q3, #6904)";
            return Some(Directive { notes: vec![note], ..Directive::unresolved(source, reason) });
        };
        let mut directive = match index.media_sets.get(&set_form) {
            Some(set) => self.location_set(set, marker, aloc.day_start, aloc.night_start),
            None => Directive::unresolved(
                Source { label: format!("{set_form:#010x}"), form: Some(set_form), via: "audio marker" },
                "not a media set in the load order",
            ),
        };
        directive.notes.push(note);
        if aloc.has_relation_sets() {
            // Q3: the faction-relation lists wait on how the relation is computed (#6902).
            directive.notes.push("relation sets Unresolved (faction relation not modelled, Q3)".to_string());
        }
        Some(directive)
    }

    fn location_set(&self, set: &MediaSetRecord, marker: &Marker, day_start: u32, night_start: u32) -> Directive {
        let source = Source::record(set.editor_id.clone(), set.form_id, "audio marker");
        let fade = Some(set.timings[2]).filter(|t| *t > 0.0).unwrap_or(self.fade_secs);
        // 23 of New Vegas's controllers list a Dungeon set among their Location sets. Its slots are
        // Battle / Explore / Suspense (xEdit's `(D)` labels), not layers: its Explore track plays;
        // when Suspense and Battle play is #6902.
        if set.kind == MediaSetKind::Dungeon {
            let directive = Directive { fade_secs: fade, looping: true, level: 10f32.powf(set.track_decibels[1] / 20.0), ..Directive::new(source, self.files.resolve(&set.tracks[1])) };
            return Directive { notes: vec!["a Dungeon set: its Explore track (Suspense and Battle: #6902)".to_string()], ..directive };
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
                    reach: set.boundaries[i] / 100.0,
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

    /// A New Vegas Battle set: its loop track, intro and outro, and recovery time (MUS-CMB-001 to -003).
    pub(crate) fn battle_set(&self, index: &EsmIndex, set: &MediaSetRecord, via: &'static str) -> Directive {
        let source = Source::record(set.editor_id.clone(), set.form_id, via);
        if set.kind != MediaSetKind::Battle {
            return Directive::unresolved(source, format!("a {:?} set, not a battle set", set.kind));
        }
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
            // `ENAM` "Loop Fade Out", `FNAM` "Recovery Time" (xEdit's (B) labels).
            fade_secs: Some(set.timings[1]).filter(|t| *t > 0.0).unwrap_or(self.fade_secs),
            recovery_secs: Some(set.timings[2]).filter(|t| *t > 0.0).or(Some(self.recovery_secs)),
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
            GameKind::Fallout3NV | GameKind::Oblivion => None,
            GameKind::Skyrim | GameKind::Fallout4 => Some("xWMA music waits on its own PRD (MUS-NG-001)"),
            _ => Some("Wwise music is out of scope (MUS-NG-005)"),
        }
    }

    /// A Fallout 3 / New Vegas music type (`MUSC`): its file or folder, looping when New Vegas's
    /// `ANAM` is positive, at `-|ANAM|` dB (MUS-SEL-003, #6902).
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
        Directive {
            looping: musc.looping(),
            level: 10f32.powf(-musc.decibels.abs() / 20.0),
            ..self.directive(source, self.files.resolve(&musc.file))
        }
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
        let region = cell
            .and_then(|cell| select_active_region_sound(&cell.regions, &index.regions))
            .and_then(|entry| match &entry.payload {
                RegionDataPayload::Sound { music, music_type, .. } => Some((*music, *music_type)),
                _ => None,
            });
        if self.game == GameKind::Oblivion {
            // Cell XCMT → worldspace SNAM → region RDMD → Default (MUS-SRC-008, #6903); an OBSE
            // SetCellMusicType override first (MUS-SCR-004).
            let (value, via) = cell
                .and_then(|c| overrides.get(&c.form_id).map(|v| (*v, "SetCellMusicType")))
                .or_else(|| cell.and_then(|c| c.music_type_enum).map(|v| (u32::from(v), "cell XCMT")))
                .or_else(|| worldspace.and_then(|w| w.music_category).map(|v| (v, "worldspace SNAM")))
                .or_else(|| region.and_then(|(_, category)| category).map(|v| (v, "region RDMD")))
                .unwrap_or((0, "no category"));
            return self.category(value, via);
        }
        // Fallout 3 / New Vegas: cell XCMO → worldspace ZNAM → region RDMO → DOBJ Default Music.
        // The region's `music` word also carries New Vegas's RDSB, a battle `MSET`: only a `MUSC`
        // is a Location source.
        if let Some(form) = cell.and_then(|c| c.music_type_form) {
            return self.music_type(index, form, "cell XCMO");
        }
        if let Some(form) = worldspace.and_then(|w| w.default_music) {
            return self.music_type(index, form, "worldspace ZNAM");
        }
        if let Some(form) = region.and_then(|(music, _)| music).filter(|form| index.music_types.contains_key(form)) {
            return self.music_type(index, form, "region RDMO");
        }
        match index.default_music.and_then(|music| music.default) {
            Some(form) => self.music_type(index, form, "DOBJ Default Music"),
            None => self.directive(Source::keyword("no music source", "none"), Content::Silent),
        }
    }

    /// An Oblivion category's folder (MUS-SRC-009).
    pub(crate) fn category(&self, value: u32, via: &'static str) -> Directive {
        match category_folder(value) {
            Some((name, folder)) => Directive { looping: false, ..self.directive(Source::keyword(name, via), self.files.resolve(folder)) },
            None => Directive::unresolved(Source::keyword(format!("category {value}"), via), "not a music category"),
        }
    }

    /// The Battle directive while the player is in combat at `place` (MUS-CMB-001): on New Vegas the
    /// marker's controller's Battle sets, then the region's `RDSB` set (#6902), then the Battle Music
    /// object; Oblivion's `Battle` folder.
    pub(crate) fn battle(&self, index: &EsmIndex, place: Option<&Place>, marker: Option<&Marker>) -> Directive {
        if self.game == GameKind::Oblivion {
            let folder = self.files.resolve("battle\\");
            return Directive { recovery_secs: Some(self.recovery_secs), ..self.directive(Source::keyword("Battle", "Data\\Music\\Battle"), folder) };
        }
        let controller_set = marker
            .and_then(|m| index.media_locations.get(&m.controller))
            .and_then(|aloc| aloc.battle_sets.first())
            .and_then(|form| index.media_sets.get(form))
            .map(|set| (set, "audio marker Battle set"));
        let region_set = || {
            let cell = Self::cell(index, place?)?;
            let entry = select_active_region_sound(&cell.regions, &index.regions)?;
            let RegionDataPayload::Sound { music: Some(form), .. } = &entry.payload else { return None };
            index.media_sets.get(form).map(|set| (set, "region RDSB"))
        };
        if let Some((set, via)) = controller_set.or_else(region_set) {
            return self.battle_set(index, set, via);
        }
        let directive = match index.default_music.and_then(|music| music.battle) {
            Some(form) => self.music_type(index, form, "DOBJ Battle Music"),
            None => self.directive(Source::keyword("no battle music", "none"), Content::Silent),
        };
        Directive { recovery_secs: Some(self.recovery_secs), ..directive }
    }

    /// Oblivion `StreamMusic` (MUS-SCR-003): a quoted path relative to the game folder, or a random
    /// track of `Public`, `Explore`, `Dungeon`, or of all three (`Random`). It gives way to battle
    /// music ("StreamMusic will also not override combat music", CS *StreamMusic*, MUS-CMB-004).
    pub(crate) fn stream_music(&self, text: &str) -> Directive {
        let text = text.trim().trim_matches('"').trim();
        let source = Source::keyword(text, "StreamMusic");
        let folder = |name: &str| self.files.resolve(&format!("{name}\\"));
        let content = match text.to_ascii_lowercase().as_str() {
            keyword @ ("public" | "explore" | "dungeon") => folder(keyword),
            "random" => {
                let files: Vec<String> = ["public", "explore", "dungeon"]
                    .into_iter()
                    .filter_map(|name| match folder(name) {
                        Content::Playlist(files) => Some(files),
                        _ => None,
                    })
                    .flatten()
                    .collect();
                if files.is_empty() { Content::Missing("Data\\Music\\{Public,Explore,Dungeon}\\".to_string()) } else { Content::Playlist(files) }
            }
            _ => {
                let path = text.replace('/', "\\");
                let lower = path.to_ascii_lowercase();
                let lower = lower.trim_start_matches(".\\");
                match lower.strip_prefix("data\\music\\") {
                    Some(rest) => self.files.resolve(&path[path.len() - rest.len()..]),
                    None => Content::Unresolved(format!("{text} is outside Data\\Music")),
                }
            }
        };
        Directive { gives_way_to_battle: true, ..self.directive(source, content) }
    }

    /// A sting's directive (MUS-STG-001, -002); the Success Music object is bound to no event
    /// (MUS-STG-003, #6901).
    pub(crate) fn sting(&self, index: &EsmIndex, sting: Sting) -> Option<Directive> {
        if self.game == GameKind::Oblivion {
            // `Oblivion.exe` names both: `.\Data\Music\Special\death.mp3`, `success.mp3`.
            let (label, file) = match sting {
                Sting::Death => ("death", "special\\death.mp3"),
                Sting::LevelUp => ("success", "special\\success.mp3"),
            };
            return Some(self.directive(Source::keyword(label, "Data\\Music\\Special"), self.files.resolve(file)));
        }
        let music = index.default_music?;
        let (form, via) = match sting {
            Sting::Death => (music.death?, "DOBJ Death Music"),
            Sting::LevelUp => (music.level_up?, "DOBJ Level Up Music"),
        };
        // A sting plays once, whatever its music type's loop flag says.
        Some(Directive { looping: false, ..self.music_type(index, form, via) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::cell::WorldspaceRecord;
    use byroredux_plugin::esm::records::{DefaultMusic, MuscRecord};

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

    #[test]
    fn battle_and_stings_come_from_the_default_music_objects() {
        let (index, catalog) = fallout3();
        let battle = catalog.battle(&index, None, None);
        assert_eq!(battle.source.label, "DefaultBattle");
        assert_eq!(battle.recovery_secs, Some(5.0), "fCombatMusicStopTime's compiled-in 5.0");
        let death = catalog.sting(&index, Sting::Death).unwrap();
        assert_eq!(death.content, Content::Playlist(vec!["music\\special\\death.mp3".into()]));
        assert!(!death.looping);
        assert_eq!(catalog.sting(&index, Sting::LevelUp), None, "an unset slot plays nothing");
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
        let d = catalog.marker_location(&index, &marker).unwrap();
        assert_eq!(d.source.label, "musSetGoodsprings");
        assert_eq!(d.fade_secs, 9.0, "the set's layer crossfade time");
        let Content::Stems { stems, area } = &d.content else { panic!("{d:?}") };
        assert_eq!(stems.iter().map(|s| (s.key.as_str(), s.night, s.reach)).collect::<Vec<_>>(), [
            ("music\\loc\\gs\\day_outer.mp3", false, 1.0),
            ("music\\loc\\gs\\day_middle.mp3", false, 0.666),
            ("music\\loc\\gs\\night_outer.mp3", true, 1.0),
        ]);
        assert_eq!((area.radius, area.day_start, area.night_start), (5000.0, 64, 255));
        assert!(d.notes.iter().any(|n| n.contains("relation sets Unresolved")), "Q3");
    }

    /// #6904 — a controller with faction-relation sets and no Location set (Goodsprings) reports
    /// Unresolved instead of falling back to music the data does not ask for.
    #[test]
    fn a_relation_only_controller_is_unresolved() {
        let (mut index, _) = new_vegas();
        let aloc = index.media_locations.get_mut(&0x70).unwrap();
        aloc.location_sets.clear();
        let catalog = Catalog::new(&index, MusicFiles::default(), None);
        let place = Place::Exterior { worldspace: "wastelandnv".into(), grid: (0, 0) };
        let marker = catalog.marker_at(&place, [1000.0, 0.0, 0.0]).unwrap();
        let d = catalog.marker_location(&index, &marker).unwrap();
        assert_eq!(d.source.label, "musCtrlGoodsprings");
        assert!(matches!(&d.content, Content::Unresolved(reason) if reason.contains("faction-relation sets only")), "{d:?}");
        index.media_locations.get_mut(&0x70).unwrap().enemy_sets.clear();
        assert_eq!(catalog.marker_location(&index, &marker), None, "battle sets only: the place's own music");
    }

    #[test]
    fn new_vegas_battle_prefers_the_markers_set_then_the_regions() {
        let (index, catalog) = new_vegas();
        let place = Place::Exterior { worldspace: "wastelandnv".into(), grid: (0, 0) };
        let marker = catalog.marker_at(&place, [1000.0, 0.0, 0.0]);
        let battle = catalog.battle(&index, Some(&place), marker.as_ref());
        assert_eq!((battle.source.label.as_str(), battle.recovery_secs), ("musSetBattleGS", Some(10.0)));
        assert_eq!(battle.content, Content::Playlist(vec!["music\\battle\\loop.mp3".into()]));
        assert!(battle.looping);
        let regional = catalog.battle(&index, Some(&place), None);
        assert_eq!((regional.source.label.as_str(), regional.source.via), ("musSetBattleRegion", "region RDSB"));
        let location = catalog.location(&index, &place);
        assert_ne!(location.source.via, "region RDMO", "an RDSB battle set is no Location source");
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

    #[test]
    fn oblivion_categories_pick_their_folders_cell_then_worldspace_then_region() {
        let (index, catalog) = oblivion();
        let public = catalog.location(&index, &Place::Interior { cell: "brumajghastashouse".into() });
        assert_eq!((public.source.label.as_str(), public.source.via), ("Public", "cell XCMT"));
        assert_eq!(public.content, Content::Playlist(vec!["music\\public\\town_01.mp3".into()]));
        let dungeon = catalog.location(&index, &Place::Interior { cell: "sageglenhollow".into() });
        assert_eq!(dungeon.content, Content::Playlist(vec!["music\\dungeon\\dungeon_01.mp3".into()]));
        let region = catalog.location(&index, &Place::Exterior { worldspace: "tamriel".into(), grid: (0, 0) });
        assert_eq!((region.source.label.as_str(), region.source.via), ("Public", "region RDMD"));
        let default = catalog.location(&index, &Place::Exterior { worldspace: "tamriel".into(), grid: (1, 0) });
        assert_eq!(default.source.label, "Default");
        assert_eq!(default.content, Content::Playlist(vec!["music\\explore\\atmosphere_01.mp3".into()]), "Q6: Default plays Explore");
        assert_eq!(default.fade_secs, FALLBACK_FADE_SECS);
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

    #[test]
    fn oblivion_battle_stings_and_stream_music() {
        let (index, catalog) = oblivion();
        assert_eq!(catalog.battle(&index, None, None).content, Content::Playlist(vec!["music\\battle\\battle_01.mp3".into()]));
        assert_eq!(catalog.sting(&index, Sting::LevelUp).unwrap().content, Content::Playlist(vec!["music\\special\\success.mp3".into()]));
        let random = catalog.stream_music("Random");
        assert!(random.gives_way_to_battle, "MUS-CMB-004");
        assert!(matches!(&random.content, Content::Playlist(files) if files.len() == 3));
        assert_eq!(catalog.stream_music("\"data\\music\\special\\success.mp3\"").content, Content::Playlist(vec!["music\\special\\success.mp3".into()]));
        assert_eq!(catalog.stream_music("\"Data\\Music\\Special\\nope.mp3\"").outcome(), crate::music::directive::Outcome::Missing);
        assert_eq!(catalog.stream_music("\"C:\\WINDOWS\\Media\\tada.wav\"").outcome(), crate::music::directive::Outcome::Unresolved);
    }
}
