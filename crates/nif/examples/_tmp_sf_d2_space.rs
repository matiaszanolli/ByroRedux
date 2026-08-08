//! SF D2: decide the coordinate space of BSGeometry `.mesh` vertex data by
//! comparing them to the NIF-side `bounding_sphere` / `bound_min_max`.
use byroredux_bsa::Ba2Archive;
use byroredux_nif::blocks::bs_geometry::{BSGeometry, BSGeometryMeshData, BSGeometryMeshKind};
use byroredux_nif::parse_nif;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ar = Ba2Archive::open(&a[1]).expect("open");
    let stride: usize = a.get(2).map(|s| s.parse().unwrap()).unwrap_or(1);
    let nifs: Vec<String> = ar.list_files().into_iter()
        .filter(|n| n.to_ascii_lowercase().ends_with(".nif")).map(|s| s.to_string()).collect();
    let mut contains_raw = 0; let mut contains_swap = 0; let mut contains_neither = 0; let mut n = 0;
    let mut bmm_raw = 0; let mut bmm_swap = 0; let mut bmm_neither = 0;
    let mut shown = 0; let mut metric_ok = 0; let mut metric_bad = 0; let mut ratios: Vec<f32> = Vec::new();
    for name in nifs.iter().step_by(stride) {
        let Ok(bytes) = ar.extract(name) else { continue };
        let Ok(scene) = parse_nif(&bytes) else { continue };
        for blk in &scene.blocks {
            let Some(g) = blk.as_any().downcast_ref::<BSGeometry>() else { continue };
            if g.has_internal_geom_data() { continue; }
            let (c, r) = g.bounding_sphere;
            if r <= 0.0 { continue; }
            let Some(m) = g.meshes.first() else { continue };
            let BSGeometryMeshKind::External { mesh_name } = &m.kind else { continue };
            let Ok(mb) = ar.extract(&format!("geometries\\{mesh_name}.mesh")) else { continue };
            let Ok(d) = BSGeometryMeshData::parse_from_bytes(&mb) else { continue };
            if d.vertices.is_empty() { continue; }
            n += 1;
            let cs = [c[0], c[2], -c[1]]; // zup->yup of the NIF sphere center
            let mut dr: f32 = 0.0; let mut ds: f32 = 0.0;
            let mut mn = [f32::MAX; 3]; let mut mx = [f32::MIN; 3];
            for p in &d.vertices {
                dr = dr.max(((p[0]-c[0]).powi(2)+(p[1]-c[1]).powi(2)+(p[2]-c[2]).powi(2)).sqrt());
                ds = ds.max(((p[0]-cs[0]).powi(2)+(p[1]-cs[1]).powi(2)+(p[2]-cs[2]).powi(2)).sqrt());
                for i in 0..3 { mn[i] = mn[i].min(p[i]); mx[i] = mx[i].max(p[i]); }
            }
            // metric-space test: divide decoded vertices by HAVOK_SCALE
            let hs = 69.969f32;
            let mut dm: f32 = 0.0;
            for p in &d.vertices {
                let q = [p[0]/hs, p[1]/hs, p[2]/hs];
                dm = dm.max(((q[0]-c[0]).powi(2)+(q[1]-c[1]).powi(2)+(q[2]-c[2]).powi(2)).sqrt());
            }
            if dm <= r * 1.05 + 1e-4 { metric_ok += 1; } else { metric_bad += 1; }
            ratios.push(dr / r);
            let tol = r * 1.05 + 1e-3;
            match (dr <= tol, ds <= tol) {
                (true, false) => contains_raw += 1,
                (false, true) => contains_swap += 1,
                (true, true) => contains_raw += 1,
                _ => { contains_neither += 1;
                       if shown < 6 { println!("  NEITHER {} r={} dr={} ds={} c={:?} aabb={:?}..{:?}", name, r, dr, ds, c, mn, mx); shown += 1; } }
            }
            // bound_min_max interpretation: [minx,miny,minz,maxx,maxy,maxz]?
            let b = g.bound_min_max;
            let fits = |bm: [f32; 6]| {
                mn[0] >= bm[0]-1e-2 && mn[1] >= bm[1]-1e-2 && mn[2] >= bm[2]-1e-2 &&
                mx[0] <= bm[3]+1e-2 && mx[1] <= bm[4]+1e-2 && mx[2] <= bm[5]+1e-2
            };
            let braw = b;
            let bswap = [b[0], b[2], -b[4], b[3], b[5], -b[1]];
            match (fits(braw), fits(bswap)) {
                (true, _) => bmm_raw += 1,
                (false, true) => bmm_swap += 1,
                _ => bmm_neither += 1,
            }
        }
    }
    println!("n={} sphere contains: raw={} swap={} neither={}", n, contains_raw, contains_swap, contains_neither);
    ratios.sort_by(|a,b| a.partial_cmp(b).unwrap());
    if !ratios.is_empty() { println!("dr/r ratio: min={} p50={} p95={} max={}", ratios[0], ratios[ratios.len()/2], ratios[ratios.len()*95/100], ratios[ratios.len()-1]); }
    println!("metric-space (verts/69.969) inside sphere: ok={} bad={}", metric_ok, metric_bad);
    println!("bound_min_max fits: raw={} swap={} neither={}", bmm_raw, bmm_swap, bmm_neither);
}
