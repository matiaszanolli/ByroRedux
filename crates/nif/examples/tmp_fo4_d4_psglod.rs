//! Throwaway (FO4 audit D4): census of BSPackedCombinedSharedGeomDataExtra
//! per-object LOD triangle-count triples in FO4 `_oc.nif` precombine files.
use byroredux_bsa::Ba2Archive;

fn main() {
    let mut files = 0usize;
    let mut objs = 0usize;
    let mut one_band = 0usize;
    let mut two_band = 0usize;
    let mut three_band = 0usize;
    let mut lost_tris: u64 = 0;
    let mut total_tris: u64 = 0;
    let mut samples: Vec<String> = Vec::new();
    let mut contiguous = 0usize;
    let mut noncontig = 0usize;
    for arg in std::env::args().skip(1) {
        let Ok(archive) = Ba2Archive::open(&arg) else { continue };
        for f in archive.list_files() {
            let l = f.to_ascii_lowercase();
            if !l.ends_with("_oc.nif") { continue; }
            let Ok(bytes) = archive.extract(&f) else { continue };
            let Ok(scene) = byroredux_nif::parse_nif(&bytes) else { continue };
            files += 1;
            let mut pool = byroredux_core::string::StringPool::new();
            for g in byroredux_nif::import::precombine::collect_precombine_geom_refs(&scene, &mut pool) {
                objs += 1;
                let c = g.lod_counts;
                let o = g.lod_offsets;
                let nz = c.iter().filter(|&&x| x > 0).count();
                let sum: u64 = c.iter().map(|&x| x as u64).sum();
                let max = *c.iter().max().unwrap() as u64;
                total_tris += sum;
                lost_tris += sum - max;
                match nz { 0 | 1 => one_band += 1, 2 => two_band += 1, _ => three_band += 1 }
                // contiguity: offsets in index units, LOD0 at 0, LOD1 at c0*3, LOD2 at (c0+c1)*3
                if o[0] == 0 && o[1] == c[0] * 3 && o[2] == (c[0] + c[1]) * 3 {
                    contiguous += 1;
                } else {
                    noncontig += 1;
                    if samples.len() < 6 {
                        samples.push(format!("NONCONTIG {f} counts={c:?} offsets={o:?}"));
                    }
                }
                if nz >= 2 && samples.len() < 12 {
                    samples.push(format!("MULTI {f} counts={c:?} offsets={o:?}"));
                }
            }
            if files > 3000 { break; }
        }
    }
    println!("_oc.nif files={files} objects={objs}");
    println!("  <=1 non-zero LOD band : {one_band}");
    println!("  2 non-zero LOD bands  : {two_band}");
    println!("  3 non-zero LOD bands  : {three_band}");
    println!("  contiguous offsets    : {contiguous} / non-contiguous {noncontig}");
    println!("  total tris {total_tris}, dropped-by-max-pick {lost_tris} ({:.2}%)",
        100.0 * lost_tris as f64 / total_tris.max(1) as f64);
    for s in &samples { println!("  {s}"); }
}
