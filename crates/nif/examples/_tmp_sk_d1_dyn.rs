//! TEMP: why do BSDynamicTriShape meshes lose per-vertex skin data?
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::skin::{BsDismemberSkinInstance, NiSkinInstance, NiSkinPartition};
use byroredux_nif::blocks::tri_shape::{BsTriShape, BsTriShapeKind};
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;

fn main() {
    let mut buf_attrs: BTreeMap<u16, usize> = BTreeMap::new();
    let mut plain_buf_attrs: BTreeMap<u16, usize> = BTreeMap::new();
    let mut dyn_with_buf = 0usize;
    let mut dyn_no_buf = 0usize;
    let mut printed = 0usize;
    for path in std::env::args().skip(1) {
        let Ok(arc) = BsaArchive::open(&path) else {
            continue;
        };
        let names: Vec<String> = arc
            .list_files()
            .into_iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".nif"))
            .map(|s| s.to_string())
            .collect();
        for name in &names {
            let Ok(bytes) = arc.extract(name) else {
                continue;
            };
            let Ok(scene) = parse_nif(&bytes) else {
                continue;
            };
            for block in scene.blocks.iter() {
                let Some(s) = block.as_any().downcast_ref::<BsTriShape>() else {
                    continue;
                };
                let is_dyn = matches!(s.kind, BsTriShapeKind::Dynamic { .. });
                let Some(si) = s.skin_ref.index() else {
                    continue;
                };
                let pref = if let Some(i) = scene.get_as::<NiSkinInstance>(si) {
                    i.skin_partition_ref
                } else if let Some(i) = scene.get_as::<BsDismemberSkinInstance>(si) {
                    i.base.skin_partition_ref
                } else {
                    continue;
                };
                let Some(pi) = pref.index() else { continue };
                let Some(p) = scene.get_as::<NiSkinPartition>(pi) else {
                    continue;
                };
                match p.global_vertex_data.as_ref() {
                    Some(b) => {
                        let a = ((b.vertex_desc >> 44) & 0xFFF) as u16;
                        if is_dyn {
                            dyn_with_buf += 1;
                            *buf_attrs.entry(a).or_default() += 1;
                            if printed < 5 && a & 0x001 == 0 {
                                println!("DYN {name}: shape_attrs=0x{:03x} buf_attrs=0x{a:03x} buf_vsize={} nverts_shape={} dyn_verts={}",
                                    ((s.vertex_desc >> 44) & 0xFFF) as u16, b.vertex_size, s.num_vertices, s.vertices.len());
                                printed += 1;
                            }
                        } else {
                            *plain_buf_attrs.entry(a).or_default() += 1;
                        }
                    }
                    None => {
                        if is_dyn {
                            dyn_no_buf += 1;
                        }
                    }
                }
            }
        }
    }
    println!("dyn_with_buf={dyn_with_buf} dyn_no_buf={dyn_no_buf}");
    println!(
        "DYN partition-buffer attrs: {:?}",
        buf_attrs
            .iter()
            .map(|(k, v)| (format!("0x{k:03x}"), *v))
            .collect::<Vec<_>>()
    );
    println!(
        "PLAIN partition-buffer attrs: {:?}",
        plain_buf_attrs
            .iter()
            .map(|(k, v)| (format!("0x{k:03x}"), *v))
            .collect::<Vec<_>>()
    );
}
