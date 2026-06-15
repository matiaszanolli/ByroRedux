//! d5_coverage — Dim-5 coverage-gap probe (audit-scratch, transient).
//!
//! Enumerates block-type RTTI names from each NIF's HEADER table
//! (parse-failure-independent) and cross-references against the live
//! dispatch key set. Reports per-archive:
//!   - distinct block types seen / covered / uncovered
//!   - per-uncovered-type instance count + how many FILES contain it
//!   - sizeless-format files (no block_sizes -> cascade risk)
//!   - of the cascade-risk files, how many actually fail parse_nif AND
//!     reference an uncovered type (the dangerous combination).
use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_nif::header::NifHeader;
use byroredux_nif::parse_nif;
use byroredux_nif::version::NifVersion;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Default)]
struct Stats {
    files: usize,
    files_header_ok: usize,
    files_parse_ok: usize,
    files_parse_err: usize,
    sizeless_files: usize,                 // block_sizes empty (Oblivion-style)
    sizeless_files_uncovered: usize,       // sizeless AND header references an uncovered type
    sizeless_files_uncovered_err: usize, // sizeless AND uncovered AND parse_nif errored (cascade-confirmed)
    block_instances: u64,                // total block instances across files (from indices)
    type_instances: BTreeMap<String, u64>, // per-type total instance count
    type_files: BTreeMap<String, u64>,   // per-type distinct-file count
}

fn covered(name: &str) -> bool {
    // #1335 — probe the LIVE dispatcher instead of a hand-written snapshot
    // that silently over-reported uncovered types as new parser arms landed.
    byroredux_nif::blocks::is_block_type_dispatched(name)
}

fn process_bytes(st: &mut Stats, bytes: &[u8]) {
    st.files += 1;
    let header = match NifHeader::parse(bytes) {
        Ok((h, _)) => h,
        Err(_) => return,
    };
    st.files_header_ok += 1;
    // Per-file distinct types and whether any are uncovered.
    let mut file_types: BTreeSet<String> = BTreeSet::new();
    // instance counts come from block_type_indices (one entry per block)
    for &ti in &header.block_type_indices {
        if let Some(name) = header.block_types.get(ti as usize) {
            st.block_instances += 1;
            *st.type_instances.entry(name.to_string()).or_insert(0) += 1;
            file_types.insert(name.to_string());
        }
    }
    for t in &file_types {
        *st.type_files.entry(t.clone()).or_insert(0) += 1;
    }
    let has_uncovered = file_types.iter().any(|t| !covered(t));
    // Sizeless detection: Oblivion-style files have empty block_sizes.
    // block_sizes is gated >= V20_2_0_5 in the header parser.
    let sizeless = header.block_sizes.is_empty() && header.version < NifVersion::V20_2_0_5;
    if sizeless {
        st.sizeless_files += 1;
        if has_uncovered {
            st.sizeless_files_uncovered += 1;
        }
    }
    // parse_nif result
    let parse_ok = parse_nif(bytes).is_ok();
    if parse_ok {
        st.files_parse_ok += 1;
    } else {
        st.files_parse_err += 1;
        if sizeless && has_uncovered {
            st.sizeless_files_uncovered_err += 1;
        }
    }
}

fn process_bsa(st: &mut Stats, path: &Path) {
    let archive = match BsaArchive::open(path) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("open BSA {}: {e}", path.display());
            return;
        }
    };
    let nifs: Vec<String> = archive
        .list_files()
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();
    eprintln!("BSA {} -> {} NIFs", path.display(), nifs.len());
    for (i, p) in nifs.iter().enumerate() {
        if i > 0 && i % 5000 == 0 {
            eprintln!("  {}/{}", i, nifs.len());
        }
        if let Ok(bytes) = archive.extract(p) {
            process_bytes(st, &bytes);
        }
    }
}

fn process_ba2(st: &mut Stats, path: &Path) {
    let archive = match Ba2Archive::open(path) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("open BA2 {}: {e}", path.display());
            return;
        }
    };
    let nifs: Vec<String> = archive
        .list_files()
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();
    eprintln!("BA2 {} -> {} NIFs", path.display(), nifs.len());
    for (i, p) in nifs.iter().enumerate() {
        if i > 0 && i % 5000 == 0 {
            eprintln!("  {}/{}", i, nifs.len());
        }
        if let Ok(bytes) = archive.extract(p) {
            process_bytes(st, &bytes);
        }
    }
}

fn main() {
    // #1335 — coverage is now probed live via `byroredux_nif::blocks::
    // is_block_type_dispatched` (see `covered`), so there is no hand-written
    // key snapshot left to keep sorted.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: d5_coverage <archive>...");
        std::process::exit(1);
    }
    let mut st = Stats::default();
    for a in &args {
        let p = PathBuf::from(a);
        let lower = a.to_ascii_lowercase();
        if lower.ends_with(".ba2") {
            process_ba2(&mut st, &p);
        } else {
            process_bsa(&mut st, &p);
        }
    }
    println!("\n========= COVERAGE SUMMARY =========");
    println!("files (nif)            : {}", st.files);
    println!("  header parsed ok     : {}", st.files_header_ok);
    println!("  parse_nif ok         : {}", st.files_parse_ok);
    println!("  parse_nif ERR        : {}", st.files_parse_err);
    println!(
        "sizeless files (no block_sizes / Oblivion-style) : {}",
        st.sizeless_files
    );
    println!(
        "  ...referencing an UNCOVERED type               : {}",
        st.sizeless_files_uncovered
    );
    println!(
        "  ...AND parse_nif errored (cascade-confirmed)   : {}",
        st.sizeless_files_uncovered_err
    );
    println!("total block instances  : {}", st.block_instances);

    let distinct = st.type_instances.len();
    let uncovered_types: Vec<(&String, &u64)> = st
        .type_instances
        .iter()
        .filter(|(t, _)| !covered(t))
        .collect();
    let uncovered_instances: u64 = uncovered_types.iter().map(|(_, c)| **c).sum();
    println!("distinct block types   : {}", distinct);
    println!(
        "  covered              : {}",
        distinct - uncovered_types.len()
    );
    println!("  UNCOVERED            : {}", uncovered_types.len());
    let cov_pct = if st.block_instances > 0 {
        100.0 * (st.block_instances - uncovered_instances) as f64 / st.block_instances as f64
    } else {
        100.0
    };
    println!(
        "instance coverage %%    : {:.4}  ({} uncovered instances / {})",
        cov_pct, uncovered_instances, st.block_instances
    );

    println!("\n--- UNCOVERED block types (instances / files) ---");
    let mut u: Vec<(String, u64, u64)> = uncovered_types
        .iter()
        .map(|(t, c)| (t.to_string(), **c, *st.type_files.get(*t).unwrap_or(&0)))
        .collect();
    u.sort_by(|a, b| b.1.cmp(&a.1));
    for (t, c, f) in &u {
        println!("  inst={:>8}  files={:>6}  {}", c, f, t);
    }
}
