//! TEMPORARY audit scratch — Starfield NiSkinPartition census (delete after use).
//!
//! Header-only walk across the 13 mesh-bearing Starfield archives. Counts
//! `NiSkinPartition` (the classic strip-authored skin path), `BSSkin::Instance`
//! (Starfield's actual skin path) and `BSGeometry`, plus a full block-type-name
//! histogram taken from the header string table (no block parsing).
//!
//! Env:
//!   SF_STRIDE   — sample every Nth NIF entry (default 1 = full walk)
//!   SF_CAP      — hard cap on NIFs examined per archive (default 0 = no cap)

use byroredux_bsa::Ba2Archive;
use byroredux_nif::header::NifHeader;
use std::collections::BTreeMap;
use std::path::PathBuf;

const ARCHIVES: &[&str] = &[
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
];

fn main() {
    let base = std::env::var("BYROREDUX_STARFIELD_DATA").unwrap_or_else(|_| {
        "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data".to_string()
    });
    let stride: usize = std::env::var("SF_STRIDE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
        .max(1);
    let cap: usize = std::env::var("SF_CAP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let mut grand_nifs = 0usize;
    let mut grand_entries = 0usize;
    let mut grand_sp_files = 0usize;
    let mut grand_sp_blocks = 0usize;
    let mut grand_bsskin = 0usize;
    let mut grand_bsgeom = 0usize;
    let mut grand_hdr_fail = 0usize;
    let mut hist: BTreeMap<String, usize> = BTreeMap::new();
    let mut sp_examples: Vec<String> = Vec::new();

    for name in ARCHIVES {
        let path = PathBuf::from(&base).join(name);
        let archive = match Ba2Archive::open(&path) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("[skip] {name}: {e}");
                continue;
            }
        };
        let files: Vec<String> = archive
            .list_files()
            .into_iter()
            .filter(|f| byroredux_nif::corpus::is_nif_entry(f))
            .map(|s| s.to_string())
            .collect();
        let total_entries = files.len();
        let mut nifs = 0usize;
        let mut sp_files = 0usize;
        let mut sp_blocks = 0usize;
        let mut bsskin = 0usize;
        let mut bsgeom = 0usize;
        let mut hdr_fail = 0usize;

        for (i, f) in files.iter().enumerate() {
            if i % stride != 0 {
                continue;
            }
            if cap > 0 && nifs + hdr_fail >= cap {
                break;
            }
            let Ok(data) = archive.extract(f) else { continue };
            let Ok((hdr, _)) = NifHeader::parse(&data) else {
                hdr_fail += 1;
                continue;
            };
            nifs += 1;
            let mut n_sp = 0usize;
            let mut has_bs = false;
            let mut has_geom = false;
            for idx in 0..hdr.num_blocks as usize {
                let Some(t) = hdr.block_type_name(idx) else {
                    continue;
                };
                *hist.entry(t.to_string()).or_insert(0) += 1;
                match t {
                    "NiSkinPartition" => n_sp += 1,
                    "BSSkin::Instance" => has_bs = true,
                    "BSGeometry" => has_geom = true,
                    _ => {}
                }
            }
            if n_sp > 0 {
                sp_files += 1;
                sp_blocks += n_sp;
                if sp_examples.len() < 20 {
                    sp_examples.push(format!("{name}|{f}|{n_sp}"));
                }
            }
            if has_bs {
                bsskin += 1;
            }
            if has_geom {
                bsgeom += 1;
            }
        }

        println!(
            "[{name}] entries={total_entries} sampled_ok={nifs} hdr_fail={hdr_fail} \
             skinpart_files={sp_files} skinpart_blocks={sp_blocks} \
             bsskin_instance_files={bsskin} bsgeometry_files={bsgeom}"
        );
        grand_entries += total_entries;
        grand_nifs += nifs;
        grand_sp_files += sp_files;
        grand_sp_blocks += sp_blocks;
        grand_bsskin += bsskin;
        grand_bsgeom += bsgeom;
        grand_hdr_fail += hdr_fail;
    }

    println!("---");
    println!(
        "TOTAL entries={grand_entries} sampled_ok={grand_nifs} hdr_fail={grand_hdr_fail} \
         skinpart_files={grand_sp_files} skinpart_blocks={grand_sp_blocks} \
         bsskin_instance_files={grand_bsskin} bsgeometry_files={grand_bsgeom} \
         (stride={stride} cap={cap})"
    );
    for e in &sp_examples {
        println!("SP_EXAMPLE {e}");
    }
    println!("--- block type histogram (header string table) ---");
    let mut v: Vec<(&String, &usize)> = hist.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    for (k, c) in v {
        println!("HIST {c}\t{k}");
    }
}
