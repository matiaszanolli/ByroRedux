//! THROWAWAY audit probe (2026-08-30, /audit-nif D3): the `.kf` animation
//! corpus — excluded from `corpus::is_nif_entry`, hence from every gate.
use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_nif::blocks::NiUnknown;
use byroredux_nif::header::NifHeader;
use std::collections::BTreeMap;

enum A {
    B(BsaArchive),
    T(Ba2Archive),
}
impl A {
    fn list(&self) -> Vec<String> {
        match self {
            A::B(a) => a.list_files().into_iter().map(|s| s.to_string()).collect(),
            A::T(a) => a.list_files().into_iter().map(|s| s.to_string()).collect(),
        }
    }
    fn ex(&self, p: &str) -> std::io::Result<Vec<u8>> {
        match self {
            A::B(a) => a.extract(p),
            A::T(a) => a.extract(p),
        }
    }
}

fn main() {
    let mut hist: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut ex: BTreeMap<String, String> = BTreeMap::new();
    let mut tot = 0;
    let mut clean = 0;
    let mut rec = 0;
    let mut trunc = 0;
    let mut fail = 0;
    for p in std::env::args().skip(1) {
        let path = std::path::Path::new(&p);
        if !path.is_file() {
            eprintln!("# MISSING {p}");
            continue;
        }
        let a = if p.to_ascii_lowercase().ends_with(".ba2") {
            match Ba2Archive::open(path) {
                Ok(x) => A::T(x),
                Err(e) => {
                    eprintln!("# open {p}: {e}");
                    continue;
                }
            }
        } else {
            match BsaArchive::open(path) {
                Ok(x) => A::B(x),
                Err(e) => {
                    eprintln!("# open {p}: {e}");
                    continue;
                }
            }
        };
        let files: Vec<String> = a
            .list()
            .into_iter()
            .filter(|f| f.to_ascii_lowercase().ends_with(".kf"))
            .collect();
        eprintln!("# {p}: {} .kf entries", files.len());
        for f in &files {
            tot += 1;
            let Ok(b) = a.ex(f) else {
                fail += 1;
                continue;
            };
            match byroredux_nif::parse_nif(&b) {
                Ok(s) => {
                    if s.truncated {
                        trunc += 1
                    } else if s.recovered_blocks > 0 {
                        rec += 1
                    } else {
                        clean += 1
                    }
                    let h = NifHeader::parse(&b).ok().map(|(h, _)| h);
                    for (i, blk) in s.blocks.iter().enumerate() {
                        if let Some(u) = blk.as_any().downcast_ref::<NiUnknown>() {
                            let n = u.type_name.as_ref().to_string();
                            hist.entry(n.clone()).or_default().1 += 1;
                            ex.entry(n).or_insert_with(|| format!("{p}::{f}"));
                        } else {
                            let w = h
                                .as_ref()
                                .and_then(|h| {
                                    h.block_type_indices
                                        .get(i)
                                        .and_then(|&t| h.block_types.get(t as usize))
                                })
                                .map(|x| x.as_ref())
                                .unwrap_or_else(|| blk.block_type_name());
                            hist.entry(w.to_string()).or_default().0 += 1;
                        }
                    }
                }
                Err(e) => {
                    fail += 1;
                    if fail < 6 {
                        eprintln!("#  FAIL {f}: {e}")
                    }
                }
            }
        }
    }
    println!("# kf total={tot} clean={clean} recovered={rec} truncated={trunc} failed={fail}");
    for (k, (p, u)) in &hist {
        println!(
            "{k}\t{p}\t{u}\t{}",
            if *u > 0 {
                ex.get(k).cloned().unwrap_or_default()
            } else {
                String::new()
            }
        );
    }
}
