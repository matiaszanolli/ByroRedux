//! THROWAWAY audit probe (audit-nif 2026-08-30, Dimension 1).
//! Walks one archive, tallies consumed-vs-block_size drift per block type
//! plus no_block_sizes truncation attribution (which block type stopped
//! the walk). Output is TSV on stdout.
//!
//! usage: _tmp_a0830_d1_drift <archive> [max_files]

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_nif::corpus::is_nif_entry;
use byroredux_nif::{header::NifHeader, parse_nif};
use std::collections::BTreeMap;
use std::env;

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

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = &args[1];
    let cap: usize = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);

    let arch = if let Ok(a) = BsaArchive::open(path) {
        Arch::Bsa(a)
    } else {
        match Ba2Archive::open(path) {
            Ok(a) => Arch::Ba2(a),
            Err(e) => {
                eprintln!("open failed {path}: {e}");
                return;
            }
        }
    };

    let files: Vec<String> = arch
        .list()
        .into_iter()
        .filter(|f| is_nif_entry(f))
        .collect();

    // type -> drift -> count
    let mut drift: BTreeMap<String, BTreeMap<i64, u64>> = BTreeMap::new();
    let mut stub_drift: BTreeMap<String, BTreeMap<i64, u64>> = BTreeMap::new();
    // no-block-sizes truncation: failing block type -> count
    let mut trunc_nbs: BTreeMap<String, u64> = BTreeMap::new();
    let mut trunc_wbs: BTreeMap<String, u64> = BTreeMap::new();
    let mut example: BTreeMap<String, String> = BTreeMap::new();
    let mut n_files = 0u64;
    let mut n_nbs = 0u64;
    let mut n_err = 0u64;

    for f in files.iter().take(cap) {
        let Ok(bytes) = arch.extract(f) else {
            continue;
        };
        let hdr = NifHeader::parse(&bytes).ok();
        let has_sizes = hdr
            .as_ref()
            .map(|(h, _)| !h.block_sizes.is_empty() && h.num_blocks > 0)
            .unwrap_or(false);
        let scene = match parse_nif(&bytes) {
            Ok(s) => s,
            Err(_) => {
                n_err += 1;
                continue;
            }
        };
        n_files += 1;
        if !has_sizes {
            n_nbs += 1;
        }
        for (t, m) in &scene.drift_histogram {
            let e = drift.entry(t.clone()).or_default();
            for (d, c) in m {
                *e.entry(*d).or_insert(0) += *c as u64;
            }
            example.entry(t.clone()).or_insert_with(|| f.clone());
        }
        for (t, m) in &scene.stubbed_drift_histogram {
            let e = stub_drift.entry(t.clone()).or_default();
            for (d, c) in m {
                *e.entry(*d).or_insert(0) += *c as u64;
            }
        }
        if scene.truncated {
            // The failing block index is the count of blocks kept.
            let idx = scene.blocks.len();
            let name = hdr
                .as_ref()
                .and_then(|(h, _)| h.block_type_name(idx).map(|s| s.to_string()))
                .unwrap_or_else(|| "<no-name>".to_string());
            let key = format!("{name}");
            if has_sizes {
                *trunc_wbs.entry(key.clone()).or_insert(0) += 1;
            } else {
                *trunc_nbs.entry(key.clone()).or_insert(0) += 1;
            }
            example
                .entry(format!("TRUNC:{key}"))
                .or_insert_with(|| f.clone());
        }
    }

    println!(
        "#ARCHIVE\t{path}\tnifs={}\tparsed={n_files}\tnoblocksizes={n_nbs}\thard_err={n_err}",
        files.len().min(cap)
    );
    for (t, m) in &drift {
        for (d, c) in m {
            println!(
                "DRIFT\t{t}\t{d}\t{c}\t{}",
                example.get(t).cloned().unwrap_or_default()
            );
        }
    }
    for (t, m) in &stub_drift {
        for (d, c) in m {
            println!("STUBDRIFT\t{t}\t{d}\t{c}");
        }
    }
    for (t, c) in &trunc_nbs {
        println!(
            "TRUNC_NOSIZES\t{t}\t{c}\t{}",
            example
                .get(&format!("TRUNC:{t}"))
                .cloned()
                .unwrap_or_default()
        );
    }
    for (t, c) in &trunc_wbs {
        println!(
            "TRUNC_SIZED\t{t}\t{c}\t{}",
            example
                .get(&format!("TRUNC:{t}"))
                .cloned()
                .unwrap_or_default()
        );
    }
}
