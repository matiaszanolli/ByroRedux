//! Throwaway (EXAL step 6): dump imported-mesh position AABB for a LOD
//! `.bto`/`.btr` to determine its coordinate convention (world-absolute vs
//! quad-relative). `translation + position` per mesh, then the overall AABB.

fn main() {
    let path = std::env::args().nth(1).expect("usage: lod_probe <path>");
    let bytes = std::fs::read(&path).expect("read");
    let scene = byroredux_nif::parse_nif(&bytes).expect("parse");
    let mut pool = byroredux_core::string::StringPool::new();
    let imported = byroredux_nif::import::import_nif_scene(&scene, &mut pool);

    let mut gmin = [f32::INFINITY; 3];
    let mut gmax = [f32::NEG_INFINITY; 3];
    for (i, m) in imported.meshes.iter().enumerate() {
        let mut lmin = [f32::INFINITY; 3];
        let mut lmax = [f32::NEG_INFINITY; 3];
        for p in &m.positions {
            for k in 0..3 {
                let w = p[k] + m.translation[k];
                lmin[k] = lmin[k].min(w);
                lmax[k] = lmax[k].max(w);
                gmin[k] = gmin[k].min(w);
                gmax[k] = gmax[k].max(w);
            }
        }
        println!(
            "mesh[{i}] verts={:5} translation={:?} aabb min={:?} max={:?}",
            m.positions.len(),
            m.translation,
            lmin,
            lmax
        );
    }
    println!("OVERALL aabb min={gmin:?} max={gmax:?}");
}
