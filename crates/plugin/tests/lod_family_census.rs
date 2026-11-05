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

// ── byte censuses over the Fallout descriptor and tree files (V3, V8) ───────────────────────────────────

/// Every file of `archives` the classifier puts in one of `families`: `(classified hit, bytes)`.
fn extract_family(
    dir: &Path,
    archives: &[&str],
    families: &[LodFamily],
) -> Option<Vec<(byroredux_bsa::lod_family::LodHit, String, Vec<u8>)>> {
    let mut out = Vec::new();
    for name in archives {
        let path = dir.join(name);
        if !path.is_file() {
            test_paths::require_or_skip(&format!("archive {path:?}"));
            return None;
        }
        let archive = BsaArchive::open(path.to_str().unwrap()).unwrap();
        for file in archive.list_files() {
            for hit in classify(file) {
                if families.contains(&hit.family) {
                    out.push((hit, file.to_string(), archive.extract(file).unwrap()));
                }
            }
        }
    }
    Some(out)
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn i16_at(b: &[u8], o: usize) -> i16 {
    i16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}

/// V3 / DLOD-TREE-004 — what the bytes of FO3's distant-tree files say.
///
/// * `treetypes.lst`: a `u32` entry count, then 32-byte entries (a `u32` index counting 0, 1, 2, … and seven
///   `f32`). 10 of the 11 shipped files are exactly `4 + 32 × count` bytes; `wasteland`'s header says 8 and the
///   file holds 9 entries, one more than it declares.
/// * `.dtl`: a `u32` chunk count, then per chunk a `u32` tree type, a `u32` record count (at most 114) and that
///   many 32-byte records `{x, y, z, scale: f32, f32, base FormID: u32, chunk index: u32, 0xFFFFFF00}`. 70 of the
///   108 files are exactly that; the other 38 (all `wasteland` and one DLC file) carry bytes after their declared
///   chunks that the header does not account for, the same way `wasteland`'s `.lst` carries an undeclared entry.
///   Whatever those trailing bytes are is NOT established here (filed apart, #6678).
#[test]
#[ignore = "needs the vanilla Fallout 3 archives on disk"]
fn fallout3_tree_file_layouts() {
    let dir = test_paths::fo3_data_dir();
    let title = &TITLES[1];
    let archives: Vec<&str> = title.base.iter().chain(title.dlc).copied().collect();
    let Some(files) = extract_family(&dir, &archives, &[LodFamily::FalloutTreeDtl, LodFamily::FalloutTreeTypes]) else {
        return;
    };

    let (mut lst_total, mut lst_exact, mut lst_indexed) = (0, 0, 0);
    let mut lst_off = Vec::new();
    let (mut dtl_total, mut dtl_exact) = (0, 0);
    let mut dtl_off = 0;
    for (hit, file, bytes) in &files {
        match hit.family {
            LodFamily::FalloutTreeTypes => {
                lst_total += 1;
                let n = u32_at(bytes, 0) as usize;
                if bytes.len() == 4 + 32 * n {
                    lst_exact += 1;
                } else {
                    lst_off.push(format!("{}: header {n}, {} entries", hit.world.as_deref().unwrap(), (bytes.len() - 4) / 32));
                }
                // The index field counts 0, 1, 2, … in file order.
                let entries = (bytes.len() - 4) / 32;
                lst_indexed += usize::from((0..entries).all(|i| u32_at(bytes, 4 + 32 * i) == i as u32));
            }
            _ => {
                dtl_total += 1;
                let (mut o, n) = (4usize, u32_at(bytes, 0));
                for _ in 0..n {
                    let (_ty, count) = (u32_at(bytes, o), u32_at(bytes, o + 4) as usize);
                    assert!(count <= 114, "{file}: a chunk of {count} records");
                    o += 8 + 32 * count;
                }
                if o == bytes.len() {
                    dtl_exact += 1;
                } else {
                    assert!(o < bytes.len(), "{file}: the declared chunks overrun the file");
                    dtl_off += 1;
                }
            }
        }
    }
    eprintln!("treetypes.lst: {lst_exact}/{lst_total} exact, {lst_indexed} indexed 0..n, off: {lst_off:?}; .dtl: {dtl_exact}/{dtl_total} exact, {dtl_off} with trailing bytes");
    assert_eq!((lst_total, lst_exact, lst_indexed), (11, 10, 11));
    assert_eq!(lst_off, ["wasteland: header 8, 9 entries"]);
    assert_eq!((dtl_total, dtl_exact, dtl_off), (108, 70, 38));
}

/// V8 / DLOD-BAND-003, -004 — the 24-byte `.dlodsettings` descriptor against the quads each worldspace authors.
///
/// Layout: `u32 4`, `u32 top level`, `u32 4 × top level`, `i16` SW cell x, y, `i16` NE cell x, y, `u32` 4 or 8. Over
/// all 53 vanilla descriptors (FO3 28, FNV 25, base game and DLC):
///
/// | relation | holds for |
/// |---|---|
/// | first `u32` is 4 | 53 |
/// | third `u32` is 4 × the second | 51 |
/// | second `u32` is the highest authored level | 46 (49 at or above it) |
/// | the SW cell is an authored quad's own corner | 48 |
/// | every authored quad sits on the lattice anchored at the SW cell (`(x − sw) % level == 0`) | 46 |
/// | last `u32` is the lowest authored level | 50 |
/// | the NE cell is the far corner of the authored quads | 4 (it is the worldspace's cell extent, not the LOD's) |
///
/// So the SW cell is NOT a universal lattice origin: 7 descriptors (FO3 `dcworld01`, `dcworld06`, `dcworld12`,
/// `dcworld17`, `dlc02baileyscrossroads`, `paradisefalls`, `washmontop`) have quads off it, and every FNV descriptor
/// is on it.
#[test]
#[ignore = "needs the vanilla Fallout 3 and New Vegas archives on disk"]
fn dlodsettings_descriptors_against_the_authored_quads() {
    use std::collections::BTreeSet;
    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    let mut off_lattice: BTreeSet<String> = BTreeSet::new();
    for title in [&TITLES[1], &TITLES[2]] {
        let dir = (title.data_dir)();
        if !dir.is_dir() {
            test_paths::require_or_skip(&format!("{} install", title.name));
            return;
        }
        let archives: Vec<&str> = title.base.iter().chain(title.dlc).copied().collect();
        let Some(files) = extract_family(
            &dir,
            &archives,
            &[LodFamily::DlodSettings, LodFamily::LegacyTerrain, LodFamily::LegacyBlocks],
        ) else {
            return;
        };
        // world -> every authored quad (x, y, level)
        let mut quads: BTreeMap<String, BTreeSet<(i32, i32, i32)>> = BTreeMap::new();
        for (hit, _, _) in &files {
            if let (Some(world), Some(level), Some((x, y))) = (&hit.world, &hit.level, hit.cell) {
                if let Some(level) = level.strip_prefix("level").and_then(|l| l.parse().ok()) {
                    quads.entry(world.clone()).or_default().insert((x, y, level));
                }
            }
        }
        for (hit, file, bytes) in files.iter().filter(|(h, _, _)| h.family == LodFamily::DlodSettings) {
            assert_eq!(bytes.len(), 24, "{file}");
            let world = hit.world.as_deref().unwrap();
            let q = &quads[world];
            let (a, top, extent) = (u32_at(bytes, 0), u32_at(bytes, 4) as i32, u32_at(bytes, 8));
            let (sw, _ne) = ((i16_at(bytes, 12) as i32, i16_at(bytes, 14) as i32), (i16_at(bytes, 16), i16_at(bytes, 18)));
            let last = u32_at(bytes, 20) as i32;
            let (highest, lowest) = (q.iter().map(|q| q.2).max().unwrap(), q.iter().map(|q| q.2).min().unwrap());
            *tally.entry("descriptors").or_default() += 1;
            *tally.entry("first is 4").or_default() += usize::from(a == 4);
            *tally.entry("third is 4x second").or_default() += usize::from(extent == 4 * top as u32);
            *tally.entry("second is the highest level").or_default() += usize::from(top == highest);
            *tally.entry("second is at or above it").or_default() += usize::from(top >= highest);
            *tally.entry("last is the lowest level").or_default() += usize::from(last == lowest);
            *tally.entry("SW is a quad corner").or_default() += usize::from(q.iter().any(|q| (q.0, q.1) == sw));
            let on_lattice = q.iter().all(|&(x, y, l)| (x - sw.0).rem_euclid(l) == 0 && (y - sw.1).rem_euclid(l) == 0);
            *tally.entry("all quads on the SW lattice").or_default() += usize::from(on_lattice);
            if !on_lattice {
                off_lattice.insert(world.to_owned());
            }
        }
    }
    eprintln!("{tally:?}\noff the SW lattice: {off_lattice:?}");
    let expect = |k: &str, n: usize| assert_eq!(tally[k], n, "{k}");
    expect("descriptors", 53);
    expect("first is 4", 53);
    expect("third is 4x second", 51);
    expect("second is the highest level", 46);
    expect("second is at or above it", 49);
    expect("last is the lowest level", 50);
    expect("SW is a quad corner", 48);
    expect("all quads on the SW lattice", 46);
    let off: Vec<&str> = off_lattice.iter().map(String::as_str).collect();
    assert_eq!(
        off,
        ["dcworld01", "dcworld06", "dcworld12", "dcworld17", "dlc02baileyscrossroads", "paradisefalls", "washmontop"]
    );
}

/// V9 / DLOD-BAND-005 — every `WRLD` `NAMA` ("Distant LOD Multiplier") the vanilla masters author.
#[test]
#[ignore = "needs the vanilla Skyrim SE and Fallout 4 masters on disk"]
fn worldspace_distant_lod_multiplier_census() {
    use byroredux_plugin::esm::reader::EsmReader;
    let mut report = Vec::new();
    for (title, dir, masters) in [
        ("Skyrim SE", test_paths::skyrim_se_data_dir(), &["Skyrim.esm", "Update.esm", "Dawnguard.esm", "HearthFires.esm", "Dragonborn.esm"][..]),
        (
            "Fallout 4",
            test_paths::fo4_data_dir(),
            &["Fallout4.esm", "DLCRobot.esm", "DLCCoast.esm", "DLCNukaWorld.esm", "DLCworkshop01.esm", "DLCworkshop02.esm", "DLCworkshop03.esm"][..],
        ),
    ] {
        for master in masters {
            let path = dir.join(master);
            let Ok(bytes) = std::fs::read(&path) else {
                test_paths::require_or_skip(&format!("{title} master {path:?}"));
                continue;
            };
            let mut reader = EsmReader::new(&bytes);
            reader.read_file_header().unwrap();
            let (mut worlds, mut authored): (usize, Vec<(String, f32)>) = (0, Vec::new());
            reader
                .for_each_record(bytes.len(), Some(b"WRLD"), |_, subs| {
                    worlds += 1;
                    if let Some(nama) = subs.iter().find(|s| s.sub_type == *b"NAMA") {
                        let edid = subs
                            .iter()
                            .find(|s| s.sub_type == *b"EDID")
                            .map(|s| String::from_utf8_lossy(&s.data).trim_end_matches('\0').to_owned())
                            .unwrap_or_default();
                        authored.push((edid, f32::from_le_bytes(nama.data[..4].try_into().unwrap())));
                    }
                })
                .unwrap();
            report.push((master.to_string(), worlds, authored));
        }
    }
    // (master, WRLD records, NAMA records, those that are not 1.0)
    let summary: Vec<(String, usize, usize, Vec<(String, f32)>)> = report
        .iter()
        .map(|(m, w, a)| (m.clone(), *w, a.len(), a.iter().filter(|(_, v)| *v != 1.0).cloned().collect()))
        .collect();
    for row in &summary {
        eprintln!("{row:?}");
    }
    // V9: every vanilla worldspace authors the field and every one authors 1.0, except Skyrim's three closed cities
    // (WhiterunWorld, WindhelmWorld, MarkarthWorld) and AlftandWorld at 0.25.
    // Nothing published says what the multiplier scales (the CK / FO4 CK wikis list the field with no description;
    // the UESP record page calls it "unknown float"), so it stays an unconsumed, documented field.
    for (master, worlds, authored, odd) in &summary {
        assert_eq!(worlds, authored, "{master}: NAMA on every WRLD");
        if master == "Skyrim.esm" {
            assert_eq!((*worlds, odd.len()), (37, 4), "{odd:?}");
        }
        let skyrim_family = ["Skyrim.esm", "Update.esm", "Dawnguard.esm", "HearthFires.esm", "Dragonborn.esm"].contains(&master.as_str());
        if skyrim_family {
            let city = ["WhiterunWorld", "WindhelmWorld", "MarkarthWorld", "AlftandWorld"];
            assert!(odd.iter().all(|(w, v)| *v == 0.25 && city.contains(&w.as_str())), "{master}: {odd:?}");
        } else {
            assert!(odd.is_empty(), "{master}: {odd:?}");
        }
    }
}

/// V11 (PRD §7.1 Skyrim) — does Skyrim LE ship the same LOD family names as SE? `BYROREDUX_SKYRIM_LE_DATA`
/// names the LE `Data` folder (default: the Wine prefix the dev machine keeps it in).
#[test]
#[ignore = "needs the Skyrim LE and SE archives on disk"]
fn skyrim_le_ships_the_same_lod_family_names_as_se() {
    use std::collections::BTreeSet;
    let le_dir = std::env::var("BYROREDUX_SKYRIM_LE_DATA").ok().filter(|s| !s.is_empty()).map_or_else(
        || {
            PathBuf::from(std::env::var("HOME").unwrap_or_default())
                .join("Games/skyrim-original/drive_c/Program Files (x86)/The Elder Scrolls V Skyrim/Data")
        },
        PathBuf::from,
    );
    let se_dir = test_paths::skyrim_se_data_dir();
    if !le_dir.is_dir() || !se_dir.is_dir() {
        test_paths::require_or_skip("Skyrim LE and SE installs");
        return;
    }
    let names = |dir: &Path, archives: &[&str]| -> BTreeSet<(String, Option<String>, Option<String>, Option<(i32, i32)>)> {
        archives
            .iter()
            .flat_map(|a| list_files(&dir.join(a)))
            .flat_map(|f| classify(&f))
            .map(|h| (h.family.label().to_owned(), h.world, h.level, h.cell))
            .collect()
    };
    let le = names(&le_dir, &["Skyrim - Meshes.bsa", "Skyrim - Misc.bsa", "Skyrim - Textures.bsa"]);
    let se = names(&se_dir, &TITLES[3].base.iter().copied().collect::<Vec<_>>());
    let only_le: Vec<_> = le.difference(&se).collect();
    let only_se: Vec<_> = se.difference(&le).collect();
    eprintln!("LE {} LOD names, SE {}; only LE: {}, only SE: {}", le.len(), se.len(), only_le.len(), only_se.len());
    for x in only_le.iter().take(5) {
        eprintln!("  only LE: {x:?}");
    }
    // V11: LE's base archives hold 5,618 LOD names; all but one exist under the same name in SE (SE also folds the
    // DLC worldspaces' LOD into its main archives, 5,459 more names, which LE ships in its DLC archives, not read here).
    assert_eq!((le.len(), only_le.len()), (5_618, 1));
    assert_eq!(only_le[0].1.as_deref(), Some("labyrinthianmazeworld"));
    let families = |s: &BTreeSet<(String, Option<String>, Option<String>, Option<(i32, i32)>)>| s.iter().map(|x| x.0.clone()).collect::<BTreeSet<_>>();
    assert_eq!(families(&le), families(&se), "the same families in both editions");
}
