//! #6636 / DLOD-GATE-007 — the distant-LOD family census.
//!
//! Counts, per title, the files of every LOD family in the vanilla archives
//! (name tables only, no extraction), with the classifier shared with
//! `examples/probe_lod_corpus.rs` (`byroredux_bsa::lod_family`). The PRD
//! `distant-lod-completion` §3.1 *Vanilla data* figures, and every count a
//! requirement repeats from them, are checked against this test.
//!
//! Skips without game data (the strict lane, `BYROREDUX_REQUIRE_GAME_DATA=1`,
//! turns a missing install or archive into a failure). Opt in with:
//! ```sh
//! cargo test -p byroredux-plugin --test lod_family_census -- --ignored --nocapture
//! ```
//! `LOD_CENSUS_PRINT=1` prints the measured tables in the form pinned below.

use byroredux_bsa::lod_family::{classify, LodFamily};
use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_plugin::esm::test_paths;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

struct Title {
    name: &'static str,
    data_dir: fn() -> PathBuf,
    /// The base game's archives, by file name.
    base: &'static [&'static str],
    /// The official DLC archives, by file name. Counted apart: the PRD's §3.1 figures are base-game
    /// figures with the DLC called out where they differ.
    dlc: &'static [&'static str],
}

const TITLES: &[Title] = &[
    Title {
        name: "Oblivion",
        data_dir: test_paths::oblivion_data_dir,
        base: &["Oblivion - Meshes.bsa", "Oblivion - Misc.bsa", "Oblivion - Textures - Compressed.bsa"],
        dlc: &["DLCShiveringIsles - Meshes.bsa", "DLCShiveringIsles - Textures.bsa"],
    },
    Title {
        name: "Fallout 3",
        data_dir: test_paths::fo3_data_dir,
        base: &["Fallout - Meshes.bsa", "Fallout - Misc.bsa", "Fallout - Textures.bsa"],
        dlc: &[
            "Anchorage - Main.bsa",
            "BrokenSteel - Main.bsa",
            "PointLookout - Main.bsa",
            "ThePitt - Main.bsa",
            "Zeta - Main.bsa",
        ],
    },
    Title {
        name: "Fallout New Vegas",
        data_dir: test_paths::fnv_data_dir,
        base: &["Fallout - Meshes.bsa", "Fallout - Misc.bsa", "Fallout - Textures.bsa", "Fallout - Textures2.bsa"],
        dlc: &["DeadMoney - Main.bsa", "HonestHearts - Main.bsa", "OldWorldBlues - Main.bsa", "LonesomeRoad - Main.bsa"],
    },
    Title {
        name: "Skyrim SE",
        data_dir: test_paths::skyrim_se_data_dir,
        base: &[
            "Skyrim - Meshes0.bsa",
            "Skyrim - Meshes1.bsa",
            "Skyrim - Misc.bsa",
            "Skyrim - Textures0.bsa",
            "Skyrim - Textures1.bsa",
            "Skyrim - Textures2.bsa",
            "Skyrim - Textures3.bsa",
            "Skyrim - Textures4.bsa",
            "Skyrim - Textures5.bsa",
            "Skyrim - Textures6.bsa",
            "Skyrim - Textures7.bsa",
            "Skyrim - Textures8.bsa",
        ],
        dlc: &[],
    },
    Title {
        name: "Fallout 4",
        data_dir: test_paths::fo4_data_dir,
        base: &[
            "Fallout4 - Meshes.ba2",
            "Fallout4 - MeshesExtra.ba2",
            "Fallout4 - Misc.ba2",
            "Fallout4 - Startup.ba2",
            "Fallout4 - Textures1.ba2",
        ],
        dlc: &[
            "DLCNukaWorld - Main.ba2",
            "DLCRobot - Main.ba2",
            "DLCworkshop01 - Main.ba2",
            "DLCworkshop02 - Main.ba2",
            "DLCworkshop03 - Main.ba2",
        ],
    },
];

fn data_dir(title: &Title) -> Option<PathBuf> {
    let dir = (title.data_dir)();
    if dir.is_dir() {
        return Some(dir);
    }
    test_paths::require_or_skip(&format!("{} at {dir:?}", title.name));
    None
}

fn list_files(path: &Path) -> Vec<String> {
    let mut magic = [0u8; 4];
    std::io::Read::read_exact(&mut std::fs::File::open(path).unwrap(), &mut magic).unwrap();
    if &magic == b"BTDX" {
        Ba2Archive::open(path.to_str().unwrap()).unwrap().list_files().iter().map(|s| s.to_string()).collect()
    } else {
        BsaArchive::open(path.to_str().unwrap()).unwrap().list_files().iter().map(|s| s.to_string()).collect()
    }
}

