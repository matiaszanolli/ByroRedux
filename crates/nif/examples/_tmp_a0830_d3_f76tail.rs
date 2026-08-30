//! THROWAWAY audit probe (2026-08-30, /audit-nif D3): attribute the FO76
//! GeneratedMeshes truncation tail — real block loss vs NiUnknown recovery.
use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::NiUnknown;
use byroredux_nif::corpus::is_nif_entry;
use std::collections::BTreeMap;

fn main() {
    let p = std::env::args().nth(1).unwrap();
    let a = Ba2Archive::open(&p).unwrap();
    let files: Vec<String> = a
        .list_files()
        .into_iter()
        .map(|s| s.to_string())
        .filter(|f| is_nif_entry(f))
        .collect();
    let mut real_trunc = 0usize; // scene.truncated == true (blocks dropped)
    let mut recov_only = 0usize; // recovered_blocks > 0 but not truncated
    let mut clean = 0usize;
    let mut dropped_total = 0usize;
    let mut recovered_total = 0usize;
    let mut unk: BTreeMap<String, usize> = BTreeMap::new();
    let mut with_bsdoed_and_trunc = 0usize;
    let mut trunc_without_bsdoed = 0usize;
    let mut examples: Vec<String> = Vec::new();
    for f in &files {
        let Ok(b) = a.extract(f) else { continue };
        let Ok(s) = byroredux_nif::parse_nif(&b) else {
            continue;
        };
        let mut has_doed = false;
        for blk in &s.blocks {
            if let Some(u) = blk.as_any().downcast_ref::<NiUnknown>() {
                let n = u.type_name.as_ref().to_string();
                if n == "BSDistantObjectExtraData" {
                    has_doed = true;
                }
                *unk.entry(n).or_default() += 1;
            }
        }
        if s.truncated {
            real_trunc += 1;
            dropped_total += s.dropped_block_count;
            if has_doed {
                with_bsdoed_and_trunc += 1;
            } else {
                trunc_without_bsdoed += 1;
                if examples.len() < 5 {
                    examples.push(f.clone());
                }
            }
        } else if s.recovered_blocks > 0 {
            recov_only += 1;
            recovered_total += s.recovered_blocks;
        } else {
            clean += 1;
        }
    }
    println!("archive={p}");
    println!(
        "  files={} clean={} recovered_only={} truly_truncated={}",
        files.len(),
        clean,
        recov_only,
        real_trunc
    );
    println!(
        "  dropped_blocks={} recovered_blocks={}",
        dropped_total, recovered_total
    );
    println!("  truncated WITH BSDistantObjectExtraData: {with_bsdoed_and_trunc}; WITHOUT: {trunc_without_bsdoed}");
    for e in &examples {
        println!("  ex-trunc-without: {e}");
    }
    for (k, v) in &unk {
        println!("  unknown {k} = {v}");
    }
}
