//! Bone-count survey — walks a BSA/BA2, parses every NIF, and reports
//! the per-mesh skin-instance bone count distribution. Built during
//! the 2026-05-17 #1135 fix to inform `MAX_BONES_PER_MESH` sizing
//! against real vanilla content rather than a guess.
//!
//! Inspects:
//!   - `NiSkinInstance.bone_refs.len()`           (Oblivion .. Skyrim)
//!   - `BsDismemberSkinInstance.bone_refs.len()`  (FO3 / FNV / Skyrim body parts)
//!   - `BsSkinInstance.bone_refs.len()`           (FO4+ BSTriShape)
//!
//! Usage:
//! ```bash
//! cargo run --release -p byroredux-nif --example r6_bone_count_survey \
//!     -- "/path/to/Fallout4 - Meshes.ba2"
//! ```
//!
//! Output: max, p50/p95/p99 + top-20 highest-bone-count NIFs. The max
//! is the value that constrains `MAX_BONES_PER_MESH` — any mesh above
//! the chosen cap renders in bind pose (importer skip-skinning path).

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_nif::blocks::skin::{BsDismemberSkinInstance, BsSkinInstance, NiSkinInstance};
use byroredux_nif::parse_nif;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: r6_bone_count_survey <bsa-or-ba2>");
        std::process::exit(2);
    }
    let archive_path = &args[1];
    let is_ba2 = Path::new(archive_path)
        .extension()
        .map(|e| e.eq_ignore_ascii_case("ba2"))
        .unwrap_or(false);

    let files: Vec<(String, Vec<u8>)> = if is_ba2 {
        let arch = Ba2Archive::open(archive_path).expect("open BA2");
        arch.list_files()
            .into_iter()
            .filter(|f| f.to_lowercase().ends_with(".nif"))
            .filter_map(|f| arch.extract(f).ok().map(|b| (f.to_string(), b)))
            .collect()
    } else {
        let arch = BsaArchive::open(archive_path).expect("open BSA");
        arch.list_files()
            .into_iter()
            .filter(|f| f.to_lowercase().ends_with(".nif"))
            .filter_map(|f| arch.extract(f).ok().map(|b| (f.to_string(), b)))
            .collect()
    };
    eprintln!("Scanning {} NIF entries...", files.len());

    let mut samples: Vec<(usize, String, &'static str)> = Vec::new();
    for (path, bytes) in &files {
        let Ok(scene) = parse_nif(bytes) else {
            continue;
        };
        for block in &scene.blocks {
            if let Some(s) = block.as_any().downcast_ref::<NiSkinInstance>() {
                samples.push((s.bone_refs.len(), path.clone(), "NiSkin"));
            } else if let Some(s) = block.as_any().downcast_ref::<BsDismemberSkinInstance>() {
                samples.push((s.base.bone_refs.len(), path.clone(), "BsDism"));
            } else if let Some(s) = block.as_any().downcast_ref::<BsSkinInstance>() {
                samples.push((s.bone_refs.len(), path.clone(), "BsSkin"));
            }
        }
    }
    if samples.is_empty() {
        println!("No skin instances found.");
        return;
    }
    samples.sort_by_key(|(n, _, _)| std::cmp::Reverse(*n));

    let total = samples.len();
    let max = samples[0].0;
    let p99 = samples[(total as f64 * 0.01) as usize].0;
    let p95 = samples[(total as f64 * 0.05) as usize].0;
    let p50 = samples[total / 2].0;

    println!("Skin instances surveyed: {}", total);
    println!("  max:   {}", max);
    println!("  p99:   {}", p99);
    println!("  p95:   {}", p95);
    println!("  p50:   {}", p50);
    println!("\nTop 20 by bone count:");
    for (n, path, kind) in samples.iter().take(20) {
        println!("  {:>4}  [{}]  {}", n, kind, path);
    }
}
