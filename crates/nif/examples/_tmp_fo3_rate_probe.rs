//! Throwaway: why does extract_emitter_rate return None on FO3 files
//! that DO carry a NiPSysEmitterCtlr? Report the block type the
//! controller's interpolator_ref actually points at.
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::particle::NiPSysEmitterCtlr;
use byroredux_nif::parse_nif;

fn main() {
    let path = std::env::args().nth(1).expect("usage: <bsa>");
    let archive = BsaArchive::open(&path).expect("open");
    let files: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();
    let mut hist: std::collections::BTreeMap<String, u32> = Default::default();
    let mut null_ref = 0u32;
    for name in &files {
        let Ok(bytes) = archive.extract(name) else { continue };
        let Ok(scene) = parse_nif(&bytes) else { continue };
        for b in scene.blocks.iter() {
            let Some(c) = b.as_any().downcast_ref::<NiPSysEmitterCtlr>() else { continue };
            match c.interpolator_ref.index() {
                None => null_ref += 1,
                Some(i) => {
                    let t = scene
                        .blocks
                        .get(i)
                        .map(|b| b.block_type_name().to_string())
                        .unwrap_or_else(|| "<out-of-range>".into());
                    *hist.entry(t).or_insert(0) += 1;
                }
            }
        }
    }
    println!("NiPSysEmitterCtlr.interpolator_ref targets: {hist:?} (null refs: {null_ref})");
}
