//! THROWAWAY audit probe (2026-08-30, /audit-nif Dimension 3).
//! Wire-keyed per-block coverage sweep across every mesh-bearing archive
//! of one game. Mirrors `tests/common::PerBlockHistogram::record_scene_blocks`
//! (#3326 wire keying) which `nif_stats --tsv` does NOT do on the parsed side.
//!
//! Usage: _tmp_a0830_d3_wire <game>
//! Emits TSV on stdout: `type<TAB>parsed<TAB>unknown`, plus `#` summary lines.

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_nif::blocks::{NiObject, NiUnknown};
use byroredux_nif::corpus::is_nif_entry;
use byroredux_nif::header::NifHeader;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

enum Arch {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}
impl Arch {
    fn list(&self) -> Vec<String> {
        match self {
            Arch::Bsa(a) => a.list_files().into_iter().map(|s| s.to_string()).collect(),
            Arch::Ba2(a) => a.list_files().into_iter().map(|s| s.to_string()).collect(),
        }
    }
    fn extract(&self, p: &str) -> std::io::Result<Vec<u8>> {
        match self {
            Arch::Bsa(a) => a.extract(p),
            Arch::Ba2(a) => a.extract(p),
        }
    }
}

#[derive(Default, Clone, Copy)]
struct C {
    parsed: usize,
    unknown: usize,
}

