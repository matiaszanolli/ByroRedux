//! Throwaway (SF audit D8): survey BSLightingShaderProperty / BSEffectShaderProperty
//! material-path names on real Starfield content — trailing junk, suffix mix,
//! stub vs full, root_material_path fallback usage.
use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::shader::{BSEffectShaderProperty, BSLightingShaderProperty};
use byroredux_nif::scene::NifScene;

fn classify(name: &str) -> (&'static str, bool) {
    let trimmed = name.trim_end_matches(|c: char| c == '\0' || c.is_ascii_whitespace());
    let lower = trimmed.to_ascii_lowercase();
    let suffix = if lower.ends_with(".mat") {
        ".mat"
    } else if lower.ends_with(".bgsm") {
        ".bgsm"
    } else if lower.ends_with(".bgem") {
        ".bgem"
    } else {
        "none"
    };
    (suffix, trimmed.len() != name.len())
}

fn main() {
    let mut n_files = 0u64;
    let mut lsp_stub = 0u64;
    let mut lsp_full = 0u64;
    let mut lsp_stub_no_path = 0u64;
    let mut lsp_full_matpath = 0u64;
    let mut root_fallback_used = 0u64;
    let mut suffix_hist: std::collections::BTreeMap<&'static str, u64> = Default::default();
    let mut trailing_junk = 0u64;
    let mut junk_samples: Vec<String> = Vec::new();
    let mut none_samples: Vec<String> = Vec::new();
    let mut lsp_full_texset = 0u64;
    let mut esp_matpath = 0u64;
    let mut esp_total = 0u64;
    let mut esp_src_tex = 0u64;
    let mut limit = usize::MAX;
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(pos) = args.iter().position(|a| a == "--limit") {
        limit = args[pos + 1].parse().unwrap();
        args.drain(pos..pos + 2);
    }
    for path in args {
        let Ok(archive) = Ba2Archive::open(&path) else {
            eprintln!("skip {path}");
            continue;
        };
        let names: Vec<String> = archive
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
            .take(limit)
            .collect();
        for name in names {
            let Ok(bytes) = archive.extract(&name) else {
                continue;
            };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
                continue;
            };
            n_files += 1;
            for i in 0..scene.blocks.len() {
                if let Some(sp) = scene.get_as::<BSLightingShaderProperty>(i) {
                    let nm = sp.net.name.as_deref().unwrap_or("");
                    let (suffix, junk) = classify(nm);
                    if sp.material_reference {
                        lsp_stub += 1;
                        *suffix_hist.entry(suffix).or_insert(0) += 1;
                        if junk {
                            trailing_junk += 1;
                            if junk_samples.len() < 5 {
                                junk_samples.push(format!("{nm:?}"));
                            }
                        }
                        if suffix == "none" {
                            lsp_stub_no_path += 1;
                            if none_samples.len() < 12 {
                                none_samples.push(format!("{}::{:?}", name, nm));
                            }
                        }
                    } else {
                        lsp_full += 1;
                        if suffix != "none" {
                            lsp_full_matpath += 1;
                        } else if sp
                            .root_material_path
                            .as_deref()
                            .map(|r| classify(r).0 != "none")
                            .unwrap_or(false)
                        {
                            root_fallback_used += 1;
                        }
                        if sp.texture_set_ref.index().is_some() {
                            lsp_full_texset += 1;
                        }
                    }
                }
                if let Some(ep) = scene.get_as::<BSEffectShaderProperty>(i) {
                    esp_total += 1;
                    let nm = ep.net.name.as_deref().unwrap_or("");
                    if classify(nm).0 != "none" {
                        esp_matpath += 1;
                    }
                    if !ep.source_texture.trim().is_empty() {
                        esp_src_tex += 1;
                    }
                }
            }
        }
    }
    println!("files={n_files}");
    println!("BSLSP stub={lsp_stub} full={lsp_full}");
    println!("  stub suffix hist: {suffix_hist:?}");
    println!("  stub with NO material suffix (path lost): {lsp_stub_no_path}");
    for s in &none_samples {
        println!("    none-sample {s}");
    }
    println!("  stub names w/ trailing NUL/ws: {trailing_junk} samples={junk_samples:?}");
    println!("  full w/ material path on name: {lsp_full_matpath}, via root_material_path: {root_fallback_used}");
    println!("  full w/ texture_set_ref: {lsp_full_texset}");
    println!(
        "BSESP total={esp_total} w/ material path={esp_matpath} w/ source_texture={esp_src_tex}"
    );
}