/// `(family, world)` → files; world is `-` for a family whose path names none.
type Census = BTreeMap<(LodFamily, String), usize>;

fn census(title: &Title, dir: &Path, archives: &[&str]) -> Option<Census> {
    let mut out = Census::new();
    for name in archives {
        let path = dir.join(name);
        if !path.is_file() {
            test_paths::require_or_skip(&format!("{} archive {path:?}", title.name));
            return None;
        }
        for file in list_files(&path) {
            for hit in classify(&file) {
                *out.entry((hit.family, hit.world.unwrap_or_else(|| "-".to_owned()))).or_default() += 1;
            }
        }
    }
    Some(out)
}

fn totals(c: &Census) -> BTreeMap<&'static str, usize> {
    let mut t = BTreeMap::new();
    for ((family, _), n) in c {
        *t.entry(family.label()).or_default() += n;
    }
    t
}

/// The families whose per-worldspace counts are pinned: the small ones the PRD's requirements, scenarios and
/// decisions quote by worldspace.
const PER_WORLD: &[LodFamily] = &[
    LodFamily::FalloutTreeDtl,
    LodFamily::FalloutTreeTypes,
    LodFamily::DlodSettings,
    LodFamily::LodSettingsLod,
    LodFamily::CreationTreesBtt,
    LodFamily::CreationTreeList,
    LodFamily::OblivionLod,
];

fn per_world(c: &Census) -> Vec<String> {
    c.iter()
        .filter(|((family, _), _)| PER_WORLD.contains(family))
        .map(|((family, world), n)| format!("{}:{world}={n}", family.label()))
        .collect()
}

struct Measured {
    base_totals: BTreeMap<&'static str, usize>,
    dlc_totals: BTreeMap<&'static str, usize>,
    base_worlds: Vec<String>,
    dlc_worlds: Vec<String>,
}

impl Measured {
    /// The measured figures as the `expected` arm below spells them (for `LOD_CENSUS_PRINT=1`).
    fn as_expected(&self, name: &str) -> String {
        let totals = |t: &BTreeMap<&'static str, usize>| {
            t.iter().map(|(k, v)| format!("(\"{k}\", {v})")).collect::<Vec<_>>().join(", ")
        };
        let worlds = |w: &[String]| w.iter().map(|x| format!("                \"{x}\",\n")).collect::<String>();
        format!(
            "        \"{name}\" => pinned(\n            &[{}],\n            &[{}],\n            &[\n{}            ],\n            &[\n{}            ],\n        ),",
            totals(&self.base_totals), totals(&self.dlc_totals), worlds(&self.base_worlds), worlds(&self.dlc_worlds)
        )
    }
}

#[test]
#[ignore = "needs the vanilla game archives on disk"]
fn lod_family_census() {
    let print = std::env::var("LOD_CENSUS_PRINT").is_ok();
    let mut checked = 0;
    for title in TITLES {
        let Some(dir) = data_dir(title) else { continue };
        let (Some(base), Some(dlc)) = (census(title, &dir, title.base), census(title, &dir, title.dlc)) else {
            continue;
        };
        let measured = Measured {
            base_totals: totals(&base),
            dlc_totals: totals(&dlc),
            base_worlds: per_world(&base),
            dlc_worlds: per_world(&dlc),
        };
        if print {
            println!("{}", measured.as_expected(title.name));
        } else {
            let expected = expected(title.name);
            assert_eq!(measured.base_totals, expected.base_totals, "{}: base family totals", title.name);
            assert_eq!(measured.dlc_totals, expected.dlc_totals, "{}: DLC family totals", title.name);
            assert_eq!(measured.base_worlds, expected.base_worlds, "{}: base per-worldspace", title.name);
            assert_eq!(measured.dlc_worlds, expected.dlc_worlds, "{}: DLC per-worldspace", title.name);
        }
        checked += 1;
    }
    eprintln!("lod_family_census: {checked} titles measured");
}