fn game_spec(g: &str) -> (&'static str, Vec<&'static str>) {
    let base = "/mnt/data/SteamLibrary/steamapps/common";
    match g {
        "oblivion" => (
            concat!("/mnt/data/SteamLibrary/steamapps/common", "/Oblivion/Data"),
            vec![
                "Oblivion - Meshes.bsa",
                "Knights.bsa",
                "DLCShiveringIsles - Meshes.bsa",
                "DLCBattlehornCastle.bsa",
                "DLCFrostcrag.bsa",
                "DLCHorseArmor.bsa",
                "DLCMehrunesRazor.bsa",
                "DLCOrrery.bsa",
                "DLCSpellTomes.bsa",
                "DLCThievesDen.bsa",
                "DLCVileLair.bsa",
            ],
        ),
        "fo3" => (
            concat!(
                "/mnt/data/SteamLibrary/steamapps/common",
                "/Fallout 3 goty/Data"
            ),
            vec![
                "Fallout - Meshes.bsa",
                "Anchorage - Main.bsa",
                "BrokenSteel - Main.bsa",
                "PointLookout - Main.bsa",
                "ThePitt - Main.bsa",
                "Zeta - Main.bsa",
            ],
        ),
        "fnv" => (
            concat!(
                "/mnt/data/SteamLibrary/steamapps/common",
                "/Fallout New Vegas/Data"
            ),
            vec![
                "Fallout - Meshes.bsa",
                "Update.bsa",
                "DeadMoney - Main.bsa",
                "HonestHearts - Main.bsa",
                "OldWorldBlues - Main.bsa",
                "LonesomeRoad - Main.bsa",
                "GunRunnersArsenal - Main.bsa",
                "CaravanPack - Main.bsa",
                "ClassicPack - Main.bsa",
                "MercenaryPack - Main.bsa",
                "TribalPack - Main.bsa",
            ],
        ),
        "skyrim" => (
            concat!(
                "/mnt/data/SteamLibrary/steamapps/common",
                "/Skyrim Special Edition/Data"
            ),
            vec![
                "Skyrim - Meshes0.bsa",
                "Skyrim - Meshes1.bsa",
                "_ResourcePack.bsa",
                "ccBGSSSE001-Fish.bsa",
                "ccBGSSSE025-AdvDSGS.bsa",
                "ccBGSSSE037-Curios.bsa",
                "ccQDRSSE001-SurvivalMode.bsa",
            ],
        ),
        "fo4" => (
            concat!("/mnt/data/SteamLibrary/steamapps/common", "/Fallout 4/Data"),
            vec![
                "Fallout4 - Meshes.ba2",
                "Fallout4 - MeshesExtra.ba2",
                "DLCCoast - Main.ba2",
                "DLCNukaWorld - Main.ba2",
                "DLCRobot - Main.ba2",
                "DLCworkshop01 - Main.ba2",
                "DLCworkshop02 - Main.ba2",
                "DLCworkshop03 - Main.ba2",
            ],
        ),
        "fo76" => (
            concat!("/mnt/data/SteamLibrary/steamapps/common", "/Fallout76/Data"),
            vec![
                "SeventySix - Meshes.ba2",
                "SeventySix - StaticMeshes.ba2",
                "SeventySix - GeneratedMeshes01.ba2",
                "SeventySix - GeneratedMeshes02.ba2",
                "SeventySix - 00UpdateMain.ba2",
                "SeventySix - 01UpdateMain.ba2",
                "SeventySix - 02UpdateMain.ba2",
                "SeventySix - 03UpdateMain.ba2",
                "SeventySix - 04UpdateMain.ba2",
                "SeventySix - 05UpdateMain.ba2",
                "SeventySix - 06UpdateMain.ba2",
                "SeventySix - 07UpdateMain.ba2",
                "SeventySix - 08UpdateMain.ba2",
                "SeventySix - 09UpdateMain.ba2",
                "SeventySix - 10UpdateMain.ba2",
                "SeventySix - 11UpdateMain.ba2",
                "SeventySix - 12UpdateMain.ba2",
                "SeventySix - 13UpdateMain.ba2",
                "SeventySix - 14UpdateMain.ba2",
                "SeventySix - 15UpdateMain.ba2",
            ],
        ),
        "starfield" => (
            concat!("/mnt/data/SteamLibrary/steamapps/common", "/Starfield/Data"),
            vec![
                "Starfield - Meshes01.ba2",
                "Starfield - Meshes02.ba2",
                "Starfield - MeshesPatch.ba2",
                "Starfield - LODMeshes.ba2",
                "Starfield - LODMeshesPatch.ba2",
                "Starfield - FaceMeshes.ba2",
                "ShatteredSpace - Main01.ba2",
                "SFBGS003 - Main.ba2",
                "SFBGS004 - Main.ba2",
                "SFBGS008 - Main.ba2",
                "SFBGS00D - Main.ba2",
                "SFBGS047 - Main.ba2",
                "SFBGS050 - Main.ba2",
            ],
        ),
        _ => {
            let _ = base;
            panic!("unknown game {g}")
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let game = args.get(1).expect("usage: <game>").clone();
    let threads: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(8);
    let (data, names) = game_spec(&game);

    let hist: Mutex<BTreeMap<String, C>> = Mutex::new(BTreeMap::new());
    // Per-archive file-level tallies.
    let mut per_archive: Vec<(String, usize, usize, usize, usize)> = Vec::new();
    // Unknown types with an example file path.
    let unk_example: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

    for name in &names {
        let path = std::path::Path::new(data).join(name);
        if !path.is_file() {
            eprintln!("# MISSING {}", path.display());
            continue;
        }
        let arch = if name.to_ascii_lowercase().ends_with(".ba2") {
            match Ba2Archive::open(&path) {
                Ok(a) => Arch::Ba2(a),
                Err(e) => {
                    eprintln!("# OPENFAIL {} {}", name, e);
                    continue;
                }
            }
        } else {
            match BsaArchive::open(&path) {
                Ok(a) => Arch::Bsa(a),
                Err(e) => {
                    eprintln!("# OPENFAIL {} {}", name, e);
                    continue;
                }
            }
        };
        let files: Vec<String> = arch
            .list()
            .into_iter()
            .filter(|f| is_nif_entry(f))
            .collect();
        let total = files.len();
        if total == 0 {
            eprintln!("# NONIF {}", name);
            continue;
        }
        let idx = AtomicUsize::new(0);
        let clean = AtomicUsize::new(0);
        let trunc = AtomicUsize::new(0);
        let fail = AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| {
                    let mut local: BTreeMap<String, C> = BTreeMap::new();
                    let mut local_unk: BTreeMap<String, String> = BTreeMap::new();
                    loop {
                        let i = idx.fetch_add(1, Ordering::Relaxed);
                        if i >= total {
                            break;
                        }
                        let p = &files[i];
                        let bytes = match arch.extract(p) {
                            Ok(b) => b,
                            Err(_) => {
                                fail.fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                        };
                        match byroredux_nif::parse_nif(&bytes) {
                            Ok(scene) => {
                                if scene.truncated || scene.recovered_blocks > 0 {
                                    trunc.fetch_add(1, Ordering::Relaxed);
                                } else {
                                    clean.fetch_add(1, Ordering::Relaxed);
                                }
                                let header = NifHeader::parse(&bytes).ok().map(|(h, _)| h);
                                record(
                                    &mut local,
                                    &mut local_unk,
                                    header.as_ref(),
                                    &scene.blocks,
                                    p,
                                );
                            }
                            Err(_) => {
                                fail.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    let mut g = hist.lock().unwrap();
                    for (k, v) in local {
                        let e = g.entry(k).or_default();
                        e.parsed += v.parsed;
                        e.unknown += v.unknown;
                    }
                    drop(g);
                    let mut u = unk_example.lock().unwrap();
                    for (k, v) in local_unk {
                        u.entry(k).or_insert(v);
                    }
                });
            }
        });
        per_archive.push((
            name.to_string(),
            total,
            clean.load(Ordering::Relaxed),
            trunc.load(Ordering::Relaxed),
            fail.load(Ordering::Relaxed),
        ));
        eprintln!(
            "# ARCHIVE\t{}\ttotal={}\tclean={}\ttrunc={}\tfail={}",
            name,
            total,
            clean.load(Ordering::Relaxed),
            trunc.load(Ordering::Relaxed),
            fail.load(Ordering::Relaxed)
        );
    }

    let g = hist.lock().unwrap();
    let tot: usize = per_archive.iter().map(|a| a.1).sum();
    let cl: usize = per_archive.iter().map(|a| a.2).sum();
    let tr: usize = per_archive.iter().map(|a| a.3).sum();
    let fa: usize = per_archive.iter().map(|a| a.4).sum();
    println!(
        "# game={game}\ttotal={tot}\tclean={cl}\ttruncated={tr}\tfailed={fa}\tarchives={}",
        per_archive.len()
    );
    for (n, t, c, x, f) in &per_archive {
        println!("# archive\t{n}\t{t}\t{c}\t{x}\t{f}");
    }
    let ux = unk_example.lock().unwrap();
    for (k, v) in g.iter() {
        let ex = if v.unknown > 0 {
            ux.get(k).cloned().unwrap_or_default()
        } else {
            String::new()
        };
        println!("{}\t{}\t{}\t{}", k, v.parsed, v.unknown, ex);
    }
}

fn record(
    hist: &mut BTreeMap<String, C>,
    unk_example: &mut BTreeMap<String, String>,
    header: Option<&NifHeader>,
    blocks: &[Box<dyn NiObject>],
    path: &str,
) {
    for (index, block) in blocks.iter().enumerate() {
        if let Some(unknown) = block.as_any().downcast_ref::<NiUnknown>() {
            let n = unknown.type_name.as_ref().to_string();
            hist.entry(n.clone()).or_default().unknown += 1;
            unk_example.entry(n).or_insert_with(|| path.to_string());
            continue;
        }
        let wire = header
            .and_then(|h| {
                h.block_type_indices
                    .get(index)
                    .and_then(|&ti| h.block_types.get(ti as usize))
            })
            .map(|n| n.as_ref())
            .unwrap_or_else(|| block.block_type_name());
        hist.entry(wire.to_string()).or_default().parsed += 1;
    }
}
