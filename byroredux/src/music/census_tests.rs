//! Game Music PRD §7.2 scenario 1 and §8 metrics (#6880): every music source of a base master and
//! its DLC, translated, against the MUS-0a census in ROADMAP's music column. Ignored: needs each
//! game's install (`BYROREDUX_FO3_DATA`, `BYROREDUX_FNV_DATA`, `BYROREDUX_OBLIVION_DATA`).
//!
//! `cargo test --release -p byroredux --bin byroredux music::census_tests -- --ignored --nocapture`

use super::directive::{MusicFiles, Outcome};
use super::translate::{Catalog, Place};
use std::collections::BTreeMap;

/// Per-source outcome tallies, and the §8 coverage figures.
#[derive(Default, Debug)]
struct Tally {
    sources: usize,
    outcomes: BTreeMap<&'static str, usize>,
}

impl Tally {
    fn add(&mut self, outcome: Outcome) {
        self.sources += 1;
        let name = match outcome {
            Outcome::Playing => "playing",
            Outcome::Silent => "silent",
            Outcome::Unresolved => "unresolved",
            Outcome::Missing => "missing",
        };
        *self.outcomes.entry(name).or_default() += 1;
    }

    fn count(&self, name: &str) -> usize {
        self.outcomes.get(name).copied().unwrap_or(0)
    }

    /// Sources translated to a directive (anything but Unresolved) ÷ sources.
    fn source_coverage(&self) -> f64 {
        (self.sources - self.count("unresolved")) as f64 / self.sources.max(1) as f64
    }

    /// Directives whose files exist ÷ translated directives (Silent is a translated directive with
    /// nothing to find).
    fn playable_coverage(&self) -> f64 {
        let translated = self.sources - self.count("unresolved") - self.count("silent");
        self.count("playing") as f64 / translated.max(1) as f64
    }
}

/// The install, or `None` after reporting it missing (`test_paths::require_or_skip`, #6184).
fn data_dir(dir: std::path::PathBuf, what: &str) -> Option<std::path::PathBuf> {
    if dir.is_dir() {
        return Some(dir);
    }
    byroredux_plugin::esm::test_paths::require_or_skip(what);
    None
}

