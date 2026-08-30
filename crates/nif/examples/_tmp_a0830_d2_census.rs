//! TEMP (audit 2026-08-30 dim2): header-only (version, user_version, bsver)
//! census across an archive set, plus NifVariant::detect routing.
use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_nif::header::NifHeader;
use byroredux_nif::version::{NifVariant, NifVersion};
use std::collections::BTreeMap;

fn main() {
    let mut tally: BTreeMap<(u32, u32, u32), (usize, String)> = BTreeMap::new();
    for path in std::env::args().skip(1) {
        let files: Vec<(String, Vec<u8>)> = if path.to_ascii_lowercase().ends_with(".ba2") {
            match Ba2Archive::open(&path) {
                Ok(a) => {
                    let names: Vec<String> = a
                        .list_files()
                        .into_iter()
                        .filter(|n| {
                            let l = n.to_ascii_lowercase();
                            l.ends_with(".nif") || l.ends_with(".bto") || l.ends_with(".btr")
                        })
                        .map(|s| s.to_string())
                        .collect();
                    names
                        .into_iter()
                        .filter_map(|n| a.extract(&n).ok().map(|b| (n, b)))
                        .collect()
                }
                Err(e) => {
                    eprintln!("ba2 open fail {path}: {e}");
                    continue;
                }
            }
        } else {
            match BsaArchive::open(&path) {
                Ok(a) => {
                    let names: Vec<String> = a
                        .list_files()
                        .into_iter()
                        .filter(|n| {
                            let l = n.to_ascii_lowercase();
                            l.ends_with(".nif") || l.ends_with(".kf") || l.ends_with(".bto")
                        })
                        .map(|s| s.to_string())
                        .collect();
                    names
                        .into_iter()
                        .filter_map(|n| a.extract(&n).ok().map(|b| (n, b)))
                        .collect()
                }
                Err(e) => {
                    eprintln!("bsa open fail {path}: {e}");
                    continue;
                }
            }
        };
        eprintln!("{path}: {} files", files.len());
        for (name, bytes) in files {
            if let Ok((h, _)) = NifHeader::parse(&bytes) {
                let k = (h.version.0, h.user_version, h.user_version_2);
                let e = tally.entry(k).or_insert((0, name.clone()));
                e.0 += 1;
            }
        }
    }
    println!(
        "{:<14} {:>4} {:>5} {:>9}  {:<12} {}",
        "version", "uv", "uv2", "count", "detect", "sample"
    );
    for ((v, uv, uv2), (n, sample)) in &tally {
        let variant = NifVariant::detect(NifVersion(*v), *uv, *uv2);
        println!(
            "{:<14} {:>4} {:>5} {:>9}  {:<12} {}",
            NifVersion(*v).to_string(),
            uv,
            uv2,
            n,
            format!("{variant:?}"),
            sample
        );
    }
}
