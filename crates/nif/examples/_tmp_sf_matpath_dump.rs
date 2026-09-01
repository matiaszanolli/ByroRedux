//! #3398 spike helper: dump the distinct material-path strings Starfield
//! NIFs actually name, one per line, so the CDB hash probe in
//! `crates/sfmaterial` can test candidate `BSResource::ID` hashes against
//! paths the engine will really be asked to resolve. Kept separate from the
//! probe so neither crate needs a new dependency on the other.
use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::shader::{BSEffectShaderProperty, BSLightingShaderProperty};
use std::collections::BTreeSet;

fn main() {
    let mut out: BTreeSet<String> = BTreeSet::new();
    let mut cap = 4000usize;
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(i) = args.iter().position(|a| a == "--limit") {
        cap = args[i + 1].parse().unwrap();
        args.drain(i..i + 2);
    }
    for arg in args {
        let Ok(a) = Ba2Archive::open(&arg) else {
            eprintln!("skip {arg}");
            continue;
        };
        let nifs: Vec<String> = a
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
            .take(cap)
            .collect();
        for n in nifs {
            let Ok(b) = a.extract(&n) else { continue };
            let Ok(scene) = byroredux_nif::parse_nif(&b) else {
                continue;
            };
            for blk in &scene.blocks {
                let nm = blk
                    .as_any()
                    .downcast_ref::<BSLightingShaderProperty>()
                    .and_then(|p| p.net.name.as_deref())
                    .or_else(|| {
                        blk.as_any()
                            .downcast_ref::<BSEffectShaderProperty>()
                            .and_then(|p| p.net.name.as_deref())
                    });
                if let Some(nm) = nm {
                    if !nm.is_empty() {
                        out.insert(nm.to_string());
                    }
                }
            }
        }
    }
    eprintln!("{} distinct material paths", out.len());
    for p in out {
        println!("{p}");
    }
}
