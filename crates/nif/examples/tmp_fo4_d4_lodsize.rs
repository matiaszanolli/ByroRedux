//! Throwaway (FO4 audit D4): does BSMeshLODTriShape's lod0/1/2 sum to
//! num_triangles (concatenated alternative triangulations) or not?
use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::tri_shape::{BsTriShape, BsTriShapeKind};

fn main() {
    let mut files = 0usize;
    let mut shapes = 0usize;
    let mut sum_eq = 0usize;
    let mut lod0_eq = 0usize;
    let mut other = 0usize;
    let mut samples: Vec<String> = Vec::new();
    let mut zero_all = 0usize;
    for arg in std::env::args().skip(1) {
        let Ok(archive) = Ba2Archive::open(&arg) else {
            eprintln!("open failed {arg}");
            continue;
        };
        for f in archive.list_files() {
            let l = f.to_ascii_lowercase();
            if !(l.ends_with(".nif") || l.ends_with(".bto") || l.ends_with(".btr")) {
                continue;
            }
            let Ok(bytes) = archive.extract(&f) else { continue };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else { continue };
            files += 1;
            for b in scene.blocks.iter() {
                let Some(s) = b.as_any().downcast_ref::<BsTriShape>() else { continue };
                let BsTriShapeKind::MeshLOD { lod0, lod1, lod2 } = s.kind else { continue };
                shapes += 1;
                let nt = s.num_triangles;
                let tris = s.triangles.len() as u32;
                if lod0 == 0 && lod1 == 0 && lod2 == 0 {
                    zero_all += 1;
                } else if lod0 + lod1 + lod2 == nt {
                    sum_eq += 1;
                    if samples.len() < 8 {
                        samples.push(format!(
                            "SUM  {f} nt={nt} tris={tris} lods=({lod0},{lod1},{lod2})"
                        ));
                    }
                } else if lod0 == nt {
                    lod0_eq += 1;
                    if samples.len() < 8 {
                        samples.push(format!(
                            "LOD0 {f} nt={nt} tris={tris} lods=({lod0},{lod1},{lod2})"
                        ));
                    }
                } else {
                    other += 1;
                    if samples.len() < 16 {
                        samples.push(format!(
                            "OTHR {f} nt={nt} tris={tris} lods=({lod0},{lod1},{lod2})"
                        ));
                    }
                }
            }
            if files > 40000 { break; }
        }
    }
    println!("files={files} meshlod_shapes={shapes}");
    println!("  lod0+lod1+lod2 == num_triangles : {sum_eq}");
    println!("  lod0 == num_triangles           : {lod0_eq}");
    println!("  all-zero lods                   : {zero_all}");
    println!("  other                           : {other}");
    for s in &samples { println!("  {s}"); }
}
