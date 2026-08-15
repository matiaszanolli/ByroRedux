//! Throwaway (SF audit 2026-08-12): survey Starfield BSLightingShaderProperty /
//! BSEffectShaderProperty stub-vs-full split and whether full-body blocks
//! carry a resolvable BSShaderTextureSet with any non-empty texture strings.
use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::shader::BSShaderTextureSet;
use byroredux_nif::blocks::shader::{BSEffectShaderProperty, BSLightingShaderProperty};

fn main() {
    let mut limit = usize::MAX;
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(pos) = args.iter().position(|a| a == "--limit") {
        limit = args[pos + 1].parse().unwrap();
        args.drain(pos..pos + 2);
    }

    let mut lsp_stub = 0u64;
    let mut lsp_full = 0u64;
    let mut lsp_full_ts_null = 0u64;
    let mut lsp_full_ts_resolved = 0u64;
    let mut lsp_full_ts_nonempty0 = 0u64;
    let mut esp_stub = 0u64;
    let mut esp_full = 0u64;
    let mut esp_full_srctex = 0u64;
    let mut texset_blocks = 0u64;
    let mut texset_nonempty = 0u64;
    let mut texset_slotcount: std::collections::BTreeMap<usize, u64> = Default::default();
    let mut sample: Vec<String> = Vec::new();

    for path in &args {
        let Ok(archive) = Ba2Archive::open(path) else {
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
        eprintln!("{path}: {} nifs", names.len());
        for name in names {
            let Ok(bytes) = archive.extract(&name) else {
                continue;
            };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else {
                continue;
            };
            for i in 0..scene.blocks.len() {
                if let Some(ts) = scene.get_as::<BSShaderTextureSet>(i) {
                    texset_blocks += 1;
                    *texset_slotcount.entry(ts.textures.len()).or_insert(0) += 1;
                    if ts.textures.iter().any(|t| !t.is_empty()) {
                        texset_nonempty += 1;
                        if sample.len() < 8 {
                            sample.push(format!("{name}: {:?}", ts.textures));
                        }
                    }
                }
                if let Some(s) = scene.get_as::<BSLightingShaderProperty>(i) {
                    if s.material_reference {
                        lsp_stub += 1;
                    } else {
                        lsp_full += 1;
                        match s.texture_set_ref.index() {
                            None => lsp_full_ts_null += 1,
                            Some(ti) => {
                                if let Some(ts) = scene.get_as::<BSShaderTextureSet>(ti) {
                                    lsp_full_ts_resolved += 1;
                                    if ts.textures.first().is_some_and(|t| !t.is_empty()) {
                                        lsp_full_ts_nonempty0 += 1;
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(s) = scene.get_as::<BSEffectShaderProperty>(i) {
                    if s.material_reference {
                        esp_stub += 1;
                    } else {
                        esp_full += 1;
                        if !s.source_texture.is_empty() {
                            esp_full_srctex += 1;
                        }
                    }
                }
            }
        }
    }

    println!("BSLSP stub={lsp_stub} full={lsp_full}");
    println!(
        "  full: ts_ref NULL={lsp_full_ts_null} resolved={lsp_full_ts_resolved} slot0_nonempty={lsp_full_ts_nonempty0}"
    );
    println!("BSESP stub={esp_stub} full={esp_full} src_tex_nonempty={esp_full_srctex}");
    println!("BSShaderTextureSet blocks={texset_blocks} with_any_nonempty={texset_nonempty}");
    println!("texset slot-count histogram: {texset_slotcount:?}");
    for s in &sample {
        println!("  sample {s}");
    }
}
