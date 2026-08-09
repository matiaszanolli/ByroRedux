//! TEMP: does the single-partition identity shortcut ever pick a wrong bone
//! for a vertex with non-zero weight?
use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::skin::{BsDismemberSkinInstance, NiSkinInstance, NiSkinPartition};
use byroredux_nif::blocks::tri_shape::BsTriShape;
use byroredux_nif::parse_nif;

fn half(h: u16) -> f32 {
    let s = ((h >> 15) & 1) as u32;
    let e = ((h >> 10) & 0x1F) as i32;
    let m = (h & 0x3FF) as u32;
    let bits = if e == 0 {
        if m == 0 {
            s << 31
        } else {
            let mut mm = m;
            let mut ee = -14i32;
            while mm & 0x400 == 0 {
                mm <<= 1;
                ee -= 1;
            }
            mm &= 0x3FF;
            (s << 31) | (((ee + 127) as u32) << 23) | (mm << 13)
        }
    } else if e == 31 {
        (s << 31) | (0xFFu32 << 23) | (m << 13)
    } else {
        (s << 31) | (((e - 15 + 127) as u32) << 23) | (m << 13)
    };
    f32::from_bits(bits)
}

fn main() {
    let (mut verts, mut wrong_verts, mut wrong_shapes) = (0u64, 0u64, 0u64);
    let mut shown = 0usize;
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
                if p.partitions.len() != 1 {
                    continue;
                }
                let part = &p.partitions[0];
                let Some(buf) = p.global_vertex_data.as_ref() else {
                    continue;
                };
                let vsz = buf.vertex_size as usize;
                if vsz == 0 || buf.raw_bytes.len() % vsz != 0 {
                    continue;
                }
                let attrs = ((buf.vertex_desc >> 44) & 0xFFF) as u16;
                if attrs & 0x040 == 0 {
                    continue;
                } // not skinned
                  // compute skin offset the same way the sequential parser does
                let mut off = 0usize;
                if attrs & 0x001 != 0 {
                    off += 16;
                }
                if attrs & 0x002 != 0 {
                    off += 4;
                }
                if attrs & 0x008 != 0 {
                    off += 4;
                }
                if attrs & 0x010 != 0 && attrs & 0x008 != 0 {
                    off += 4;
                }
                if attrs & 0x020 != 0 {
                    off += 4;
                }
                let nv = buf.raw_bytes.len() / vsz;
                let mut shape_bad = false;
                for v in 0..nv {
                    let b = &buf.raw_bytes[v * vsz..(v + 1) * vsz];
                    if off + 12 > vsz {
                        break;
                    }
                    verts += 1;
                    for k in 0..4 {
                        let w = half(u16::from_le_bytes([b[off + k * 2], b[off + k * 2 + 1]]));
                        let li = b[off + 8 + k] as usize;
                        if w <= 1e-4 {
                            continue;
                        }
                        let Some(&pv) = part.bones.get(li) else {
                            continue;
                        };
                        let palette = pv as usize;
                        if palette != li {
                            wrong_verts += 1;
                            shape_bad = true;
                            if shown < 8 {
                                println!(
                                    "{name}: local {li} -> palette {palette} (w={w:.3}) bones={:?}",
                                    &part.bones[..part.bones.len().min(12)]
                                );
                                shown += 1;
                            }
                            break;
                        }
                    }
                }
                if shape_bad {
                    wrong_shapes += 1;
                }
            }
        }
    }
    println!("single-partition skinned vertices = {verts}");
    println!("vertices where identity != palette (non-zero weight) = {wrong_verts}");
    println!("shapes affected = {wrong_shapes}");
}
