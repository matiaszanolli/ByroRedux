//! TEMP scratch: NIFAL D6 — bhkCMSChunk transform_index census.
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::collision::*;
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

fn main() {
    let mut idx_hist: BTreeMap<u16, usize> = BTreeMap::new();
    let mut ntransforms_hist: BTreeMap<usize, usize> = BTreeMap::new();
    let mut chunks_total = 0usize;
    let mut chunks_nonzero_idx = 0usize;
    let mut chunks_nonidentity = 0usize;
    let mut chunks_oob = 0usize;
    let mut data_blocks = 0usize;
    let mut data_with_nonidentity_used = 0usize;
    let mut nonident_transforms = 0usize;
    let mut transforms_total = 0usize;
    let mut ex: Vec<String> = Vec::new();
    let mut nfiles = 0usize;

    for path in std::env::args().skip(1) {
        let Ok(arc) = BsaArchive::open(&path) else {
            eprintln!("skip {path}");
            continue;
        };
        let files: Vec<String> = arc
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
            .collect();
        eprintln!("{path}: {} nifs", files.len());
        for name in &files {
            let Ok(bytes) = arc.extract(name) else {
                continue;
            };
            let Ok(scene) = parse_nif(&bytes) else {
                continue;
            };
            nfiles += 1;
            for i in 0..scene.blocks.len() {
                let Some(d) = scene.get_as::<BhkCompressedMeshShapeData>(i) else {
                    continue;
                };
                data_blocks += 1;
                *ntransforms_hist
                    .entry(d.chunk_transforms.len())
                    .or_default() += 1;
                transforms_total += d.chunk_transforms.len();
                for t in &d.chunk_transforms {
                    let ti = t.translation;
                    let r = t.rotation;
                    let ident = ti[0] == 0.0
                        && ti[1] == 0.0
                        && ti[2] == 0.0
                        && r[0] == 0.0
                        && r[1] == 0.0
                        && r[2] == 0.0
                        && (r[3] - 1.0).abs() < 1e-6;
                    if !ident {
                        nonident_transforms += 1;
                    }
                }
                let mut used_nonident = false;
                for c in &d.chunks {
                    chunks_total += 1;
                    *idx_hist.entry(c.transform_index).or_default() += 1;
                    if c.transform_index != 0 {
                        chunks_nonzero_idx += 1;
                    }
                    match d.chunk_transforms.get(c.transform_index as usize) {
                        None => chunks_oob += 1,
                        Some(t) => {
                            let ti = t.translation;
                            let r = t.rotation;
                            let ident = ti[0] == 0.0
                                && ti[1] == 0.0
                                && ti[2] == 0.0
                                && r[0] == 0.0
                                && r[1] == 0.0
                                && r[2] == 0.0
                                && (r[3] - 1.0).abs() < 1e-6;
                            if !ident {
                                chunks_nonidentity += 1;
                                used_nonident = true;
                                if ex.len() < 10 {
                                    ex.push(format!(
                                        "{name} idx={} T={:?} R={:?}",
                                        c.transform_index,
                                        &ti[..3],
                                        r
                                    ));
                                }
                            }
                        }
                    }
                }
                if used_nonident {
                    data_with_nonidentity_used += 1;
                }
            }
        }
    }
    println!("nifs parsed: {nfiles}");
    println!("bhkCompressedMeshShapeData blocks: {data_blocks}");
    println!("chunks total: {chunks_total}");
    println!("  transform_index != 0: {chunks_nonzero_idx}");
    println!("  resolving to a NON-IDENTITY transform: {chunks_nonidentity}");
    println!("  transform_index out of range: {chunks_oob}");
    println!("data blocks using >=1 non-identity chunk transform: {data_with_nonidentity_used}");
    println!("chunk_transforms entries: {transforms_total}, non-identity: {nonident_transforms}");
    println!("num_transforms histogram: {ntransforms_hist:?}");
    println!(
        "transform_index histogram (top): {:?}",
        idx_hist.iter().take(12).collect::<Vec<_>>()
    );
    for e in &ex {
        println!("  ex {e}");
    }
}
