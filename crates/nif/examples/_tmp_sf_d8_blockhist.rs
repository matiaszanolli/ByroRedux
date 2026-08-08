//! Throwaway (SF audit D8): header-advertised block-type histogram over a BA2.
use byroredux_bsa::Ba2Archive;
use byroredux_nif::header::NifHeader;
fn main() {
    let mut hist: std::collections::BTreeMap<String, u64> = Default::default();
    let mut n = 0u64;
    for path in std::env::args().skip(1) {
        let Ok(a) = Ba2Archive::open(&path) else { eprintln!("skip {path}"); continue };
        let names: Vec<String> = a.list_files().into_iter().filter(|f| f.to_ascii_lowercase().ends_with(".nif")).map(|s| s.to_string()).collect();
        for name in names {
            let Ok(bytes) = a.extract(&name) else { continue };
            let Ok((h, _)) = NifHeader::parse(&bytes) else { continue };
            n += 1;
            for &ti in &h.block_type_indices {
                if let Some(t) = h.block_types.get(ti as usize) { *hist.entry(t.to_string()).or_insert(0) += 1; }
            }
        }
    }
    println!("files={n}");
    let mut v: Vec<_> = hist.into_iter().collect();
    v.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
    for (k, c) in v { println!("{c}\t{k}"); }
}