/// Every music source of the load order `plugins` (base master first) under `data`, translated.
fn census(data: &std::path::Path, plugins: &[&str]) -> (Catalog, Vec<(&'static str, Tally)>) {
    let paths: Vec<String> = plugins.iter().map(|p| data.join(p).to_string_lossy().into_owned()).collect();
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let (index, _) = crate::cell_loader::parse_record_indexes_in_load_order(&refs).expect("parse the load order");
    let args = vec!["byroredux".to_string(), "--esm".to_string(), paths[0].clone()];
    let provider = crate::asset_provider::build_sound_archive_provider(&args);
    let ini = crate::game_ini::read_default_ini(&args, index.game);
    let catalog = Catalog::new(&index, MusicFiles::new(provider.music_keys()), ini.as_deref());

    let mut tallies: BTreeMap<&'static str, Tally> = BTreeMap::new();
    let mut add = |kind: &'static str, outcome: Outcome| tallies.entry(kind).or_default().add(outcome);
    // A marker's Location set, or — a controller with Battle sets only — the place's own music, which
    // the place's census already counts.
    let add_marker = |add: &mut dyn FnMut(&'static str, Outcome), catalog: &Catalog, place: &Place, r: &byroredux_plugin::esm::cell::PlacedRef| {
        let found = catalog.marker_at(place, r.position).filter(|m| m.form == r.form_id).expect("a marker is found at its own center");
        match catalog.marker_location(&index, &found) {
            Some(directive) => {
                if directive.outcome() == Outcome::Playing && std::env::var_os("MUSIC_CENSUS_LIST").is_some() {
                    let day_night = matches!(&directive.content, super::directive::Content::Stems { stems, .. } if stems.iter().any(|s| s.night) && stems.iter().any(|s| !s.night));
                    println!("  marker {:#010x} at {:?} r={} {} day+night={day_night} {:?}", found.form, found.center, found.radius, directive.source, directive.notes);
                }
                add("audio marker", directive.outcome())
            }
            None => add("audio marker (battle sets only)", Outcome::Playing),
        }
    };
    // Each place that authors its own music: what the translate step makes of it.
    for (key, cell) in &index.cells.cells {
        if cell.music_type_form.is_some() || cell.music_type_enum.is_some() {
            add("cell", catalog.location(&index, &Place::Interior { cell: key.clone() }).outcome());
        }
    }
    for (worldspace, grids) in &index.cells.exterior_cells {
        for (grid, cell) in grids {
            if cell.music_type_form.is_some() || cell.music_type_enum.is_some() {
                add("cell", catalog.location(&index, &Place::Exterior { worldspace: worldspace.clone(), grid: *grid }).outcome());
            }
        }
    }
    for worldspace in index.cells.worldspaces.values() {
        if let Some(form) = worldspace.default_music {
            add("worldspace", catalog.music_type(&index, form, "census").outcome());
        } else if let Some(category) = worldspace.music_category {
            add("worldspace", catalog.category(category, "census").outcome());
        }
    }
    for region in index.regions.values() {
        for entry in &region.entries {
            let byroredux_plugin::esm::records::RegionDataPayload::Sound { music, music_type, .. } = &entry.payload else { continue };
            if let Some(form) = *music {
                let outcome = match index.media_sets.get(&form) {
                    Some(set) => catalog.battle_set(&index, set, "census").outcome(),
                    None => catalog.music_type(&index, form, "census").outcome(),
                };
                add("region", outcome);
            } else if let Some(category) = music_type.filter(|_| catalog.game == byroredux_plugin::esm::reader::GameKind::Oblivion) {
                add("region", catalog.category(category, "census").outcome());
            }
        }
    }
    for musc in index.music_types.values() {
        add("music type", catalog.music_type(&index, musc.form_id, "census").outcome());
    }
    if let Some(music) = index.default_music {
        for form in [music.default, music.battle, music.death, music.success, music.level_up].into_iter().flatten() {
            add("default object", catalog.music_type(&index, form, "census").outcome());
        }
    }
    for (key, cell) in &index.cells.cells {
        for marker in cell.references.iter().filter(|r| r.audio_marker.is_some()) {
            let place = Place::Interior { cell: key.clone() };
            add_marker(&mut add, &catalog, &place, marker);
        }
    }
    for (space, cells) in index.cells.exterior_cells.iter().map(|(ws, grids)| (ws, grids.values().collect::<Vec<_>>())) {
        let persistent = index.cells.worldspace_persistent_cells.get(space);
        for cell in cells.into_iter().chain(persistent) {
            for marker in cell.references.iter().filter(|r| r.audio_marker.is_some()) {
                let place = Place::Exterior { worldspace: space.clone(), grid: (0, 0) };
                add_marker(&mut add, &catalog, &place, marker);
            }
        }
    }
    (catalog, tallies.into_iter().collect())
}

fn report(game: &str, tallies: &[(&'static str, Tally)]) {
    for (kind, tally) in tallies {
        println!(
            "[{game}] {kind}: {} source(s) {:?}; source coverage {:.1}%, playable coverage {:.1}%",
            tally.sources,
            tally.outcomes,
            tally.source_coverage() * 100.0,
            tally.playable_coverage() * 100.0
        );
    }
}

fn tally<'a>(tallies: &'a [(&'static str, Tally)], kind: &str) -> &'a Tally {
    &tallies.iter().find(|(k, _)| *k == kind).unwrap_or_else(|| panic!("no {kind} sources")).1
}

/// Fallout 3 (base + 5 DLC): 31 music types — 22 files and 6 folders play, 2 missing
/// (`TenpennyLobbyMusic`'s `.wav`, `ProjPurityActivate`), 1 silent (`1NoMusic`); all five default
/// music objects play.
#[test]
#[ignore = "needs Fallout 3 game data on disk"]
fn fallout3_music_sources_translate_against_the_census() {
    let Some(data) = data_dir(byroredux_plugin::esm::test_paths::fo3_data_dir(), "Fallout 3 data") else { return };
    let (catalog, tallies) =
        census(&data, &["Fallout3.esm", "Anchorage.esm", "ThePitt.esm", "BrokenSteel.esm", "PointLookout.esm", "Zeta.esm"]);
    report("FO3", &tallies);
    assert_eq!((catalog.fade_secs, catalog.recovery_secs), (1.0, 5.0));
    let types = tally(&tallies, "music type");
    assert_eq!((types.sources, types.count("playing"), types.count("missing"), types.count("silent")), (31, 28, 2, 1));
    let objects = tally(&tallies, "default object");
    assert_eq!((objects.sources, objects.count("playing")), (5, 5));
    let cells = tally(&tallies, "cell");
    assert_eq!(cells.count("unresolved"), 0, "every cell source translates");
}

/// New Vegas (base + 9 DLC): 29 music types — 15 play, 13 missing (the `Default*` folders and the
/// death / success stings are not shipped), 1 silent; every audio marker's Location set plays.
/// The cells' music types all name those missing folders or `1NoMusic` (#6900).
#[test]
#[ignore = "needs New Vegas game data on disk"]
fn new_vegas_music_sources_translate_against_the_census() {
    let Some(data) = data_dir(byroredux_plugin::esm::test_paths::fnv_data_dir(), "New Vegas data") else { return };
    let (catalog, tallies) = census(
        &data,
        &[
            "FalloutNV.esm",
            "DeadMoney.esm",
            "HonestHearts.esm",
            "OldWorldBlues.esm",
            "LonesomeRoad.esm",
            "GunRunnersArsenal.esm",
            "CaravanPack.esm",
            "ClassicPack.esm",
            "MercenaryPack.esm",
            "TribalPack.esm",
        ],
    );
    report("FNV", &tallies);
    assert_eq!(catalog.recovery_secs, 3.0, "FalloutNV.esm's fCombatMusicStopTime");
    let types = tally(&tallies, "music type");
    assert_eq!((types.sources, types.count("playing"), types.count("missing"), types.count("silent")), (29, 15, 13, 1));
    // 689 markers (MUS-0a): 511 under a controller with a Location set, every one playing; 9 under a
    // controller with faction-relation sets only (Goodsprings), Unresolved until the relation is
    // modelled (Q3, #6904); 169 under a controller with Battle sets only, where the place's music plays.
    let markers = tally(&tallies, "audio marker");
    assert_eq!((markers.sources, markers.count("playing"), markers.count("unresolved")), (520, 511, 9));
    assert_eq!(tally(&tallies, "audio marker (battle sets only)").sources, 169);
}

/// Oblivion (base + the official DLC): every category source plays its folder; the stings are
/// shipped. Worldspace `SNAM` and region `RDMD` are not counted per cell here — the cells that author
/// `XCMT` are.
#[test]
#[ignore = "needs Oblivion game data on disk"]
fn oblivion_music_sources_translate_against_the_census() {
    let Some(data) = data_dir(byroredux_plugin::esm::test_paths::oblivion_data_dir(), "Oblivion data") else { return };
    let (catalog, tallies) = census(
        &data,
        &[
            "Oblivion.esm",
            "DLCShiveringIsles.esp",
            "Knights.esp",
            "DLCBattlehornCastle.esp",
            "DLCFrostcrag.esp",
            "DLCHorseArmor.esp",
            "DLCMehrunesRazor.esp",
            "DLCOrrery.esp",
            "DLCSpellTomes.esp",
            "DLCThievesDen.esp",
            "DLCVileLair.esp",
        ],
    );
    report("Oblivion", &tallies);
    let cells = tally(&tallies, "cell");
    assert_eq!(cells.count("playing"), cells.sources, "every XCMT category plays its folder");
    assert!(cells.sources > 0);
    let index_free = |sting| catalog.sting(&byroredux_plugin::esm::records::EsmIndex { game: catalog.game, ..Default::default() }, sting);
    for sting in [super::Sting::Death, super::Sting::LevelUp] {
        assert_eq!(index_free(sting).map(|d| d.outcome()), Some(Outcome::Playing), "{sting:?}");
    }
    assert_eq!(catalog.battle(&Default::default(), None, None).outcome(), Outcome::Playing);
}
