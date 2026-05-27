//! Scratch: dump every BSShaderNoLightingProperty's file_name in a NIF.
//! Used to investigate F2 (Fallout material plumbing gap, 2026-05-26).

use byroredux_nif::blocks::shader::{
    BSShaderNoLightingProperty, BSShaderPPLightingProperty, BSShaderTextureSet,
};

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump_nolighting <path.nif>");
    let bytes = std::fs::read(&path).expect("read");
    let scene = byroredux_nif::parse_nif(&bytes).expect("parse");

    println!("BSShaderNoLightingProperty entries:");
    let mut nlcount = 0;
    let mut empty_filename = 0;
    for (i, block) in scene.blocks.iter().enumerate() {
        if let Some(s) = block.as_any().downcast_ref::<BSShaderNoLightingProperty>() {
            nlcount += 1;
            let label = if s.file_name.is_empty() {
                empty_filename += 1;
                "<EMPTY>"
            } else {
                s.file_name.as_str()
            };
            println!("  block {:3}: file_name={:?}", i, label);
        }
    }
    println!("  total NoLighting: {} ({} with empty file_name)", nlcount, empty_filename);

    println!("\nBSShaderPPLightingProperty entries (with linked BSShaderTextureSet):");
    let mut ppcount = 0;
    let mut empty_ts = 0;
    for (i, block) in scene.blocks.iter().enumerate() {
        if let Some(s) = block.as_any().downcast_ref::<BSShaderPPLightingProperty>() {
            ppcount += 1;
            let ts_idx = s.texture_set_ref.index();
            let tex0 = ts_idx
                .and_then(|idx| scene.get_as::<BSShaderTextureSet>(idx))
                .and_then(|ts| ts.textures.first().cloned())
                .unwrap_or_else(|| {
                    empty_ts += 1;
                    "<no texture_set>".to_string()
                });
            println!("  block {:3}: ts_ref={:?} tex0={:?}", i, ts_idx, tex0);
        }
    }
    println!(
        "  total PPLighting: {} ({} with empty texture_set)",
        ppcount, empty_ts
    );
}
