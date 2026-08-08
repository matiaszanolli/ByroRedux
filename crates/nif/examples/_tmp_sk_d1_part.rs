//! TEMP: validate remap_bs_tri_shape_bone_indices' single-partition identity shortcut.
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::tri_shape::BsTriShape;
use byroredux_nif::blocks::skin::{BsDismemberSkinInstance, NiSkinInstance, NiSkinPartition};
use byroredux_nif::parse_nif;

fn main() {
    let (mut single, mut single_identity, mut single_non_identity, mut multi) = (0usize,0usize,0usize,0usize);
    let mut examples = 0usize;
    for path in std::env::args().skip(1) {
        let Ok(arc) = BsaArchive::open(&path) else { continue };
        let names: Vec<String> = arc.list_files().into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif")).map(|s| s.to_string()).collect();
        for name in &names {
            let Ok(bytes) = arc.extract(name) else { continue };
            let Ok(scene) = parse_nif(&bytes) else { continue };
            for block in scene.blocks.iter() {
                let Some(s) = block.as_any().downcast_ref::<BsTriShape>() else { continue };
                let Some(si) = s.skin_ref.index() else { continue };
                let pref = if let Some(i) = scene.get_as::<NiSkinInstance>(si) { i.skin_partition_ref }
                    else if let Some(i) = scene.get_as::<BsDismemberSkinInstance>(si) { i.base.skin_partition_ref }
                    else { continue };
                let Some(pi) = pref.index() else { continue };
                let Some(p) = scene.get_as::<NiSkinPartition>(pi) else { continue };
                if p.partitions.len() > 1 { multi += 1; continue; }
                if p.partitions.is_empty() { continue; }
                single += 1;
                let b = &p.partitions[0].bones;
                let ident = b.iter().enumerate().all(|(i, &v)| v as usize == i);
                if ident { single_identity += 1 } else {
                    single_non_identity += 1;
                    if examples < 6 { println!("NON-IDENTITY single partition: {name} bones={:?}", &b[..b.len().min(16)]); examples += 1; }
                }
            }
        }
    }
    println!("single-partition skins = {single} (identity palette {single_identity}, NON-identity {single_non_identity})");
    println!("multi-partition skins  = {multi}");
}