/// A pinned figure set, in the shape [`Measured`] reads.
fn pinned(
    base_totals: &[(&'static str, usize)],
    dlc_totals: &[(&'static str, usize)],
    base_worlds: &[&str],
    dlc_worlds: &[&str],
) -> Measured {
    Measured {
        base_totals: base_totals.iter().copied().collect(),
        dlc_totals: dlc_totals.iter().copied().collect(),
        base_worlds: base_worlds.iter().map(|s| (*s).to_owned()).collect(),
        dlc_worlds: dlc_worlds.iter().map(|s| (*s).to_owned()).collect(),
    }
}

/// The vanilla figures, measured 2026-11-05 (`LOD_CENSUS_PRINT=1` regenerates this table).
fn expected(title: &str) -> Measured {
    match title {
        "Oblivion" => pinned(
            &[("far-nif", 130), ("legacy-terrain", 100), ("oblivion-lod", 9889)],
            &[("far-nif", 27), ("legacy-terrain", 4), ("oblivion-lod", 921)],
            &[
                "oblivion-lod:anvilcastlecourtyardworld=1",
                "oblivion-lod:anvilworld=12",
                "oblivion-lod:bravilworld=10",
                "oblivion-lod:brumaworld=7",
                "oblivion-lod:camoranparadise=79",
                "oblivion-lod:cheydinhalworld=13",
                "oblivion-lod:chorrolworld=16",
                "oblivion-lod:daboethiarealm=1",
                "oblivion-lod:e3kvatch=3",
                "oblivion-lod:icarboretumdistrict=4",
                "oblivion-lod:icarenadistrict=6",
                "oblivion-lod:icelvengardensdistrict=1",
                "oblivion-lod:icimperialpalace=6",
                "oblivion-lod:icimperialpalacemq16=6",
                "oblivion-lod:icimperialprisondistrict=1",
                "oblivion-lod:icmarketdistrict=2",
                "oblivion-lod:ictalosplazadistrict=2",
                "oblivion-lod:ictempledistrict=3",
                "oblivion-lod:ictempledistrictmq16=3",
                "oblivion-lod:icthearcaneuniversity=3",
                "oblivion-lod:kvatcheast=6",
                "oblivion-lod:kvatchentrance=4",
                "oblivion-lod:kvatchplaza=9",
                "oblivion-lod:leyawiinworld=13",
                "oblivion-lod:mq14oblivionworld=6",
                "oblivion-lod:ms13cheydinhaloblivionworld=4",
                "oblivion-lod:ms14world=14",
                "oblivion-lod:ms37world=18",
                "oblivion-lod:oblivionmqkvatch=5",
                "oblivion-lod:oblivionrd001=4",
                "oblivion-lod:oblivionrd003=4",
                "oblivion-lod:oblivionrd004=7",
                "oblivion-lod:oblivionrd006=3",
                "oblivion-lod:oblivionrd007=3",
                "oblivion-lod:palepassworld=56",
                "oblivion-lod:skingradworld=8",
                "oblivion-lod:tamriel=9347",
                "oblivion-lod:testgragtown=5",
                "oblivion-lod:testmatt=25",
                "oblivion-lod:testworld=21",
                "oblivion-lod:toddland=148",
            ],
            &[
                "oblivion-lod:seworld=920",
                "oblivion-lod:tamriel=1",
            ],
        ),
        "Fallout 3" => pinned(
            &[("dlodsettings", 15), ("dtl", 75), ("far-nif", 2), ("legacy-blocks", 452), ("legacy-terrain", 5571), ("treetypes-lst", 4)],
            &[("dlodsettings", 13), ("dtl", 33), ("legacy-blocks", 419), ("legacy-terrain", 1500), ("treetypes-lst", 7)],
            &[
                "dlodsettings:dcworld01=1",
                "dlodsettings:dcworld03=1",
                "dlodsettings:dcworld05=1",
                "dlodsettings:dcworld06=1",
                "dlodsettings:dcworld08=1",
                "dlodsettings:dcworld09=1",
                "dlodsettings:dcworld10=1",
                "dlodsettings:dcworld11=1",
                "dlodsettings:dcworld12=1",
                "dlodsettings:dcworld15=1",
                "dlodsettings:dcworld17=1",
                "dlodsettings:dcworld18=1",
                "dlodsettings:paradisefalls=1",
                "dlodsettings:washmontop=1",
                "dlodsettings:wasteland=1",
                "dtl:dcworld09=8",
                "dtl:dcworld18=2",
                "dtl:washmontop=5",
                "dtl:wasteland=60",
                "treetypes-lst:dcworld09=1",
                "treetypes-lst:dcworld18=1",
                "treetypes-lst:washmontop=1",
                "treetypes-lst:wasteland=1",
            ],
            &[
                "dlodsettings:dlc01haven=1",
                "dlodsettings:dlc01marketsquare=1",
                "dlodsettings:dlc01pittworld=1",
                "dlodsettings:dlc01steelmillexterior=1",
                "dlodsettings:dlc02anchoragebattle=1",
                "dlodsettings:dlc02baileyscrossroads=1",
                "dlodsettings:dlc02chinesehq=1",
                "dlodsettings:dlc02glacier=1",
                "dlodsettings:dlc02overlook=1",
                "dlodsettings:dlc03adamsafb=1",
                "dlodsettings:dlc03relaystation=1",
                "dlodsettings:dlc4bog=1",
                "dlodsettings:dlc4pointlookout=1",
                "dtl:dlc01pittworld=4",
                "dtl:dlc02glacier=6",
                "dtl:dlc02overlook=4",
                "dtl:dlc03adamsafb=8",
                "dtl:dlc03relaystation=1",
                "dtl:dlc4bog=2",
                "dtl:dlc4pointlookout=8",
                "treetypes-lst:dlc01pittworld=1",
                "treetypes-lst:dlc02glacier=1",
                "treetypes-lst:dlc02overlook=1",
                "treetypes-lst:dlc03adamsafb=1",
                "treetypes-lst:dlc03relaystation=1",
                "treetypes-lst:dlc4bog=1",
                "treetypes-lst:dlc4pointlookout=1",
            ],
        ),
        "Fallout New Vegas" => pinned(
            &[("dlodsettings", 9), ("legacy-blocks", 371), ("legacy-terrain", 7278)],
            &[("dlodsettings", 16), ("legacy-blocks", 232), ("legacy-terrain", 3777)],
            &[
                "dlodsettings:bouldercityworld=1",
                "dlodsettings:freesidefortworld=1",
                "dlodsettings:freesidenorthworld=1",
                "dlodsettings:freesideworld=1",
                "dlodsettings:gamorrahworld=1",
                "dlodsettings:lucky38world=1",
                "dlodsettings:thestripworldnew=1",
                "dlodsettings:wastelandnv=1",
                "dlodsettings:wastelandnvmini=1",
            ],
            &[
                "dlodsettings:nvdlc01easttownn=1",
                "dlodsettings:nvdlc01easttowns=1",
                "dlodsettings:nvdlc01villa=1",
                "dlodsettings:nvdlc01villachristine=1",
                "dlodsettings:nvdlc01villadean=1",
                "dlodsettings:nvdlc01westtownn=1",
                "dlodsettings:nvdlc01westtowns=1",
                "dlodsettings:nvdlc02zioncanyon=1",
                "dlodsettings:nvdlc03bigmt=1",
                "dlodsettings:nvdlc04dividevistaworld=1",
                "dlodsettings:nvdlc04divideworld=1",
                "dlodsettings:nvdlc04nukelegion=1",
                "dlodsettings:nvdlc04nukencr=1",
                "dlodsettings:nvdlc04nukesilo2=1",
                "dlodsettings:nvdlc04road01world=1",
                "dlodsettings:nvdlc04road02world=1",
            ],
        ),
        "Skyrim SE" => pinned(
            &[("bto", 1078), ("btr", 9584), ("btt", 386), ("lodsettings-lod", 12), ("tree-lst", 9), ("treelod-atlas", 7)],
            &[],
            &[
                "btt:deepwoodredoubtworld=4",
                "btt:dlc01falmervalley=5",
                "btt:dlc1hunterhqworld=4",
                "btt:dlc2solstheimworld=24",
                "btt:skuldafnworld=8",
                "btt:sovngarde=12",
                "btt:tamriel=329",
                "lodsettings-lod:blackreach=1",
                "lodsettings-lod:deepwoodredoubtworld=1",
                "lodsettings-lod:dlc01falmervalley=1",
                "lodsettings-lod:dlc01soulcairn=1",
                "lodsettings-lod:dlc1hunterhqworld=1",
                "lodsettings-lod:dlc2apocryphaworld=1",
                "lodsettings-lod:dlc2solstheimworld=1",
                "lodsettings-lod:japhetsfollyworld=1",
                "lodsettings-lod:markarthworld=1",
                "lodsettings-lod:skuldafnworld=1",
                "lodsettings-lod:sovngarde=1",
                "lodsettings-lod:tamriel=1",
                "tree-lst:deepwoodredoubtworld=1",
                "tree-lst:dlc01falmervalley=1",
                "tree-lst:dlc01soulcairn=1",
                "tree-lst:dlc1hunterhqworld=1",
                "tree-lst:dlc2apocryphaworld=1",
                "tree-lst:dlc2solstheimworld=1",
                "tree-lst:skuldafnworld=1",
                "tree-lst:sovngarde=1",
                "tree-lst:tamriel=1",
            ],
            &[
            ],
        ),
        "Fallout 4" => pinned(
            &[("bto", 497), ("btr", 6205), ("lodsettings-lod", 3), ("tree-lst", 2)],
            &[("bto", 156), ("btr", 340), ("lodsettings-lod", 1)],
            &[
                "lodsettings-lod:commonwealth=1",
                "lodsettings-lod:diamondcity=1",
                "lodsettings-lod:sanctuaryhillsworld=1",
                "tree-lst:diamondcity=1",
                "tree-lst:sanctuaryhillsworld=1",
            ],
            &[
                "lodsettings-lod:nukaworld=1",
            ],
        ),
        other => panic!("no pinned figures for {other}"),
    }
}
