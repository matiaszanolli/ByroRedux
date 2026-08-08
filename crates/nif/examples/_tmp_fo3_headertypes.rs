//! Throwaway: TRUE header-advertised block-type histogram (header parse
//! only), which the per-block baseline collapses via block_type_name().
use byroredux_bsa::BsaArchive;
use byroredux_nif::header::NifHeader;

fn main() {
    let mut hist: std::collections::BTreeMap<String, u64> = Default::default();
    let mut bsvers: std::collections::BTreeMap<(u32, u32), u64> = Default::default();
    let mut nfiles = 0u64;
    for path in std::env::args().skip(1) {
        let Ok(archive) = BsaArchive::open(&path) else { eprintln!("skip {path}"); continue };
        for name in archive.list_files().into_iter().filter(|n| n.to_ascii_lowercase().ends_with(".nif")).map(|s| s.to_string()).collect::<Vec<_>>() {
            let Ok(bytes) = archive.extract(&name) else { continue };
            let Ok((h, _)) = NifHeader::parse(&bytes) else { continue };
            nfiles += 1;
            *bsvers.entry((h.version.0, h.user_version_2)).or_insert(0) += 1;
            for &ti in &h.block_type_indices {
                if let Some(t) = h.block_types.get(ti as usize) {
                    *hist.entry(t.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
    println!("# files={nfiles}");
    println!("# (version,bsver) distribution: {bsvers:?}");
    for (k, v) in &hist {
        println!("{v}\t{k}");
    }
}
