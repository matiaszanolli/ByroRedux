use byroredux_bsa::BsaArchive;
use byroredux_nif::blocks::collision::*;
use byroredux_nif::parse_nif;
use std::collections::BTreeMap;
fn main() {
    let mut vc: BTreeMap<usize, usize> = BTreeMap::new();
    let mut degenerate: Vec<String> = Vec::new();
    let mut coplanar = 0usize;
    for path in std::env::args().skip(1) {
        let Ok(arc) = BsaArchive::open(&path) else { continue };
        for name in arc.list_files().into_iter().filter(|n| n.to_ascii_lowercase().ends_with(".nif")).map(|s| s.to_string()) {
            let Ok(bytes) = arc.extract(&name) else { continue };
            let Ok(scene) = parse_nif(&bytes) else { continue };
            for block in scene.blocks.iter() {
                if let Some(s) = block.as_any().downcast_ref::<BhkConvexVerticesShape>() {
                    let n = s.vertices.len();
                    *vc.entry(n.min(64)).or_default() += 1;
                    if n < 4 { if degenerate.len() < 20 { degenerate.push(format!("{name} n={n}")); } }
                    else {
                        // crude coplanarity test on first 4 verts
                        let p: Vec<[f32;3]> = s.vertices.iter().take(4).map(|v| [v[0],v[1],v[2]]).collect();
                        let a=p[0]; let b=p[1]; let c=p[2]; let d=p[3];
                        let u=[b[0]-a[0],b[1]-a[1],b[2]-a[2]];
                        let v=[c[0]-a[0],c[1]-a[1],c[2]-a[2]];
                        let w=[d[0]-a[0],d[1]-a[1],d[2]-a[2]];
                        let cr=[u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]];
                        let det=(cr[0]*w[0]+cr[1]*w[1]+cr[2]*w[2]).abs();
                        if n==4 && det < 1e-9 { coplanar += 1; }
                    }
                }
            }
        }
    }
    println!("bhkConvexVerticesShape vertex-count hist (capped 64): {vc:?}");
    println!("under-4-vertex hulls: {degenerate:?}");
    println!("4-vertex coplanar hulls: {coplanar}");
}
