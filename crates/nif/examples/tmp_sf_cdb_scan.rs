//! TEMPORARY audit scratch — Starfield CDB / material-sidecar census (delete after use).
//!
//! Walks every BA2 in the Starfield Data dir and reports:
//!   * every entry whose name ends in `.cdb` (with size), so the CDB
//!     discovery surface can be compared against what the launch profile
//!     actually opens;
//!   * counts of `.mat` / `.bgsm` / `.bgem` sidecar FILES per archive;
//!   * whether each archive named by the shipped `starfield` launch profile
//!     exists on disk.

use byroredux_bsa::Ba2Archive;
use std::path::PathBuf;

const PROFILE_MESHES: &[&str] = &[
    "Starfield - Meshes01.ba2",
    "Starfield - Meshes02.ba2",
    "Starfield - LODMeshes.ba2",
    "Starfield - FaceMeshes.ba2",
    "Starfield - LODMeshesPatch.ba2",
    "Starfield - MeshesPatch.ba2",
];
const PROFILE_TEX: &[&str] = &[
    "Starfield - GeneratedTextures.ba2",
    "Starfield - Textures01.ba2",
    "Starfield - Textures02.ba2",
    "Starfield - Textures03.ba2",
    "Starfield - Textures04.ba2",
    "Starfield - Textures05.ba2",
    "Starfield - Textures06.ba2",
    "Starfield - Textures07.ba2",
    "Starfield - Textures08.ba2",
    "Starfield - Textures09.ba2",
    "Starfield - Textures10.ba2",
    "Starfield - Textures11.ba2",
    "Starfield - TexturesPatch01.ba2",
    "Starfield - TexturesPatch02.ba2",
];
const PROFILE_MAT: &[&str] = &["Starfield - Materials.ba2"];

fn main() {
    let base = std::env::var("BYROREDUX_STARFIELD_DATA").unwrap_or_else(|_| {
        "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data".to_string()
    });

    println!("=== launch-profile archive existence ===");
    for (label, list) in [
        ("bsas", PROFILE_MESHES),
        ("textures", PROFILE_TEX),
        ("materials", PROFILE_MAT),
    ] {
        for n in list {
            let p = PathBuf::from(&base).join(n);
            println!(
                "PROFILE {label} {} {}",
                if p.exists() { "PRESENT" } else { "MISSING" },
                n
            );
        }
    }

    println!("=== CDB + sidecar census over every BA2 ===");
    let mut archives: Vec<PathBuf> = std::fs::read_dir(&base)
        .expect("data dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .map(|x| x.eq_ignore_ascii_case("ba2"))
                .unwrap_or(false)
        })
        .collect();
    archives.sort();

    let mut total_cdb = 0usize;
    let mut total_mat = 0usize;
    let mut total_bgsm = 0usize;
    let mut total_bgem = 0usize;
    for path in &archives {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let Ok(archive) = Ba2Archive::open(path) else {
            continue;
        };
        let mut cdbs: Vec<String> = Vec::new();
        let (mut m, mut bs, mut be) = (0usize, 0usize, 0usize);
        for f in archive.list_files() {
            let l = f.to_lowercase();
            if l.ends_with(".cdb") {
                let sz = archive.extract(f).map(|d| d.len()).unwrap_or(0);
                cdbs.push(format!("{f} ({sz} B)"));
            } else if l.ends_with(".mat") {
                m += 1;
            } else if l.ends_with(".bgsm") {
                bs += 1;
            } else if l.ends_with(".bgem") {
                be += 1;
            }
        }
        if !cdbs.is_empty() || m + bs + be > 0 {
            println!("[{name}] mat={m} bgsm={bs} bgem={be} cdbs={}", cdbs.len());
            for c in &cdbs {
                println!("  CDB {c}");
            }
        }
        total_cdb += cdbs.len();
        total_mat += m;
        total_bgsm += bs;
        total_bgem += be;
    }
    println!("---");
    println!(
        "TOTAL archives={} cdbs={total_cdb} mat_files={total_mat} bgsm_files={total_bgsm} bgem_files={total_bgem}",
        archives.len()
    );
}
