//! Throwaway (FO4 audit D4): are BSMeshLODTriShape's 3 LOD triangle bands
//! DISJOINT subsets of one surface (cumulative/additive, NifSkope's rule)
//! or ALTERNATIVE triangulations of the same surface (pick-one)?
//! Discriminator: vertex-set overlap between bands.
use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::tri_shape::{BsTriShape, BsTriShapeKind};
use std::collections::HashSet;

fn main() {
    let mut multi = 0usize;
    let mut high_overlap = 0usize;
    let mut low_overlap = 0usize;
    let mut samples = Vec::new();
    for arg in std::env::args().skip(1) {
        let Ok(archive) = Ba2Archive::open(&arg) else { continue };
        for f in archive.list_files() {
            let l = f.to_ascii_lowercase();
            if !l.ends_with(".nif") { continue; }
            let Ok(bytes) = archive.extract(&f) else { continue };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else { continue };
            for b in scene.blocks.iter() {
                let Some(s) = b.as_any().downcast_ref::<BsTriShape>() else { continue };
                let BsTriShapeKind::MeshLOD { lod0, lod1, lod2 } = s.kind else { continue };
                let c = [lod0 as usize, lod1 as usize, lod2 as usize];
                if c.iter().filter(|&&x| x > 0).count() < 2 { continue; }
                if s.triangles.len() != c[0] + c[1] + c[2] { continue; }
                multi += 1;
                let band = |start: usize, n: usize| -> HashSet<u16> {
                    s.triangles[start..start + n].iter().flatten().copied().collect()
                };
                let mut sets = Vec::new();
                let mut off = 0;
                for &n in &c { if n > 0 { sets.push(band(off, n)); } off += n; }
                // pairwise Jaccard-ish overlap of the two largest bands
                let a = &sets[0]; let bset = &sets[1];
                let inter = a.intersection(bset).count();
                let frac = inter as f64 / a.len().min(bset.len()).max(1) as f64;
                if frac > 0.5 { high_overlap += 1 } else { low_overlap += 1 }
                if samples.len() < 8 {
                    samples.push(format!("{f} counts={c:?} |A|={} |B|={} inter={inter} frac={frac:.3}", a.len(), bset.len()));
                }
            }
        }
    }
    println!("multi-band shapes={multi}");
    println!("  overlap>50% (alternative triangulations): {high_overlap}");
    println!("  overlap<=50% (disjoint / additive)      : {low_overlap}");
    for s in &samples { println!("  {s}"); }
}
